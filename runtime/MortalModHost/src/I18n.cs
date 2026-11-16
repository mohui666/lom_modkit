using System;
using System.Collections.Generic;
using System.Reflection;
using Lean.Localization;

namespace MortalModHost
{
    /// <summary>
    /// 游戏语言识别与工具多语言文案。工具语言可单独选择，
    /// 不改变游戏与作品的剧情语言。
    /// </summary>
    internal static partial class I18n
    {
        internal const string Chs = "chs";
        internal const string Cht = "cht";
        internal const string Ja = "ja";
        internal const string Ko = "ko";
        private static string _menuLanguage = "";

        static I18n() { ExtendDevelopmentCatalogs(); }

        private static readonly Dictionary<string, Dictionary<string, string>> Catalogs =
            new Dictionary<string, Dictionary<string, string>>(StringComparer.OrdinalIgnoreCase)
            {
                { Chs, MakeChs() },
                { Cht, MakeCht() },
                { Ja, MakeJa() },
                { Ko, MakeKo() },
            };

        internal static string Current
        {
            get
            {
                string detected = DetectGameLanguage();
                return Catalogs.ContainsKey(detected) ? detected : Chs;
            }
        }

        // Tool UI language is independent of story/game localization.
        internal static string MenuLanguage { get { return _menuLanguage; } }
        internal static string MenuLocale { get { return _menuLanguage.Length == 0 ? Current : _menuLanguage; } }
        internal static string MenuCharacters { get { return string.Join("", Catalogs[MenuLocale].Values)+"0123456789=≠<>≤≥∧∨−"; } }
        internal static void SetMenuLanguage(string language)
        {
            language = language ?? "";
            if (language.Length != 0 && !Catalogs.ContainsKey(language)) throw new ArgumentException("Unsupported tool language", "language");
            _menuLanguage = language;
        }
        internal static string MenuT(string key) { return Catalogs[MenuLocale][key]; }
        internal static string MenuT(string key, params object[] args) { return string.Format(MenuT(key), args); }

        /// <summary>Story localization uses canonical chs/cht/ja/ko IDs.</summary>
        internal static string StoryLocale
        {
            get { return Current; }
        }

        internal static string T(string key)
        {
            Dictionary<string, string> map;
            if (Catalogs.TryGetValue(Current, out map) && map.ContainsKey(key))
                return map[key];
            return Catalogs[Chs][key];
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
                return Chs;
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
                return Cht;
            if (folded.Contains("cn") || folded.Contains("hans") || n.Contains("简") ||
                folded.Contains("simplified") || folded.Contains("chinese"))
                return Chs;
            return Chs;
        }

