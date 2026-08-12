using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>
    /// PositionType 枚举成员名 → 契约 position id 的映射（纯逻辑，可离线单测）。
    /// 游戏枚举成员是中文名（校場 等），契约 manifest.triggers.position 用其 StringValue id（Center 等）。
    /// 映射以反编译源码 Mortal.Core.decompiled.cs:7000 的 [StringValue] 标注为准。
    /// </summary>
    internal static class PositionNameMap
    {
        private static readonly Dictionary<string, string> ByEnumName = new Dictionary<string, string>
        {
            { "正心堂", "Mall" },
            { "校場", "Center" },
            { "煉丹房", "Alchemy" },
            { "鍛冶場", "Forge" },
            { "後山", "BackMountain" },
            { "弟子房", "Room1" },
            { "大門", "Door" },
            { "講經堂", "Study" },
            { "伙房", "Kitchen" },
            { "女弟子房", "Room2" },
            { "神秘房子", "Secret" }
        };

        /// <summary>中文枚举成员名转契约 id；无映射（如 "無"）返回 null。</summary>
        public static string ToContractId(string enumName)
        {
            if (enumName == null) return null;
            string id;
            return ByEnumName.TryGetValue(enumName, out id) ? id : null;
        }
    }
}
