using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Reflection;
using Lean.Localization;

namespace MortalModHost
{
    internal static class ReadTextRegistryTests
    {
        private delegate void PrefixCall(bool forceUpdate, out bool state);
        private delegate void PostfixCall(bool state);
        private static readonly BindingFlags PrivateStatic = BindingFlags.NonPublic | BindingFlags.Static;
        private static readonly PrefixCall Prefix = (PrefixCall)Delegate.CreateDelegate(typeof(PrefixCall), typeof(ReadTextPatch).GetMethod("Prefix", PrivateStatic));
        private static readonly PostfixCall Postfix = (PostfixCall)Delegate.CreateDelegate(typeof(PostfixCall), typeof(ReadTextPatch).GetMethod("Postfix", PrivateStatic));
        private static int _registrations;
        private static bool _capture;
        private static string _lineKey;

        internal static void Registering(string name)
        {
            _registrations++;
            if (_capture && name == "Story/line") _lineKey = name;
        }

        internal static void Update(bool forceUpdate)
        {
#if LOM_READTEXT_FOCUSED
            LeanLocalization.UpdateTranslations(forceUpdate);
#else
            bool state;
            Prefix(forceUpdate, out state);
            LeanLocalization.UpdateTranslations(forceUpdate);
            Postfix(state);
#endif
        }

        internal static void Run()
        {
            string locale = I18n.CurrentStoryLocale;
#if LOM_READTEXT_FOCUSED
            string originalLanguage = LeanLocalization.CurrentLanguage;
#endif
            try
            {
                LeanLocalization.CurrentTranslations.Clear();
                I18n.CurrentStoryLocale = "chs";
                var first = Package("first", "简体");
                first.Texts["other"] = "第二句";
                first.LocalizedTexts["cht"] = new Dictionary<string, string> { { "line", "繁體" }, { "other", "第二句繁體" } };
                var second = Package("second", "冲突后加载");
                int warnings = 0;
                ReadTextRegistry.Rebuild(new[] { first, second }, _ => warnings++);
                Require(ReadTextRegistry.Count == 2 && warnings == 1, "first-loaded ownership and duplicate warning");
                _capture = true;
                ReadTextRegistry.Apply();
                string key = _lineKey;
                Require(Text("line") == "简体", "initial Apply");
                int custom = CustomGameplayRuntime.ReadTextTestCalls;
                _registrations = 0;
                for (int frame = 0; frame < 1000; frame++) Update(false);
                Require(_registrations == 0 && CustomGameplayRuntime.ReadTextTestCalls == custom, "idle 1000 updates must not register");
                LeanLocalization.CurrentTranslations["Story/line"].Data = "tampered";
                ReadTextRegistry.Apply();
                Require(Text("line") == "简体" && ReferenceEquals(key, _lineKey), "explicit Apply rewrites data with cached prefixed key");
                var replacement = new LeanTranslation("Story/line");
                LeanLocalization.CurrentTranslations["Story/line"] = replacement;
                ReadTextRegistry.Apply();
                Require(ReferenceEquals(LeanLocalization.CurrentTranslations["Story/line"], replacement)
                    && (string)replacement.Data == "简体", "Apply must use the current translation object");
                _registrations = 0;
                Update(true);
                Require(Text("line") == "简体" && _registrations == 3, "force update must restore both package rows and gameplay text");
                _registrations = 0;
                LeanLocalization.DelayUpdateTranslations();
                Update(false);
                Require(Text("line") == "简体" && _registrations == 3, "pending false-argument update must restore text");
                _registrations = 0;
                Update(false);
                Require(_registrations == 0, "pending flag was cleared by original method");
                I18n.CurrentStoryLocale = "cht";
#if LOM_READTEXT_FOCUSED
                LeanLocalization.CurrentLanguage = "codex-test-cht";
#else
                LeanLocalization.DelayUpdateTranslations();
                Update(false);
#endif
                Require(Text("line") == "繁體" && ReadTextRegistry.Count == 2, "locale selection invalidates merged rows");
                key = _lineKey;
                ReadTextRegistry.Apply();
                Require(ReferenceEquals(key, _lineKey), "unchanged locale reuses registration key");
                I18n.CurrentStoryLocale = "ja";
                Update(true);
                Require(Text("line") == "简体", "missing locale uses package fallback");
                first.Texts["line"] = "热重载";
                first.Texts.Remove("other");
                ReadTextRegistry.Rebuild(new[] { first });
                Update(true);
                Require(Text("line") == "热重载" && ReadTextRegistry.Count == 1
                    && !LeanLocalization.CurrentTranslations.ContainsKey("Story/other"), "Rebuild replaces loaded package snapshot");
                CustomGameplayRuntime.ReadTextTestValue = "dynamic changed";
                ReadTextRegistry.Apply();
                Require((string)LeanLocalization.CurrentTranslations["Gameplay/test"].Data == "dynamic changed", "dynamic gameplay texts still applied explicitly");
                ReadTextRegistry.SelfCheck(null, message => { throw new Exception(message); });
                Console.WriteLine("PASS read-text registration: idle 1000 updates, pending/force refresh, locale/fallback, Rebuild, ownership and dynamic gameplay text");
            }
            finally
            {
                _capture = false;
                I18n.CurrentStoryLocale = locale;
                ReadTextRegistry.Rebuild(new ModPackage[0]);
#if LOM_READTEXT_FOCUSED
                LeanLocalization.CurrentLanguage = originalLanguage;
#endif
                LeanLocalization.CurrentTranslations.Clear();
                CustomGameplayRuntime.ReadTextTestValue = "dynamic";
            }
        }