        private static Dictionary<string, string> MakeChs()
        {
            return new Dictionary<string, string>
            {
                { "auto.speed", "自动播放速度" }, { "auto.slow", "慢" }, { "auto.standard", "标准" }, { "auto.fast", "快" },
                { "dev.title", "—— 开发测试 ——" },
                { "menu.title", "MOD 工坊" }, { "menu.subtitle", "作品管理与剧情测试" },
                { "menu.works", "作品列表" }, { "menu.developer", "开发测试" },
                { "menu.context_addon", "原版 ＋ MOD" }, { "menu.context_campaign", "独立作品" }, { "menu.shortcut", "{0} 打开 / 关闭" },
                { "dev.timeline", "原版事件" }, { "dev.details", "跳转设置" },
                { "dev.search_hint", "搜索事件、时间或章节" }, { "dev.matches", "{0} 个节点" }, { "dev.no_matches", "没有匹配的事件，试试其他关键词。" },
                { "dev.choose", "选择左侧事件，查看时间与章节。" }, { "dev.enable", "启用开发测试" }, { "dev.disable", "停用开发测试" },
                { "dev.year", "年" }, { "dev.month", "月" }, { "dev.date_section", "游戏时间" }, { "dev.chapter_section", "原版章节" },
                { "dev.opt_in", "将当前进度用于开发测试" },
                { "dev.warning", "会修改当前模式的进度，原自动保存规则照常；本面板不会主动保存。" },
                { "dev.history", "跳时间会重置行动次数，不补齐过去的任务或选择，事件仍按当前条件判定。跳章节从原版正文开头开始，不补历史或伪造来源。" },
                { "dev.require_free", "请在养成界面结束读取、对话和暂停后使用。" },
                { "dev.current", "当前日期：第{0}年 {1}月{2}" },
                { "dev.date", "年 / 月" }, { "dev.early", "上旬" }, { "dev.middle", "中旬" }, { "dev.late", "下旬" },
                { "dev.set_date", "跳到日期" }, { "dev.script", "原版章节" }, { "dev.goto_script", "跳到开头" },
                { "dev.search", "任务节点" }, { "dev.refresh", "刷新" }, { "dev.inactive", "[停用]" },
                { "dev.selected", "已选择：{0} · {1}；确认日期或章节后再跳转。" },
                { "dev.invalid_date", "请输入1～9年、1～12月和有效旬。" },
                { "dev.invalid_script", "本机没有此原版章节，未跳转。" },
                { "dev.failed", "开发跳转失败：{0}" },
                { "entry", "活侠MOD" },
                { "title.mod_campaign", "开始 MOD 战役" },
                { "window", "MortalModHost — Mod 菜单（{0} 开关）" },
                { "empty", "未发现任何 mod。把 .lommod 包放进 BepInEx/plugins/MortalModHost/mods/ 后重启游戏。" },
                { "section.campaign", "—— 开始新战役 ——" },
                { "section.play", "—— 演出 mod 剧情 ——" },
                { "title.hint", "进入自由场景后，本菜单还可演出 mod 剧情。" },
                { "campaign.start", "开始新战役" },
                { "campaign.none", "暂无独立 MOD 战役" },
                { "campaign.none_hint", "安装并启用作品后，重启游戏" },
                { "campaign.addon_only", "已加载原版附加剧情" },
                { "campaign.addon_hint", "从原版游戏进入，按条件触发" },
                { "campaign.slot", "MOD 战役 {0:00}" },
                { "campaign.continue", "继续战役 · {0}" },
                { "campaign.recent", "最近选择" },
                { "campaign.restart", "重新开始战役" },
                { "campaign.auto.auto", "剧情自动存档" },
                { "campaign.auto.auto_free", "自由模式自动存档" },
                { "campaign.auto.auto_battle", "战役自动存档" },
                { "campaign.new_slot", "新战役" },
                { "campaign.choose_new", "选择自定义战役" },
                { "campaign.choose_hint", "使用独立 MOD 存档，不影响原版进度" },
                { "campaign.back", "返回" },
                { "campaign.back_to_saves", "返回 MOD 战役存档" },
                { "campaign.back_to_list", "返回战役列表" },
                { "campaign.option", "可选战役 {0:00}" },
                { "campaign.author", "作者自报：{0}" },
                { "campaign.no_unused", "没有可新建的战役" },
                { "campaign.no_unused_hint", "已安装的战役都已有独立存档" },
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
                { "disclosure.label", "玩家制作 MOD｜非官方内容" },
                { "disclosure.detail_author", "包指纹 {2} · 作品：{0} · 作者自报：{1}" },
                { "disclosure.detail", "包指纹 {1} · 作品：{0} · 未署名" },
            };
        }

