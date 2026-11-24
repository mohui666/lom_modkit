using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.RegularExpressions;

namespace MortalModHost
{
    // Static locations in the installed Lua resources. No script is executed.
    internal sealed class DevelopmentStoryIndex
    {
        internal sealed class Site
        {
            internal readonly string Chapter, Key, Text, SearchText;
            internal readonly int Line;

            internal Site(string chapter, int line, string key, string text, string aliases = "")
            {
                Chapter = chapter; Line = line; Key = key; Text = text;
                SearchText = DevelopmentEventText.ToSimplified(aliases.Length == 0 ? text : text + " " + aliases);
            }
        }

        private const string Quoted = @"(?:""(?:\\.|[^""\\])*""|'(?:\\.|[^'\\])*')";
        private static readonly Regex Calls = new Regex(
            @"--\[(?<comment>=*)\[.*?\]\k<comment>\]|--[^\r\n]*|\[(?<long>=*)\[.*?\]\k<long>\]"
            + @"|\bluamanager\s*\.\s*GetStoryText\s*\(\s*(?<text>" + Quoted + @")\s*\)"
            + @"|(?<![\w.:])say\s*\(\s*(?<literal>" + Quoted + @")\s*\)"
            + @"|\bluamanager\s*\.\s*ChangeScene\s*\(\s*(?<scene>" + Quoted + @")\s*,\s*(?:(?<sceneKey>" + Quoted + @")|(?<sceneVariable>[A-Za-z_]\w*))\s*[,)]"
            + @"|\bendgamepanel\s*\.\s*Open\s*\(\s*(?<ending>" + Quoted + @")\s*[,)]"
            + @"|\bluamanager\s*\.\s*(?:Set(?:Next|CurrentStory|CurrentTravel)Script\s*\(\s*|SetTempScript\s*\(\s*\d+\s*,\s*)(?<next>" + Quoted + @")\s*\)"
            + @"|\b(?:(?<flagOp>statmodifymanager\s*\.\s*(?:SetFlag|AddFlag))|luamanager\s*\.\s*GetFlagData)\s*\(\s*(?<flag>" + Quoted + @")\s*(?:,\s*(?:(?<value>[+-]?[0-9]+)\s*\))?|\))"
            + @"|\bcheckpointmanager\s*\.\s*(?<check>Condition|Switch|Dice|Position)\s*\(\s*(?<checkKey>" + Quoted + @")\s*[,)]"
            + @"|\bstatmodifymanager\s*\.\s*Mission\s*\(\s*(?<missionSeries>" + Quoted + @")\s*,\s*(?<missionKey>" + Quoted + @")\s*\)"
            + @"|\bmainui\s*\.\s*Show(?:Big)?Picture\s*\(\s*(?<picture>" + Quoted + @")\s*[,)]"
            + @"|(?<quoted>" + Quoted + @")", RegexOptions.Singleline | RegexOptions.CultureInvariant);
        private static readonly Regex Escapes = new Regex(@"\\([\\'""nrt])");
        private static readonly Regex TextMarkup = new Regex(@"</?(?:color|size|b|i)(?:=[^>]+)?>|\{x\}", RegexOptions.IgnoreCase);
        private static readonly Regex FlagValue = new Regex(@"^\s*(?<value>[+-]?[0-9]+)\s*[.．、=：:]\s*(?<label>.+)$");

        private readonly Dictionary<string, List<Site>> _dialogue = new Dictionary<string, List<Site>>(StringComparer.OrdinalIgnoreCase);
        private readonly List<Site> _chapterEntries = new List<Site>();
        private readonly List<Site> _references = new List<Site>();
        private readonly Dictionary<string, List<Site>> _pictures = new Dictionary<string, List<Site>>(StringComparer.OrdinalIgnoreCase);
        private readonly Dictionary<string, List<Site>> _deathCalls = new Dictionary<string, List<Site>>(StringComparer.Ordinal);
        private readonly Dictionary<string, string> _deathText = new Dictionary<string, string>(StringComparer.Ordinal);
        private readonly Dictionary<string, string> _endingText = new Dictionary<string, string>(StringComparer.Ordinal);
        private readonly Dictionary<string, HashSet<string>> _predecessors = new Dictionary<string, HashSet<string>>(StringComparer.OrdinalIgnoreCase);
        private readonly Dictionary<string, HashSet<string>> _successors = new Dictionary<string, HashSet<string>>(StringComparer.OrdinalIgnoreCase);

        private readonly Func<string, string> _flagNote;
        private readonly Func<string, string, string> _missionNote;

