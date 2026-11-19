using System;
using System.Collections.Generic;
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

            internal Site(string chapter, int line, string key, string text)
            {
                Chapter = chapter; Line = line; Key = key; Text = text;
                SearchText = DevelopmentEventText.ToSimplified(text);
            }
        }

        private const string Quoted = @"(?:""(?:\\.|[^""\\])*""|'(?:\\.|[^'\\])*')";
        private static readonly Regex Calls = new Regex(
            @"--\[(?<comment>=*)\[.*?\]\k<comment>\]|--[^\r\n]*|\[(?<long>=*)\[.*?\]\k<long>\]"
            + @"|\bluamanager\s*\.\s*GetStoryText\s*\(\s*(?<text>" + Quoted + @")\s*\)"
            + @"|(?<![\w.:])say\s*\(\s*(?<literal>" + Quoted + @")\s*\)"
            + @"|\bluamanager\s*\.\s*ChangeScene\s*\(\s*[""']GameOver[""']\s*,\s*(?<death>" + Quoted + @")\s*[,)]"
            + @"|\bluamanager\s*\.\s*Set(?:Next|CurrentStory)Script\s*\(\s*(?<next>" + Quoted + @")\s*\)"
            + @"|\bmainui\s*\.\s*Show(?:Big)?Picture\s*\(\s*(?<picture>" + Quoted + @")\s*[,)]"
            + @"|(?<quoted>" + Quoted + @")", RegexOptions.Singleline | RegexOptions.CultureInvariant);
        private static readonly Regex Escapes = new Regex(@"\\([\\'""nrt])");
        private static readonly Regex TextMarkup = new Regex(@"</?(?:color|size|b|i)(?:=[^>]+)?>|\{x\}", RegexOptions.IgnoreCase);

        private readonly Dictionary<string, List<Site>> _dialogue = new Dictionary<string, List<Site>>(StringComparer.OrdinalIgnoreCase);
        private readonly Dictionary<string, List<Site>> _pictures = new Dictionary<string, List<Site>>(StringComparer.OrdinalIgnoreCase);
        private readonly Dictionary<string, List<Site>> _deathCalls = new Dictionary<string, List<Site>>(StringComparer.Ordinal);
        private readonly Dictionary<string, string> _deathText = new Dictionary<string, string>(StringComparer.Ordinal);
        private readonly Dictionary<string, HashSet<string>> _predecessors = new Dictionary<string, HashSet<string>>(StringComparer.OrdinalIgnoreCase);
        private readonly Dictionary<string, HashSet<string>> _successors = new Dictionary<string, HashSet<string>>(StringComparer.OrdinalIgnoreCase);

        internal void AddDeath(string id, string text) { _deathText.Add(id, text); }

        internal void AddChapter(string chapter, string source, Func<string, string> translate)
        {
            // Even a chapter without dialogue is a possible navigation target.
            var lines = new List<Site>();
            _dialogue.Add(chapter, lines);
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
                else if (call.Groups["death"].Success)
                {
                    string id = Unquote(call.Groups["death"]);
                    string text;
                    _deathText.TryGetValue(id, out text);
                    AddSite(_deathCalls, id, new Site(chapter, line, id, text ?? id));
                }
                else if (call.Groups["next"].Success)
                {
                    string next = Unquote(call.Groups["next"]);
                    if (!VanillaStoryPatch.ValidTarget(next) || next.Equals(chapter, StringComparison.OrdinalIgnoreCase)) continue;
                    AddLink(_successors, chapter, next);
                    AddLink(_predecessors, next, chapter);
                }
                else if (call.Groups["picture"].Success)
                {
                    string picture = Unquote(call.Groups["picture"]);
                    if (picture.Length > 0) AddSite(_pictures, chapter, new Site(chapter, line, picture, ""));
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
