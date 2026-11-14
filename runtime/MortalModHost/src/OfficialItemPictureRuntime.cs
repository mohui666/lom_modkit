using System;
using Mortal.Core;
using UnityEngine;

namespace MortalModHost
{
    internal static class OfficialItemPictureRuntime
    {
        // Tool-owned references to inventory icons that have no pic_ address.
        // ItemDatabase collections and ItemData.Image remain owned by the game.
        internal static bool IsReference(string key)
        {
            return key != null && key.StartsWith("item:", StringComparison.Ordinal);
        }

        internal static bool TryResolve(string key, out Sprite sprite)
        {
            sprite = null;
            if (!IsReference(key)) return false;
            string[] parts = key.Split(':');
            ItemDatabase database = ItemDatabase.Instance;
            ItemData item = null;
            if (database != null && parts.Length == 3)
            {
                if (parts[1] == "misc") item = database.Miscs.Get(parts[2]);
                else if (parts[1] == "book") item = database.Books.Get(parts[2]);
                else if (parts[1] == "special") item = database.Special.Get(parts[2]);
            }
            if (item == null || item.Image == null)
                throw new InvalidOperationException("原版物品图标不存在：" + key);
            sprite = item.Image;
            return true;
        }
    }
}
