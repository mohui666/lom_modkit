using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using BepInEx;
using BepInEx.Configuration;
using BepInEx.Unity.Mono;
using BepInEx.Unity.Mono.Configuration;
using HarmonyLib;
using Mortal.Core;
using Mortal.Free;
using Mortal.Story;
using UnityEngine;
using UnityEngine.InputSystem;

namespace MortalModHost
{
    /// <summary>
    /// 活侠传 mod 宿主插件入口：发现 .lommod 包 → 解析 → Harmony 注入 LuaManager → Free 场景内 IMGUI 菜单演出。
    /// 运行行为契约见 docs/zh_CN/mod_format.md §6。
    /// </summary>
    [BepInPlugin(GUID, NAME, VERSION)]
    public class Plugin : BaseUnityPlugin
    {
        public const string GUID = "com.mohui666.mortalmodhost";
        public const string NAME = "MortalModHost";
        public const string VERSION = "0.6.0";

        private const int WindowId = 886310; // IMGUI 窗口 id，取个不易与其他插件撞车的数
        private const int DebugWindowId = 886311;

        /// <summary>本轮解析到的全部 mod 包（patch 与菜单共用）。</summary>
        internal static List<ModPackage> LoadedMods { get; private set; }

        private ConfigEntry<bool> _enabled;
        private ConfigEntry<KeyboardShortcut> _menuHotkey;
        private ConfigEntry<KeyboardShortcut> _vanillaStoryHotkey;
        private ConfigEntry<KeyboardShortcut> _debuggerHotkey;

        private bool _showMenu;
        private bool _inTitleScene; // 当前菜单是否处于标题画面（Title 场景仅提供战役区）
        private Rect _windowRect = new Rect(40f, 40f, 460f, 420f);
        private Vector2 _scroll;
        private float _nextPreviewPoll;
        private long _loadedPreviewStamp = -1L;
        private string _previewWaitingScene = "";
        private bool _showDebugger = true;
        private bool _wasTraceActive;
        private Rect _debugWindowRect = new Rect(20f, 20f, 520f, 680f);
        private Vector2 _debugScroll;

        private void Awake()
        {
            _enabled = Config.Bind("General", "Enabled", true,
                "总开关。false 时禁用热键、mod 菜单与 LuaManager 注入。");
            _menuHotkey = Config.Bind("General", "MenuHotkey", new KeyboardShortcut(KeyCode.F8),
                "打开/关闭 mod 菜单的快捷键（Free 自由场景与 Title 标题画面生效，契约 §6.3）。旧默认 F9 与同机 MortalInstantWin 冲突，启动时自动迁移为 F8。");
            _vanillaStoryHotkey = Config.Bind("General", "VanillaStoryHotkey", new KeyboardShortcut(KeyCode.F7),
                "切换「禁用原版游戏剧情」的全局临时开关（任意场景可切换；会话级，不持久化）。开启后跳过返回 Free 时自动触发及地点点击触发的官方主线、支线和默认脚本，mod 触发器仍优先。");
            _debuggerHotkey = Config.Bind("Development", "DebuggerHotkey", new KeyboardShortcut(KeyCode.F10),
                "仅编辑器 F5 开发包生效：显示/隐藏 Runtime Debugger。正式 Mod 不启用调试器。");
            MigrateLegacyHotkey();
            Logger.LogInfo("菜单热键：" + _menuHotkey.Value + "（Free 场景左下角也有常驻入口按钮）；原版剧情开关热键：" + _vanillaStoryHotkey.Value);

            // 契约 §6.13：死亡/结局文本覆盖的静态初始态（重复启动时防止残留上次会话的文本）
            ModOverlay.Clear();
            // 契约 §2：mod 战役运行态同样重置（插件重载后不残留旧战役的禁原版事件状态）
            ModCampaignState.Clear();

            // mods 目录：BepInEx/plugins/MortalModHost/mods/（契约 §6.1）
            string modsDir = Path.Combine(Paths.PluginPath, "MortalModHost", "mods");
            Logger.LogInfo("MortalModHost " + VERSION + " 启动，扫描 mods 目录：" + modsDir);

            ReloadMods();

            // 官方播放器默认失焦暂停：标题画面试玩请求等 Update 逻辑在后台也照常运行。
            Application.runInBackground = true;
            Logger.LogInfo("已开启 runInBackground（失焦时 Update 仍跑）");

            ApplyHarmonyPatch();
        }