        internal DevelopmentStoryIndex(Func<string, string> flagNote, Func<string, string, string> missionNote)
        {
            _flagNote = flagNote;
            _missionNote = missionNote;
        }

        internal void AddDeath(string id, string text) { _deathText.Add(id, text); }

        internal void AddEnding(string id, string text) { _endingText.Add(id, text); }
        internal void AddChapter(string chapter, string source, Func<string, string> translate)
        {
            var lines = new List<Site>();
            _dialogue.Add(chapter, lines);
            _chapterEntries.Add(new Site(chapter, 0, "", chapter));
            int line = 1, cursor = 0;
            foreach (Match call in Calls.Matches(source))
            {
                while (cursor < call.Index) if (source[cursor++] == '\n') line++;
                if (call.Groups["text"].Success)
                {
                    string key = Unquote(call.Groups["text"]);
                    lines.Add(new Site(chapter, line, key, Clean(translate(key))));
                }
                else if (call.Groups["literal"].Success)
                {
                    lines.Add(new Site(chapter, line, "", Clean(Unquote(call.Groups["literal"]))));
                }
                else if (call.Groups["quoted"].Success)
                {
                    string option = call.Groups["quoted"].Value;
                    if (option.Length <= 4 || option[1] != 'O' || option[2] != '_') continue;
                    string key = Unquote(call.Groups["quoted"]).Split('|')[0];
                    lines.Add(new Site(chapter, line, key, Clean(translate(key))));
                }
                else if (call.Groups["scene"].Success || call.Groups["ending"].Success)
                {
                    string scene = call.Groups["scene"].Success ? Unquote(call.Groups["scene"]) : "End";
                    Group literal = call.Groups["ending"].Success ? call.Groups["ending"] : call.Groups["sceneKey"];
                    string key = literal.Success ? Unquote(literal) : call.Groups["sceneVariable"].Value;
                    if (key.Length == 0) continue;
                    if (scene == "GameOver" && literal.Success)
                    {
                        string text;
                        _deathText.TryGetValue(key, out text);
                        AddSite(_deathCalls, key, new Site(chapter, line, key, text ?? key));
                    }
                    else if (scene == "End" && literal.Success)
                    {
                        string title;
                        _endingText.TryGetValue(key, out title);
                        _references.Add(new Site(chapter, line, key, title ?? key, "结局 汗青书"));
                    }
                    else if (scene == "Combat" || scene == "Battle")
                        _references.Add(new Site(chapter, line, key, scene + " · " + key,
                            scene == "Combat" ? "决斗 对决 戰鬥" : "战役 戰鬥"));
                }
                else if (call.Groups["next"].Success)
                {
                    string next = Unquote(call.Groups["next"]);
                    if (!VanillaStoryPatch.ValidTarget(next) || next.Equals(chapter, StringComparison.OrdinalIgnoreCase)) continue;
                    AddLink(_successors, chapter, next);
                    AddLink(_predecessors, next, chapter);
                }
                else if (call.Groups["flag"].Success)
                {
                    string key = Unquote(call.Groups["flag"]);
                    if (key.Length == 0) continue;
                    string operation = call.Groups["flagOp"].Value;
                    string number = call.Groups["value"].Value;
                    bool set = operation.EndsWith("SetFlag", StringComparison.Ordinal);
                    string expression = key;
                    if (number.Length != 0) expression += (set ? " = " : " += ") + number;
                    string note = FlagLabel(_flagNote(key), set ? number : "");
                    _references.Add(new Site(chapter, line, expression, note.Length == 0 ? expression : note, "旗标 " + operation));
                }
                else if (call.Groups["check"].Success)
                {
                    string key = Unquote(call.Groups["checkKey"]), check = call.Groups["check"].Value;
                    if (key.Length != 0) _references.Add(new Site(chapter, line, key, check + " · " + key, "检定"));
                }
                else if (call.Groups["missionKey"].Success)
                {
                    string series = Unquote(call.Groups["missionSeries"]), key = Unquote(call.Groups["missionKey"]);
                    if (series.Length == 0 || key.Length == 0) continue;
                    string expression = series + " / " + key;
                    string note = _missionNote(series, key);
                    _references.Add(new Site(chapter, line, expression, string.IsNullOrWhiteSpace(note) ? expression : note, "任务"));
                }
                else if (call.Groups["picture"].Success)
                {
                    string picture = Unquote(call.Groups["picture"]);
                    if (picture.Length == 0) continue;
                    var site = new Site(chapter, line, picture, picture, "插图 CG");
                    AddSite(_pictures, chapter, site);
                    _references.Add(site);
                }
            }
        }

