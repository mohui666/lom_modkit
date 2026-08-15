using System;
using System.Text.RegularExpressions;

namespace MortalModHost
{
    /// <summary>
    /// 用户内容引用协议（与 compiler/lomc/content.py、docs/zh_CN/user_content.md 一致）。
    /// 官方 ID 不含 user: 前缀，本类型只解析用户内容。无 Unity 依赖，可离线单测。
    /// </summary>
    internal sealed class ContentRef
    {
        public const string UserPrefix = "user:";
        public const string PackageUserRoot = "assets/user";
        public const long MaxAudioBytes = 20L * 1024 * 1024;
        public const long MaxImageBytes = 8L * 1024 * 1024;

        private static readonly Regex ContentIdRegex = new Regex(
            @"^[a-z][a-z0-9_]{0,31}\.[a-z0-9][a-z0-9_]{0,47}$",
            RegexOptions.CultureInvariant | RegexOptions.Compiled);

        private static readonly Regex ContentTypeRegex = new Regex(
            @"^[a-z][a-z0-9_]{0,15}$",
            RegexOptions.CultureInvariant | RegexOptions.Compiled);

        public readonly string Raw;
        public readonly string ContentId;
        public readonly string Namespace;
        public readonly string LocalId;

        private ContentRef(string contentId)
        {
            ContentId = contentId;
            Raw = UserPrefix + contentId;
            int dot = contentId.IndexOf('.');
            Namespace = contentId.Substring(0, dot);
            LocalId = contentId.Substring(dot + 1);
        }

        public static bool IsUserRef(string value)
        {
            return !string.IsNullOrEmpty(value) && value.StartsWith(UserPrefix, StringComparison.Ordinal);
        }

        public static bool TryParse(string value, out ContentRef parsed, out string error)
        {
            parsed = null;
            error = null;
            if (!IsUserRef(value))
            {
                error = "不是 user: 引用";
                return false;
            }
            string body = value.Substring(UserPrefix.Length);
            if (!IsValidContentId(body, out error))
                return false;
            parsed = new ContentRef(body);
            return true;
        }

        public static bool IsValidContentId(string contentId, out string error)
        {
            error = null;
            if (string.IsNullOrEmpty(contentId))
            {
                error = "内容 ID 不能为空";
                return false;
            }
            if (contentId.IndexOf("..", StringComparison.Ordinal) >= 0
                || contentId.IndexOf('/') >= 0
                || contentId.IndexOf('\\') >= 0
                || contentId.IndexOf(':') >= 0)
            {
                error = "内容 ID 含有非法路径字符：" + contentId;
                return false;
            }
            if (!ContentIdRegex.IsMatch(contentId))
            {
                error = "内容 ID 不合法（必须是 小写命名空间.名称）：" + contentId;
                return false;
            }
            return true;
        }

        public static bool IsSafePackageRelative(string path)
        {
            if (string.IsNullOrEmpty(path))
                return false;
            string normalized = path.Replace('\\', '/');
            if (normalized.IndexOf("..", StringComparison.Ordinal) >= 0)
                return false;
            if (!normalized.StartsWith(PackageUserRoot + "/", StringComparison.Ordinal))
                return false;
            string rest = normalized.Substring(PackageUserRoot.Length + 1);
            string[] parts = rest.Split('/');
            if (parts.Length != 3)
                return false;
            string type = parts[0];
            string id = parts[1];
            string file = parts[2];
            if (!ContentTypeRegex.IsMatch(type))
                return false;
            string ignore;
            if (!IsValidContentId(id, out ignore))
                return false;
            if (string.IsNullOrEmpty(file) || file.IndexOf("..", StringComparison.Ordinal) >= 0)
                return false;
            return true;
        }

        public static string PackageDir(string contentType, string contentId)
        {
            return PackageUserRoot + "/" + contentType + "/" + contentId;
        }
    }
}