        /// <summary>
        /// 旧版本默认热键是 F9，与同机 MortalInstantWin 冲突；现默认改 F8。
        /// 已有 cfg 里 MenuHotkey 若仍是 F9（旧默认残留），直接改写文件为 F8 并 Reload。
        /// 无法可靠区分"用户主动改回 F9"的情况，本轮统一迁移并在日志说明。
        /// </summary>
        private void MigrateLegacyHotkey()
        {
            if (_menuHotkey.Value.MainKey != KeyCode.F9 || HasModifiers(_menuHotkey.Value)) return;
            try
            {
                string path = Config.ConfigFilePath;
                string migrated;
                if (File.Exists(path) && HotkeyMigration.TryRewriteLegacyHotkey(File.ReadAllText(path), out migrated))
                {
                    File.WriteAllText(path, migrated);
                    Config.Reload();
                    Logger.LogInfo("检测到旧默认热键 F9 残留（与 MortalInstantWin 冲突），已一次性迁移为 F8。");
                }
                else
                {
                    // cfg 文件缺失或该行未落盘：直接改当前值（BepInEx 会自动保存）
                    _menuHotkey.Value = new KeyboardShortcut(KeyCode.F8);
                    Logger.LogInfo("检测到旧默认热键 F9，已迁移为 F8。");
                }
            }
            catch (Exception ex)
            {
                Logger.LogWarning("热键 F9→F8 迁移失败：" + ex.Message);
            }
        }

        private static bool HasModifiers(KeyboardShortcut shortcut)
        {
            foreach (KeyCode modifier in shortcut.Modifiers) return true;
            return false;
        }

