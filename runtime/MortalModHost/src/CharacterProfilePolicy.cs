using System;
using System.Collections.Generic;

namespace MortalModHost
{
    internal static class CharacterProfilePolicy
    {
        internal const int MaxPages=4;

        internal static void ReadDefinition(Dictionary<string,object> root,UserContent content)
        {
            object known;
            if(root.TryGetValue("initially_known",out known)) {
                if(!(known is bool))throw new FormatException("initially_known 必须是布尔值");
                content.InitiallyKnown=(bool)known;
            }
            object raw;
            if(root.TryGetValue("biography_pages",out raw)) {
                var pages=raw as List<object>;
                if(pages==null || pages.Count>MaxPages)throw new FormatException("biography_pages 必须是最多4页的数组");
                foreach(object item in pages) {
                    var page=item as Dictionary<string,object>;
                    object text,level;
                    if(page==null || page.Count!=2 || !page.TryGetValue("text",out text) || !(text is string)
                        || !page.TryGetValue("unlock_level",out level) || !(level is double)
                        || (double)level<0 || (double)level>10 || (double)level%1!=0)
                        throw new FormatException("传记页需要 text 字符串与 unlock_level 整数（0～10）");
                    content.BiographyPages.Add(new CharacterBiographyPage {Text=(string)text,UnlockLevel=(int)(double)level});
                }
            }
            if(content.BiographyPages.Count>0 && !string.IsNullOrWhiteSpace(content.Biography))
                throw new FormatException("人物同时存在 biography 旧传记与 biography_pages 分页，请保留所需正文后明确选择一种传记配置");
            if(content.BiographyPages.Count==0 && content.Biography!=null)
                content.BiographyPages.Add(new CharacterBiographyPage {Text=content.Biography,UnlockLevel=0});
        }

        internal static int PageBit(int page)
        {
            if(page<1 || page>MaxPages)throw new ArgumentOutOfRangeException("page","传记页码必须为1～4");
            return 1<<(page-1);
        }

        // A manual decision replaces the level rule. Knowledge remains a
        // separate gate so forgetting never erases affinity or page decisions.
        internal static bool CanRead(bool known,int affinity,int unlockLevel,int manual)
        {
            return known && manual>=0 && (manual>0 || affinity/10>=unlockLevel);
        }
    }
}
