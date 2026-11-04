using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using HarmonyLib;
using Lean.Localization;
using Mortal.Combat;
using Mortal.Core;
using OBB.Framework.Utils;
using UnityEngine;
using UObject = UnityEngine.Object;

namespace MortalModHost
{
    /// <summary>Owns cloned runtime databases; official item/skill definition assets are never edited.</summary>
    internal static class CustomGameplayRuntime
    {
        private static ItemDatabase _source, _database;
        private static readonly Dictionary<string, ModPackage> Packages = new Dictionary<string, ModPackage>(StringComparer.Ordinal);
        private static readonly List<UObject> Owned = new List<UObject>();
        private static readonly List<UObject> SupportOwned = new List<UObject>();
        private static readonly Dictionary<UserContent, Sprite> Icons = new Dictionary<UserContent, Sprite>();
        private static readonly Dictionary<int, ItemData> SourceItems = new Dictionary<int, ItemData>();
        private static readonly Dictionary<string, string> Texts = new Dictionary<string, string>(StringComparer.Ordinal);
        internal static ItemDatabase Database { get { return _database; } }

        internal static void EnsurePackage(ModPackage package)
        {
            if (package == null) throw new ArgumentNullException(nameof(package));
            ModPackage existing;
            if (Packages.TryGetValue(package.Id, out existing))
            {
                if (!string.Equals(existing.PackageFingerprint, package.PackageFingerprint, StringComparison.Ordinal))
                    throw new InvalidOperationException("同一会话不能替换已注册的 MOD 玩法定义：" + package.Id);
                ApplyTexts();
                return;
            }
            if (!package.UserContents.Values.Any(c => c.Gameplay != null)) return;
            EnsureDatabase();
            int firstOwned = Owned.Count;
            var added = new List<ItemData>();
            var keys = new List<string>();
            try
            {
                foreach (UserContent content in package.UserContents.Values)
                {
                    if (content.Gameplay == null) continue;
                    string id = RuntimeId(package, content.Id);
                    if (content.Type != "item" && content.Type != "martial") continue;
                    ItemData item = BuildItem(package, content, id);
                    Collection(content.Gameplay.Category).List.Add(item);
                    added.Add(item);
                    AddText(item.GetNameKey(), content.Name, keys);
                    AddText(item.GetTitleKey(), content.Name, keys);
                    AddText(item.GetDescKey(), content.Gameplay.Description, keys);
                }
                Packages.Add(package.Id, package);
                ApplyTexts();
            }
            catch
            {
                foreach (ItemData item in added)
                {
                    Collection(item.ItemType).List.Remove(item);
                    CustomConsumableRuntime.Unregister(item);
                }
                foreach (string key in keys) { Texts.Remove(key); LeanLocalization.CurrentTranslations.Remove(key); }
                DestroyOwnedFrom(firstOwned);
                throw;
            }
        }

        internal static void Clear()
        {
            ClearSupports();
            CustomConsumableRuntime.Clear();
            // Only replace the singleton that we own; never overwrite another plugin's replacement.
            var singleton = AccessTools.Field(typeof(ItemDatabase), "_instance");
            if (singleton != null && ReferenceEquals(singleton.GetValue(null), _database)) singleton.SetValue(null, _source);
            foreach (string key in Texts.Keys) LeanLocalization.CurrentTranslations.Remove(key);
            Texts.Clear();
            Packages.Clear();
            _database = null;
            _source = null;
            Icons.Clear();
            SourceItems.Clear();
            DestroyOwnedFrom(0);
        }

        internal static void ClearSupports()
        {
            CustomConsumableRuntime.ResetCombat();
            CustomCombatSkillRuntime.Clear();
            for (int i = SupportOwned.Count - 1; i >= 0; i--)
                if (SupportOwned[i] != null) UObject.Destroy(SupportOwned[i]);
            SupportOwned.Clear();
        }

        internal static void ApplyTexts()
        {
            foreach (var pair in Texts)
            {
                LeanTranslation translation = LeanLocalization.RegisterTranslation(pair.Key);
                if (translation != null) translation.Data = pair.Value;
            }
        }

