using System;
using System.Collections.Generic;

namespace MortalModHost
{
    internal static class CombatIdentityPolicyTests
    {
        private static int _assertions;
        private static CombatIdentityCandidate C(string name,string portrait,string frames)
        {return new CombatIdentityCandidate(name,portrait,frames+"/idle",frames+"/attack",frames+"/hurt",frames+"/defence");}
        private static void Check(bool ok,string message){_assertions++;if(!ok)throw new Exception("Combat identity: "+message);}
        private static void Ambiguous(string character,string portrait,params CombatIdentityCandidate[] rows)
        {
            try{CombatIdentityPolicy.Select(character,portrait,rows);}
            catch(InvalidOperationException error){Check(error.Message=="官方人物存在多套不同 Combat 动画，无法唯一绑定："+character,"concrete ambiguity diagnostic");return;}
            throw new Exception("Expected combat identity ambiguity");
        }
        internal static void Run()
        {
            _assertions=0;
            var normal=C("girl5","normal","mei");var demonic=C("girl5","demonic","demonic");
            Check(CombatIdentityPolicy.Select("girl5","normal",new[]{demonic,normal,normal})==1,"normal appearance wins over earlier same-name variant");
            Check(CombatIdentityPolicy.Select("girl5","demonic",new[]{normal,demonic})==1,"same-name alternate appearance");
            Check(CombatIdentityPolicy.Select("girl5_9","demonic",new[]{normal,demonic})==1,"actual portrait alias without invented id trimming");
            Check(CombatIdentityPolicy.Select("girl5_4","normal",new[]{demonic,normal})==1,"Story alias shares exact normal portrait");
            Check(CombatIdentityPolicy.Select("girl5","normal",new[]{demonic,C("girl5","other","other"),normal})==2,"lower-tier ambiguity cannot preempt later exact match");
            Ambiguous("girl5","",normal,demonic);
            Ambiguous("girl5","missing",normal,demonic);
            Ambiguous("girl5","normal",normal,C("girl5","normal","different"));
            Ambiguous("girl5_9","demonic",demonic,C("different_name","demonic","different"));
            Check(CombatIdentityPolicy.Select("girl5","",new[]{normal,normal})==0,"unchanged unique-name legacy resolution");
            Check(CombatIdentityPolicy.Select("girl5","missing",new[]{normal})==0,"unique name remains usable when capture lacks matching art");
            Check(CombatIdentityPolicy.Select("girl5","normal",new[]{C("other","normal","other"),C("girl5","different","mei")})==1,"exact official name precedes cross-name portrait aliases");
            foreach(string character in new[]{"girl4","girl6","girl7"}){
                var rows=new[]{C(character,"dress",character+"_dress"),C(character,"normal",character)};
                Check(CombatIdentityPolicy.Select(character,"normal",rows)==1,"adjacent female normal "+character);
                Check(CombatIdentityPolicy.Select(character+"_appearance","dress",rows)==0,"adjacent female actual alias "+character);
            }
            Check(CombatIdentityPolicy.Select("girl1","portrait",new[]{normal,demonic})==-1,"no combat identity retains Story portrait fallback");
            Check(CombatIdentityPolicy.Select("girl2","portrait",new[]{normal,demonic})==-1,"another unmapped character is not hard-bound to girl5");
            Check(CombatIdentityPolicy.Select("user:demo.enemy","normal",new[]{normal,C("user:demo.enemy","normal","custom")})==-1,"custom characters never select official identity");
            Check(CombatIdentityPolicy.Select("Girl5","",new[]{normal})==-1,"official names are ordinal");
            Check(CombatIdentityPolicy.Select("girl5_9","DEMONIC",new[]{demonic})==-1,"resource addresses are ordinal");
            var incomplete=new CombatIdentityCandidate("girl5","normal","","a","h","d");
            Check(CombatIdentityPolicy.Select("girl5","normal",new[]{incomplete})==-1,"incomplete frames are not a playable identity");
            Check(CombatIdentityPolicy.Select("girl5","normal",new[]{incomplete,demonic})==-1,"broken preferred appearance does not select a different form");
            var frames=CombatCharacterPolicy.ResolveFrames(null,"normal.png");
            Check(frames["idle"]=="normal.png" && frames["attack"]=="normal.png" && frames["hurt"]=="normal.png" && frames["defence"]=="normal.png","custom frame fallback unchanged");
            frames=CombatCharacterPolicy.ResolveFrames(new Dictionary<string,string>{{"attack","attack.png"}},"normal.png");
            Check(frames["idle"]=="normal.png" && frames["attack"]=="attack.png","custom explicit frame retained");
            Console.WriteLine("CombatIdentityPolicyTests PASS: "+_assertions+" assertions; no fixed CL/stat preset");
        }
    }
}