        /// <summary>挂 Harmony patch。目标方法/字段找不到时明确报错而不是静默失效。</summary>
        private void ApplyHarmonyPatch()
        {
            LuaManagerPatch.Log = Logger;
            NewGameDataPatch.Log = Logger;
            FreePositionPatch.Log = Logger;
            MoodControl.Log = Logger;
            ReadTextPatch.Log = Logger;
            GameOverOverlayPatch.Log = Logger;
            EndGamePanelOverlayPatch.Log = Logger;
            EndGameOverlayPatch.Log = Logger;
            ModOverlay.Log = Logger;
            NewGamePlusPatch.Log = Logger;
            DiceRevolutionPatch.Log = Logger;
            VanillaStorySwitch.Log = Logger;
            CharacterIntroSupport.Log = Logger;
            CustomAudioPlayer.Log = Logger;
            CustomAudioPlayer.Init(this);
            CustomCharacterRuntime.Log = Logger;
            CustomCharacterRuntime.Init(this);
            CustomImageRuntime.Log = Logger;
            CustomImageRuntime.Init(this);

            bool ok = true;
            ok &= CheckTarget("LuaManager.ExecuteLuaScript",
                AccessTools.Method(typeof(LuaManager), "ExecuteLuaScript"));
            ok &= CheckTarget("SaveSystem.NewGameData",
                AccessTools.Method(typeof(SaveSystem), "NewGameData"));
            ok &= CheckTarget("FreePositionData.GetExecuteScript",
                AccessTools.Method(typeof(FreePositionData), "GetExecuteScript"));
            ok &= CheckTarget("PositionController.OnPositionClick",
                AccessTools.Method(typeof(PositionController), "OnPositionClick"));
            ok &= CheckTarget("PositionController.HasTriggerSubMissions",
                AccessTools.Method(typeof(PositionController), "HasTriggerSubMissions"));
            ok &= CheckTarget("MissionManagerData.get_MainMissionStart",
                AccessTools.Method(typeof(MissionManagerData), "get_MainMissionStart"));
            ok &= CheckTarget("MissionManagerData.UpdateCheckMissions",
                AccessTools.Method(typeof(MissionManagerData), "UpdateCheckMissions"));
            ok &= CheckTarget("MissionManagerData.HasAnyMissionTrigger",
                AccessTools.Method(typeof(MissionManagerData), "HasAnyMissionTrigger"));
            ok &= CheckTarget("PositionController._positionData",
                AccessTools.Field(typeof(PositionController), "_positionData"));
            ok &= CheckTarget("PositionController._position",
                AccessTools.Field(typeof(PositionController), "_position"));
            ok &= CheckTarget("LeanLocalization.UpdateTranslations(bool)",
                AccessTools.Method(typeof(Lean.Localization.LeanLocalization), "UpdateTranslations", new Type[] { typeof(bool) }));
            ok &= CheckTarget("StoryCharacterController.ShowMood",
                AccessTools.Method(typeof(StoryCharacterController), "ShowMood", new Type[0]));
            ok &= CheckTarget("GameOverController.Start",
                AccessTools.Method(typeof(GameOverController), "Start"));
            ok &= CheckTarget("EndGamePanel.Open",
                AccessTools.Method(typeof(EndGamePanel), "Open"));
            ok &= CheckTarget("EndGameController.Start",
                AccessTools.Method(typeof(EndGameController), "Start"));
            ok &= CheckTarget("PlayerStatManagerData.get_NewGamePlus",
                AccessTools.Method(typeof(PlayerStatManagerData), "get_NewGamePlus"));
            ok &= CheckTarget("DiceMenuDialog.CheckRevolution",
                AccessTools.Method(typeof(DiceMenuDialog), "CheckRevolution"));
            ok &= CheckTarget("CharacterIntroPanel.Show",
                AccessTools.Method(typeof(CharacterIntroPanel), "Show"));
            ok &= CheckTarget("LuaManager.PlayMusic",
                AccessTools.Method(typeof(LuaManager), "PlayMusic", new Type[] { typeof(string) }));
            ok &= CheckTarget("LuaManager.PlaySound",
                AccessTools.Method(typeof(LuaManager), "PlaySound", new Type[] { typeof(string) }));
            ok &= CheckTarget("LuaManager.PlayEnvSound",
                AccessTools.Method(typeof(LuaManager), "PlayEnvSound", new Type[] { typeof(string) }));
            ok &= CheckTarget("LuaManager.StopMusic",
                AccessTools.Method(typeof(LuaManager), "StopMusic", Type.EmptyTypes));
            ok &= CheckTarget("LuaManager.FadeOutMusic",
                AccessTools.Method(typeof(LuaManager), "FadeOutMusic", new Type[] { typeof(float) }));
            ok &= CheckTarget("LuaManager.FadeOutEnvSound",
                AccessTools.Method(typeof(LuaManager), "FadeOutEnvSound", new Type[] { typeof(float) }));
            ok &= CheckTarget("SoundManager.PlayMusic",
                AccessTools.Method(typeof(SoundManager), "PlayMusic", new Type[] { typeof(string) }));
            ok &= CheckTarget("SoundManager.StopMusic",
                AccessTools.Method(typeof(SoundManager), "StopMusic", Type.EmptyTypes));
            ok &= CheckTarget("SceneController.LoadNewScene",
                AccessTools.Method(typeof(SceneController), "LoadNewScene", new Type[] { typeof(string) }));
            if (!ok)
            {
                Logger.LogError("部分 Harmony 目标缺失（游戏版本可能已变更），战役/触发器功能可能不可用");
                return;
            }
            new Harmony(GUID).PatchAll(); // patch 本程序集全部 [HarmonyPatch] 类
            PatchSteamRestart();
            Logger.LogInfo("Harmony patch 已挂载：ExecuteLuaScript / NewGameData / Free 自动与地点剧情抑制 / GetExecuteScript / UpdateTranslations / ShowMood / CharacterIntroPanel / GameOver/EndGamePanel/EndGame / NewGamePlus / DiceRevolution / CustomAudio / SoundManager / LoadNewScene");
        }

        /// <summary>
        /// Steam 从非 steam.exe 子进程拉起时会 RestartAppIfNecessary 自杀。
        /// 前缀直接返回 false，已经挂上 BepInEx 的那局就能留在标题画面。
        /// </summary>
        private void PatchSteamRestart()
        {
            MethodInfo method = AccessTools.Method("Steamworks.SteamAPI:RestartAppIfNecessary");
            if (method == null)
            {
                Logger.LogWarning("未找到 SteamAPI.RestartAppIfNecessary，跳过防重启补丁");
                return;
            }
            new Harmony(GUID + ".steamrestart").Patch(method,
                prefix: new HarmonyMethod(typeof(Plugin), nameof(SkipSteamRestart)));
            Logger.LogInfo("已拦截 SteamAPI.RestartAppIfNecessary（非 Steam 直启也不会自杀）");
        }

