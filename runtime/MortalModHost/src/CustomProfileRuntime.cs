using System;

namespace MortalModHost
{
    internal static class CustomProfileRuntime
    {
        internal static bool Known(ModPackage package,UserContent content)
        {
            return PersistentModState.CharacterKnown(package,"user:"+content.Id,content.InitiallyKnown);
        }
        internal static bool PageUnlocked(ModPackage package,UserContent content,int page,int affinity)
        {
            if(page<1 || page>content.BiographyPages.Count)return false;
            string character="user:"+content.Id;
            return CharacterProfilePolicy.CanRead(Known(package,content),affinity,content.BiographyPages[page-1].UnlockLevel,
                PersistentModState.BiographyOverride(package,character,page));
        }
        internal static void Change(ModPackage package,string character,string operation,int page)
        {
            UserContent content=CustomAffinityRuntime.RequireCharacter(package,character);
            if(operation=="meet" || operation=="forget") {
                if(page!=0)throw new ArgumentException("认识／忘记人物操作不能指定传记页码");
                PersistentModState.SetCharacterKnown(package,character,operation=="meet");
                return;
            }
            if(operation!="unlock_biography" && operation!="lock_biography")throw new ArgumentException("人物资料操作无效："+operation);
            CharacterProfilePolicy.PageBit(page);
            if(page>content.BiographyPages.Count)throw new ArgumentException("本人物未配置第"+page+"页传记："+character);
            PersistentModState.SetBiographyOverride(package,character,page,operation=="unlock_biography");
        }
    }
}
