using System;
using System.Collections.Generic;
using Mortal.Core;

namespace MortalModHost
{
    /// <summary>
    /// 受 Host 所有权约束的临时原版商店库存。只替换 ShopPanel 实际读取的
    /// Books/Miscs/Specials 三张表；购买与官方价格计算仍完全由原版完成。
    /// </summary>
    internal static class CustomShopSession
    {
        private static readonly HashSet<string> Added =
            new HashSet<string>(StringComparer.Ordinal);
        private static List<ShopItem> _books;
        private static List<ShopItem> _miscs;
        private static List<ShopItem> _consumables;
        private static List<ShopItem> _specials;
        private static string _owner = "";

        internal static bool Active { get { return _owner.Length > 0; } }

        internal static void Begin(ModPackage package)
        {
            if (Active)
                throw new InvalidOperationException("上一间 MOD 自定义商店尚未关闭，拒绝嵌套打开");
            ShopDatabase database = ShopDatabase.Instance;
            if (database == null)
                throw new InvalidOperationException("ShopDatabase.Instance 为 null");
            _owner = Owner(package);
            _books = new List<ShopItem>(database.Books);
            _miscs = new List<ShopItem>(database.Miscs);
            _consumables = new List<ShopItem>(database.Consumables);
            _specials = new List<ShopItem>(database.Specials);
            Added.Clear();
            try
            {
                database.ResetItems();
            }
            catch
            {
                ClearState();
                throw;
            }
        }

        internal static void Add(ModPackage package, string category, string itemId, int count)
        {
            RequireOwner(package);
            if (string.IsNullOrEmpty(itemId))
                throw new ArgumentException("自定义商店 item id 不能为空");
            if (count < 1 || count > 9999)
                throw new ArgumentOutOfRangeException("count", "自定义商店库存必须为 1~9999");
            GameItemType type;
            List<ShopItem> target;
            ShopDatabase database = ShopDatabase.Instance;
            if (database == null)
                throw new InvalidOperationException("ShopDatabase.Instance 为 null");
            switch (category)
            {
                case "book":
                    type = GameItemType.書籍;
                    target = database.Books;
                    break;
                case "misc":
                    type = GameItemType.雜物;
                    target = database.Miscs;
                    break;
                case "special":
                    type = GameItemType.貴重品;
                    target = database.Specials;
                    break;
                default:
                    throw new ArgumentException("自定义商店类别必须是 book/misc/special");
            }
            string identity = category + "\n" + itemId;
            if (!Added.Add(identity))
                throw new InvalidOperationException("自定义商店商品重复：" + category + "/" + itemId);
            ItemDatabase items = ItemDatabase.Instance;
            if (items == null)
                throw new InvalidOperationException("ItemDatabase.Instance 为 null");
            ItemData data = items.GetItem(type, itemId) as ItemData;
            if (data == null)
                throw new InvalidOperationException(
                    "原版物品不存在或类型不匹配：" + category + "/" + itemId);
            target.Add(new ShopItem(data, count));
        }

        internal static void Complete(ModPackage package)
        {
            RequireOwner(package);
            Restore();
        }

        /// <summary>故障、插件卸载或可信场景边界均可幂等调用。</summary>
        internal static void Restore()
        {
            if (!Active) return;
            ShopDatabase database = ShopDatabase.Instance;
            if (database == null)
                throw new InvalidOperationException("恢复自定义商店时 ShopDatabase.Instance 为 null");
            List<ShopItem> books = _books;
            List<ShopItem> miscs = _miscs;
            List<ShopItem> consumables = _consumables;
            List<ShopItem> specials = _specials;
            database.ResetItems();
            database.Books.AddRange(books ?? new List<ShopItem>());
            database.Miscs.AddRange(miscs ?? new List<ShopItem>());
            database.Consumables.AddRange(consumables ?? new List<ShopItem>());
            database.Specials.AddRange(specials ?? new List<ShopItem>());
            ClearState();
        }

        private static void RequireOwner(ModPackage package)
        {
            if (!Active)
                throw new InvalidOperationException("没有活动的 MOD 自定义商店会话");
            if (!string.Equals(_owner, Owner(package), StringComparison.Ordinal))
                throw new InvalidOperationException("自定义商店包身份不匹配，拒绝操作");
        }

        private static string Owner(ModPackage package)
        {
            if (package == null || string.IsNullOrEmpty(package.Id)
                || string.IsNullOrEmpty(package.PackageFingerprint)
                || package.PackageFingerprint.Length != 64)
                throw new InvalidOperationException("自定义商店缺少可信的包 id / 完整 SHA-256 身份");
            return package.Id + "\n" + package.PackageFingerprint;
        }

        private static void ClearState()
        {
            _owner = "";
            _books = null;
            _miscs = null;
            _consumables = null;
            _specials = null;
            Added.Clear();
        }
    }
}