        internal static bool Redirect(ItemDatabase database)
        {
            return _database != null && database != null && database != _database && database == _source;
        }

        internal static ItemData SessionItem(ItemData source)
        {
            ItemData result;
            return _database != null && source != null && SourceItems.TryGetValue(source.GetInstanceID(), out result)
                ? result : null;
        }

        internal static string ResolveItemId(ModPackage package, string category, string raw)
        {
            if (raw == null || !raw.StartsWith("user:", StringComparison.Ordinal)) return raw;
            UserContent content = ResolveContent(package, raw);
            if (content.Gameplay == null || content.Gameplay.Category != category)
                throw new InvalidOperationException("自定义物品类型不匹配：" + category + "/" + raw);
            EnsurePackage(package);
            return RuntimeId(package, content.Id);
        }

        internal static void ChangeItem(ModPackage package, string category, string raw, int count)
        {
            string id = ResolveItemId(package, category, raw);
            ItemDatabase database = ItemDatabase.Instance;
            ItemData item = database != null ? database.GetItem(ItemType(category), id) as ItemData : null;
            if (item == null) throw new InvalidOperationException("物品不存在：" + raw);
            long result = (long)item.CurrentCount + count;
            // Keep AddCount's existing zero clamp when consuming more than owned.
            if (result > int.MaxValue) throw new InvalidOperationException("物品数量超出上限：" + raw);
            database.AddItem(ItemType(category), id, count);
        }

        internal static IEnumerator AddSupports(CombatActionController actor)
        {
            if (!GameplaySession.PendingCombat || actor == null) yield break;
            CustomCombatSkillRuntime.Attach(actor);
            CombatManager manager = CombatManager.Instance;
            CombatActionController player = manager != null ? manager.GetActionController(CombatCharacterType.我方) : null;
            string field = actor == player ? "player_supports" : "enemy_supports";
            string encoded = GameplaySession.ConfigString(field);
            if (string.IsNullOrEmpty(encoded)) yield break;
            ModPackage package = CurrentPackage();
            if (package == null) throw new InvalidOperationException("决斗援助缺少所属 MOD");
            EnsurePackage(package);
            var seen = new HashSet<string>(StringComparer.Ordinal);
            foreach (string raw in encoded.Split(','))
            {
                if (!seen.Add(raw)) throw new InvalidOperationException("决斗援助重复：" + raw);
                UserContent content = ResolveContent(package, raw);
                if (content.Type != "support" || content.Gameplay == null)
                    throw new InvalidOperationException("不是自定义援助：" + raw);
                string id = RuntimeId(package, content.Id);
                if (actor.ExistEffect(id)) throw new InvalidOperationException("决斗援助已存在：" + raw);
                var effect = ScriptableObject.CreateInstance<CombatStateEffectScriptable>();
                SupportOwned.Add(effect);
                effect.name = id;
                var unit = ScriptableObject.CreateInstance<CustomSupportEffectUnit>();
                SupportOwned.Add(unit);
                unit.Definition = content.Gameplay;
                Set(effect, "_key", id);
                Set(effect, "_stateType", CombatStateType.Support);
                Set(effect, "_type", CombatStateEffectType.常駐);
                Set(effect, "_rounds", -1);
                Set(effect, "_roundOverrides", new CombatStateRoundOverride[0]);
                Set(effect, "_icon", Icon(content));
                Set(effect, "_units", new CombatStateEffectUnitScriptable[] { unit });
                Set(effect, "_noRepeat", true);
                Set(effect, "_desc", content.Gameplay.Description);
                AddText("CombatSkill/Name/" + id, content.Name, null);
                AddText("CombatSkill/Desc/" + id, content.Gameplay.Description, null);
                ApplyTexts();
                yield return actor.AddEffect(effect.Init(actor.gameObject));
            }
        }

