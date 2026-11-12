using System;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.InputSystem;

namespace MortalModHost
{
    // Hotkey settings can change during a session. Cache each actual KeyCode,
    // including unsupported keys, instead of caching one configured shortcut.
    internal static class HotkeyKeyCache
    {
        private static readonly Dictionary<KeyCode, Key?> Keys = new Dictionary<KeyCode, Key?>();

        internal static bool TryConvert(KeyCode code, out Key key)
        {
            Key? cached;
            if (Keys.TryGetValue(code, out cached))
            {
                key = cached.GetValueOrDefault();
                return cached.HasValue;
            }
            string name = code.ToString();
            if (name == "LeftControl") name = "LeftCtrl";
            else if (name == "RightControl") name = "RightCtrl";
            bool converted = Enum.TryParse(name, true, out key);
            Keys.Add(code, converted ? key : (Key?)null);
            return converted;
        }
    }
}
