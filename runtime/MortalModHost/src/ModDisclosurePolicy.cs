using System;
using System.Globalization;
using System.IO;
using System.Text;

namespace MortalModHost
{
    /// <summary>
    /// 可见「非官方剧情」披露的纯规则（不碰 Unity）：
    /// 只在 Host 强制开关为开时显示；Title / Free 是官方枢纽，必须关；
    /// Loading 是过场，必须保持，否则进 GameOver/End 时标已经没了。
    /// </summary>
    internal static class ModDisclosurePolicy
    {
        public const string LabelKey = "disclosure.label";
        public const string DetailWithAuthorKey = "disclosure.detail_author";
        public const string DetailWithoutAuthorKey = "disclosure.detail";
        public const string ChipObjectName = "lom_disclosure_chip";
        public const string EdgeRootName = "lom_disclosure_overlay";
        public const int FingerprintDisplayLength = 16;
        public const int MaxNameElements = 28;
        public const int MaxAuthorElements = 20;
        public const int MaxVersionElements = 16;
        public const int MaxDescriptionElements = 120;

        public static bool IsDevelopmentPreviewPackage(ModPackage package)
        {
            return package != null
                && string.Equals(package.Id, "lom_modkit_preview", StringComparison.Ordinal)
                && string.Equals(Path.GetFileName(package.PackagePath),
                    "__lom_modkit_preview.lommod", StringComparison.OrdinalIgnoreCase);
        }

        public static bool CanReplaceDevelopmentPreview(
            bool disclosureActive,
            bool traceActive,
            string activeModId,
            ModPackage candidate)
        {
            return disclosureActive
                && traceActive
                && string.Equals(activeModId, "lom_modkit_preview", StringComparison.Ordinal)
                && string.Equals(activeModId, candidate != null ? candidate.Id : null, StringComparison.Ordinal)
                && IsDevelopmentPreviewPackage(candidate);
        }

        /// <summary>
        /// F5 只有在仍处于固定编辑器试玩会话时才是“热重载”。RuntimeTrace 在
        /// Title/Free 边界可能刚结束或来自旧版本的残留状态，不能单独作为依据。
        /// </summary>
        public static bool CanHotReloadDevelopmentPreview(
            bool traceActive,
            bool disclosureActive,
            string activeModId,
            string requestModId)
        {
            return traceActive
                && disclosureActive
                && string.Equals(activeModId, "lom_modkit_preview", StringComparison.Ordinal)
                && string.Equals(requestModId, "lom_modkit_preview", StringComparison.Ordinal);
        }

        /// <summary>
        /// 当前场景是否允许保持已开启的披露。
        /// 空场景名视为未知（保持），避免 SceneController 未就绪时误关。
        /// </summary>
        public static bool ShouldKeepOnScene(string scene)
        {
            if (string.IsNullOrEmpty(scene)) return true;
            return scene != "Title" && scene != "Free";
        }

        /// <summary>正在披露的 mod 演出不能靠 cfg 总开关即时隐藏标记。</summary>
        public static bool ShouldDeferHostDisable(bool disclosureActive)
        {
            return disclosureActive;
        }