        internal static void ValidateSupports(ModPackage package)
        {
            if (!GameplaySession.PendingCombat) return;
            foreach (string field in new[] { "player_supports", "enemy_supports" })
            {
                string value = GameplaySession.ConfigString(field);
                if (string.IsNullOrEmpty(value)) continue;
                var seen = new HashSet<string>(StringComparer.Ordinal);
                foreach (string raw in value.Split(','))
                {
                    UserContent content = ResolveContent(package, raw);
                    if (content.Type != "support" || content.Gameplay == null || !seen.Add(raw))
                        throw new InvalidOperationException("决斗援助未定义、类型错误或重复：" + raw);
                    Icon(content);
                }
            }
        }

        internal static ModPackage CurrentPackage()
        {
            if (GameplaySession.HasPending && Plugin.LoadedMods != null)
            {
                ModPackage owner = Plugin.LoadedMods.Find(GameplaySession.BelongsTo);
                if (owner != null) return owner;
            }
            if (ModCampaignState.Active)
            {
                ModPackage campaign = Plugin.FindCampaignPackage(ModCampaignState.ActiveCampaignId);
                if (campaign != null) return campaign;
            }
            return ModOverlay.CurrentPackage;
        }

        internal static UserContent ResolveContent(ModPackage package, string raw)
        {
            ContentRef reference; string error;
            UserContent content;
            if (package == null || !ContentRef.TryParse(raw, out reference, out error)
                || !package.TryGetUserContent(reference.ContentId, out content) || content == null)
                throw new InvalidOperationException("本 MOD 未定义该内容：" + raw);
            return content;
        }

        internal static string RuntimeId(ModPackage package, string contentId)
        {
            return "lommod_" + package.Id.Length + "_" + package.Id + "_" + contentId;
        }

        private static void EnsureDatabase()
        {
            if (_database != null) return;
            ItemDatabase original = ItemDatabase.Instance;
            if (original == null) throw new InvalidOperationException("ItemDatabase 尚未就绪");
            _source = original;
            _database = Own(UObject.Instantiate(original));
            foreach (string field in new[] { "_books", "_miscs", "_special", "_consumables" })
            {
                ItemDataCollection source = (ItemDataCollection)AccessTools.Field(typeof(ItemDatabase), field).GetValue(original);
                var clone = Own(UObject.Instantiate(source));
                var items = new List<ItemData>();
                foreach (ItemData item in source.List)
                {
                    ItemData copy = Own(UObject.Instantiate(item));
                    SourceItems[item.GetInstanceID()] = copy;
                    items.Add(copy);
                }
                Set(clone, "_list", items);
                Set(_database, field, clone);
            }
            AccessTools.Field(typeof(ItemDatabase), "_instance").SetValue(null, _database);
        }

        private static ItemData BuildItem(ModPackage package, UserContent content, string id)
        {
            ItemData item;
            if (content.Type == "martial")
            {
                Book template = _source.Books.List.OfType<Book>().FirstOrDefault(b => b.IsMartial && b.Complete);
                if (template == null) throw new InvalidOperationException("缺少原版武功书籍依赖");
                Book book = Own(UObject.Instantiate(template));
                Set(book, "_martial", true);
                Set(book, "_complete", true);
                Set(book, "_partId", new string[0]);
                Set(book, "_completeId", "");
                Set(book, "_martialType", MartialType.特殊);
                Set(book, "_martialFaction", new MartialFactionType[0]);
                Set(book, "_martialLevel", 1);
                Set(book, "_relationStatConvert", new BookLevelStatConvertData[0]);
                var levels = new List<MartialLevelData>();
                foreach (CustomMartialLevel definition in content.Gameplay.Levels)
                {
                    var level = Own(ScriptableObject.CreateInstance<MartialLevelData>());
                    Set(level, "_points", definition.Points);
                    var condition = Own(ScriptableObject.CreateInstance<MartialLearnCondition>());
                    foreach (CustomStatValue requirement in definition.Requirements)
                    {
                        var check = new MartialLearnCompareItem();
                        Set(check, "_stat", StatReference(requirement.Stat));
                        Set(check, "_value", requirement.Value);
                        Set(check, "_compareOp", StatCompareType.GreaterEqual);
                        var group = new MartialLearnCompareGroup();
                        Set(group, "_itemList", new[] { check });
                        condition.List.Add(group);
                    }
                    Set(level, "_condition", condition);
                    foreach (CustomStatValue bonus in definition.Bonuses)
                    {
                        var change = new MartialModifyStat();
                        Set(change, "_stat", StatReference(bonus.Stat));
                        Set(change, "_value", bonus.Value);
                        level.LevelUpStat.Add(change);
                    }
                    levels.Add(level);
                }
                Set(book, "_martialLevelData", levels.ToArray());
                book.SetMartial(0, 0);
                item = book;
            }
            else if (content.Gameplay.Category == "consumable")
                item = CustomConsumableRuntime.Create(package, content);
            else item = content.Gameplay.Category == "misc"
                ? (ItemData)Own(ScriptableObject.CreateInstance<Misc>())
                : Own(ScriptableObject.CreateInstance<SpecialItem>());
            item.name = id;
            Set(item, "_id", id);
            Set(item, "_image", Icon(content));
            Set(item, "_cost", content.Gameplay.Cost);
            Set(item, "_applyMissionStat", new PlayerMissionStat[0]);
            item.SetCount(0);
            return item;
        }

