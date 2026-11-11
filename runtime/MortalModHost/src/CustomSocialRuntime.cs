using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using HarmonyLib;
using Mortal.Core;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    // Custom rows use private RelationshipStat instances only as native UI handles.
    // They are never registered with the official relationship collection or save schema.
    internal static class CustomSocialRuntime
    {
        internal sealed class Entry
        {
            internal ModPackage Package;
            internal UserContent Content;
            internal string Reference;
            internal SocialStatPanel Panel;
            internal SocialCharacterButton Button;
            internal RelationshipStat Data;
            internal int LastValue;
            internal bool LastKnown;
            internal int LastPages;
            internal Color[] IconColors;
        }

        private static readonly Dictionary<RelationshipStat, Entry> Entries = new Dictionary<RelationshipStat, Entry>();
        private static readonly HashSet<CustomSocialPanelController> Panels = new HashSet<CustomSocialPanelController>();

        internal static List<ModPackage> AllowedPackages()
        {
            var result = new List<ModPackage>();
            CollectAllowedPackages(result);
            return result;
        }

        private static void CollectAllowedPackages(List<ModPackage> result)
        {
            result.Clear();
            SaveSystem saves = SaveSystem.Instance;
            if (saves == null || Plugin.LoadedMods == null || ModPlayModeSession.Selected == ModPlayModeKind.Original) return;
            foreach (ModPackage package in Plugin.LoadedMods)
            {
                if (!ModPlayModeSession.Allows(package)) continue;
                bool owns = package.PlayMode == "addon" ? ModSaveSlotPolicy.IsAddonSlot(saves.CurrentSlot)
                    : CampaignIdentity.OwnsSlot(package.CampaignId, saves.CurrentSlot);
                if (owns && HasCharacters(package)) result.Add(package);
            }
            result.Sort((left, right) => StringComparer.Ordinal.Compare(left.Id, right.Id));
        }

        private static bool HasCharacters(ModPackage package)
        {
            foreach (UserContent content in package.UserContents.Values)
                if (content.Type == "character" && string.IsNullOrEmpty(content.Extends)) return true;
            return false;
        }

        internal static bool IsCustomData(RelationshipStat data) { return data != null && Entries.ContainsKey(data); }

        internal static void Prepare(SocialStatPanel panel)
        {
            CustomSocialPanelController controller = panel.GetComponent<CustomSocialPanelController>();
            if (controller == null) controller = panel.gameObject.AddComponent<CustomSocialPanelController>();
            controller.Initialize(panel);
            controller.Refresh(false);
        }

        internal static bool Select(SocialCharacterButton button, bool active)
        {
            Entry entry;
            if (button.Data == null || !Entries.TryGetValue(button.Data, out entry)) return false;
            if (active) entry.Panel.CharacterButtonOnActive(entry.Data);
            return true;
        }

        internal static bool Render(SocialStatPanel panel, bool resetScroll)
        {
            var fields = Traverse.Create(panel);
            Entry entry;
            var data = fields.Field("_currentData").GetValue<RelationshipStat>();
            if (data == null || !Entries.TryGetValue(data, out entry)) {
                var controller=panel.GetComponent<CustomSocialPanelController>();
                if(controller!=null)controller.RestoreNativeDetails();
                return false;
            }
            panel.GetComponent<CustomSocialPanelController>().MarkCustomDetails();
            int value,pages;bool known;
            ReadState(entry,out value,out known,out pages);
            RefreshButton(entry,value,known,pages);
            fields.Field("_character").GetValue<GameObject>().SetActive(true);
            var avatar=fields.Field("_avatarImage").GetValue<Image>();
            avatar.sprite = CustomAffinityRuntime.Portrait(entry.Content);
            avatar.color=known ? Color.white : Color.black;
            string name, title;
            if (!PersistentModState.TryGetCharacterName(entry.Package, "user:" + entry.Content.Id, out name)) name = entry.Content.Name;
            if (!PersistentModState.TryGetCharacterTitle(entry.Package, "user:" + entry.Content.Id, out title)) title = entry.Content.Title;
            fields.Field("_nameText").GetValue<Text>().text = known ? name ?? "" : "？？？";
            fields.Field("_titleText").GetValue<Text>().text = known ? title ?? "" : "";
            fields.Field("_levelText").GetValue<Text>().text = known ? (value / 10).ToString(CultureInfo.InvariantCulture) : "？";
            fields.Field("_progressBar").GetValue<Image>().fillAmount = !known ? 0f : value == 100 ? 1f : value % 10 / 10f;
            var toggles = fields.Field("_characterInfoToggles").GetValue<SocialCharacterInfoToggle[]>();
            foreach(var toggle in toggles) {
                int page=Traverse.Create(toggle).Field("_index").GetValue<int>();
                toggle.gameObject.SetActive(page>=1 && page<=entry.Content.BiographyPages.Count);
                toggle.SetInteractable(page>=1 && page<=CharacterProfilePolicy.MaxPages && (pages&CharacterProfilePolicy.PageBit(page))!=0);
            }
            if (resetScroll)
            {
                fields.Field("_currentInfoIndex").SetValue(0);
                var buttons = fields.Field("_infoButtons").GetValue<Toggle[]>();
                if (buttons.Length > 0 && entry.Content.BiographyPages.Count>0) buttons[0].isOn = true;
                fields.Field("_descScrollView").GetValue<ScrollRect>().verticalNormalizedPosition = 1f;
            }
            RenderBiography(panel,entry,fields.Field("_currentInfoIndex").GetValue<int>()+1,value);
            return true;
        }

        private static void ReadState(Entry entry,out int value,out bool known,out int pages)
        {
            int unlocked,locked;
            PersistentModState.ReadCharacterProfile(entry.Package,entry.Reference,entry.Content,out value,out known,out unlocked,out locked);
            pages=0;
            for(int index=0;index<entry.Content.BiographyPages.Count;index++) {
                int bit=1<<index;
                if(CharacterProfilePolicy.CanRead(known,value,entry.Content.BiographyPages[index].UnlockLevel,
                    (locked&bit)!=0 ? -1 : (unlocked&bit)!=0 ? 1 : 0))pages|=bit;
            }
        }

        private static void RefreshButton(Entry entry,int value,bool known,int pages)
        {
            entry.LastValue=value;entry.LastKnown=known;entry.LastPages=pages;
            entry.Data.SetValue(value);entry.Data.SetActive(known);
            entry.Button.InitData();
            var buttonFields=Traverse.Create(entry.Button);
            buttonFields.Field("_menuToggleButton").GetValue<MenuToggleButton>().Toggle(
                known && buttonFields.Field("_toggleButton").GetValue<Toggle>().isOn);
            var icons=Traverse.Create(entry.Button).Field("_icons").GetValue<Image[]>();
            for(int i=0;i<icons.Length;i++)icons[i].color=known ? entry.IconColors[i] : Color.black;
        }

        private static void RenderBiography(SocialStatPanel panel,Entry entry,int page,int value)
        {
            bool readable=CustomProfileRuntime.PageUnlocked(entry.Package,entry.Content,page,value);
            string text=readable ? entry.Content.BiographyPages[page-1].Text : entry.Content.BiographyPages.Count>0 ? "？？？" : "";
            Traverse.Create(panel).Field("_introText").GetValue<Text>().text=text;
        }

        internal static bool Biography(SocialStatPanel panel,int page)
        {
            var fields = Traverse.Create(panel);
            Entry entry;
            var data = fields.Field("_currentData").GetValue<RelationshipStat>();
            if (data == null || !Entries.TryGetValue(data, out entry)) return false;
            fields.Field("_currentInfoIndex").SetValue(page-1);
            RenderBiography(panel,entry,page,CustomAffinityRuntime.Value(entry.Package,"user:"+entry.Content.Id));
            fields.Field("_descScrollView").GetValue<ScrollRect>().verticalNormalizedPosition = 1f;
            return true;
        }

        internal static void EnableSocialButton(PlayerStatusPanel panel)
        {
            if (AllowedPackages().Count == 0) return;
            var button = Traverse.Create(panel).Field("_socialButton").GetValue<MenuToggleButton>();
            button.SetActive(true);
            button.Init();
            button.GetComponent<Toggle>().interactable = true;
        }

        internal static void Clear()
        {
            foreach (CustomSocialPanelController panel in new List<CustomSocialPanelController>(Panels))
                if (panel != null) panel.ClearEntries(true);
        }

        internal sealed class CustomSocialPanelController : MonoBehaviour
        {
            private SocialStatPanel _panel;
            private SocialCharacterButton[] _original;
            private readonly List<Entry> _entries = new List<Entry>();
            private List<ModPackage> _packages = new List<ModPackage>();
            private readonly List<ModPackage> _candidates = new List<ModPackage>();
            private string _slot;
            private Color _nativeAvatarColor;
            private bool[] _nativePageActive;
            private bool _customDetails;

            internal void Initialize(SocialStatPanel panel)
            {
                if (_panel != null) return;
                _panel = panel;
                _original = Traverse.Create(panel).Field("_characterButtons").GetValue<SocialCharacterButton[]>();
                _nativeAvatarColor=Traverse.Create(panel).Field("_avatarImage").GetValue<Image>().color;
                _nativePageActive=Traverse.Create(panel).Field("_characterInfoToggles").GetValue<SocialCharacterInfoToggle[]>()
                    .Select(toggle=>toggle.gameObject.activeSelf).ToArray();
                Panels.Add(this);
            }

            internal void MarkCustomDetails(){_customDetails=true;}
            internal void RestoreNativeDetails()
            {
                if(!_customDetails || _panel==null)return;
                var fields=Traverse.Create(_panel);
                fields.Field("_avatarImage").GetValue<Image>().color=_nativeAvatarColor;
                var toggles=fields.Field("_characterInfoToggles").GetValue<SocialCharacterInfoToggle[]>();
                for(int i=0;i<toggles.Length;i++)toggles[i].gameObject.SetActive(_nativePageActive[i]);
                _customDetails=false;
            }

            internal void Rebuild(List<ModPackage> packages)
            {
                ClearEntries(false);
                _packages.AddRange(packages);
                _slot = SaveSystem.Instance != null ? SaveSystem.Instance.CurrentSlot : null;
                var all = new List<SocialCharacterButton>(_original);
                if (packages.Count > 0 && _original.Length == 0)
                    throw new InvalidOperationException("原版人物状态页缺少可克隆的人物按钮");
                foreach (ModPackage package in packages)
                foreach (UserContent content in package.UserContents.Values.OrderBy(content => content.Id, StringComparer.Ordinal))
                {
                    if (content.Type != "character" || !string.IsNullOrEmpty(content.Extends)) continue;
                    var original = _original[0];
                    var root = UnityEngine.Object.Instantiate(original.gameObject, original.transform.parent, false);
                    root.name = "lom_social_" + package.Id + "_" + content.Id;
                    var button = root.GetComponent<SocialCharacterButton>();
                    var data = ScriptableObject.CreateInstance<RelationshipStat>();
                    data.name = root.name;
                    int value = CustomAffinityRuntime.Value(package, "user:" + content.Id);
                    data.SetValue(value);
                    var fields = Traverse.Create(data);
                    fields.Field("_headIcon").SetValue(CustomAffinityRuntime.Avatar(content));
                    fields.Field("_icon").SetValue(CustomAffinityRuntime.Avatar(content));
                    Traverse.Create(button).Field("_data").SetValue(data);
                    var icons=Traverse.Create(button).Field("_icons").GetValue<Image[]>();
                    var entry = new Entry { Package = package, Content = content, Reference="user:"+content.Id, Panel = _panel, Button = button, Data = data,
                        IconColors=icons.Select(icon=>icon.color).ToArray() };
                    Entries.Add(data, entry);
                    _entries.Add(entry);
                    all.Add(button);
                    root.SetActive(true);
                    int pages;bool known;
                    ReadState(entry,out value,out known,out pages);
                    RefreshButton(entry,value,known,pages);
                }
                Traverse.Create(_panel).Field("_characterButtons").SetValue(all.ToArray());
                if(!all.Exists(button=>button.Active))Traverse.Create(_panel).Field("_character").GetValue<GameObject>().SetActive(false);
            }

            internal void ClearEntries(bool selectOfficial)
            {
                if (_panel == null) return;
                var fields = Traverse.Create(_panel);
                var current = fields.Field("_currentData").GetValue<RelationshipStat>();
                bool selectedCustom = current != null && Entries.ContainsKey(current);
                RestoreNativeDetails();
                if (selectedCustom) fields.Field("_currentData").SetValue(null);
                fields.Field("_characterButtons").SetValue(_original);
                foreach (Entry entry in _entries)
                {
                    Entries.Remove(entry.Data);
                    if (entry.Button != null)
                    {
                        entry.Button.gameObject.SetActive(false);
                        UnityEngine.Object.Destroy(entry.Button.gameObject);
                    }
                    if (entry.Data != null) UnityEngine.Object.Destroy(entry.Data);
                }
                _entries.Clear();
                _packages.Clear();
                if (selectedCustom)
                {
                    fields.Field("_character").GetValue<GameObject>().SetActive(false);
                    if (selectOfficial)
                        foreach (SocialCharacterButton button in _original)
                            if (button != null && button.Active)
                            {
                                button.ToggleActive();
                                _panel.CharacterButtonOnActive(button.Data);
                                break;
                            }
                }
            }

            private void LateUpdate()
            {
                Refresh(true);
            }

            internal void Refresh(bool updateStatus)
            {
                if (_panel == null) return;
                CollectAllowedPackages(_candidates);
                bool same = SaveSystem.Instance != null && SaveSystem.Instance.CurrentSlot == _slot && _candidates.Count == _packages.Count;
                if (same)
                    for (int i = 0; i < _candidates.Count; i++) if (!ReferenceEquals(_candidates[i], _packages[i])) { same = false; break; }
                if (!same)
                {
                    Rebuild(_candidates);
                    var status = _panel.GetComponentInParent<PlayerStatusPanel>(true);
                    if (updateStatus && status != null) AccessTools.Method(typeof(PlayerStatusPanel), "SetSocialButtonActive").Invoke(status, null);
                }
                foreach(Entry entry in _entries) {
                    int value,pages;bool known;
                    ReadState(entry,out value,out known,out pages);
                    if(value==entry.LastValue && known==entry.LastKnown && pages==entry.LastPages)continue;
                    RefreshButton(entry,value,known,pages);
                    var current = Traverse.Create(_panel).Field("_currentData").GetValue<RelationshipStat>();
                    if(current==entry.Data)Render(_panel,false);
                }
            }

            private void OnDestroy()
            {
                ClearEntries(false);
                Panels.Remove(this);
            }
        }
    }

    [HarmonyPatch(typeof(SocialStatPanel), "OnPanelOpen")]
    internal static class CustomSocialOpenPatch
    {
        private static void Prefix(SocialStatPanel __instance) { CustomSocialRuntime.Prepare(__instance); }
    }

    [HarmonyPatch(typeof(SocialCharacterButton), "RaiseActiveEvent")]
    internal static class CustomSocialSelectPatch
    {
        private static bool Prefix(SocialCharacterButton __instance, bool active) { return !CustomSocialRuntime.Select(__instance, active); }
    }

    [HarmonyPatch(typeof(SocialStatPanel), "UpdateInfo")]
    internal static class CustomSocialInfoPatch
    {
        private static bool Prefix(SocialStatPanel __instance) { return !CustomSocialRuntime.Render(__instance, true); }
    }

    [HarmonyPatch(typeof(SocialStatPanel), "CharacterInfoOnActive")]
    internal static class CustomSocialBiographyPatch
    {
        private static bool Prefix(SocialStatPanel __instance,int index) { return !CustomSocialRuntime.Biography(__instance,index); }
    }

    [HarmonyPatch(typeof(PlayerStatusPanel), "SetSocialButtonActive")]
    internal static class CustomSocialAccessPatch
    {
        private static void Postfix(PlayerStatusPanel __instance) { CustomSocialRuntime.EnableSocialButton(__instance); }
    }
}