        private static Dictionary<string, string> MakeCht()
        {
            return new Dictionary<string, string>
            {
                { "auto.speed", "自動播放速度" }, { "auto.slow", "慢" }, { "auto.standard", "標準" }, { "auto.fast", "快" },
                { "dev.title", "—— 開發測試 ——" },
                { "menu.title", "MOD 工坊" }, { "menu.subtitle", "作品管理與劇情測試" },
                { "menu.works", "作品列表" }, { "menu.developer", "開發測試" },
                { "menu.context_addon", "原版 ＋ MOD" }, { "menu.context_campaign", "獨立作品" }, { "menu.shortcut", "{0} 開啟 / 關閉" },
                { "dev.timeline", "原版事件" }, { "dev.details", "跳轉設定" },
                { "dev.search_hint", "搜尋事件、時間或章節" }, { "dev.matches", "{0} 個節點" }, { "dev.no_matches", "沒有符合的事件，請試試其他關鍵字。" },
                { "dev.choose", "選擇左側事件，查看時間與章節。" }, { "dev.enable", "啟用開發測試" }, { "dev.disable", "停用開發測試" },
                { "dev.year", "年" }, { "dev.month", "月" }, { "dev.date_section", "遊戲時間" }, { "dev.chapter_section", "原版章節" },
                { "dev.opt_in", "將目前進度用於開發測試" },
                { "dev.warning", "會修改目前模式的進度，原自動存檔規則照常；本面板不會主動存檔。" },
                { "dev.history", "跳時間會重設行動次數，不補齊過去的任務或選擇，事件仍按目前條件判定。跳章節從原版正文開頭開始，不補歷史或偽造來源。" },
                { "dev.require_free", "請在養成畫面結束讀取、對話和暫停後使用。" },
                { "dev.current", "目前日期：第{0}年 {1}月{2}" },
                { "dev.date", "年 / 月" }, { "dev.early", "上旬" }, { "dev.middle", "中旬" }, { "dev.late", "下旬" },
                { "dev.set_date", "跳至日期" }, { "dev.script", "原版章節" }, { "dev.goto_script", "跳至開頭" },
                { "dev.search", "任務節點" }, { "dev.refresh", "重新整理" }, { "dev.inactive", "[停用]" },
                { "dev.selected", "已選擇：{0} · {1}；確認日期或章節後再跳轉。" },
                { "dev.invalid_date", "請輸入1～9年、1～12月和有效旬。" },
                { "dev.invalid_script", "本機沒有此原版章節，未跳轉。" },
                { "dev.failed", "開發跳轉失敗：{0}" },
                { "entry", "活俠MOD" },
                { "title.mod_campaign", "開始 MOD 戰役" },
                { "window", "MortalModHost — Mod 選單（{0} 開關）" },
                { "empty", "未發現任何 mod。把 .lommod 包放進 BepInEx/plugins/MortalModHost/mods/ 後重啟遊戲。" },
                { "section.campaign", "—— 開始新戰役 ——" },
                { "section.play", "—— 演出 mod 劇情 ——" },
                { "title.hint", "進入自由場景後，本選單還可演出 mod 劇情。" },
                { "campaign.start", "開始新戰役" },
                { "campaign.none", "暫無獨立 MOD 戰役" },
                { "campaign.none_hint", "安裝並啟用作品後，重啟遊戲" },
                { "campaign.addon_only", "已載入原版附加劇情" },
                { "campaign.addon_hint", "從原版遊戲進入，依條件觸發" },
                { "campaign.slot", "MOD 戰役 {0:00}" },
                { "campaign.continue", "繼續戰役 · {0}" },
                { "campaign.recent", "最近選擇" },
                { "campaign.restart", "重新開始戰役" },
                { "campaign.auto.auto", "劇情自動存檔" },
                { "campaign.auto.auto_free", "自由模式自動存檔" },
                { "campaign.auto.auto_battle", "戰役自動存檔" },
                { "campaign.new_slot", "新戰役" },
                { "campaign.choose_new", "選擇自訂戰役" },
                { "campaign.choose_hint", "使用獨立 MOD 存檔，不影響原版進度" },
                { "campaign.back", "返回" },
                { "campaign.back_to_saves", "返回 MOD 戰役存檔" },
                { "campaign.back_to_list", "返回戰役列表" },
                { "campaign.option", "可選戰役 {0:00}" },
                { "campaign.author", "作者自報：{0}" },
                { "campaign.no_unused", "沒有可新建的戰役" },
                { "campaign.no_unused_hint", "已安裝的戰役都有獨立存檔" },
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
                { "disclosure.label", "玩家製作 MOD｜非官方內容" },
                { "disclosure.detail_author", "包指紋 {2} · 作品：{0} · 作者自報：{1}" },
                { "disclosure.detail", "包指紋 {1} · 作品：{0} · 未署名" },
            };
        }