        private static ModPackage Package(string id, string text)
        {
            var package = new ModPackage { Id = id, DefaultLocale = "chs", FallbackLocale = "chs" };
            package.Texts["line"] = text;
            return package;
        }
        private static string Text(string key) { return LeanLocalization.GetTranslationText("Story/" + key, null, false); }
        private static void Require(bool value, string message) { if (!value) throw new Exception(message); }

        // The baseline below is the exact previous registry merge/register
        // algorithm. Its timing is used only with the real installed Lean DLL.
        private sealed class LegacyRegistry
        {
            private readonly ModPackage _package;
            private readonly Dictionary<string, string> _texts = new Dictionary<string, string>();
            internal LegacyRegistry(ModPackage package) { _package = package; }
            internal void Apply()
            {
                _texts.Clear();
                foreach (var pair in _package.GetTexts(I18n.StoryLocale))
                    if (!_texts.ContainsKey(pair.Key)) _texts[pair.Key] = pair.Value;
                foreach (var pair in _texts)
                {
                    LeanTranslation translation = LeanLocalization.RegisterTranslation("Story/" + pair.Key);
                    if (translation != null) translation.Data = pair.Value;
                }
                CustomGameplayRuntime.ApplyTexts();
            }
        }

        private static LegacyRegistry _benchmarkPrevious;
        private static int _benchmarkRows;
        internal static void PrepareBenchmark(int rows)
        {
            I18n.CurrentStoryLocale = "chs";
            var package = new ModPackage { Id = "benchmark" };
            for (int row = 0; row < rows; row++) package.Texts["MOD_benchmark_main_say_" + row] = "unchanged text " + row;
            ReadTextRegistry.Rebuild(new[] { package });
            ReadTextRegistry.Apply();
            _benchmarkPrevious = new LegacyRegistry(package);
            _benchmarkPrevious.Apply();
            _benchmarkRows = rows;
            _capture = false;
        }

