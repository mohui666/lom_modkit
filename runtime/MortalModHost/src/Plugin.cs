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
using Mortal.Combat;
using BattleGameLevelManager = Mortal.Battle.GameLevelManager;
using BattleGameOverType = Mortal.Battle.GameOverType;
using Mortal.Core;
using Mortal.Free;
using Mortal.Story;
using UnityEngine;
using UnityEngine.InputSystem;

namespace MortalModHost
{
    /// <summary>
    /// 活侠传 mod 宿主插件入口：发现 .lommod 包 → 解析 → Harmony 注入 LuaManager → Free 场景内 IMGUI 菜单演出。
    /// 运行行为契约见 docs/chs/mod_format.md §6。
    /// </summary>
    [BepInPlugin(GUID, NAME, VERSION)]
    public class Plugin : BaseUnityPlugin
    {
        public const string GUID = "com.mohui666.mortalmodhost";
        public const string NAME = "MortalModHost";
        public const string VERSION = "1.0.0";

        private const int WindowId = 886310; // IMGUI 窗口 id，取个不易与其他插件撞车的数
        private const int DebugWindowId = 886311;

        /// <summary>本轮解析到的全部 mod 包（patch 与菜单共用）。</summary>
        internal static List<ModPackage> LoadedMods { get; private set; }

        private ConfigEntry<bool> _enabled;
        private ConfigEntry<KeyboardShortcut> _menuHotkey;
        private ConfigEntry<KeyboardShortcut> _vanillaStoryHotkey;
        private ConfigEntry<KeyboardShortcut> _debuggerHotkey;
        private ConfigEntry<bool> _hotkeyMigrationCompleted;
        private bool _harmonyPatched;
        private bool _runtimeReady;
        private bool _disablePendingForDisclosure;
        private bool _disclosureAbortRequested;
        private bool _applicationQuitting;
        private bool _originalRunInBackground;
        private bool _runInBackgroundOverridden;

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
                "总开关。false 时禁用热键、mod 菜单与 LuaManager 注入；若 mod 剧情正在演出，为保持强制披露，会延迟到回到官方枢纽后卸载补丁。");
            _menuHotkey = Config.Bind("General", "MenuHotkey", new KeyboardShortcut(KeyCode.F8),
                "打开/关闭 mod 菜单的快捷键（Free 自由场景与 Title 标题画面生效，契约 §6.3）。旧默认 F9 与同机 MortalInstantWin 冲突，启动时自动迁移为 F8。");
            _vanillaStoryHotkey = Config.Bind("General", "VanillaStoryHotkey", new KeyboardShortcut(KeyCode.F7),
                "切换「禁用原版游戏剧情」的全局临时开关（任意场景可切换；会话级，不持久化）。开启后跳过返回 Free 时自动触发及地点点击触发的官方主线、支线和默认脚本，mod 触发器仍优先。");
            _debuggerHotkey = Config.Bind("Development", "DebuggerHotkey", new KeyboardShortcut(KeyCode.F10),
                "仅编辑器 F5 开发包生效：显示/隐藏 Runtime Debugger。正式 Mod 不启用调试器。");
            _hotkeyMigrationCompleted = Config.Bind("Migration", "MenuHotkeyF9ToF8Completed", false,
                "内部一次性迁移标记。设为 true 后 Runtime 不会再自动改写 MenuHotkey。除非排查旧配置迁移，请勿修改。");
            MigrateLegacyHotkey();
            Logger.LogInfo("菜单热键：" + _menuHotkey.Value + "（Title 使用原版风格战役入口，Free 左下角保留常驻入口）；原版剧情开关热键：" + _vanillaStoryHotkey.Value);

            // 契约 §6.13：死亡/结局文本覆盖的静态初始态（重复启动时防止残留上次会话的文本）
            ModOverlay.Clear();
            // 契约 §2：mod 战役运行态同样重置（插件重载后不残留旧战役的禁原版事件状态）
            ModCampaignState.Clear();
            GameplaySession.Reset();
            ModQuestSession.Reset();
            PersistentModState.Log = Logger;
            PersistentModState.Initialize(Path.Combine(
                Paths.ConfigPath, "MortalModHost", "campaign_state"));
            ModSaveIsolation.Log = Logger;
            ModSaveIsolation.Initialize(SaveSystem.Instance);

