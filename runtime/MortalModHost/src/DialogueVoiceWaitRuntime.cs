using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using Fungus;
using HarmonyLib;

namespace MortalModHost
{
    internal static class DialogueVoiceWaitRuntime
    {
        private static readonly FieldInfo Input = AccessTools.Field(typeof(Writer), "inputFlag");
        private static readonly FieldInfo Exit = AccessTools.Field(typeof(Writer), "exitFlag");
        private static readonly FieldInfo Instant = AccessTools.Field(typeof(Writer), "instantComplete");

        internal static void ProtectCompletion(List<TextTagToken> tokens)
        {
            bool naturalCompletion = true;
            for (int index = 0; index < tokens.Count; index++)
            {
                TokenType type = tokens[index].type;
                if (type == TokenType.WaitForInputNoClear || type == TokenType.WaitForInputAndClear)
                    naturalCompletion = false;
                else if (type == TokenType.Words && tokens[index].paramList.Exists(text => !string.IsNullOrWhiteSpace(text)))
                    naturalCompletion = true;
                else if (type == TokenType.Exit)
                {
                    if (naturalCompletion && (index == 0 || tokens[index - 1].type != TokenType.WaitForVoiceOver))
                        tokens.Insert(index, VoiceWait());
                    return;
                }
            }
            // A terminal {wi}/{wc} already lets the player leave the line. Do
            // not put a new voice wait after that deliberate advance.
            if (naturalCompletion && (tokens.Count == 0 || tokens[tokens.Count - 1].type != TokenType.WaitForVoiceOver))
                tokens.Add(VoiceWait());
        }

        private static TextTagToken VoiceWait()
        {
            return new TextTagToken { type = TokenType.WaitForVoiceOver, paramList = new List<string>() };
        }

        internal static IEnumerator WaitForVoice(Writer writer, IEnumerator original)
        {
            yield return original;
            while (writer != null && !(bool)Exit.GetValue(writer)
                && (!(bool)Instant.GetValue(writer) || !(bool)Input.GetValue(writer))
                && (writer.Paused || !string.IsNullOrEmpty(CustomAudioPlayer.CurrentVoice)))
                yield return null;
        }
    }

    [HarmonyPatch(typeof(Writer), "ProcessTokens")]
    internal static class DialogueVoiceCompletionPatch
    {
        private static void Prefix(List<TextTagToken> tokens)
        {
            if (!string.IsNullOrEmpty(CustomAudioPlayer.CurrentVoice))
                DialogueVoiceWaitRuntime.ProtectCompletion(tokens);
        }
    }

    [HarmonyPatch(typeof(Writer), "DoWaitVO")]
    internal static class DialogueVoiceTokenPatch
    {
        private static void Postfix(Writer __instance, ref IEnumerator __result)
        {
            if (!string.IsNullOrEmpty(CustomAudioPlayer.CurrentVoice))
                __result = DialogueVoiceWaitRuntime.WaitForVoice(__instance, __result);
        }
    }
}