        private static MartialStatReference StatReference(string raw)
        {
            GameStatType type;
            if (!EnumUtils.TryParseByStringValue<GameStatType>(raw, out type))
                throw new InvalidOperationException("自定义功法属性不存在：" + raw);
            GameStat stat = PlayerStatManagerData.Instance != null ? PlayerStatManagerData.Instance.Stats.Get(type) : null;
            if (stat == null) throw new InvalidOperationException("自定义功法属性尚未就绪：" + raw);
            var reference = new MartialStatReference();
            Set(reference, "_type", MartialLevelupStatType.角色數值);
            Set(reference, "_stat", stat);
            return reference;
        }

        internal static Sprite Icon(UserContent content)
        {
            Sprite cached;
            if (Icons.TryGetValue(content, out cached) && cached != null) return cached;
            var texture = Own(new Texture2D(2, 2, TextureFormat.RGBA32, false));
            if (content.Bytes == null || !texture.LoadImage(content.Bytes))
                throw new InvalidOperationException("自定义玩法图标解码失败：" + content.Id);
            texture.filterMode = FilterMode.Bilinear;
            Sprite sprite = Own(Sprite.Create(texture, new Rect(0, 0, texture.width, texture.height), new Vector2(.5f, .5f), 100));
            Icons[content] = sprite;
            return sprite;
        }
        private static ItemDataCollection Collection(string category) { return Collection(ItemType(category)); }
        private static ItemDataCollection Collection(GameItemType type)
        {
            if (type == GameItemType.書籍) return _database.Books;
            if (type == GameItemType.雜物) return _database.Miscs;
            if (type == GameItemType.貴重品) return _database.Special;
            if (type == GameItemType.消耗品) return _database.Consumables;
            throw new InvalidOperationException("不支持的自定义物品类型");
        }
        internal static GameItemType ItemType(string category)
        {
            if (category == "book") return GameItemType.書籍;
            if (category == "misc") return GameItemType.雜物;
            if (category == "special") return GameItemType.貴重品;
            if (category == "consumable") return GameItemType.消耗品;
            throw new ArgumentException("物品类型必须是 book/misc/special/consumable");
        }
        internal static T Own<T>(T obj) where T : UObject { Owned.Add(obj); return obj; }
        private static void DestroyOwnedFrom(int index)
        {
            for (int i = Owned.Count - 1; i >= index; i--) if (Owned[i] != null) UObject.Destroy(Owned[i]);
            if (Owned.Count > index) Owned.RemoveRange(index, Owned.Count - index);
        }
        internal static void Set(object obj, string field, object value)
        {
            var member = AccessTools.Field(obj.GetType(), field);
            if (member == null) throw new MissingFieldException(obj.GetType().FullName, field);
            member.SetValue(obj, value);
        }
        internal static void AddText(string key, string value, List<string> added)
        {
            Texts[key] = value ?? "";
            if (added != null) added.Add(key);
        }
    }
}