        private static Dictionary<string, string> MakeJa()
        {
            return new Dictionary<string, string>
            {
                { "auto.speed", "自動再生速度" }, { "auto.slow", "遅い" }, { "auto.standard", "標準" }, { "auto.fast", "速い" },
                { "dev.title", "—— 開発テスト ——" },
                { "menu.title", "MOD 工房" }, { "menu.subtitle", "作品管理とシナリオテスト" },
                { "menu.works", "作品一覧" }, { "menu.developer", "開発テスト" },
                { "menu.context_addon", "本編 ＋ MOD" }, { "menu.context_campaign", "独立作品" }, { "menu.shortcut", "{0} 開く / 閉じる" },
                { "dev.timeline", "本編イベント" }, { "dev.details", "移動設定" },
                { "dev.search_hint", "イベント・時期・章を検索" }, { "dev.matches", "{0} 件" }, { "dev.no_matches", "該当するイベントがありません。" },
                { "dev.choose", "左のイベントで時期と章を確認。" }, { "dev.enable", "開発テストを有効化" }, { "dev.disable", "開発テストを終了" },
                { "dev.year", "年" }, { "dev.month", "月" }, { "dev.date_section", "ゲーム内の日付" }, { "dev.chapter_section", "本編の章" },
                { "dev.opt_in", "現在の進行状況を開発テストに使用" },
                { "dev.warning", "現在のモードの進行状況を変更します。通常の自動保存は有効です。このパネルは保存を直接実行しません。" },
                { "dev.history", "日付変更は行動回数をリセットします。過去の任務や選択は補完せず、イベントは現在の条件で判定します。章移動は原文の冒頭から始まり、履歴や移動元は作成しません。" },
                { "dev.require_free", "育成画面で読み込み・会話・一時停止が終了してから使用してください。" },
                { "dev.current", "現在の日付：{0}年 {1}月 {2}" },
                { "dev.date", "年 / 月" }, { "dev.early", "上旬" }, { "dev.middle", "中旬" }, { "dev.late", "下旬" },
                { "dev.set_date", "日付へ移動" }, { "dev.script", "原版の章" }, { "dev.goto_script", "冒頭へ移動" },
                { "dev.search", "任務ノード" }, { "dev.refresh", "更新" }, { "dev.inactive", "[無効]" },
                { "dev.selected", "選択：{0} · {1}。日付または章を確認してから移動してください。" },
                { "dev.invalid_date", "年は1～9、月は1～12、有効な旬を指定してください。" },
                { "dev.invalid_script", "この原版の章は見つかりません。移動していません。" },
                { "dev.failed", "開発用の移動に失敗：{0}" },
                { "entry", "活俠MOD" },
                { "title.mod_campaign", "MOD戦役を開始" },
                { "window", "MortalModHost — Mod メニュー（{0} で開閉）" },
                { "empty", "mod が見つかりません。.lommod を BepInEx/plugins/MortalModHost/mods/ に入れて再起動してください。" },
                { "section.campaign", "—— 新戦役を開始 ——" },
                { "section.play", "—— mod シナリオを再生 ——" },
                { "title.hint", "フリーシーンに入ると、このメニューからシナリオも再生できます。" },
                { "campaign.start", "新戦役を開始" },
                { "campaign.none", "独立 MOD 戦役はありません" },
                { "campaign.none_hint", "作品を導入・有効化してゲームを再起動" },
                { "campaign.addon_only", "本編追加シナリオを読み込み済み" },
                { "campaign.addon_hint", "本編のゲーム内で条件を満たすと発生" },
                { "campaign.slot", "MOD キャンペーン {0:00}" },
                { "campaign.continue", "続きから · {0}" },
                { "campaign.recent", "最近選択" },
                { "campaign.restart", "キャンペーンを最初から" },
                { "campaign.auto.auto", "ストーリー自動セーブ" },
                { "campaign.auto.auto_free", "自由行動自動セーブ" },
                { "campaign.auto.auto_battle", "戦役自動セーブ" },
                { "campaign.new_slot", "新規キャンペーン" },
                { "campaign.choose_new", "カスタムキャンペーンを選択" },
                { "campaign.choose_hint", "原作の進行とは別の MOD セーブを使用します" },
                { "campaign.back", "戻る" },
                { "campaign.back_to_saves", "MOD キャンペーンセーブへ戻る" },
                { "campaign.back_to_list", "キャンペーン一覧へ戻る" },
                { "campaign.option", "キャンペーン {0:00}" },
                { "campaign.author", "作者申告：{0}" },
                { "campaign.no_unused", "新規作成できるキャンペーンがありません" },
                { "campaign.no_unused_hint", "インストール済みの全キャンペーンに個別セーブがあります" },
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
                { "disclosure.label", "ユーザー制作 MOD｜非公式コンテンツ" },
                { "disclosure.detail_author", "パッケージ指紋 {2}・作品：{0}・作者申告：{1}" },
                { "disclosure.detail", "パッケージ指紋 {1}・作品：{0}・作者未記載" },
            };
        }