        private static bool SkipSteamRestart(ref bool __result)
        {
            __result = false;
            return false;
        }

        private bool CheckTarget(string name, MemberInfo member)
        {
            if (member != null) return true;
            Logger.LogError("找不到 Harmony 目标：" + name);
            return false;
        }

        private string _previousScene = ""; // 上一帧场景名，用于检测离开 GameOver/End（契约 §6.13）

        private void Update()
        {
            UpdateOverlaySceneTracking();
            bool traceActive = RuntimeTrace.Active;
            if (traceActive && !_wasTraceActive) _showDebugger = true;
            _wasTraceActive = traceActive;
            if (traceActive && IsHotkeyDown(_debuggerHotkey.Value))
                _showDebugger = !_showDebugger;
            if (!_enabled.Value) return;

            HandlePreviewRequest();

            // F7：任意场景可切换，返回 Free 的自动任务与下一次地点点击都会生效；独立于 F8 菜单分支。
            HandleVanillaStoryHotkey();

            if (!IsHotkeyDown(_menuHotkey.Value)) return;

            // 诊断日志：热键按下时无条件记录场景名与门控结果，方便定位"热键被抢/场景不符"类反馈
            string scene = SceneController.Instance != null ? SceneController.Instance.CurrentScene : "(SceneController 未就绪)";
            bool isTitle;
            if (!IsMenuScene(out isTitle))
            {
                Logger.LogInfo("检测到菜单热键按下（当前场景：" + scene + "），当前场景不支持 mod 菜单，已忽略。");
                return;
            }
            _showMenu = !_showMenu;
            if (_showMenu) ClampWindowToScreen();
            Logger.LogInfo("检测到菜单热键按下（当前场景：" + scene + "），" + (_showMenu ? "已打开菜单。" : "已关闭菜单。"));
        }

        /// <summary>重新扫描包并原子替换注册表；编辑器热更新试玩包时使用。</summary>
        private void ReloadMods()
        {
            CustomAudioPlayer.ReleaseAll();
            CustomCharacterRuntime.ClearAll();
            CustomImageRuntime.ClearAll();
            string modsDir = Path.Combine(Paths.PluginPath, "MortalModHost", "mods");
            LoadedMods = ModLoader.ScanMods(
                modsDir,
                msg => Logger.LogInfo(msg),
                msg => Logger.LogWarning(msg));
            ModRegistry.Rebuild(LoadedMods, msg => Logger.LogWarning(msg));
            ReadTextRegistry.Rebuild(LoadedMods, msg => Logger.LogWarning(msg));
            try
            {
                ReadTextRegistry.Apply();
            }
            catch (Exception ex)
            {
                Logger.LogError("已读文本注册进 LeanLocalization 失败（mod 台词将退化为裸文本）：" + ex);
            }
            ReadTextRegistry.SelfCheck(msg => Logger.LogInfo(msg), msg => Logger.LogWarning(msg));
            foreach (var mod in LoadedMods)
                Logger.LogInfo(FormatModSummary(mod));
            Logger.LogInfo("mod 扫描完成：成功 " + LoadedMods.Count + " 个，注册脚本 " + ModRegistry.Count + " 个。");
        }

