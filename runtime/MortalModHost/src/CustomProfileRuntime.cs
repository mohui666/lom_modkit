using System;
using Mortal.Core;
using OBB.Framework.Utils;

namespace MortalModHost
{
    internal static class CustomProfileRuntime
    {
        internal static bool HasCustomRow(UserContent content)
        {
            return content.Type == "character" && (string.IsNullOrEmpty(content.Extends)
                || (content.HasProfile || !string.IsNullOrEmpty(content.SocialThumbnailFile))
                    && !EnumUtils.TryParseByStringValue<RelationshipStatType>(content.Extends, out _));
        }

        internal static string Reference(UserContent content)
        {
            return string.IsNullOrEmpty(content.Extends) ? ContentRef.UserPrefix + content.Id
                : ContentRef.StoryAffinityPrefix + content.Extends;
        }

        internal static bool Known(ModPackage package,UserContent content)
        {
            return PersistentModState.CharacterKnown(package,Reference(content),content.InitiallyKnown);
        }
        internal static bool PageUnlocked(ModPackage package,UserContent content,int page,int affinity)
        {
            if(page<1 || page>content.BiographyPages.Count)return false;
            string character=Reference(content);
            return CharacterProfilePolicy.CanRead(Known(package,content),affinity,content.BiographyPages[page-1].UnlockLevel,
                PersistentModState.BiographyOverride(package,character,page));
        }
        internal static void Change(ModPackage package,string character,string operation,int page)
        {
            UserContent content;
            if (ContentRef.IsStoryAffinityRef(character))
            {
                if (package == null
                    || !package.CharacterExtensions.TryGetValue(character.Substring(ContentRef.StoryAffinityPrefix.Length), out content)
                    || !HasCustomRow(content))
                    throw new ArgumentException("本作品未绑定剧情人物资料：" + character);
            }
            else content=CustomAffinityRuntime.RequireCharacter(package,character);
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
