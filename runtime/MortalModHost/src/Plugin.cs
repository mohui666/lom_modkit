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
    /// 运行行为契约见 docs/mod_format.md §6。
    /// </summary>
    [BepInPlugin(GUID, NAME, VERSION)]
    public class Plugin : BaseUnityPlugin
    {
        public const string GUID = "com.mohui666.mortalmodhost";
        public const string NAME = "MortalModHost";
        public const string VERSION = "0.2.0";

        private const int WindowId = 886310; // IMGUI 窗口 id，取个不易与其他插件撞车的数

        /// <summary>本轮解析到的全部 mod 包（patch 与菜单共用）。</summary>
        internal static List<ModPackage> LoadedMods { get; private set; }

        private ConfigEntry<bool> _enabled;
        private ConfigEntry<KeyboardShortcut> _menuHotkey;

        private bool _showMenu;
        private Rect _windowRect = new Rect(40f, 40f, 460f, 420f);
        private Vector2 _scroll;

        private void Awake()
        {
            _enabled = Config.Bind("General", "Enabled", true,
                "总开关。false 时禁用热键、mod 菜单与 LuaManager 注入。");
            _menuHotkey = Config.Bind("General", "MenuHotkey", new KeyboardShortcut(KeyCode.F8),
                "打开/关闭 mod 菜单的快捷键（仅在 Free 自由场景生效）。旧默认 F9 与同机 MortalInstantWin 冲突，启动时自动迁移为 F8。");
            MigrateLegacyHotkey();
            Logger.LogInfo("菜单热键：" + _menuHotkey.Value + "（Free 场景左下角也有常驻入口按钮）");

            // mods 目录：BepInEx/plugins/MortalModHost/mods/（契约 §6.1）
            string modsDir = Path.Combine(Paths.PluginPath, "MortalModHost", "mods");
            Logger.LogInfo("MortalModHost " + VERSION + " 启动，扫描 mods 目录：" + modsDir);

            LoadedMods = ModLoader.ScanMods(
                modsDir,
                msg => Logger.LogInfo(msg),
                msg => Logger.LogWarning(msg));
            ModRegistry.Rebuild(LoadedMods, msg => Logger.LogWarning(msg));

            foreach (var mod in LoadedMods)
                Logger.LogInfo(FormatModSummary(mod));
            Logger.LogInfo("mod 扫描完成：成功 " + LoadedMods.Count + " 个，注册脚本 " + ModRegistry.Count + " 个。");

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

            bool ok = true;
            ok &= CheckTarget("LuaManager.ExecuteLuaScript",
                AccessTools.Method(typeof(LuaManager), "ExecuteLuaScript"));
            ok &= CheckTarget("SaveSystem.NewGameData",
                AccessTools.Method(typeof(SaveSystem), "NewGameData"));
            ok &= CheckTarget("FreePositionData.GetExecuteScript",
                AccessTools.Method(typeof(FreePositionData), "GetExecuteScript"));
            ok &= CheckTarget("PositionController._positionData",
                AccessTools.Field(typeof(PositionController), "_positionData"));
            ok &= CheckTarget("PositionController._position",
                AccessTools.Field(typeof(PositionController), "_position"));
            if (!ok)
            {
                Logger.LogError("部分 Harmony 目标缺失（游戏版本可能已变更），战役/触发器功能可能不可用");
                return;
            }
            new Harmony(GUID).PatchAll(); // patch 本程序集全部 [HarmonyPatch] 类
            Logger.LogInfo("Harmony patch 已挂载：ExecuteLuaScript prefix / NewGameData postfix / GetExecuteScript postfix");
        }

        private bool CheckTarget(string name, MemberInfo member)
        {
            if (member != null) return true;
            Logger.LogError("找不到 Harmony 目标：" + name);
            return false;
        }

        private void Update()
        {
            if (!_enabled.Value) return;
            if (!IsHotkeyDown()) return;

            // 诊断日志：热键按下时无条件记录场景名与门控结果，方便定位"热键被抢/场景不符"类反馈
            string scene = SceneController.Instance != null ? SceneController.Instance.CurrentScene : "(SceneController 未就绪)";
            if (!IsInFreeScene())
            {
                Logger.LogInfo("检测到菜单热键按下（当前场景：" + scene + "），当前场景不是 Free，已忽略。");
                return;
            }
            _showMenu = !_showMenu;
            if (_showMenu) ClampWindowToScreen();
            Logger.LogInfo("检测到菜单热键按下（当前场景：" + scene + "），" + (_showMenu ? "已打开菜单。" : "已关闭菜单。"));
        }

        /// <summary>仅 Free 自由场景显示菜单（Story/Battle 等场景不出，避免遮挡演出）。</summary>
        private static bool IsInFreeScene()
        {
            return SceneController.Instance != null && SceneController.Instance.CurrentScene == "Free";
        }

        /// <summary>
        /// 用新 InputSystem 判定快捷键。不能用 BepInEx 自带的 KeyboardShortcut.IsDown()——
        /// 它走旧 UnityEngine.Input，本游戏只启用新输入系统，运行时会抛 InvalidOperationException。
        /// </summary>
        private bool IsHotkeyDown()
        {
            Keyboard keyboard = Keyboard.current;
            if (keyboard == null) return false;
            Key mainKey;
            if (!TryConvertKeyCode(_menuHotkey.Value.MainKey, out mainKey) || mainKey == Key.None)
                return false;
            if (!keyboard[mainKey].wasPressedThisFrame)
                return false;
            foreach (KeyCode modifier in _menuHotkey.Value.Modifiers)
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
            if (!_enabled.Value || !IsInFreeScene())
            {
                _showMenu = false; // 切场景/禁用时自动关窗，按钮也随之隐藏
                return;
            }
            DrawEntryButton();
            if (_showMenu)
                _windowRect = GUI.Window(WindowId, _windowRect, DrawWindow, "MortalModHost — Mod 菜单（" + _menuHotkey.Value + " 开关）");
        }

        /// <summary>Free 场景左下角常驻小按钮：不依赖热键的菜单入口；菜单打开时隐藏，避免重复。</summary>
        private void DrawEntryButton()
        {
            if (_showMenu) return;
            Color oldColor = GUI.color;
            GUI.color = new Color(1f, 1f, 1f, 0.8f); // 半透明，尽量不显眼
            if (GUI.Button(new Rect(10f, Screen.height - 38f, 100f, 28f), "活侠MOD"))
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
                GUILayout.Label("未发现任何 mod。把 .lommod 包放进 BepInEx/plugins/MortalModHost/mods/ 后重启游戏。");
            }
            else
            {
                _scroll = GUILayout.BeginScrollView(_scroll);
                GUILayout.Label("—— 演出 mod 剧情 ——");
                foreach (var mod in LoadedMods)
                    DrawModEntry(mod);
                GUILayout.Label("—— 开始新战役 ——");
                DrawCampaignSection();
                GUILayout.EndScrollView();
            }
            GUILayout.Space(6f);
            if (GUILayout.Button("关闭"))
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
                if (GUILayout.Button("开始新战役"))
                    StartCampaign(mod);
                GUILayout.EndVertical();
            }
            if (!any)
                GUILayout.Label("（没有声明 campaign.new_game 的 mod）");
        }

        /// <summary>
        /// 开始新战役（契约 §6.4）：隔离存档槽 SetSlot("mod_&lt;modid&gt;") → 官方 NewGameData()
        /// （NewGameDataPatch postfix 把首脚本替换为本 mod 入口）→ LoadStory。
        /// 等价于 TitleManager.NewGame() 的调用序列（Mortal.Core.decompiled.cs:8377），
        /// 但 TitleManager 只在 Title 场景存在，Free 场景里直接复刻这三步。
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
            GUILayout.Label("入口：" + mod.Entry + "（" + mod.LuaScripts.Count + " 个脚本）");
            if (GUILayout.Button("演出"))
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