        private static Dictionary<string, string> MakeKo()
        {
            return new Dictionary<string, string>
            {
                { "auto.speed", "자동 진행 속도" }, { "auto.slow", "느림" }, { "auto.standard", "보통" }, { "auto.fast", "빠름" },
                { "dev.title", "—— 개발 테스트 ——" },
                { "menu.title", "MOD 공방" }, { "menu.subtitle", "작품 관리와 스토리 테스트" },
                { "menu.works", "작품 목록" }, { "menu.developer", "개발 테스트" },
                { "menu.context_addon", "원본 ＋ MOD" }, { "menu.context_campaign", "독립 작품" }, { "menu.shortcut", "{0} 열기 / 닫기" },
                { "dev.timeline", "원본 이벤트" }, { "dev.details", "이동 설정" },
                { "dev.search_hint", "이벤트, 시기, 장 검색" }, { "dev.matches", "{0}개 노드" }, { "dev.no_matches", "일치하는 이벤트가 없습니다." },
                { "dev.choose", "왼쪽 이벤트로 시기와 장을 확인하세요." }, { "dev.enable", "개발 테스트 켜기" }, { "dev.disable", "개발 테스트 끄기" },
                { "dev.year", "년" }, { "dev.month", "월" }, { "dev.date_section", "게임 날짜" }, { "dev.chapter_section", "원본 장" },
                { "dev.opt_in", "현재 진행 상황을 개발 테스트에 사용" },
                { "dev.warning", "현재 모드의 진행 상황을 변경합니다. 기존 자동 저장은 유지됩니다. 이 패널은 직접 저장하지 않습니다." },
                { "dev.history", "날짜 이동은 행동 횟수를 초기화합니다. 지난 임무나 선택을 채우지 않고 현재 조건으로 이벤트를 판정합니다. 장 이동은 원본 첫 부분부터 시작하며 과거나 출처를 만들지 않습니다." },
                { "dev.require_free", "육성 화면에서 로딩, 대화, 일시 정지가 끝난 뒤 사용하세요." },
                { "dev.current", "현재 날짜: {0}년 {1}월 {2}" },
                { "dev.date", "년 / 월" }, { "dev.early", "상순" }, { "dev.middle", "중순" }, { "dev.late", "하순" },
                { "dev.set_date", "날짜로 이동" }, { "dev.script", "원본 장" }, { "dev.goto_script", "처음으로 이동" },
                { "dev.search", "임무 노드" }, { "dev.refresh", "새로 고침" }, { "dev.inactive", "[비활성]" },
                { "dev.selected", "선택: {0} · {1}. 날짜 또는 장을 확인한 뒤 이동하세요." },
                { "dev.invalid_date", "1~9년, 1~12월과 올바른 순을 입력하세요." },
                { "dev.invalid_script", "이 원본 장이 없어 이동하지 않았습니다." },
                { "dev.failed", "개발 이동 실패: {0}" },
                { "entry", "활협MOD" },
                { "title.mod_campaign", "MOD 전역 시작" },
                { "window", "MortalModHost — Mod 메뉴（{0} 로 여닫기）" },
                { "empty", "mod 가 없습니다. .lommod 을 BepInEx/plugins/MortalModHost/mods/ 에 넣고 게임을 다시 시작하세요." },
                { "section.campaign", "—— 새 전역 시작 ——" },
                { "section.play", "—— mod 시나리오 재생 ——" },
                { "title.hint", "자유 장면에 들어가면 이 메뉴에서 시나리오도 재생할 수 있습니다." },
                { "campaign.start", "새 전역 시작" },
                { "campaign.none", "독립 MOD 전역이 없습니다" },
                { "campaign.none_hint", "작품 설치·활성화 후 게임을 재시작하세요" },
                { "campaign.addon_only", "원본 추가 시나리오를 불러왔습니다" },
                { "campaign.addon_hint", "원본 게임에서 조건을 충족하면 발생합니다" },
                { "campaign.slot", "MOD 전역 {0:00}" },
                { "campaign.continue", "전역 계속 · {0}" },
                { "campaign.recent", "최근 선택" },
                { "campaign.restart", "전역 다시 시작" },
                { "campaign.auto.auto", "스토리 자동 저장" },
                { "campaign.auto.auto_free", "자유 모드 자동 저장" },
                { "campaign.auto.auto_battle", "전역 자동 저장" },
                { "campaign.new_slot", "새 전역" },
                { "campaign.choose_new", "사용자 전역 선택" },
                { "campaign.choose_hint", "원작 진행과 분리된 MOD 저장을 사용합니다" },
                { "campaign.back", "뒤로" },
                { "campaign.back_to_saves", "MOD 전역 저장으로 돌아가기" },
                { "campaign.back_to_list", "전역 목록으로" },
                { "campaign.option", "선택 전역 {0:00}" },
                { "campaign.author", "작성자 표기: {0}" },
                { "campaign.no_unused", "새로 만들 수 있는 전역이 없습니다" },
                { "campaign.no_unused_hint", "설치된 모든 전역에 독립 저장이 있습니다" },
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
                { "disclosure.label", "사용자 제작 MOD｜비공식 콘텐츠" },
                { "disclosure.detail_author", "패키지 지문 {2} · 작품: {0} · 작성자 표기: {1}" },
                { "disclosure.detail", "패키지 지문 {1} · 작품: {0} · 작성자 미표기" },
            };
        }
    }
}
