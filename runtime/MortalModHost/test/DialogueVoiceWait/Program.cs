using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using Fungus;

namespace MortalModHost
{
    internal static class Program
    {
        private static int _checks;

        private static void Require(bool condition, string message)
        {
            _checks++;
            if (!condition) throw new Exception(message);
        }

        private static TextTagToken Token(TokenType type)
        {
            return new TextTagToken { type = type, paramList = new List<string> { "正文" } };
        }

        private static void Completion(string name, TokenType[] before, params TokenType[] after)
        {
            var tokens = before.Select(Token).ToList();
            DialogueVoiceWaitRuntime.ProtectCompletion(tokens);
            Require(tokens.Select(token => token.type).SequenceEqual(after), name);
        }

        private static IEnumerator Original(Action completed)
        {
            yield return null;
            completed();
        }

        private static IEnumerator StartWait(Writer writer)
        {
            bool completed = false;
            var wait = DialogueVoiceWaitRuntime.WaitForVoice(writer, Original(() => completed = true));
            Require(wait.MoveNext() && wait.Current is IEnumerator, "original voice wait retained");
            var original = (IEnumerator)wait.Current;
            while (original.MoveNext()) { }
            Require(completed, "original wait completed first");
            return wait;
        }

        private static void Main()
        {
            Completion("exit before generated input wait", new[] { TokenType.Words, TokenType.Exit, TokenType.WaitForInputNoClear },
                TokenType.Words, TokenType.WaitForVoiceOver, TokenType.Exit, TokenType.WaitForInputNoClear);
            Completion("waitforinput false natural end", new[] { TokenType.Words }, TokenType.Words, TokenType.WaitForVoiceOver);
            Completion("normal manual line unchanged", new[] { TokenType.Words, TokenType.WaitForInputNoClear },
                TokenType.Words, TokenType.WaitForInputNoClear);
            Completion("manual input before exit stays skippable", new[] { TokenType.Words, TokenType.WaitForInputNoClear, TokenType.SizeEnd, TokenType.Exit },
                TokenType.Words, TokenType.WaitForInputNoClear, TokenType.SizeEnd, TokenType.Exit);
            Completion("manual clear stays skippable", new[] { TokenType.Words, TokenType.WaitForInputAndClear },
                TokenType.Words, TokenType.WaitForInputAndClear);
            Completion("later text after page input waits", new[] { TokenType.Words, TokenType.WaitForInputNoClear, TokenType.Words, TokenType.Exit },
                TokenType.Words, TokenType.WaitForInputNoClear, TokenType.Words, TokenType.WaitForVoiceOver, TokenType.Exit);
            Completion("explicit voice token not duplicated", new[] { TokenType.Words, TokenType.WaitForVoiceOver, TokenType.Exit },
                TokenType.Words, TokenType.WaitForVoiceOver, TokenType.Exit);

            var patch = typeof(DialogueVoiceCompletionPatch).GetMethod("Prefix", BindingFlags.Static | BindingFlags.NonPublic);
            var unvoiced = new List<TextTagToken> { Token(TokenType.Words), Token(TokenType.Exit) };
            CustomAudioPlayer.CurrentVoice = "";
            patch.Invoke(null, new object[] { unvoiced });
            Require(unvoiced.Count == 2, "no custom voice leaves native tokens untouched");
            CustomAudioPlayer.CurrentVoice = "user:line";
            patch.Invoke(null, new object[] { unvoiced });
            Require(unvoiced.Count == 3 && unvoiced[1].type == TokenType.WaitForVoiceOver, "active custom voice guards exit");

            var writer = new Writer();
            var natural = StartWait(writer);
            Require(natural.MoveNext(), "natural completion waits for actual playback");
            Require(natural.MoveNext(), "voice wait is not a fixed frame count");
            CustomAudioPlayer.CurrentVoice = "";
            Require(!natural.MoveNext(), "natural completion releases when voice finishes");

            CustomAudioPlayer.CurrentVoice = "user:line";
            var clicked = StartWait(writer);
            Require(clicked.MoveNext(), "click case initially waits");
            writer.Input = true;
            Require(!clicked.MoveNext(), "explicit next line/skip releases voice wait");

            writer.Instant = false;
            var noInstant = StartWait(writer);
            Require(noInstant.MoveNext(), "instantComplete false preserves native input behavior");
            writer.Exit = true;
            Require(!noInstant.MoveNext(), "writer stop always releases voice wait");

            writer = new Writer { Paused = true };
            CustomAudioPlayer.CurrentVoice = "";
            var paused = StartWait(writer);
            Require(paused.MoveNext(), "paused writer does not naturally complete");
            writer.Paused = false;
            Require(!paused.MoveNext(), "unpaused completed voice releases immediately");
            Console.WriteLine("PASS " + _checks + " dialogue voice completion/input checks (offline stubs; live Writer requires game test)");
        }
    }

    internal static class CustomAudioPlayer
    {
        internal static string CurrentVoice = "";
    }
}

namespace Fungus
{
    public enum TokenType { Words, Exit, WaitForInputNoClear, WaitForInputAndClear, WaitForVoiceOver, SizeEnd }
    public class TextTagToken
    {
        public TokenType type;
        public List<string> paramList;
    }
    public class Writer
    {
        protected bool inputFlag;
        protected bool exitFlag;
        protected bool instantComplete = true;
        public bool Paused { get; set; }
        public bool Input { set { inputFlag = value; } }
        public bool Exit { set { exitFlag = value; } }
        public bool Instant { set { instantComplete = value; } }
    }
}

namespace HarmonyLib
{
    [AttributeUsage(AttributeTargets.Class)]
    internal class HarmonyPatch : Attribute
    {
        internal HarmonyPatch(Type type, string method) { }
    }
    internal static class AccessTools
    {
        internal static FieldInfo Field(Type type, string field)
        {
            return type.GetField(field, BindingFlags.Instance | BindingFlags.NonPublic);
        }
    }
}
