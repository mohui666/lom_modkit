using System;
using System.Reflection;
using BepInEx.Configuration;
using Fungus;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    internal static class AutoDialogueRuntime
    {
        [ThreadStatic] internal static bool NativeAutoPulse;
        private static readonly FieldInfo Auto = AccessTools.Field(typeof(StoryManager), "_autoDialog");
        private static readonly FieldInfo Skip = AccessTools.Field(typeof(StoryManager), "_skipDialog");
        private static Writer _writer;
        private static SayDialog _dialog;
        private static bool _hadVoice;
        private static AutoDialogueTiming _timing;
        internal static ConfigEntry<AutoDialogueSpeed> Speed;

        internal static void Begin(Writer writer)
        {
            if (ModOverlay.CurrentPackage == null) return;
            SayDialog dialog = writer.GetComponent<SayDialog>();
            if (dialog == null) return;
            Clear();
            _writer = writer;
            _dialog = dialog;
            _hadVoice = VoicePlaying();
        }

        private static bool VoicePlaying()
        {
            return !string.IsNullOrEmpty(CustomAudioPlayer.CurrentVoice)
                || _writer != null && _writer.AttachedWriterAudio != null
                && _writer.AttachedWriterAudio.GetSecondsRemaining() > 0f;
        }

        internal static void Tick()
        {
            if (ModOverlay.CurrentPackage == null) { Clear(); return; }
            if (_writer == null || _dialog == null) return;
            bool voice = VoicePlaying();
            _hadVoice |= voice;
            if (!_writer.IsWaitingForInput) { _timing = null; return; }
            if (_timing == null) _timing = new AutoDialogueTiming(_dialog.StoryText, _hadVoice);
            StoryManager manager = StoryManager.Instance;
            if (manager == null) return;
            DialogInput input = _writer.GetComponent<DialogInput>();
            MenuDialog menu = MenuDialog.ActiveMenuDialog;
            bool active = (bool)Auto.GetValue(manager) && !(bool)Skip.GetValue(manager)
                && !manager.IsStoryPause && !_writer.Paused && !RuntimeDebugControl.Paused
                && input != null && input.isActiveAndEnabled
                && !(menu != null && menu.IsActive() && menu.DisplayedOptionsCount > 0);
            if (_timing.Tick(Time.deltaTime, active, voice, Speed != null ? Speed.Value : AutoDialogueSpeed.Standard))
                input.SetNextLineFlag();
        }

        internal static void Clear()
        {
            _writer = null;
            _dialog = null;
            _timing = null;
            _hadVoice = false;
        }
    }

    [HarmonyPatch(typeof(Writer), "Write")]
    internal static class AutoDialogueWriterPatch
    {
        private static void Prefix(Writer __instance) { AutoDialogueRuntime.Begin(__instance); }
    }

    // Mark only the original automatic coroutine's call stack. Manual submit
    // uses the same StoryManager method and must never be blocked.
    [HarmonyPatch]
    internal static class AutoDialogueNativePulsePatch
    {
        private static MethodBase TargetMethod()
        {
            return AccessTools.EnumeratorMoveNext(AccessTools.Method(typeof(StoryManager), "StartAutoDialogue"));
        }
        private static void Prefix(out bool __state)
        {
            __state = AutoDialogueRuntime.NativeAutoPulse;
            AutoDialogueRuntime.NativeAutoPulse = true;
        }
        private static Exception Finalizer(Exception __exception, bool __state)
        {
            AutoDialogueRuntime.NativeAutoPulse = __state;
            return __exception;
        }
    }

    [HarmonyPatch(typeof(StoryManager), "SetDialogNextLineFlag")]
    internal static class AutoDialogueNativeInputPatch
    {
        private static bool Prefix(bool ____skipDialog)
        {
            return !AutoDialogueRuntime.NativeAutoPulse || ModOverlay.CurrentPackage == null || ____skipDialog;
        }
    }
}
