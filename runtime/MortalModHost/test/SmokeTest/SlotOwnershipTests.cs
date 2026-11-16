using System;
using System.Collections.Generic;
using System.Globalization;

namespace MortalModHost
{
    internal static class SlotOwnershipTests
    {
        private static int _checks;
        private static void Check(bool value,string label)
        {
            _checks++;
            if(!value)throw new InvalidOperationException("slot ownership: "+label);
        }

        internal static int Run()
        {
            _checks=0;
            foreach(string campaign in new[]{null,"","alpha","alpha_s002","alpha_auto","alpha_auto_free","alpha_s+02",new string('a',64),new string('a',65),"Alpha","alpha\n","alpha\r\n","alpha\0","阿尔法"})
            {
                foreach(string slot in Corpus(campaign))
                {
                    Compare(campaign,slot);
                }
            }
            var random=new Random(190622);
            const string alphabet="ab_s-002019+ auto\n\r\0Ａ٢";
            for(int i=0;i<2000;i++)
            {
                string campaign=i%2==0 ? "alpha" : "alpha_s002";
                string root=i%3==0 ? "mod_addon_default" : "mod_campaign_"+campaign;
                var suffix=new char[random.Next(0,17)];
                for(int j=0;j<suffix.Length;j++)suffix[j]=alphabet[random.Next(alphabet.Length)];
                Compare(campaign,root+new string(suffix));
            }
            // TryParseSlot deliberately keeps its released, broader integer syntax.
            // Ownership of a known campaign still accepts only generated slot names.
            foreach(string suffix in new[]{"_s+02","_s 02","_s02 "})
            {
                string parsed;
                Check(CampaignIdentity.TryParseSlot("mod_campaign_alpha"+suffix,out parsed)&&parsed=="alpha","historical signed/space integer parser "+suffix);
                Check(!CampaignIdentity.OwnsSlot("alpha","mod_campaign_alpha"+suffix),"generated manual suffix remains exact "+suffix);
            }
            string unchanged;
            Check(!CampaignIdentity.TryParseSlot("mod_campaign_alpha\n",out unchanged),"campaign LF remains invalid");
            Check(ModSaveSlotPolicy.IsModSlot("mod_campaign_alpha\n"),"broad namespace classifier is not tightened");
            var previous=CultureInfo.CurrentCulture;
            try
            {
                foreach(string culture in new[]{"en-US","tr-TR","ar-SA"})
                {
                    CultureInfo.CurrentCulture=new CultureInfo(culture);
                    foreach(string slot in new[]{"mod_campaign_alpha_s002","mod_campaign_alpha_s020","mod_campaign_alpha_s019\n","mod_addon_default_auto_free","MOD_addon_default_s002"})Compare("alpha",slot);
                }
            }
            finally { CultureInfo.CurrentCulture=previous; }
            return _checks;
        }

        private static IEnumerable<string> Corpus(string campaign)
        {
            yield return null;yield return "";yield return "001";yield return "auto";yield return "mod_campaign_";yield return "mod_addon_";
            foreach(string root in new[]{"mod_campaign_"+(campaign??""),"mod_addon_default"})
            {
                foreach(string suffix in new[]{"","_auto","_auto_free","_auto_battle","_AUTO","_auto_free\n","_auto_battle\0","_s001","_s000","_s021","_s002\n","_s020\r\n","_s02","_s0002","_s+02","_s 02","_s02 ","_s٠٠٢","_s００２","__s002"})yield return root+suffix;
                for(int index=0;index<=25;index++)yield return root+"_s"+index.ToString("000",CultureInfo.InvariantCulture);
                string valid=root+"_s020";
                // Exhaust each ASCII unit at every suffix position, including controls.
                for(int at=root.Length;at<valid.Length;at++)
                    for(char unit=(char)0;unit<128;unit++)
                    {
                        char[] text=valid.ToCharArray();text[at]=unit;yield return new string(text);
                    }
            }
        }