            // mods 目录：BepInEx/plugins/MortalModHost/mods/（契约 §6.1）
            string modsDir = Path.Combine(Paths.PluginPath, "MortalModHost", "mods");
            Logger.LogInfo("MortalModHost " + VERSION + " 启动；游戏版本 "
                + (string.IsNullOrEmpty(Application.version) ? "<unknown>" : Application.version)
                + "；扫描 mods 目录：" + modsDir);

            ReloadMods();

            if (_enabled.Value)
            {
                EnableBackgroundExecution();
                ApplyHarmonyPatch();
            }
            else
                Logger.LogInfo("MortalModHost 已禁用：未挂载 Harmony 补丁");
            _enabled.SettingChanged += OnEnabledChanged;
        }

        private void OnEnabledChanged(object sender, EventArgs args)
        {
            if (_enabled.Value)
            {
                _disablePendingForDisclosure = false;
                EnableBackgroundExecution();
                ApplyHarmonyPatch();
                if (_runtimeReady)
                    Logger.LogInfo("MortalModHost 已启用");
                else
                    Logger.LogError("MortalModHost 无法安全挂载运行时，MOD 演出入口保持关闭");
                return;
            }

            _showMenu = false;
            if (ModDisclosurePolicy.ShouldDeferHostDisable(ModDisclosure.Active))
            {
                // 已开演的 mod 协程不会因 Unpatch 自动停止。此时强拆补丁/台标会给出
                // 一键隐藏「非官方剧情」披露的路径，因此延迟到官方 Title/Free 再完成禁用。
                _disablePendingForDisclosure = true;
                Logger.LogWarning("当前 mod 剧情仍在演出：已关闭菜单/热键，将在回到 Title/Free 后卸载补丁；非官方剧情披露会保持。");
                return;
            }

            CompleteDisable();
        }

        private void CompleteDisable()
        {
            _disablePendingForDisclosure = false;
            VanillaModCampaignPanel.Remove();
            VanillaTitleModEntry.Remove();
            RemoveHarmonyPatches();
            LuaManagerPatch.CleanupRuntimeState();
            RestoreBackgroundExecution();
            _showMenu = false;
            ModCampaignState.Clear();
            GameplaySession.Reset();
            ModQuestSession.Reset();
            PersistentModState.ResetMemory();
            ModOverlay.Clear();
            ModDisclosure.Disable();
            CustomAudioPlayer.ReleaseAll();
            CustomCharacterRuntime.ClearAll();
            CustomImageRuntime.ClearAll();
            Logger.LogInfo("MortalModHost 已禁用：Harmony 补丁及运行态效果已清除");
        }

        /// <summary>
        /// 旧版本默认热键是 F9，与同机 MortalInstantWin 冲突；现默认改 F8。
        /// 已有 cfg 里 MenuHotkey 若仍是 F9（旧默认残留），首次改写为 F8 并记录完成标记。
        /// 标记落盘后永不重复迁移，因此用户之后主动设置 F9 不会在下次启动被覆盖。
        /// </summary>
        private void MigrateLegacyHotkey()
        {
            if (_hotkeyMigrationCompleted.Value) return;
            try
            {
                string path = Config.ConfigFilePath;
                string migrated;
                bool markCompleted;
                bool changed = HotkeyMigration.TryRewriteLegacyHotkeyOnce(
                    File.Exists(path) ? File.ReadAllText(path) : "",
                    _hotkeyMigrationCompleted.Value,
                    out migrated,
                    out markCompleted);
                if (changed)
                {
                    File.WriteAllText(path, migrated);
                    Config.Reload();
                    Logger.LogInfo("检测到旧默认热键 F9 残留（与 MortalInstantWin 冲突），已一次性迁移为 F8。");
                }
                else if (_menuHotkey.Value.MainKey == KeyCode.F9 && !HasModifiers(_menuHotkey.Value))
                {
                    // cfg 文件缺失或该行未落盘：直接改当前值（BepInEx 会自动保存）
                    _menuHotkey.Value = new KeyboardShortcut(KeyCode.F8);
                    Logger.LogInfo("检测到旧默认热键 F9，已迁移为 F8。");
                }
                if (markCompleted)
                {
                    _hotkeyMigrationCompleted.Value = true;
                    Config.Save();
                }
            }
            catch (Exception ex)
            {
                Logger.LogWarning("热键 F9→F8 迁移失败：" + ex.Message);
            }
        }

        private void EnableBackgroundExecution()
        {
            if (_runInBackgroundOverridden) return;
            _originalRunInBackground = Application.runInBackground;
            Application.runInBackground = true;
            _runInBackgroundOverridden = true;
            Logger.LogInfo("已开启 runInBackground（失焦时 Update 仍跑）");
        }

        private void RestoreBackgroundExecution()
        {
            if (!_runInBackgroundOverridden) return;
            Application.runInBackground = _originalRunInBackground;
            _runInBackgroundOverridden = false;
            Logger.LogInfo("已恢复 runInBackground 原值：" + _originalRunInBackground);
        }

        private static bool HasModifiers(KeyboardShortcut shortcut)
        {
            foreach (KeyCode modifier in shortcut.Modifiers) return true;
            return false;
        }

        /// <summary>挂 Harmony patch。目标方法/字段找不到时明确报错而不是静默失效。</summary>
        private void ApplyHarmonyPatch()
        {
            if (_harmonyPatched) return;
            _runtimeReady = false;
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
            ModDisclosure.Log = Logger;

            bool ok = true;
            ok &= CheckTarget("LuaManager.ExecuteLuaScript",
                AccessTools.Method(typeof(LuaManager), "ExecuteLuaScript"));
            ok &= CheckTarget("SaveSystem.NewGameData",
                AccessTools.Method(typeof(SaveSystem), "NewGameData"));
            ok &= CheckTarget("SaveSystem.SetSlot",
                AccessTools.Method(typeof(SaveSystem), "SetSlot", new Type[] { typeof(string) }));
            ok &= CheckTarget("SaveSystem.SaveGameData",
                PersistentStateSavePatch.TargetMethod());
            ok &= CheckTarget("SaveSystem.AutoSaveStoryData",
                AccessTools.Method(typeof(SaveSystem), "AutoSaveStoryData", Type.EmptyTypes));
            ok &= CheckTarget("SaveSystem.AutoSaveFreeData",
                AccessTools.Method(typeof(SaveSystem), "AutoSaveFreeData", Type.EmptyTypes));
            ok &= CheckTarget("SaveSystem.AutoSaveBattleData",
                AccessTools.Method(typeof(SaveSystem), "AutoSaveBattleData", Type.EmptyTypes));
            ok &= CheckTarget("SaveSystem.AutoSaveData(string)",
                AccessTools.Method(typeof(SaveSystem), "AutoSaveData", new Type[] { typeof(string) }));
            ok &= CheckTarget("SaveSystem.AutoLoadGameData(string)",
                AccessTools.Method(typeof(SaveSystem), "AutoLoadGameData", new Type[] { typeof(string) }));
            ok &= CheckTarget("SaveSystem.SaveUniverseData",
                AccessTools.Method(typeof(SaveSystem), "SaveUniverseData", Type.EmptyTypes));
            ok &= CheckTarget("SaveSystem._currentSlot",
                AccessTools.Field(typeof(SaveSystem), "_currentSlot"));
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
            ok &= CheckTarget("CheckPointManager.Dice(string,int)",
                AccessTools.Method(typeof(CheckPointManager), "Dice",
                    new Type[] { typeof(string), typeof(int) }));
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
            ok &= CheckTarget("CombatManager.GameOver(bool)",
                AccessTools.Method(typeof(CombatManager), "GameOver", new Type[] { typeof(bool) }));
            ok &= CheckTarget("CombatLevel.get_DeadEnd",
                AccessTools.Method(typeof(CombatLevel), "get_DeadEnd", Type.EmptyTypes));
            ok &= CheckTarget("GameLevelManager.ShowGameOver(GameOverType,bool)",
                AccessTools.Method(typeof(BattleGameLevelManager), "ShowGameOver",
                    new Type[] { typeof(BattleGameOverType), typeof(bool) }));
            if (!ok)
            {
                Logger.LogError("部分 Harmony 目标缺失（游戏版本可能已变更）：为保证玩家内容披露，已禁用全部 MOD 演出入口");
                return;
            }
            try
            {
                new Harmony(GUID).PatchAll(); // patch 本程序集全部 [HarmonyPatch] 类
                PatchSteamRestart();
                _harmonyPatched = true;
                _runtimeReady = true;
                Logger.LogInfo("Harmony patch 已挂载：ExecuteLuaScript / NewGameData / MOD 手动、自动、universe 存档隔离 / Free 自动与地点剧情抑制 / GetExecuteScript / UpdateTranslations / ShowMood / CharacterIntroPanel / GameOver/EndGamePanel/EndGame / NewGamePlus / DiceRevolution / CustomAudio / SoundManager / LoadNewScene / Combat/Battle 结果回流");
            }
            catch (Exception ex)
            {
                new Harmony(GUID).UnpatchSelf();
                new Harmony(GUID + ".steamrestart").UnpatchSelf();
                _harmonyPatched = false;
                _runtimeReady = false;
                Logger.LogError("Harmony 挂载失败：为保证玩家内容披露，已回滚补丁并关闭全部 MOD 演出入口。" + ex);
            }
        }

        private void RemoveHarmonyPatches()
        {
            if (_harmonyPatched)
            {
                new Harmony(GUID).UnpatchSelf();
                new Harmony(GUID + ".steamrestart").UnpatchSelf();
            }
            _harmonyPatched = false;
            _runtimeReady = false;
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
            if (LuaManagerPatch.HasPendingAbort)
                LuaManagerPatch.RetryPendingAbort();
            if (_disablePendingForDisclosure)
            {
                UpdateOverlaySceneTracking();
                MaintainModDisclosure();
                if (!ModDisclosure.Active)
                    CompleteDisable();
                return;
            }
            if (!_enabled.Value || !_runtimeReady) return;
            UpdateOverlaySceneTracking();
            bool traceActive = RuntimeTrace.Active;
            if (traceActive && !_wasTraceActive) _showDebugger = true;
            _wasTraceActive = traceActive;
            if (traceActive && IsHotkeyDown(_debuggerHotkey.Value))
                _showDebugger = !_showDebugger;
            MaintainModDisclosure();
            MaintainVanillaTitleEntry();
            VanillaModCampaignPanel.Maintain();

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

        private void MaintainModDisclosure()
        {
            if (!ModDisclosure.Active)
            {
                _disclosureAbortRequested = false;
                return;
            }
            if (ModDisclosure.Tick()) return;
            if (_disclosureAbortRequested) return;
            _disclosureAbortRequested = LuaManagerPatch.AbortActivePlayback(
                "强制玩家内容披露无法维持：" + (ModDisclosure.FailureReason ?? "未知错误"),
                null, null, "mandatory_disclosure");
        }

        private void LateUpdate()
        {
            // Fungus/MoonSharp 协程通常在 Update 阶段推进；LateUpdate 再校验一次，
            // 缩短第三方脚本或组件在同帧改动披露 UI 后的可见窗口。
            if (ModDisclosure.Active)
                MaintainModDisclosure();
        }

        /// <summary>重新扫描包并原子替换注册表；编辑器热更新试玩包时使用。</summary>
        private void ReloadMods()
        {
            CustomAudioPlayer.ReleaseAll();
            CustomCharacterRuntime.ClearAll();
            CustomImageRuntime.ClearAll();
            string modsDir = Path.Combine(Paths.PluginPath, "MortalModHost", "mods");
            List<ModPackage> scannedMods = ModLoader.ScanMods(
                modsDir,
                msg => Logger.LogInfo(msg),
                msg => Logger.LogWarning(msg));
            var compatibleMods = new List<ModPackage>();
            foreach (ModPackage package in scannedMods)
            {
                CompatibilityResult compatibility = RuntimeCompatibility.Evaluate(
                    package, VERSION, Application.version);
                foreach (string warning in compatibility.Warnings)
                    Logger.LogWarning("mod " + package.Id + " 兼容性提示：" + warning);
                if (!compatibility.IsCompatible)
                {
                    Logger.LogError("拒绝加载 mod " + package.Id + "："
                        + compatibility.Error);
                    continue;
                }
                compatibleMods.Add(package);
            }
            scannedMods = compatibleMods;
            ModRegistry.Rebuild(scannedMods, msg => Logger.LogWarning(msg));
            LoadedMods = scannedMods.FindAll(ModRegistry.IsPackageFullyRegistered);
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
                bool hotReload = ModDisclosurePolicy.CanHotReloadDevelopmentPreview(
                    RuntimeTrace.Active,
                    ModDisclosure.Active,
                    ModDisclosure.ModId,
                    request.ModId);
                if (hotReload)
                {
                    Logger.LogInfo("收到新的 F5 试玩包，清理当前开发演出并从节点 " + request.NodeId + " 重启。");
                    try
                    {
                        ModDisclosure.PrepareDevelopmentHotReload();
                        RuntimeTrace.PrepareHotReload(request.NodeId);
                        CleanupDevelopmentPlayback();
                    }
                    catch (Exception ex)
                    {
                        LuaManagerPatch.AbortActivePlayback(
                            "F5 热重载清理失败", null, ex, "hot_reload");
                        return;
                    }
                }
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
            bool reloadCurrentPreview = ModDisclosurePolicy.CanHotReloadDevelopmentPreview(
                    RuntimeTrace.Active,
                    ModDisclosure.Active,
                    ModDisclosure.ModId,
                    target.Id)
                && RuntimeTrace.IsDevelopmentPackage(target)
                && scene == "Story";
            if (reloadCurrentPreview && SceneController.Instance != null
                && (SceneController.Instance.IsPrepare || SceneController.Instance.IsLoading))
            {
                return;
            }
            if (reloadCurrentPreview)
            {
                Logger.LogInfo("F5 热重载：从节点 " + request.NodeId + " 重新载入 Story 场景");
                PlayMod(target);
                CompletePreviewRequest(requestPath, target);
            }
            else if (scene == "Free")
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
            // 可见披露：Title / Free 是官方枢纽，强制关；Loading / GameOver / End 保持，
            // 否则进死亡/结局卡时标已经没了。
            if (!ModDisclosurePolicy.ShouldKeepOnScene(scene))
            {
                ModDisclosure.Disable();
                _disclosureAbortRequested = false;
                LuaManagerPatch.ResetAbortGuard();
                GameplaySession.Reset();
                if (scene == "Title")
                {
                    RestoreOfficialSlotAtTitle();
                    ModCampaignState.Clear();
                    ModQuestSession.Reset();
                    PersistentModState.ResetMemory();
                }
            }
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

        private void RestoreOfficialSlotAtTitle()
        {
            SaveSystem saves = SaveSystem.Instance;
            if (saves == null || !ModSaveIsolation.IsModSlot(saves.CurrentSlot)) return;
            if (string.IsNullOrEmpty(ModSaveIsolation.LastOfficialSlot))
                ModSaveIsolation.Initialize(saves);
            string official = ModSaveIsolation.LastOfficialSlot;
            if (string.IsNullOrEmpty(official) || ModSaveIsolation.IsModSlot(official))
            {
                Logger.LogWarning(
                    "已回到标题，但没有可确认的原版槽；保持 Universe 文件不变并拒绝把 MOD 槽写入其中");
                return;
            }
            try
            {
                saves.SetSlot(official);
                Logger.LogInfo("回到标题：CurrentSlot 已从 MOD 隔离槽恢复为原版槽 " + official);
            }
            catch (Exception ex)
            {
                Logger.LogError("回到标题时恢复原版槽失败；Universe 保护仍保持：" + ex);
            }
        }

        private void OnDestroy()
        {
            if (_enabled != null)
                _enabled.SettingChanged -= OnEnabledChanged;
            if (ModDisclosure.Active && !_applicationQuitting)
            {
                LuaManagerPatch.AbortActivePlayback(
                    "MortalModHost 正在卸载，已终止活动中的 MOD 演出",
                    null, null, "host_unload");
                // 恶意 Lua 可通过 UnityEngine.Object.Destroy 销毁 BepInEx 宿主。此时不得
                // 连带撤下披露：独立 guardian / Canvas 回调会继续自愈、遮罩和重试
                // 回 Free，只有真正到达 Title / Free 可信边界才会清除。
            }
            VanillaModCampaignPanel.Remove();
            VanillaTitleModEntry.Remove();
            RemoveHarmonyPatches();
            LuaManagerPatch.CleanupRuntimeState();
            RestoreBackgroundExecution();
            CustomAudioPlayer.ReleaseAll();
            CustomCharacterRuntime.ClearAll();
            CustomImageRuntime.ClearAll();
            if (_applicationQuitting || !ModDisclosure.Active)
                ModDisclosure.Disable();
        }

        private void OnApplicationQuit()
        {
            _applicationQuitting = true;
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

        private void MaintainVanillaTitleEntry()
        {
            bool isTitle;
            bool shouldExist = _enabled.Value && _runtimeReady
                && IsMenuScene(out isTitle) && isTitle;
            VanillaTitleModEntry.Maintain(
                shouldExist,
                OpenMenuFromVanillaTitle,
                msg => Logger.LogInfo(msg),
                msg => Logger.LogWarning(msg));
        }

        private void OpenMenuFromVanillaTitle()
        {
            bool isTitle;
            if (!_enabled.Value || !_runtimeReady || !IsMenuScene(out isTitle) || !isTitle)
                return;
            _inTitleScene = true;
            _showMenu = !VanillaModCampaignPanel.Open(
                LoadedMods,
                StartCampaign,
                LoadCampaign,
                msg => Logger.LogInfo(msg),
                msg => Logger.LogWarning(msg));
            if (_showMenu) ClampWindowToScreen();
            Logger.LogInfo(_showMenu
                ? "原版战役存档页不可用，已打开兼容 MOD 菜单。"
                : "通过标题画面入口打开原版风格 MOD 战役存档页。");
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
            if (DrawDisclosureFailureGuard())
            {
                _showMenu = false;
                return;
            }
            if (_enabled != null && _enabled.Value && RuntimeTrace.Active && _showDebugger)
            {
                _debugWindowRect.height = Mathf.Min(680f, Math.Max(240f, Screen.height - 40f));
                _debugWindowRect = GUI.Window(DebugWindowId, _debugWindowRect, DrawDebuggerWindow, I18n.T("debug.window", _debuggerHotkey.Value));
            }
            bool isTitle;
            if (!_enabled.Value || !_runtimeReady || !IsMenuScene(out isTitle))
            {
                _showMenu = false; // 切场景/禁用时自动关窗，按钮也随之隐藏
                return;
            }
            _inTitleScene = isTitle;
            // 标题画面已有克隆自原版“开始游戏”的入口；注入失败时仍显示旧入口作为兼容回退。
            if (!_inTitleScene || !VanillaTitleModEntry.IsVisible)
                DrawEntryButton();
            if (_showMenu)
                _windowRect = GUI.Window(WindowId, _windowRect, DrawWindow, I18n.T("window", _menuHotkey.Value));
        }

        private void DrawDebuggerWindow(int id)
        {
            try
            {
                GUILayout.BeginHorizontal();
                string state = RuntimeDebugControl.Paused ? I18n.T("debug.state.paused")
                    : RuntimeDebugControl.PausePending ? I18n.T("debug.state.pending") : I18n.T("debug.state.running");
                GUILayout.Label(I18n.T("debug.state") + ": " + state);
                if (GUILayout.Button(I18n.T("debug.pause"))) RuntimeDebugControl.PauseBeforeNextNode();
                bool oldEnabled = GUI.enabled;
                GUI.enabled = RuntimeDebugControl.Paused;
                if (GUILayout.Button(I18n.T("debug.step"))) RuntimeDebugControl.Step();
                GUI.enabled = oldEnabled;
                if (GUILayout.Button(I18n.T("debug.continue"))) RuntimeDebugControl.Continue();
                GUILayout.EndHorizontal();
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

        /// <summary>
        /// Clears every package-owned runtime surface before replacing the F5 package.
        /// The Story scene reload removes official dialog UI; this method handles all
        /// host-owned resources immediately so no old coroutine or asset survives a frame.
        /// </summary>
        private static void CleanupDevelopmentPlayback()
        {
            LuaManagerPatch.StopActiveDevelopmentPlayback();
            CharacterIntroSupport.Clear();
            ModOverlay.Clear();
            CustomAudioPlayer.ReleaseAll();
            CustomCharacterRuntime.ClearAll();
            CustomImageRuntime.ClearAll();
            MoodControl.Disabled = false;
        }

        private static string JoinOrNone(List<string> values)
        {
            return values == null || values.Count == 0 ? I18n.T("debug.none") : string.Join(", ", values.ToArray());
        }

        private static string EmptyAsNone(string value)
        {
            return string.IsNullOrEmpty(value) ? I18n.T("debug.none") : value;
        }

        /// <summary>
        /// Canvas 披露本身故障时的独立安全层。IMGUI 不依赖被破坏的 Canvas/Text 对象；
        /// 在 StopAllCoroutines→LoadFree 的过渡期间全屏遮住任何可能继续渲染的 MOD 内容。
        /// </summary>
        private bool DrawDisclosureFailureGuard()
        {
            return ModDisclosure.DrawFailureGuard();
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
                GUILayout.Label(FormatModHeading(mod));
                string description = ModDisclosurePolicy.SafePackageDescription(mod);
                if (!string.IsNullOrEmpty(description))
                    GUILayout.Label(description);
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
            if (!_runtimeReady)
            {
                Logger.LogError("运行时未安全就绪：已拒绝开始 MOD 战役");
                return;
            }
            SaveSystem saves = SaveSystem.Instance;
            SceneController scenes = SceneController.Instance;
            if (saves == null || scenes == null)
            {
                Logger.LogError("无法开始新战役：SaveSystem/SceneController 单例尚未就绪");
                return;
            }
            if (scenes.IsPrepare || scenes.IsLoading)
            {
                Logger.LogError("无法开始新战役：场景正在切换，请返回标题后重试");
                return;
            }
            string slot = "mod_" + mod.Id;
            string previousOfficialSlot = ModSaveIsolation.IsModSlot(saves.CurrentSlot)
                ? ModSaveIsolation.LastOfficialSlot : saves.CurrentSlot;
            Logger.LogInfo("开始新战役：" + mod.Id + "（隔离存档槽 " + slot + "）");
            ModSaveIsolation.BeforeEnterModSlot(saves.CurrentSlot);
            try
            {
                saves.SetSlot(slot);
                // SetSlot postfix 会丢弃上一槽的内存快照；新战役随后以空 sidecar 开局。
                // NewGameData 内的原版 SaveGameData 会通过 postfix 原子写出这份空状态。
                PersistentModState.BeginNewCampaign(mod);
                NewGameDataPatch.CampaignFailure = null;
                NewGameDataPatch.PendingCampaign = mod;
                // 契约 §2：记录 mod 战役运行态——该战役期间 Free 自动任务与位置点击是否禁用原版事件
                // 由本 mod 的 disable_official_events 决定（官方开局时 NewGameDataPatch 清除）；
                // 同时记录战役 mod id，位置触发器按当前战役 mod 隔离（见 FreePositionPatch）。
                ModCampaignState.Enter(mod);
                ModQuestSession.Reset();
                if (mod.Campaign.DisableOfficialEvents)
                    Logger.LogInfo("该战役已声明 disable_official_events：返回 Free 的自动任务与位置点击不再触发原版故事脚本（仅 mod 触发器命中）。");
                saves.NewGameData();
                if (!string.IsNullOrEmpty(NewGameDataPatch.CampaignFailure))
                    throw new InvalidOperationException(
                        "MOD 首脚本或隔离存档写入失败：" + NewGameDataPatch.CampaignFailure);
                scenes.LoadStory();
                _showMenu = false;
            }
            catch (Exception ex)
            {
                NewGameDataPatch.PendingCampaign = null;
                NewGameDataPatch.CampaignFailure = null;
                ModCampaignState.Clear();
                ModQuestSession.Reset();
                PersistentModState.ResetMemory();
                RestoreOfficialSlotAfterCampaignFailure(saves, previousOfficialSlot);
                Logger.LogError("MOD 新战役初始化失败；已回滚到进入前的原版槽：" + ex);
                return;
            }
        }

        /// <summary>
        /// 从 mod_&lt;id&gt; 隔离槽继续已有战役。读取顺序与原版
        /// LoadSlotPanel.OnTitleClick 一致，并恢复该包的战役隔离状态。
        /// </summary>
        private void LoadCampaign(ModPackage mod)
        {
            if (!_runtimeReady || mod == null)
            {
                Logger.LogError("运行时未安全就绪：已拒绝读取 MOD 战役");
                return;
            }
            SaveSystem saves = SaveSystem.Instance;
            SceneController scenes = SceneController.Instance;
            if (saves == null || scenes == null || scenes.IsPrepare || scenes.IsLoading)
            {
                Logger.LogError("无法读取 MOD 战役：存档或场景系统尚未就绪");
                return;
            }
            string slot = "mod_" + mod.Id;
            string previousOfficialSlot = ModSaveIsolation.IsModSlot(saves.CurrentSlot)
                ? ModSaveIsolation.LastOfficialSlot : saves.CurrentSlot;
            try
            {
                if (saves.GetSaveData(slot) == null)
                    throw new InvalidOperationException("隔离存档不存在或已经损坏");
                ModSaveIsolation.BeforeEnterModSlot(saves.CurrentSlot);
                saves.SetSlot(slot);
                if (SoundManager.Instance != null) SoundManager.Instance.StopMusic();
                saves.LoadGameData();
                ModCampaignState.Enter(mod);
                ModQuestSession.Reset();
                if (MissionManagerData.Instance != null)
                    MissionManagerData.Instance.UpdateCheckMissions();
                Logger.LogInfo("继续 MOD 战役：" + mod.Id + "（隔离存档槽 " + slot + "）");
                scenes.LoadCurrentScene();
                _showMenu = false;
            }
            catch (Exception ex)
            {
                ModCampaignState.Clear();
                ModQuestSession.Reset();
                PersistentModState.ResetMemory();
                RestoreOfficialSlotAfterCampaignFailure(saves, previousOfficialSlot);
                Logger.LogError("读取 MOD 战役失败，未切换场景：" + ex);
            }
        }

        private void RestoreOfficialSlotAfterCampaignFailure(
            SaveSystem saves, string previousOfficialSlot)
        {
            if (saves == null || string.IsNullOrEmpty(previousOfficialSlot)
                || ModSaveIsolation.IsModSlot(previousOfficialSlot)) return;
            try
            {
                saves.SetSlot(previousOfficialSlot);
                Logger.LogWarning("已恢复进入 MOD 战役前的原版槽 " + previousOfficialSlot);
            }
            catch (Exception restoreEx)
            {
                Logger.LogError("恢复原版槽失败；已停止后续 MOD 操作：" + restoreEx);
            }
        }

        private void DrawModEntry(ModPackage mod)
        {
            GUILayout.BeginVertical("box");
            GUILayout.Label(FormatModHeading(mod));
            string description = ModDisclosurePolicy.SafePackageDescription(mod);
            if (!string.IsNullOrEmpty(description))
                GUILayout.Label(description);
            GUILayout.Label(I18n.T("entry.scripts", mod.Entry, mod.LuaScripts.Count));
            if (GUILayout.Button(I18n.T("play")))
                PlayMod(mod);
            GUILayout.EndVertical();
        }

        /// <summary>演出入口脚本：写入 CurrentStoryScript（注册名）后切 Story 场景，由 Harmony prefix 接管实际加载。</summary>
        private void PlayMod(ModPackage mod)
        {
            if (!_runtimeReady)
            {
                Logger.LogError("运行时未安全就绪：已拒绝演出 MOD");
                return;
            }
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
              .Append("\n  名称：").Append(ModDisclosurePolicy.SafePackageName(mod))
              .Append("\n  版本：").Append(ModDisclosurePolicy.SafePackageVersion(mod))
              .Append("\n  作者（自报）：").Append(ModDisclosurePolicy.SafePackageAuthor(mod))
              .Append("\n  简介：").Append(ModDisclosurePolicy.SafePackageDescription(mod))
              .Append("\n  SHA-256：").Append(mod.PackageFingerprint)
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

        private static string FormatModHeading(ModPackage mod)
        {
            string heading = ModDisclosurePolicy.SafePackageName(mod);
            string version = ModDisclosurePolicy.SafePackageVersion(mod);
            string author = ModDisclosurePolicy.SafePackageAuthor(mod);
            if (!string.IsNullOrEmpty(version)) heading += "  v" + version;
            if (!string.IsNullOrEmpty(author)) heading += "  by " + author;
            return heading;
        }
    }
}
