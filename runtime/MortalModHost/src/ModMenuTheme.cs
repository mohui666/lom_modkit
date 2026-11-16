using System.Collections.Generic;
using UnityEngine;

namespace MortalModHost
{
    // Own only our skin/font/textures. Never change a game skin or font asset.
    internal static class ModMenuTheme
    {
        internal static readonly Color Ink = new Color32(232, 237, 244, 255);
        internal static readonly Color MutedInk = new Color32(163, 176, 193, 255);
        internal static readonly Color Rule = new Color32(60, 69, 82, 255);
        internal static readonly Color Accent = new Color32(144, 195, 229, 255);
        private static readonly List<Texture2D> Textures = new List<Texture2D>();
        private static GUISkin _skin;
        private static Font _font;
        private static string _locale;
        internal static GUIStyle Title, Heading, Body, Muted, Caption, Card, Primary, Tab, SelectedTab, Row, SelectedRow, Badge, Notice, Close;
        internal static GUIStyle Header, HeaderTitle, HeaderCaption, Sidebar, RowTitle, Single, Chip, QuietChip, ErrorChip, Link;
        internal static float Scale { get { return Mathf.Clamp(Mathf.Min(Screen.width / 1440f, Screen.height / 900f), .5f, 2f); } }

        internal static GUISkin Skin
        {
            get
            {
                if (_skin == null || _locale != I18n.MenuLocale) { Release(); Build(); }
                return _skin;
            }
        }

        private static Texture2D Surface(Color32 fill, Color32 edge)
        {
            const int size = 64;
            var pixels = new Color32[size * size];
            for (int y = 0; y < size; y++) for (int x = 0; x < size; x++)
            {
                bool outside = (x == 0 || x == size - 1) && (y < 2 || y >= size - 2)
                    || (y == 0 || y == size - 1) && (x < 2 || x >= size - 2);
                pixels[y * size + x] = outside ? new Color32(0, 0, 0, 0)
                    : x == 0 || y == 0 || x == size - 1 || y == size - 1 ? edge
                    : fill;
            }
            var texture = new Texture2D(size, size, TextureFormat.RGBA32, false) { hideFlags = HideFlags.HideAndDontSave, filterMode = FilterMode.Bilinear };
            texture.SetPixels32(pixels);
            texture.Apply(false, true);
            Textures.Add(texture);
            return texture;
        }

        private static GUIStyle Text(int size, Color color, FontStyle weight = FontStyle.Normal)
        {
            var style = new GUIStyle(_skin.label) { font = _font, fontSize = size, fontStyle = weight,
                richText = false, wordWrap = true, margin = new RectOffset(), padding = new RectOffset(),
                fixedWidth = 0, fixedHeight = 0, contentOffset = Vector2.zero, stretchWidth = true,
                alignment = TextAnchor.UpperLeft, clipping = TextClipping.Clip };
            style.normal.textColor = color;
            return style;
        }

        private static void State(GUIStyleState state, Texture2D texture, Color text)
        { state.background = texture; state.textColor = text; }