        /// <summary>
        /// manifest 文本只作为次要身份行显示：折叠空白、移除控制/双向覆盖等 Format 字符，
        /// 并按 Unicode 文本元素截断，避免换行、富文本和超长名称破坏 Host 固定标签。
        /// </summary>
        public static string SanitizeDisplayText(string value, int maxElements)
        {
            if (string.IsNullOrWhiteSpace(value) || maxElements <= 0) return "";

            string normalized;
            try
            {
                normalized = value.Normalize(NormalizationForm.FormKC);
            }
            catch (ArgumentException)
            {
                return "";
            }

            var output = new StringBuilder();
            TextElementEnumerator elements = StringInfo.GetTextElementEnumerator(normalized);
            int kept = 0;
            bool pendingSpace = false;
            bool truncated = false;
            while (elements.MoveNext())
            {
                string element = elements.GetTextElement();
                var safe = new StringBuilder(element.Length);
                bool whitespace = false;
                for (int i = 0; i < element.Length; i++)
                {
                    char c = element[i];
                    if (char.IsHighSurrogate(c) && i + 1 < element.Length && char.IsLowSurrogate(element[i + 1]))
                    {
                        UnicodeCategory scalarCategory = CharUnicodeInfo.GetUnicodeCategory(element, i);
                        if (scalarCategory == UnicodeCategory.Control
                            || scalarCategory == UnicodeCategory.Format
                            || scalarCategory == UnicodeCategory.LineSeparator
                            || scalarCategory == UnicodeCategory.ParagraphSeparator
                            || scalarCategory == UnicodeCategory.SpaceSeparator)
                        {
                            i++;
                            continue;
                        }
                        if (safe.Length == 0
                            && (scalarCategory == UnicodeCategory.NonSpacingMark
                                || scalarCategory == UnicodeCategory.SpacingCombiningMark
                                || scalarCategory == UnicodeCategory.EnclosingMark))
                        {
                            i++;
                            continue;
                        }
                        safe.Append(c);
                        safe.Append(element[++i]);
                        continue;
                    }
                    if (char.IsSurrogate(c)) continue;
                    UnicodeCategory category = CharUnicodeInfo.GetUnicodeCategory(element, i);
                    if (char.IsWhiteSpace(c) || category == UnicodeCategory.LineSeparator
                        || category == UnicodeCategory.ParagraphSeparator || category == UnicodeCategory.SpaceSeparator)
                    {
                        whitespace = true;
                        continue;
                    }
                    if (category == UnicodeCategory.Control || category == UnicodeCategory.Format)
                        continue;
                    // NFKC 会把全角尖括号还原成 ASCII；即使 Unity Text 已禁用 rich text，
                    // 清洗层仍主动移除，避免未来渲染组件改动后重新出现标签注入面。
                    if (c == '<' || c == '>') continue;
                    if (safe.Length == 0
                        && (category == UnicodeCategory.NonSpacingMark
                            || category == UnicodeCategory.SpacingCombiningMark
                            || category == UnicodeCategory.EnclosingMark))
                        continue;
                    safe.Append(c);
                }

                if (safe.Length == 0)
                {
                    if (whitespace && output.Length > 0) pendingSpace = true;
                    continue;
                }
                if (kept >= maxElements)
                {
                    truncated = true;
                    break;
                }
                if (pendingSpace && output.Length > 0) output.Append(' ');
                pendingSpace = false;
                output.Append(safe);
                kept++;
            }
            if (truncated) output.Append('…');
            return output.ToString().Trim();
        }

        public static string SafePackageName(ModPackage package)
        {
            if (package == null) return "";
            string name = SanitizeDisplayText(package.Name, MaxNameElements);
            if (!string.IsNullOrEmpty(name)) return name;
            name = SanitizeDisplayText(package.Id, MaxNameElements);
            return string.IsNullOrEmpty(name) ? "MOD" : name;
        }

        public static string SafePackageAuthor(ModPackage package)
        {
            return package == null ? "" : SanitizeDisplayText(package.Author, MaxAuthorElements);
        }

        public static string SafePackageVersion(ModPackage package)
        {
            return package == null ? "" : SanitizeDisplayText(package.Version, MaxVersionElements);
        }

        public static string SafePackageDescription(ModPackage package)
        {
            return package == null ? "" : SanitizeDisplayText(package.Description, MaxDescriptionElements);
        }

        public static bool IsValidPackageFingerprint(string fingerprint)
        {
            if (string.IsNullOrEmpty(fingerprint) || fingerprint.Length != 64) return false;
            for (int i = 0; i < fingerprint.Length; i++)
            {
                char c = fingerprint[i];
                if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')))
                    return false;
            }
            return true;
        }

        public static string ShortFingerprint(string fingerprint)
        {
            if (!IsValidPackageFingerprint(fingerprint)) return "";
            return fingerprint.Substring(0, FingerprintDisplayLength).ToUpperInvariant();
        }
    }
}
