using System;
using System.Collections.Generic;
using System.IO;

namespace MortalModHost
{
    internal static class CharacterProfileTests
    {
        private static int _checks;
        private static void Check(bool value,string message){_checks++;if(!value)throw new Exception("Character profile: "+message);}
        private static void Reject(Action action,string message){try{action();}catch(ArgumentException){_checks++;return;}catch(FormatException){_checks++;return;}throw new Exception("Expected profile rejection: "+message);}
        private static UserContent Parse(string json,string legacy=null)
        {
            var content=new UserContent {Biography=legacy};
            CharacterProfilePolicy.ReadDefinition((Dictionary<string,object>)MiniJson.Parse(json),content);
            return content;
        }
        internal static void Run(string root)
        {
            var legacy=Parse("{}","旧稿原样\n保留");
            Check(!legacy.InitiallyKnown && legacy.BiographyPages.Count==1 && legacy.BiographyPages[0].UnlockLevel==0
                && legacy.BiographyPages[0].Text=="旧稿原样\n保留","legacy biography remains a level-zero page, knowledge is separate");
            var definition=Parse("{\"initially_known\":true,\"biography_pages\":[{\"text\":\"重文\",\"unlock_level\":0},{\"text\":\"重文\",\"unlock_level\":4},{\"text\":\"\",\"unlock_level\":7},{\"text\":\"  终篇  \",\"unlock_level\":10}]}");
            Check(definition.InitiallyKnown && definition.BiographyPages.Count==4 && definition.BiographyPages[2].Text==""
                && definition.BiographyPages[3].Text=="  终篇  ","four ordered pages preserve duplicates, empty drafts and whitespace");
            Check(Parse("{\"biography_pages\":[]}").BiographyPages.Count==0,"empty page array is a valid draft");
            var whitespace=Parse("{\"biography_pages\":[{\"text\":\"new page\",\"unlock_level\":0}]}" ," \r\n\t ");
            Check(whitespace.Biography==" \r\n\t " && whitespace.BiographyPages.Count==1 && whitespace.BiographyPages[0].Text=="new page",
                "whitespace-only legacy draft does not conflict with new pages or rewrite either manuscript");
            Reject(()=>Parse("{\"initially_known\":1}"),"nonboolean knowledge");
            Reject(()=>Parse("{\"biography_pages\":[{\"text\":\"x\",\"unlock_level\":4.5}]}"),"fractional level");
            Reject(()=>Parse("{\"biography_pages\":[{\"text\":\"x\",\"unlock_level\":11}]}"),"level above ten");
            Reject(()=>Parse("{\"biography_pages\":[{\"text\":\"x\"}]}"),"missing threshold");
            Reject(()=>Parse("{\"biography_pages\":[{\"text\":\"x\",\"unlock_level\":0}]}" ,"old"),"conflicting old and new manuscripts");
            Check(!CharacterProfilePolicy.CanRead(false,100,0,1),"manual page unlock does not implicitly meet a person");
            Check(!CharacterProfilePolicy.CanRead(true,39,4,0) && CharacterProfilePolicy.CanRead(true,40,4,0),"native level threshold boundary");
            Check(CharacterProfilePolicy.CanRead(true,0,10,1) && !CharacterProfilePolicy.CanRead(true,100,0,-1),"manual unlock and lock override affinity rules");
            Reject(()=>CharacterProfilePolicy.PageBit(0),"zero page");
            Reject(()=>CharacterProfilePolicy.PageBit(5),"fifth page");

            string folder=Path.Combine(root,"character-profiles");
            var first=new ModPackage {Id="profile_one",CampaignId="profile_one",PlayMode="addon",PackageFingerprint=new string('A',64)};
            var second=new ModPackage {Id="profile_two",CampaignId="profile_two",PlayMode="addon",PackageFingerprint=new string('B',64)};
            string slot=ModSaveSlotPolicy.AddonManualSlot(1),copy=ModSaveSlotPolicy.AddonManualSlot(2),automatic=ModSaveSlotPolicy.AddonAutoSlot("auto");
            const string actor="user:demo.person";
            var store=new PersistentStateStore(folder);
            store.SetCharacterAffinity(first,slot,actor,61);store.Flush();
            store=new PersistentStateStore(folder);
            Check(!store.CharacterKnown(first,slot,actor,false) && store.CharacterKnown(first,slot,actor,true)
                && store.BiographyOverride(first,slot,actor,1)==0,"existing sidecar without profile fields uses metadata defaults");
            store.SetCharacterKnown(first,slot,actor,true);store.SetBiographyOverride(first,slot,actor,4,true);store.SetBiographyOverride(first,slot,actor,1,false);store.Flush();
            store=new PersistentStateStore(folder);
            Check(store.CharacterKnown(first,slot,actor,false) && store.BiographyOverride(first,slot,actor,4)==1
                && store.BiographyOverride(first,slot,actor,1)==-1,"knowledge and both manual decisions persist on disk");
            int affinity,unlocked,locked;bool known;
            store.ReadCharacterProfile(first,slot,actor,0,false,out affinity,out known,out unlocked,out locked);
            Check(affinity==61 && known && unlocked==8 && locked==1,"combined UI read restores affinity, recognition and both page decisions from one slot binding");
            store.SetCharacterKnown(first,slot,actor,false);
            Check(!store.CharacterKnown(first,slot,actor,true) && store.CharacterAffinity(first,slot,actor,0)==61
                && store.BiographyOverride(first,slot,actor,4)==1,"forget hides the person without erasing affinity or unlocked pages");
            store.SetCharacterKnown(first,slot,actor,true);store.SetBiographyOverride(first,slot,actor,1,true);
            Check(store.CharacterKnown(first,slot,actor,false) && store.BiographyOverride(first,slot,actor,1)==1,"meet restores knowledge and unlock replaces a prior forced lock");
            store.SetBiographyOverride(first,slot,actor,4,false);
            Check(store.BiographyOverride(first,slot,actor,4)==-1,"forced lock replaces an explicit unlock");
            store.SaveToSlot(first,copy);store.SaveToSlot(first,automatic);
            Check(new PersistentStateStore(folder).BiographyOverride(first,slot,actor,4)==1
                && new PersistentStateStore(folder).BiographyOverride(first,copy,actor,4)==-1
                && new PersistentStateStore(folder).BiographyOverride(first,automatic,actor,4)==-1,"save-as and autosave snapshot live decisions without changing old manual save");
            var foreign=new PersistentStateStore(folder);
            Check(!foreign.CharacterKnown(second,slot,actor,false) && foreign.BiographyOverride(second,slot,actor,4)==0,"same character ID in another package is isolated");
            Reject(()=>store.SetCharacterKnown(first,slot,"player",true),"official identity must not be changed");
            store.BeginNewCampaign(first,slot);
            Check(!store.CharacterKnown(first,slot,actor,false) && store.BiographyOverride(first,slot,actor,4)==0
                && store.CharacterAffinity(first,slot,actor,0)==0,"new campaign clears all owned profile and affinity state");
            Console.WriteLine("PASS character profile: "+_checks+" metadata, knowledge, level/manual gates and isolated save assertions (offline)");
        }
    }
}