        /// <summary>
        /// 消费编辑器的单次试玩请求。Free 场景直接演出；Title 场景使用隔离存档开局；
        /// 其它演出场景暂存请求，回到安全场景后自动继续。
        /// </summary>
        private void HandlePreviewRequest()
        {
            if (Time.unscaledTime < _nextPreviewPoll) return;
            _nextPreviewPoll = Time.unscaledTime + 0.35f;
            string pluginDir = Path.Combine(Paths.PluginPath, "MortalModHost");
            string requestPath = Path.Combine(pluginDir, "preview-request.json");
            if (!File.Exists(requestPath)) return;

            PreviewRequest request;
            string error;
            if (!PreviewRequest.TryRead(requestPath, out request, out error))
            {
                Logger.LogWarning("试玩请求无效，已删除：" + error);
                TryDelete(requestPath, "无效试玩请求");
                return;
            }

            long stamp;
            try { stamp = File.GetLastWriteTimeUtc(requestPath).Ticks; }
            catch (IOException) { return; }
            if (_loadedPreviewStamp != stamp)
            {
                ReloadMods();
                _loadedPreviewStamp = stamp;
                _previewWaitingScene = "";
            }

            ModPackage target = null;
            foreach (var mod in LoadedMods)
                if (string.Equals(mod.Id, request.ModId, StringComparison.Ordinal))
                {
                    target = mod;
                    break;
                }
            if (target == null || !string.Equals(target.Entry, request.ScriptId, StringComparison.Ordinal))
            {
                Logger.LogWarning("试玩包尚未加载或入口不匹配，将继续等待：" + request.ModId + "/" + request.ScriptId);
                return;
            }

            string scene = SceneController.Instance != null ? SceneController.Instance.CurrentScene : "";
            if (scene == "Free")
            {
                Logger.LogInfo("收到编辑器试玩请求：" + request.ScriptId + "/" + request.NodeId + "（自由场景直接演出）");
                PlayMod(target);
                CompletePreviewRequest(requestPath, target);
            }
            else if (scene == "Title")
            {
                Logger.LogInfo("收到编辑器试玩请求：" + request.ScriptId + "/" + request.NodeId + "（隔离存档开局）");
                StartCampaign(target);
                CompletePreviewRequest(requestPath, target);
            }
            else if (!string.Equals(_previewWaitingScene, scene, StringComparison.Ordinal))
            {
                _previewWaitingScene = scene;
                Logger.LogInfo("试玩请求正在等待 Title/Free 安全场景，当前场景：" + (scene.Length > 0 ? scene : "未就绪"));
            }
        }

        private void CompletePreviewRequest(string requestPath, ModPackage package)
        {
            TryDelete(requestPath, "已完成试玩请求");
            // 仅删除编辑器固定临时包；Lua、文本和图片已经加载进内存，不影响本次演出。
            if (package.Id == "lom_modkit_preview"
                && string.Equals(Path.GetFileName(package.PackagePath), "__lom_modkit_preview.lommod", StringComparison.OrdinalIgnoreCase))
                TryDelete(package.PackagePath, "编辑器临时试玩包");
            _loadedPreviewStamp = -1L;
        }

        private void TryDelete(string path, string label)
        {
            try
            {
                if (File.Exists(path)) File.Delete(path);
            }
            catch (Exception ex)
            {
                Logger.LogWarning("无法删除" + label + "：" + ex.Message);
            }
        }

        /// <summary>
        /// 契约 §6.13：场景切换离开 GameOver/End 时清除 mod 死亡/结局文本覆盖。
        /// 只盯"上一帧是 GameOver/End 且现在变了"，进入这些场景（含经由 Loading 场景的过渡）不清除。
        /// </summary>
        private void UpdateOverlaySceneTracking()
        {
            if (SceneController.Instance == null) return;
            string scene = SceneController.Instance.CurrentScene;
            if (scene == _previousScene) return;
            if (_previousScene == "GameOver" || _previousScene == "End")
                ModOverlay.Clear();
            // waveOut 不跟场景走：回标题/自由/死亡/结局/Loading 时再兜一层停播。
            if (scene == "Title" || scene == "Free" || scene == "GameOver"
                || scene == "End" || scene == "Loading")
            {
                CustomAudioPlayer.StopEverything();
                CustomCharacterRuntime.ClearAll();
                CustomImageRuntime.ClearAll();
            }
            _previousScene = scene;
        }

        private void OnDestroy()
        {
            CustomAudioPlayer.ReleaseAll();
            CustomCharacterRuntime.ClearAll();
            CustomImageRuntime.ClearAll();
        }

        /// <summary>
        /// mod 菜单可用场景：Free（自由模式：演出剧情 + 开新战役）与 Title（标题画面：仅开新战役）。
        /// Story/Battle/GameOver/End 等演出场景不出菜单，避免遮挡演出。out isTitle 标识是否标题画面。
        /// </summary>
        private static bool IsMenuScene(out bool isTitle)
        {
            isTitle = false;
            if (SceneController.Instance == null) return false;
            string scene = SceneController.Instance.CurrentScene;
            if (scene == "Free") return true;
            if (scene == "Title")
            {
                isTitle = true;
                return true;
            }
            return false;
        }

        /// <summary>
        /// F7 全局临时开关：切换 <see cref="VanillaStorySwitch.Enabled"/>。
        /// 任意场景都可切换；开关不会强制中断已经开始的 Story 演出，而是在下一次
        /// Free 场景地点点击时生效。
        /// 开关状态是会话级的，不进 cfg：重载插件即复位。
        /// </summary>
        private void HandleVanillaStoryHotkey()
        {
            if (!IsHotkeyDown(_vanillaStoryHotkey.Value)) return;
            VanillaStorySwitch.Toggle();
        }

