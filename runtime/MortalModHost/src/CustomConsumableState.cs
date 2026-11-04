using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;

namespace MortalModHost
{
    // ItemDatabase's original Save/Load does not serialize Consumables. Keep only
    // our item counts in the same native StorySave list used by MOD event state.
    internal static class CustomConsumableState
    {
        private static string Prefix(string package, string content)
        {
            return "LOMMOD_CONSUMABLE_V1:" + package + ":" + content + ":";
        }

        internal static int Read(IEnumerable<string> records, string package, string content)
        {
            string prefix = Prefix(package, content);
            bool found = false;
            int count = 0;
            foreach (string record in records)
            {
                if (record == null || !record.StartsWith(prefix, StringComparison.Ordinal)) continue;
                if (found || !int.TryParse(record.Substring(prefix.Length), NumberStyles.None,
                    CultureInfo.InvariantCulture, out count))
                    throw new InvalidDataException("消耗品存档数量重复或无效：" + content);
                found = true;
            }
            return count;
        }

        internal static void Write(IList<string> records, string package, string content, int count)
        {
            if (count < 0) throw new ArgumentOutOfRangeException(nameof(count));
            Read(records, package, content);
            string prefix = Prefix(package, content);
            for (int i = records.Count - 1; i >= 0; i--)
                if (records[i] != null && records[i].StartsWith(prefix, StringComparison.Ordinal)) records.RemoveAt(i);
            if (count > 0) records.Add(prefix + count.ToString(CultureInfo.InvariantCulture));
        }
    }
}