        private static void Compare(string campaign,string slot)
        {
            Check(CampaignIdentity.OwnsSlot(campaign,slot)==OldCampaignOwns(campaign,slot),"campaign ordinal suffix");
            Check(ModSaveSlotPolicy.IsAddonSlot(slot)==OldAddonSlot(slot),"addon ordinal suffix");
            Check(ModSaveSlotPolicy.IsAddonManualSlot(slot)==OldAddonManual(slot),"addon manual suffix");
            foreach(bool addon in new[]{false,true})foreach(bool auto in new[]{false,true})
                Check(ModSaveSlotPolicy.OwnsBrowserSlot(campaign,addon,slot,auto)==OldBrowser(campaign,addon,slot,auto),"browser manual/auto ownership");
            Check(ModSaveSlotPolicy.IsIsolatedAutoSlotForCampaign(campaign,slot)==OldCampaignAuto(campaign,slot),"campaign automatic suffix");
        }

        // Frozen pre-optimization decisions: builders and TryParseSlot are unchanged.
        internal static bool OldCampaignOwns(string campaignId,string slot)
        {
            if(!CampaignIdentity.IsValid(campaignId)||string.IsNullOrEmpty(slot))return false;
            string root=CampaignIdentity.SaveSlot(campaignId);
            if(string.Equals(slot,root,StringComparison.Ordinal))return true;
            if(string.Equals(slot,root+"_auto",StringComparison.Ordinal)||string.Equals(slot,root+"_auto_free",StringComparison.Ordinal)||string.Equals(slot,root+"_auto_battle",StringComparison.Ordinal))return true;
            for(int index=2;index<=20;index++)if(string.Equals(slot,root+"_s"+index.ToString("000"),StringComparison.Ordinal))return true;
            return false;
        }
        internal static bool OldAddonSlot(string slot)
        {
            if(string.IsNullOrEmpty(slot))return false;
            for(int i=1;i<=ModSaveSlotPolicy.OfficialManualCount;i++)if(string.Equals(slot,ModSaveSlotPolicy.AddonManualSlot(i),StringComparison.Ordinal))return true;
            return slot==ModSaveSlotPolicy.AddonAutoSlot("auto")||slot==ModSaveSlotPolicy.AddonAutoSlot("auto_free")||slot==ModSaveSlotPolicy.AddonAutoSlot("auto_battle");
        }
        internal static bool OldAddonManual(string slot)
        {
            for(int i=1;i<=ModSaveSlotPolicy.OfficialManualCount;i++)if(slot==ModSaveSlotPolicy.AddonManualSlot(i))return true;
            return false;
        }
        private static bool OldBrowser(string campaignId,bool addon,string slot,bool allowAuto)
        {
            if(string.IsNullOrEmpty(slot)||!addon&&!CampaignIdentity.IsValid(campaignId))return false;
            for(int i=1;i<=ModSaveSlotPolicy.OfficialManualCount;i++)if(slot==(addon?ModSaveSlotPolicy.AddonManualSlot(i):ModSaveSlotPolicy.IsolatedManualSlot(campaignId,i)))return true;
            if(!allowAuto)return false;
            foreach(string kind in new[]{"auto","auto_free","auto_battle"})if(slot==(addon?ModSaveSlotPolicy.AddonAutoSlot(kind):ModSaveSlotPolicy.IsolatedAutoSlotForCampaign(campaignId,kind)))return true;
            return false;
        }
        private static bool OldCampaignAuto(string campaignId,string slot)
        {
            if(!CampaignIdentity.IsValid(campaignId)||string.IsNullOrEmpty(slot))return false;
            return string.Equals(slot,ModSaveSlotPolicy.IsolatedAutoSlotForCampaign(campaignId,"auto"),StringComparison.Ordinal)
                ||string.Equals(slot,ModSaveSlotPolicy.IsolatedAutoSlotForCampaign(campaignId,"auto_free"),StringComparison.Ordinal)
                ||string.Equals(slot,ModSaveSlotPolicy.IsolatedAutoSlotForCampaign(campaignId,"auto_battle"),StringComparison.Ordinal);
        }
    }
}
