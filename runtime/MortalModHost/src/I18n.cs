using System;
using System.Collections.Generic;
using System.Reflection;
using Lean.Localization;

namespace MortalModHost
{
    /// <summary>
    /// 游戏内 Mod 菜单文案。跟随官方 LeanLocalization 当前语言：
    /// 繁中 / 简中 / 韩语来自游戏官方语言表用语；日语官方游戏没有，回退简中。
    /// </summary>
    internal static class I18n
    {
        internal const string ZhCn = "zh-CN";
        internal const string ZhTw = "zh-TW";
        internal const string Ja = "ja";
        internal const string Ko = "ko";

        private static readonly Dictionary<string, Dictionary<string, string>> Catalogs =
            new Dictionary<string, Dictionary<string, string>>(StringComparer.OrdinalIgnoreCase)
            {
                { ZhCn, MakeZhCn() },
                { ZhTw, MakeZhTw() },
                { Ja, MakeJa() },
                { Ko, MakeKo() },
            };

        internal static string Current
        {
            get
            {
                string detected = DetectGameLanguage();
                return Catalogs.ContainsKey(detected) ? detected : ZhCn;
            }
        }

        /// <summary>Story localization contract uses underscore locale IDs.</summary>
        internal static string StoryLocale
        {
            get { return Current.Replace('-', '_'); }
        }

        internal static string T(string key)
        {
            Dictionary<string, string> map;
            if (Catalogs.TryGetValue(Current, out map) && map.ContainsKey(key))
                return map[key];
            return Catalogs[ZhCn][key];
        }

        internal static string T(string key, params object[] args)
        {
            return string.Format(T(key), args);
        }

        private static string DetectGameLanguage()
        {
            try
            {
                Type type = typeof(LeanLocalization);
                PropertyInfo current = type.GetProperty("CurrentLanguage", BindingFlags.Public | BindingFlags.Static);
                object value = current != null ? current.GetValue(null, null) : null;
                if (value == null)
                {
                    MethodInfo first = type.GetMethod("GetFirstCurrentLanguage", BindingFlags.Public | BindingFlags.Static);
                    value = first != null ? first.Invoke(null, null) : null;
                }
                string name = Convert.ToString(value) ?? "";
                return MapLanguageName(name);
            }
            catch
            {
                return ZhCn;
            }
        }

        private static string MapLanguageName(string name)
        {
            string n = (name ?? "").Trim();
            string folded = n.ToLowerInvariant();
            if (folded.Contains("kr") || folded.Contains("ko") || n.Contains("한") || folded.Contains("korean"))
                return Ko;
            if (folded.Contains("ja") || n.Contains("日") || folded.Contains("japan"))
                return Ja;
            if (folded.Contains("tw") || folded.Contains("hk") || folded.Contains("hant") ||
                n.Contains("繁") || folded.Contains("traditional"))
                return ZhTw;
            if (folded.Contains("cn") || folded.Contains("hans") || n.Contains("简") ||
                folded.Contains("simplified") || folded.Contains("chinese"))
                return ZhCn;
            return ZhCn;
        }

        private static Dictionary<string, string> MakeZhCn()
        {
            return new Dictionary<string, string>
            {
                { "entry", "活侠MOD" },
                { "window", "MortalModHost — Mod 菜单（{0} 开关）" },
                { "empty", "未发现任何 mod。把 .lommod 包放进 BepInEx/plugins/MortalModHost/mods/ 后重启游戏。" },
                { "section.campaign", "—— 开始新战役 ——" },
                { "section.play", "—— 演出 mod 剧情 ——" },
                { "title.hint", "进入自由场景后，本菜单还可演出 mod 剧情。" },
                { "campaign.start", "开始新战役" },
                { "campaign.none", "（没有声明 campaign.new_game 的 mod）" },
                { "play", "演出" },
                { "close", "关闭" },
                { "entry.scripts", "入口：{0}（{1} 个脚本）" },
                { "debug.window", "F5 Runtime Debugger（{0} 隐藏/显示）" },
                { "debug.mod", "当前 Mod" }, { "debug.story", "当前剧情" }, { "debug.node", "当前节点" },
                { "debug.variables", "变量" }, { "debug.flags", "Flags" }, { "debug.characters", "自定义角色" },
                { "debug.music", "自定义音乐" }, { "debug.voice", "当前语音" }, { "debug.trace", "最近 Trace" },
                { "debug.none", "（无）" }, { "debug.hide", "隐藏调试器" }, { "debug.draw_error", "调试器绘制失败" },
                { "debug.state", "执行" }, { "debug.state.running", "运行中" }, { "debug.state.pending", "将在下一节点前暂停" }, { "debug.state.paused", "已暂停（节点体尚未执行）" },
                { "debug.pause", "下一节点前暂停" }, { "debug.step", "单步" }, { "debug.continue", "继续" },
            };
        }

