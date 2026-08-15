using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace MortalModHost
{
    /// <summary>
    /// 极简 JSON 解析器（net48 没有 System.Text.Json，Unity JsonUtility 脱离引擎又没法单测）。
    /// 仅支持解析：对象 / 数组 / 字符串 / 数字 / true / false / null。
    /// 返回值类型：Dictionary&lt;string, object&gt;、List&lt;object&gt;、string、double、bool、null。
    /// </summary>
    internal static class MiniJson
    {
        private const int MaxNestingDepth = 128;

        public static object Parse(string json)
        {
            if (json == null) throw new FormatException("JSON 文本为 null");
            int pos = 0;
            object result = ParseValue(json, ref pos, 0);
            SkipWhitespace(json, ref pos);
            if (pos != json.Length)
                throw new FormatException("JSON 末尾存在多余内容，位置 " + pos);
            return result;
        }

        private static object ParseValue(string s, ref int pos, int depth)
        {
            SkipWhitespace(s, ref pos);
            if (pos >= s.Length) throw new FormatException("JSON 意外结束");
            char c = s[pos];
            switch (c)
            {
                case '{': return ParseObject(s, ref pos, depth + 1);
                case '[': return ParseArray(s, ref pos, depth + 1);
                case '"': return ParseString(s, ref pos);
                case 't': return ParseLiteral(s, ref pos, "true", true);
                case 'f': return ParseLiteral(s, ref pos, "false", false);
                case 'n': return ParseLiteral(s, ref pos, "null", null);
                default: return ParseNumber(s, ref pos);
            }
        }

        private static Dictionary<string, object> ParseObject(string s, ref int pos, int depth)
        {
            CheckDepth(depth);
            var dict = new Dictionary<string, object>();
            pos++; // 跳过 '{'
            SkipWhitespace(s, ref pos);
            if (pos < s.Length && s[pos] == '}')
            {
                pos++;
                return dict;
            }
            while (true)
            {
                SkipWhitespace(s, ref pos);
                if (pos >= s.Length || s[pos] != '"')
                    throw new FormatException("对象键必须是字符串，位置 " + pos);
                string key = ParseString(s, ref pos);
                SkipWhitespace(s, ref pos);
                Expect(s, ref pos, ':');
                dict[key] = ParseValue(s, ref pos, depth);
                SkipWhitespace(s, ref pos);
                if (pos >= s.Length) throw new FormatException("对象未闭合");
                if (s[pos] == ',') { pos++; continue; }
                if (s[pos] == '}') { pos++; return dict; }
                throw new FormatException("对象元素间缺少 ','，位置 " + pos);
            }
        }

        private static List<object> ParseArray(string s, ref int pos, int depth)
        {
            CheckDepth(depth);
            var list = new List<object>();
            pos++; // 跳过 '['
            SkipWhitespace(s, ref pos);
            if (pos < s.Length && s[pos] == ']')
            {
                pos++;
                return list;
            }
            while (true)
            {
                list.Add(ParseValue(s, ref pos, depth));
                SkipWhitespace(s, ref pos);
                if (pos >= s.Length) throw new FormatException("数组未闭合");
                if (s[pos] == ',') { pos++; continue; }
                if (s[pos] == ']') { pos++; return list; }
                throw new FormatException("数组元素间缺少 ','，位置 " + pos);
            }
        }

        private static string ParseString(string s, ref int pos)
        {
            var sb = new StringBuilder();
            pos++; // 跳过起始 '"'
            while (pos < s.Length)
            {
                char c = s[pos++];
                if (c == '"') return sb.ToString();
                if (c != '\\') { sb.Append(c); continue; }
                if (pos >= s.Length) break;
                char esc = s[pos++];
                switch (esc)
                {
                    case '"': sb.Append('"'); break;
                    case '\\': sb.Append('\\'); break;
                    case '/': sb.Append('/'); break;
                    case 'b': sb.Append('\b'); break;
                    case 'f': sb.Append('\f'); break;
                    case 'n': sb.Append('\n'); break;
                    case 'r': sb.Append('\r'); break;
                    case 't': sb.Append('\t'); break;
                    case 'u':
                        if (pos + 4 > s.Length) throw new FormatException("\\u 转义不完整，位置 " + pos);
                        sb.Append((char)int.Parse(s.Substring(pos, 4), NumberStyles.HexNumber, CultureInfo.InvariantCulture));
                        pos += 4;
                        break;
                    default:
                        throw new FormatException("非法转义字符 \\" + esc);
                }
            }
            throw new FormatException("字符串未闭合");
        }

        private static object ParseNumber(string s, ref int pos)
        {
            int start = pos;
            if (pos < s.Length && s[pos] == '-') pos++;
            while (pos < s.Length && "0123456789+-.eE".IndexOf(s[pos]) >= 0) pos++;
            string text = s.Substring(start, pos - start);
            double value;
            if (!double.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture, out value)
                || double.IsNaN(value) || double.IsInfinity(value))
                throw new FormatException("非法数字 '" + text + "'，位置 " + start);
            return value;
        }

        private static void CheckDepth(int depth)
        {
            if (depth > MaxNestingDepth)
                throw new FormatException("JSON 嵌套层数超过 " + MaxNestingDepth + " 上限");
        }

        private static object ParseLiteral(string s, ref int pos, string literal, object value)
        {
            if (string.Compare(s, pos, literal, 0, literal.Length, StringComparison.Ordinal) != 0)
                throw new FormatException("非法字面值，位置 " + pos);
            pos += literal.Length;
            return value;
        }

        private static void SkipWhitespace(string s, ref int pos)
        {
            while (pos < s.Length && char.IsWhiteSpace(s[pos])) pos++;
        }

        private static void Expect(string s, ref int pos, char expected)
        {
            if (pos >= s.Length || s[pos] != expected)
                throw new FormatException("期望 '" + expected + "'，位置 " + pos);
            pos++;
        }
    }
}
