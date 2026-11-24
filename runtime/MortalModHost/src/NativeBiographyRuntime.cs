using HarmonyLib;
using Mortal.Core;
using OBB.Framework.Extensions;
using UnityEngine.UI;

namespace MortalModHost
{
    // Authored pages can use an existing official relationship without replacing its data.
    internal static class NativeBiographyRuntime
    {
        private static UserContent Find(Traverse fields, out RelationshipStat relationship)
        {
            relationship = fields.Field("_currentData").GetValue<RelationshipStat>();
            if (relationship == null || CustomSocialRuntime.IsCustomData(relationship)) return null;
            string id = relationship.Type.GetStringValue();
            foreach (ModPackage package in CustomSocialRuntime.AllowedPackages())
                if (package.CharacterExtensions.TryGetValue(id, out var content)
                    && content.HasProfile && content.BiographyPages.Count > 0)
                    return content;
            return null;
        }

        internal static bool Render(SocialStatPanel panel, int page)
        {
            var fields = Traverse.Create(panel);
            var content = Find(fields, out var relationship);
            if (content == null || page < 1 || page > content.BiographyPages.Count) return false;
            var biography = content.BiographyPages[page - 1];
            bool readable = CharacterProfilePolicy.CanRead(
                relationship.Active, relationship.Value, biography.UnlockLevel, 0);
            fields.Field("_introText").GetValue<Text>().text = readable ? biography.Text : "？？？";
            fields.Field("_descScrollView").GetValue<ScrollRect>().verticalNormalizedPosition = 1f;
            return true;
        }

        internal static void UpdatePages(SocialStatPanel panel)
        {
            var fields = Traverse.Create(panel);
            var content = Find(fields, out var relationship);
            if (content == null) return;
            foreach (var toggle in fields.Field("_characterInfoToggles").GetValue<SocialCharacterInfoToggle[]>())
            {
                int page = Traverse.Create(toggle).Field("_index").GetValue<int>();
                if (page < 1 || page > content.BiographyPages.Count) continue;
                bool readable = CharacterProfilePolicy.CanRead(
                    relationship.Active, relationship.Value, content.BiographyPages[page - 1].UnlockLevel, 0);
                toggle.gameObject.SetActive(true);
                toggle.SetInteractable(readable);
            }
        }
    }

    [HarmonyPatch(typeof(SocialStatPanel), "UpdateCharacterInfoToggle")]
    internal static class NativeBiographyPagesPatch
    {
        private static void Postfix(SocialStatPanel __instance) { NativeBiographyRuntime.UpdatePages(__instance); }
    }
}