        /// <summary>
        /// 用新 InputSystem 判定快捷键。不能用 BepInEx 自带的 KeyboardShortcut.IsDown()——
        /// 它走旧 UnityEngine.Input，本游戏只启用新输入系统，运行时会抛 InvalidOperationException。
        /// 接受 KeyboardShortcut 参数：菜单（F8）与原版剧情开关（F7）共用同一判定逻辑。
        /// </summary>
        private bool IsHotkeyDown(KeyboardShortcut shortcut)
        {
            Keyboard keyboard = Keyboard.current;
            if (keyboard == null) return false;
            Key mainKey;
            if (!TryConvertKeyCode(shortcut.MainKey, out mainKey) || mainKey == Key.None)
                return false;
            if (!keyboard[mainKey].wasPressedThisFrame)
                return false;
            foreach (KeyCode modifier in shortcut.Modifiers)
            {
                Key modifierKey;
                if (!TryConvertKeyCode(modifier, out modifierKey) || !keyboard[modifierKey].isPressed)
                    return false;
            }
            return true;
        }

        /// <summary>KeyCode 与 InputSystem.Key 基本同名，仅 Control 系列命名不同（LeftControl→LeftCtrl），做个映射。</summary>
        private static bool TryConvertKeyCode(KeyCode keyCode, out Key key)
        {
            string name = keyCode.ToString();
            if (name == "LeftControl") name = "LeftCtrl";
            else if (name == "RightControl") name = "RightCtrl";
            return Enum.TryParse(name, true, out key);
        }

        private void OnGUI()
        {
            if (_enabled != null && _enabled.Value && RuntimeTrace.Active && _showDebugger)
            {
                _debugWindowRect.height = Mathf.Min(680f, Math.Max(240f, Screen.height - 40f));
                _debugWindowRect = GUI.Window(DebugWindowId, _debugWindowRect, DrawDebuggerWindow, I18n.T("debug.window", _debuggerHotkey.Value));
            }
            bool isTitle;
            if (!_enabled.Value || !IsMenuScene(out isTitle))
            {
                _showMenu = false; // 切场景/禁用时自动关窗，按钮也随之隐藏
                return;
            }
            _inTitleScene = isTitle;
            DrawEntryButton();
            if (_showMenu)
                _windowRect = GUI.Window(WindowId, _windowRect, DrawWindow, I18n.T("window", _menuHotkey.Value));
        }

        private void DrawDebuggerWindow(int id)
        {
            try
            {
                _debugScroll = GUILayout.BeginScrollView(_debugScroll);
                GUILayout.Label(I18n.T("debug.mod") + ": " + RuntimeTrace.CurrentMod);
                GUILayout.Label(I18n.T("debug.story") + ": " + RuntimeTrace.CurrentStory);
                GUILayout.Label(I18n.T("debug.node") + ": " + RuntimeTrace.CurrentNode);
                GUILayout.Label(I18n.T("debug.characters") + ": " + JoinOrNone(CustomCharacterRuntime.ActiveCharacterIds()));
                GUILayout.Label(I18n.T("debug.music") + ": " + EmptyAsNone(CustomAudioPlayer.CurrentMusic));
                GUILayout.Label(I18n.T("debug.voice") + ": " + EmptyAsNone(CustomAudioPlayer.CurrentVoice));
                DrawDebugMap(I18n.T("debug.variables"), RuntimeTrace.VariablesSnapshot());
                DrawDebugMap(I18n.T("debug.flags"), RuntimeTrace.FlagsSnapshot());
                GUILayout.Label(I18n.T("debug.trace"));
                List<RuntimeTrace.Entry> entries = RuntimeTrace.Snapshot();
                int start = Math.Max(0, entries.Count - 24);
                for (int i = start; i < entries.Count; i++)
                {
                    RuntimeTrace.Entry item = entries[i];
                    GUILayout.Label("#" + item.Sequence + " " + item.EventType + "  "
                        + EmptyAsNone(item.NodeId) + (string.IsNullOrEmpty(item.Detail) ? "" : "  " + item.Detail));
                }
                GUILayout.EndScrollView();
                if (GUILayout.Button(I18n.T("debug.hide"))) _showDebugger = false;
            }
            catch (Exception ex)
            {
                GUILayout.Label(I18n.T("debug.draw_error") + ": " + ex.Message);
            }
            GUI.DragWindow(new Rect(0f, 0f, _debugWindowRect.width, 20f));
        }

