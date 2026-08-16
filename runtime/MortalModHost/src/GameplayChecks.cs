using System;
using Mortal.Core;
using OBB.Framework.Utils;

namespace MortalModHost
{
    /// <summary>只读封装已反编译确认、但未直接暴露给 Story Lua 的玩家状态。</summary>
    internal static class GameplayChecks
    {
        internal static int AffinityValue(string character)
        {
            PlayerStatManagerData stats = PlayerStatManagerData.Instance;
            if (stats == null) throw new InvalidOperationException("PlayerStatManagerData.Instance 为 null");
            RelationshipStatType type;
            if (!EnumUtils.TryParseByStringValue<RelationshipStatType>(character, out type))
                throw new ArgumentException("不存在的原版好感角色 id：" + character);
            RelationshipStat relationship = stats.Relationships.Get(type);
            if (relationship == null)
                throw new InvalidOperationException("找不到原版好感数据：" + character);
            return relationship.Value;
        }

        internal static bool HasItem(string category, string itemId)
        {
            ItemDatabase items = ItemDatabase.Instance;
            if (items == null) throw new InvalidOperationException("ItemDatabase.Instance 为 null");
            GameItemType type;
            switch (category)
            {
                case "book": type = GameItemType.書籍; break;
                case "misc": type = GameItemType.雜物; break;
                case "special": type = GameItemType.貴重品; break;
                default: throw new ArgumentException("物品检定类别必须是 book/misc/special");
            }
            return items.HasItem(type, itemId);
        }

        internal static int TalentLevel(string talentId)
        {
            PlayerStatManagerData stats = PlayerStatManagerData.Instance;
            if (stats == null) throw new InvalidOperationException("PlayerStatManagerData.Instance 为 null");
            PlayerTalentData talent = stats.Talents.Get(talentId);
            if (talent == null)
                throw new ArgumentException("不存在的原版天赋 id：" + talentId);
            return talent.Level;
        }
    }
}