        private static Dictionary<string, string> MakeZhTw()
        {
            return new Dictionary<string, string>
            {
                { "entry", "活俠MOD" },
                { "window", "MortalModHost — Mod 選單（{0} 開關）" },
                { "empty", "未發現任何 mod。把 .lommod 包放進 BepInEx/plugins/MortalModHost/mods/ 後重啟遊戲。" },
                { "section.campaign", "—— 開始新戰役 ——" },
                { "section.play", "—— 演出 mod 劇情 ——" },
                { "title.hint", "進入自由場景後，本選單還可演出 mod 劇情。" },
                { "campaign.start", "開始新戰役" },
                { "campaign.none", "（沒有聲明 campaign.new_game 的 mod）" },
                { "play", "演出" },
                { "close", "關閉" },
                { "entry.scripts", "入口：{0}（{1} 個腳本）" },
                { "debug.window", "F5 Runtime Debugger（{0} 隱藏/顯示）" },
                { "debug.mod", "目前 Mod" }, { "debug.story", "目前劇情" }, { "debug.node", "目前節點" },
                { "debug.variables", "變數" }, { "debug.flags", "Flags" }, { "debug.characters", "自訂角色" },
                { "debug.music", "自訂音樂" }, { "debug.voice", "目前語音" }, { "debug.trace", "最近 Trace" },
                { "debug.none", "（無）" }, { "debug.hide", "隱藏偵錯器" }, { "debug.draw_error", "偵錯器繪製失敗" },
                { "debug.state", "執行" }, { "debug.state.running", "執行中" }, { "debug.state.pending", "將在下一節點前暫停" }, { "debug.state.paused", "已暫停（節點內容尚未執行）" },
                { "debug.pause", "下一節點前暫停" }, { "debug.step", "單步" }, { "debug.continue", "繼續" },
            };
        }

        private static Dictionary<string, string> MakeJa()
        {
            return new Dictionary<string, string>
            {
                { "entry", "活俠MOD" },
                { "window", "MortalModHost — Mod メニュー（{0} で開閉）" },
                { "empty", "mod が見つかりません。.lommod を BepInEx/plugins/MortalModHost/mods/ に入れて再起動してください。" },
                { "section.campaign", "—— 新戦役を開始 ——" },
                { "section.play", "—— mod シナリオを再生 ——" },
                { "title.hint", "フリーシーンに入ると、このメニューからシナリオも再生できます。" },
                { "campaign.start", "新戦役を開始" },
                { "campaign.none", "（campaign.new_game を宣言した mod がありません）" },
                { "play", "再生" },
                { "close", "閉じる" },
                { "entry.scripts", "入口：{0}（スクリプト {1}）" },
                { "debug.window", "F5 Runtime Debugger（{0} 表示切替）" },
                { "debug.mod", "現在の Mod" }, { "debug.story", "現在のストーリー" }, { "debug.node", "現在のノード" },
                { "debug.variables", "変数" }, { "debug.flags", "Flags" }, { "debug.characters", "カスタムキャラクター" },
                { "debug.music", "カスタム音楽" }, { "debug.voice", "現在のボイス" }, { "debug.trace", "最近の Trace" },
                { "debug.none", "（なし）" }, { "debug.hide", "デバッガーを隠す" }, { "debug.draw_error", "デバッガー描画エラー" },
                { "debug.state", "実行" }, { "debug.state.running", "実行中" }, { "debug.state.pending", "次のノード直前で一時停止" }, { "debug.state.paused", "一時停止中（ノード本体は未実行）" },
                { "debug.pause", "次ノード前で停止" }, { "debug.step", "ステップ" }, { "debug.continue", "続行" },
            };
        }

        private static Dictionary<string, string> MakeKo()
        {
            return new Dictionary<string, string>
            {
                { "entry", "활협MOD" },
                { "window", "MortalModHost — Mod 메뉴（{0} 로 여닫기）" },
                { "empty", "mod 가 없습니다. .lommod 을 BepInEx/plugins/MortalModHost/mods/ 에 넣고 게임을 다시 시작하세요." },
                { "section.campaign", "—— 새 전역 시작 ——" },
                { "section.play", "—— mod 시나리오 재생 ——" },
                { "title.hint", "자유 장면에 들어가면 이 메뉴에서 시나리오도 재생할 수 있습니다." },
                { "campaign.start", "새 전역 시작" },
                { "campaign.none", "（campaign.new_game 을 선언한 mod 가 없습니다）" },
                { "play", "재생" },
                { "close", "닫기" },
                { "entry.scripts", "입구：{0}（스크립트 {1}개）" },
                { "debug.window", "F5 Runtime Debugger（{0} 숨기기/표시）" },
                { "debug.mod", "현재 Mod" }, { "debug.story", "현재 스토리" }, { "debug.node", "현재 노드" },
                { "debug.variables", "변수" }, { "debug.flags", "Flags" }, { "debug.characters", "사용자 캐릭터" },
                { "debug.music", "사용자 음악" }, { "debug.voice", "현재 음성" }, { "debug.trace", "최근 Trace" },
                { "debug.none", "（없음）" }, { "debug.hide", "디버거 숨기기" }, { "debug.draw_error", "디버거 그리기 실패" },
                { "debug.state", "실행" }, { "debug.state.running", "실행 중" }, { "debug.state.pending", "다음 노드 전에 일시정지" }, { "debug.state.paused", "일시정지됨（노드 본문 실행 전）" },
                { "debug.pause", "다음 노드 전 정지" }, { "debug.step", "한 단계" }, { "debug.continue", "계속" },
            };
        }
    }
}