        private static string Unquote(Group group)
        {
            return Escapes.Replace(group.Value.Substring(1, group.Length - 2), m =>
            {
                switch (m.Value[1])
                {
                    case 'n': return "\n";
                    case 'r': return "\r";
                    case 't': return "\t";
                    default: return m.Value.Substring(1);
                }
            });
        }

        private static string Clean(string text) { return TextMarkup.Replace(text ?? "", "").Trim(); }
        private static string FlagLabel(string note, string number)
        {
            if (string.IsNullOrWhiteSpace(note)) return "";
            string[] lines = note.Trim().Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries);
            string title = lines[0].Trim();
            int value;
            if (!int.TryParse(number, NumberStyles.Integer, CultureInfo.InvariantCulture, out value)) return title;
            foreach (string line in lines.Skip(1))
            {
                Match item = FlagValue.Match(line);
                int listed;
                if (item.Success && int.TryParse(item.Groups["value"].Value, NumberStyles.Integer, CultureInfo.InvariantCulture, out listed)
                    && listed == value)
                    return title + " · " + item.Groups["label"].Value.Trim();
            }
            return title;
        }

        private static void AddSite(Dictionary<string, List<Site>> sites, string key, Site site)
        {
            List<Site> list;
            if (!sites.TryGetValue(key, out list)) sites.Add(key, list = new List<Site>());
            list.Add(site);
        }

        private static void AddLink(Dictionary<string, HashSet<string>> links, string chapter, string other)
        {
            HashSet<string> list;
            if (!links.TryGetValue(chapter, out list)) links.Add(chapter, list = new HashSet<string>(StringComparer.OrdinalIgnoreCase));
            list.Add(other);
        }

        internal List<Site> Search(string[] terms, bool death)
        {
            var matches = new List<Site>();
            if (death)
            {
                foreach (string id in _deathText.Keys.Union(_deathCalls.Keys).OrderBy(id => id, StringComparer.Ordinal))
                {
                    List<Site> sites;
                    if (_deathCalls.TryGetValue(id, out sites))
                        matches.AddRange(sites.Where(site => Matches(site, terms)));
                    else
                    {
                        var site = new Site("", 0, id, _deathText[id]);
                        if (Matches(site, terms)) matches.Add(site);
                    }
                }
            }
            else
            {
                matches.AddRange(_chapterEntries.Where(site => Matches(site, terms))
                    .OrderBy(site => site.Chapter, StringComparer.OrdinalIgnoreCase));
                matches.AddRange(_references.Where(site => Matches(site, terms))
                    .OrderBy(site => site.Chapter, StringComparer.OrdinalIgnoreCase).ThenBy(site => site.Line));
                foreach (var chapter in _dialogue.OrderBy(pair => pair.Key, StringComparer.OrdinalIgnoreCase))
                    matches.AddRange(chapter.Value.Where(site => Matches(site, terms)));
            }
            return matches;
        }

        private static bool Matches(Site site, string[] terms)
        {
            if (terms.Length == 0) return true;
            foreach (string term in terms)
                if (site.Chapter.IndexOf(term, StringComparison.OrdinalIgnoreCase) < 0
                    && site.Key.IndexOf(term, StringComparison.OrdinalIgnoreCase) < 0
                    && site.SearchText.IndexOf(term, StringComparison.OrdinalIgnoreCase) < 0) return false;
            return true;
        }

        internal IEnumerable<Site> Context(Site site)
        {
            List<Site> lines;
            if (!_dialogue.TryGetValue(site.Chapter, out lines)) return Enumerable.Empty<Site>();
            int index = lines.IndexOf(site);
            if (index >= 0) return lines.Skip(Math.Max(0, index - 2)).Take(Math.Min(index, 2) + 3);
            return lines.Where(line => line.Line < site.Line).Reverse().Take(3).Reverse();
        }

        internal IEnumerable<Site> Pictures(string chapter)
        {
            List<Site> sites;
            return _pictures.TryGetValue(chapter, out sites) ? sites : Enumerable.Empty<Site>();
        }

        internal IEnumerable<string> Linked(string chapter, bool predecessors)
        {
            HashSet<string> group;
            return (predecessors ? _predecessors : _successors).TryGetValue(chapter, out group)
                ? group.Where(_dialogue.ContainsKey).OrderBy(value => value, StringComparer.OrdinalIgnoreCase)
                : Enumerable.Empty<string>();
        }
    }
}