        private static void Build()
        {
            _skin = Object.Instantiate(GUI.skin);
            _skin.hideFlags = HideFlags.HideAndDontSave;
            _locale = I18n.MenuLocale;
            string[] fonts = _locale == I18n.Ja ? new[] { "Yu Gothic", "Meiryo", "Hiragino Kaku Gothic ProN", "Microsoft YaHei", "Arial" }
                : _locale == I18n.Ko ? new[] { "Malgun Gothic", "Apple SD Gothic Neo", "Microsoft YaHei", "Arial" }
                : new[] { "Microsoft YaHei", "PingFang SC", "Arial" };
            _font = Font.CreateDynamicFontFromOSFont(fonts, 20);
            _font.hideFlags = HideFlags.HideAndDontSave;
            string characters = I18n.MenuCharacters;
            foreach (int size in new[] { 13, 14, 16 }) _font.RequestCharactersInTexture(characters, size, FontStyle.Normal);
            foreach (int size in new[] { 16, 19, 23 }) _font.RequestCharactersInTexture(characters, size, FontStyle.Bold);
            _skin.font = _font;
            var frame = Surface(new Color32(28, 32, 39, 255), new Color32(66, 75, 89, 255));
            var card = Surface(new Color32(37, 43, 52, 255), new Color32(54, 63, 76, 255));
            var sidebar = Surface(new Color32(31, 36, 44, 255), new Color32(31, 36, 44, 255));
            var control = Surface(new Color32(48, 56, 68, 255), new Color32(63, 74, 89, 255));
            var hover = Surface(new Color32(59, 71, 87, 255), new Color32(78, 94, 114, 255));
            var pressed = Surface(new Color32(43, 51, 63, 255), new Color32(78, 101, 124, 255));
            var selected = Surface(new Color32(44, 65, 85, 255), new Color32(62, 91, 119, 255));
            var primary = Surface(new Color32(51, 103, 141, 255), new Color32(73, 131, 172, 255));
            var primaryHover = Surface(new Color32(63, 122, 162, 255), new Color32(92, 152, 193, 255));
            _skin.window = new GUIStyle(_skin.window) { border = new RectOffset(4, 4, 4, 4), padding = new RectOffset(), margin = new RectOffset(), fixedWidth = 0, fixedHeight = 0 };
            State(_skin.window.normal, frame, Ink); State(_skin.window.onNormal, frame, Ink);
            _skin.label = Text(16, Ink);
            _skin.button = Text(16, Ink);
            _skin.button.wordWrap = true;
            _skin.button.alignment = TextAnchor.MiddleCenter;
            _skin.button.border = new RectOffset(4, 4, 4, 4);
            _skin.button.padding = new RectOffset(10, 10, 1, 1);
            _skin.button.margin = new RectOffset(0, 6, 3, 3);
            State(_skin.button.normal, control, Ink); State(_skin.button.hover, hover, Ink);
            State(_skin.button.active, pressed, Ink); State(_skin.button.focused, selected, Ink);
            State(_skin.button.onNormal, selected, Accent); State(_skin.button.onHover, selected, Accent); State(_skin.button.onActive, pressed, Accent);
            _skin.textField = new GUIStyle(_skin.button) { alignment = TextAnchor.MiddleLeft, fontSize = 16, wordWrap = false, padding = new RectOffset(10, 10, 2, 2) };
            State(_skin.textField.normal, card, Ink); State(_skin.textField.focused, selected, Ink);
            _skin.settings.cursorColor = Accent;
            _skin.settings.selectionColor = new Color(.35f, .6f, .85f, .4f);
            _skin.verticalScrollbar.fixedWidth = 9;
            _skin.verticalScrollbar.normal.background = null;
            _skin.verticalScrollbarThumb = new GUIStyle(_skin.button) { fixedWidth = 9, fixedHeight = 0, border = new RectOffset(3, 3, 3, 3), padding = new RectOffset(), margin = new RectOffset() };
            State(_skin.verticalScrollbarThumb.normal, pressed, Ink);
            _skin.verticalScrollbarUpButton.fixedHeight = _skin.verticalScrollbarDownButton.fixedHeight = 0;
            _skin.box = new GUIStyle(_skin.window) { padding = new RectOffset(18, 18, 18, 18), margin = new RectOffset(0, 0, 0, 12) };
            State(_skin.box.normal, card, Ink);
            Title = Text(24, Ink, FontStyle.Bold);
            Heading = Text(19, Ink, FontStyle.Bold);
            Body = Text(16, Ink);
            Muted = Text(14, MutedInk);
            Caption = Text(13, MutedInk);
            Card = _skin.box;
            Primary = new GUIStyle(_skin.button) { fontStyle = FontStyle.Bold };
            State(Primary.normal, primary, Color.white);
            State(Primary.hover, primaryHover, Color.white);
            State(Primary.active, primary, Color.white); State(Primary.focused, primaryHover, Color.white);
            Tab = new GUIStyle(_skin.button) { alignment = TextAnchor.MiddleCenter, padding = new RectOffset(10, 10, 1, 1), fontSize = 16 };
            State(Tab.normal, null, MutedInk);
            SelectedTab = new GUIStyle(Tab) { fontStyle = FontStyle.Bold };
            State(SelectedTab.normal, selected, Ink); State(SelectedTab.hover, selected, Ink);
            Row = new GUIStyle(_skin.button) { margin = new RectOffset() };
            State(Row.normal, null, Ink);
            SelectedRow = new GUIStyle(Row);
            State(SelectedRow.normal, selected, Ink); State(SelectedRow.hover, selected, Ink);
            Badge = Text(13, Accent, FontStyle.Bold);
            Badge.alignment = TextAnchor.MiddleCenter;
            Notice = Text(14, new Color32(232, 189, 132, 255));
            Close = new GUIStyle(_skin.button) { padding = new RectOffset(), margin = new RectOffset(), fontSize = 23 };
            Header = new GUIStyle(Card);
            State(Header.normal, null, Ink);
            HeaderTitle = Text(23, Ink, FontStyle.Bold); HeaderTitle.alignment = TextAnchor.MiddleLeft;
            HeaderCaption = Text(13, MutedInk); HeaderCaption.alignment = TextAnchor.MiddleLeft;
            Sidebar = new GUIStyle(Card); State(Sidebar.normal, sidebar, Ink);
            RowTitle = Text(16, Ink, FontStyle.Bold);
            Single = Text(14, MutedInk); Single.wordWrap = false; Single.alignment = TextAnchor.MiddleLeft;
            Chip = Text(13, Accent, FontStyle.Bold); Chip.alignment = TextAnchor.MiddleCenter;
            Chip.normal.background = selected; Chip.border = new RectOffset(4, 4, 4, 4); Chip.padding = new RectOffset(8, 8, 1, 1);
            QuietChip = new GUIStyle(Chip); QuietChip.normal.textColor = MutedInk; QuietChip.normal.background = sidebar;
            ErrorChip = new GUIStyle(Chip); ErrorChip.normal.textColor = new Color32(232, 189, 132, 255);
            ErrorChip.normal.background = Surface(new Color32(69, 52, 42, 255), new Color32(91, 69, 53, 255));
            Link = new GUIStyle(_skin.button) { fontSize = 14, padding = new RectOffset(8, 8, 1, 1) };
        }

        internal static void Fill(Rect rect, Color color)
        {
            if (Event.current.type != EventType.Repaint) return;
            Color previous = GUI.color;
            GUI.color = color;
            GUI.DrawTexture(rect, Texture2D.whiteTexture);
            GUI.color = previous;
        }

        internal static void RuleAt(float x, float y, float width) { Fill(new Rect(x, y, width, 1), Rule); }

        internal static float LabelHeight(string text, GUIStyle style, float width)
        { return Mathf.Ceil(style.CalcHeight(new GUIContent(text), width)) + 4; }

        internal static void Shadow(Rect area)
        { Fill(new Rect(area.x + 7, area.y + 9, area.width, area.height), new Color(0, 0, 0, .22f)); }

        internal static void Release()
        {
            if (_skin != null) Object.Destroy(_skin);
            if (_font != null) Object.Destroy(_font);
            foreach (var texture in Textures) if (texture != null) Object.Destroy(texture);
            Textures.Clear(); _skin = null; _font = null; _locale = null;
        }
    }
}
