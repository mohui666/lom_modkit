using System;
using System.Collections.Generic;

namespace MortalModHost
{
    internal struct CombatIdentityCandidate
    {
        internal readonly string Name,Portrait,Idle,Attack,Hurt,Defence;
        internal CombatIdentityCandidate(string name,string portrait,string idle,string attack,string hurt,string defence)
        {Name=name;Portrait=portrait;Idle=idle;Attack=attack;Hurt=hurt;Defence=defence;}
        internal bool HasFrames {get{return !string.IsNullOrEmpty(Name) && !string.IsNullOrEmpty(Idle)
            && !string.IsNullOrEmpty(Attack) && !string.IsNullOrEmpty(Hurt) && !string.IsNullOrEmpty(Defence);}}
    }

    // Story normal and CombatStat.AvatarAddressKey are explicit references to
    // the same official portrait. Asset folder names and numeric CL presets
    // are not character identities.
    internal static class CombatIdentityPolicy
    {
        private static int Rank(string character,string portrait,CombatIdentityCandidate candidate)
        {
            if(string.IsNullOrEmpty(candidate.Name))return 3;
            bool name=string.Equals(character,candidate.Name,StringComparison.Ordinal);
            bool image=!string.IsNullOrEmpty(portrait) && string.Equals(portrait,candidate.Portrait,StringComparison.Ordinal);
            return name?(image?0:1):(image?2:3);
        }
        internal static int Select(string character,string portrait,IList<CombatIdentityCandidate> candidates)
        {
            if(candidates==null)throw new ArgumentNullException("candidates");
            if(string.IsNullOrEmpty(character) || CombatCharacterPolicy.IsUserCharacter(character))return -1;
            int rank=3;
            for(int i=0;i<candidates.Count;++i)rank=Math.Min(rank,Rank(character,portrait,candidates[i]));
            if(rank==3)return -1;
            int selected=-1;
            // First choose the best identity tier, then check all four frames.
            // An earlier ambiguous lower tier must not defeat a later exact
            // Story/Combat portrait match.
            for(int i=0;i<candidates.Count;++i) {
                if(!candidates[i].HasFrames || Rank(character,portrait,candidates[i])!=rank)continue;
                if(selected<0)selected=i;
                else if(!SameFrames(candidates[selected],candidates[i]))
                    throw new InvalidOperationException("官方人物存在多套不同 Combat 动画，无法唯一绑定："+character);
            }
            return selected;
        }
        private static bool SameFrames(CombatIdentityCandidate a,CombatIdentityCandidate b)
        {
            return string.Equals(a.Idle,b.Idle,StringComparison.Ordinal)
                && string.Equals(a.Attack,b.Attack,StringComparison.Ordinal)
                && string.Equals(a.Hurt,b.Hurt,StringComparison.Ordinal)
                && string.Equals(a.Defence,b.Defence,StringComparison.Ordinal);
        }
    }
}