        internal static double[] BenchmarkPrepared(bool legacy, bool explicitApply, int iterations)
        {
            _registrations = 0;
            int custom = CustomGameplayRuntime.ReadTextTestCalls;
            var clock = Stopwatch.StartNew();
            for (int i = 0; i < iterations; i++)
            {
                if (legacy) _benchmarkPrevious.Apply();
                else if (explicitApply) ReadTextRegistry.Apply();
                else Update(false);
            }
            clock.Stop();
            return new[] { clock.Elapsed.TotalMilliseconds, (double)_registrations, CustomGameplayRuntime.ReadTextTestCalls - custom, _benchmarkRows };
        }
    }
}

#if LOM_OFFLINE_SMOKE || LOM_READTEXT_FOCUSED
namespace MortalModHost
{
    internal static class CustomGameplayRuntime
    {
        internal static int ReadTextTestCalls;
        internal static string ReadTextTestValue = "dynamic";
        internal static void ApplyTexts()
        {
            ReadTextTestCalls++;
            Lean.Localization.LeanLocalization.RegisterTranslation("Gameplay/test").Data = ReadTextTestValue;
        }
    }
}
#endif

#if LOM_OFFLINE_SMOKE
namespace HarmonyLib
{
    internal static class AccessTools
    {
        internal delegate ref T FieldRef<T>();
        internal static FieldRef<T> StaticFieldRefAccess<T>(FieldInfo field)
        {
            var method = new System.Reflection.Emit.DynamicMethod("ReadTextTestField", typeof(T).MakeByRefType(), Type.EmptyTypes, typeof(AccessTools), true);
            var code = method.GetILGenerator();
            code.Emit(System.Reflection.Emit.OpCodes.Ldsflda, field);
            code.Emit(System.Reflection.Emit.OpCodes.Ret);
            return (FieldRef<T>)method.CreateDelegate(typeof(FieldRef<T>));
        }
    }
}
namespace BepInEx.Logging
{
    internal sealed class ManualLogSource { internal void LogWarning(object message) { } }
}
namespace Lean.Localization
{
    internal sealed class LeanTranslation { internal object Data; internal LeanTranslation(string name) { } }
    internal static class LeanLocalization
    {
        private static bool pendingUpdates;
        internal static readonly Dictionary<string, LeanTranslation> CurrentTranslations = new Dictionary<string, LeanTranslation>();
        internal static LeanTranslation RegisterTranslation(string name)
        {
            MortalModHost.ReadTextRegistryTests.Registering(name);
            LeanTranslation result;
            if (!CurrentTranslations.TryGetValue(name, out result)) CurrentTranslations[name] = result = new LeanTranslation(name);
            return result;
        }
        internal static string GetTranslationText(string name, string fallback = null, bool replaceTokens = true)
        {
            LeanTranslation value;
            return CurrentTranslations.TryGetValue(name, out value) && value.Data is string ? (string)value.Data : fallback;
        }
        internal static void DelayUpdateTranslations() { pendingUpdates = true; }
        internal static void UpdateTranslations(bool forceUpdate = true)
        { if (!pendingUpdates && !forceUpdate) return; pendingUpdates = false; CurrentTranslations.Clear(); }
    }
}
#endif

#if LOM_READTEXT_FOCUSED
namespace MortalModHost
{
    // The focused runner uses the real Lean/Harmony assemblies. Package and
    // locale sources are the only engine-independent test doubles.
    internal sealed class ModPackage
    {
        public string Id, DefaultLocale = "chs", FallbackLocale = "chs";
        public readonly Dictionary<string, string> Texts = new Dictionary<string, string>();
        public readonly Dictionary<string, Dictionary<string, string>> LocalizedTexts = new Dictionary<string, Dictionary<string, string>>(StringComparer.OrdinalIgnoreCase);
        public IDictionary<string, string> GetTexts(string locale)
        {
            Dictionary<string, string> texts;
            if (!string.IsNullOrEmpty(locale) && LocalizedTexts.TryGetValue(locale, out texts)) return texts;
            if (LocalizedTexts.TryGetValue(FallbackLocale, out texts)) return texts;
            if (LocalizedTexts.TryGetValue(DefaultLocale, out texts)) return texts;
            return Texts;
        }
    }
}
#endif