        private static void DrawDebugMap(string title, Dictionary<string, string> values)
        {
            GUILayout.Label(title + " (" + values.Count + ")");
            if (values.Count == 0) { GUILayout.Label("  " + I18n.T("debug.none")); return; }
            int shown = 0;
            foreach (var pair in values)
            {
                GUILayout.Label("  " + pair.Key + " = " + pair.Value);
                if (++shown >= 50) { GUILayout.Label("  …"); break; }
            }
        }

        private static string JoinOrNone(List<string> values)
        {
            return values == null || values.Count == 0 ? I18n.T("debug.none") : string.Join(", ", values.ToArray());
        }

        private static string EmptyAsNone(string value)
        {
            return string.IsNullOrEmpty(value) ? I18n.T("debug.none") : value;
        }

        /// <summary>Free 场景左下角常驻小按钮：不依赖热键的菜单入口；菜单打开时隐藏，避免重复。</summary>
        private void DrawEntryButton()
        {
            if (_showMenu) return;
            Color oldColor = GUI.color;
            GUI.color = new Color(1f, 1f, 1f, 0.8f); // 半透明，尽量不显眼
            if (GUI.Button(new Rect(10f, Screen.height - 38f, 100f, 28f), I18n.T("entry")))
            {
                _showMenu = true;
                ClampWindowToScreen();
                Logger.LogInfo("通过左下角按钮打开 mod 菜单。");
            }
            GUI.color = oldColor;
        }

        private void DrawWindow(int id)
        {
            GUILayout.BeginVertical();
            if (LoadedMods.Count == 0)
            {
                GUILayout.Label(I18n.T("empty"));
            }
            else if (_inTitleScene)
            {
                // 标题画面：尚无已加载的存档，演出剧情会缺玩家状态；只提供"开始新战役"（独立开局）
                _scroll = GUILayout.BeginScrollView(_scroll);
                GUILayout.Label(I18n.T("section.campaign"));
                DrawCampaignSection();
                GUILayout.EndScrollView();
                GUILayout.Label(I18n.T("title.hint"));
            }
            else
            {
                _scroll = GUILayout.BeginScrollView(_scroll);
                GUILayout.Label(I18n.T("section.play"));
                foreach (var mod in LoadedMods)
                    DrawModEntry(mod);
                GUILayout.Label(I18n.T("section.campaign"));
                DrawCampaignSection();
                GUILayout.EndScrollView();
            }
            GUILayout.Space(6f);
            if (GUILayout.Button(I18n.T("close")))
                _showMenu = false;
            GUILayout.EndVertical();
            GUI.DragWindow(new Rect(0f, 0f, _windowRect.width, 20f)); // 仅标题栏可拖动，避免与关闭按钮/滚动区抢点击
        }

        /// <summary>"开始新战役"区（契约 §6.3）：列出 manifest.campaign.new_game=true 的 mod。</summary>
        private void DrawCampaignSection()
        {
            bool any = false;
            foreach (var mod in LoadedMods)
            {
                if (mod.Campaign == null || !mod.Campaign.NewGame) continue;
                any = true;
                GUILayout.BeginVertical("box");
                GUILayout.Label(mod.Name + "  v" + mod.Version + "  by " + mod.Author);
                if (!string.IsNullOrEmpty(mod.Description))
                    GUILayout.Label(mod.Description);
                if (GUILayout.Button(I18n.T("campaign.start")))
                    StartCampaign(mod);
                GUILayout.EndVertical();
            }
            if (!any)
                GUILayout.Label(I18n.T("campaign.none"));
        }

        /// <summary>
        /// 开始新战役（契约 §6.4）：隔离存档槽 SetSlot("mod_&lt;modid&gt;") → 官方 NewGameData()
        /// （NewGameDataPatch postfix 把首脚本替换为本 mod 入口）→ LoadStory。
        /// 同时记录 ModCampaignState（该 mod 的 campaign.disable_official_events），战役期间
        /// Free 场景自动任务与位置点击的官方脚本由对应 patch 据其抑制；官方开局时由
        /// NewGameDataPatch 清除。
        /// 等价于 TitleManager.NewGame() 的调用序列（Mortal.Core.decompiled.cs:8377）。
        /// Free 自由场景与 Title 标题画面均可调用（SaveSystem/SceneController 是常驻单例）；
        /// 标题画面直接可开新战役，无需先进自由模式。
        /// </summary>
        private void StartCampaign(ModPackage mod)
        {
            SaveSystem saves = SaveSystem.Instance;
            SceneController scenes = SceneController.Instance;
            if (saves == null || scenes == null)
            {
                Logger.LogError("无法开始新战役：SaveSystem/SceneController 单例尚未就绪");
                return;
            }
            string slot = "mod_" + mod.Id;
            Logger.LogInfo("开始新战役：" + mod.Id + "（隔离存档槽 " + slot + "）");
            NewGameDataPatch.PendingCampaign = mod;
            // 契约 §2：记录 mod 战役运行态——该战役期间 Free 自动任务与位置点击是否禁用原版事件
            // 由本 mod 的 disable_official_events 决定（官方开局时 NewGameDataPatch 清除）；
            // 同时记录战役 mod id，位置触发器按当前战役 mod 隔离（见 FreePositionPatch）。
            ModCampaignState.Enter(mod);
            if (mod.Campaign.DisableOfficialEvents)
                Logger.LogInfo("该战役已声明 disable_official_events：返回 Free 的自动任务与位置点击不再触发原版故事脚本（仅 mod 触发器命中）。");
            saves.SetSlot(slot);
            saves.NewGameData();
            scenes.LoadStory();
            _showMenu = false;
        }

        private void DrawModEntry(ModPackage mod)
        {
            GUILayout.BeginVertical("box");
            GUILayout.Label(mod.Name + "  v" + mod.Version + "  by " + mod.Author);
            if (!string.IsNullOrEmpty(mod.Description))
                GUILayout.Label(mod.Description);
            GUILayout.Label(I18n.T("entry.scripts", mod.Entry, mod.LuaScripts.Count));
            if (GUILayout.Button(I18n.T("play")))
                PlayMod(mod);
            GUILayout.EndVertical();
        }

        /// <summary>演出入口脚本：写入 CurrentStoryScript（注册名）后切 Story 场景，由 Harmony prefix 接管实际加载。</summary>
        private void PlayMod(ModPackage mod)
        {
            PlayerStatManagerData stat = PlayerStatManagerData.Instance;
            SceneController scenes = SceneController.Instance;
            if (stat == null || scenes == null)
            {
                Logger.LogError("无法演出 mod：PlayerStatManagerData/SceneController 单例尚未就绪");
                return;
            }
            string registeredName = mod.GetRegisteredScriptName(mod.Entry);
            Logger.LogInfo("演出 mod 剧情：" + registeredName);
            stat.SetStoryScript(registeredName);
            scenes.LoadStory();
            _showMenu = false;
        }

        private void ClampWindowToScreen()
        {
            _windowRect.x = Mathf.Clamp(_windowRect.x, 0f, Math.Max(0f, Screen.width - _windowRect.width));
            _windowRect.y = Mathf.Clamp(_windowRect.y, 0f, Math.Max(0f, Screen.height - _windowRect.height));
        }

        /// <summary>把单个 mod 的元信息和脚本清单拼成多行日志文本。</summary>
        private static string FormatModSummary(ModPackage mod)
        {
            var sb = new StringBuilder();
            sb.Append("已加载 mod：").Append(mod.Id)
              .Append("\n  名称：").Append(mod.Name)
              .Append("\n  版本：").Append(mod.Version)
              .Append("\n  作者：").Append(mod.Author)
              .Append("\n  简介：").Append(mod.Description)
              .Append("\n  入口脚本：").Append(mod.Entry)
              .Append("（注册名 ").Append(mod.GetRegisteredScriptName(mod.Entry)).Append("）")
              .Append("\n  Lua 脚本（").Append(mod.LuaScripts.Count).Append(" 个）：");

            var ids = new List<string>(mod.LuaScripts.Keys);
            ids.Sort(StringComparer.Ordinal);
            foreach (var scriptId in ids)
            {
                sb.Append("\n    - ").Append(scriptId)
                  .Append("（").Append(mod.LuaScripts[scriptId].Length).Append(" 字符）");
            }
            return sb.ToString();
        }
    }
}
