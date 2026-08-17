using System;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;

namespace MortalModHost
{
    /// <summary>
    /// 在 Story 尚未卸载时，把所选人物的官方名称与 normal 立绘地址冻结到本场
    /// GameplaySession。Combat 场景里 CharacterPlaceholder 已销毁，不能再通过
    /// 固定 CL 壳或资源路径猜人物身份。
    /// </summary>
    internal static class CombatCharacterBinding
    {
        internal static void Capture(ModPackage package)
        {
            if (!GameplaySession.PendingCombat)
                throw new InvalidOperationException("Combat 人物绑定时没有待启动的决斗会话");

            string character = GameplaySession.ConfigString("character");
            try
            {
                CaptureResolved(package, character);
            }
            catch (Exception ex)
            {
                // 人物名称/图片属于显示层。解析异常不得终止 Lua 或把玩家踢回 Free。
                string fallbackName = character;
                if (!CombatCharacterPolicy.IsUserCharacter(character))
                {
                    try
                    {
                        string localized = LocalizationManager.Instance.LocaleResolver.GetString(
                            "Character/" + character);
                        if (!string.IsNullOrWhiteSpace(localized)
                            && !string.Equals(localized, "Character/" + character, StringComparison.Ordinal))
                            fallbackName = localized;
                    }
                    catch { }
                }
                if (string.IsNullOrWhiteSpace(fallbackName)) fallbackName = "MOD";
                GameplaySession.BindCombatCharacter(fallbackName, "");
                LuaManagerPatch.Log?.LogError(
                    "Combat 人物显示资源解析失败；使用人物 ID/本地化名称继续决斗：" + ex);
            }
        }

        private static void CaptureResolved(ModPackage package, string character)
        {
            if (CombatCharacterPolicy.IsUserCharacter(character))
            {
                ContentRef parsed;
                string error;
                UserContent content;
                if (!ContentRef.TryParse(character, out parsed, out error)
                    || package == null
                    || !package.TryGetUserContent(parsed.ContentId, out content)
                    || content == null || content.Type != "character"
                    || string.IsNullOrWhiteSpace(content.Name))
                    throw new InvalidOperationException(
                        "无法从当前包解析自定义决斗人物名称：" + character);
                GameplaySession.BindCombatCharacter(content.Name, "");
                return;
            }

            CharacterPlaceholder placeholder = CharacterPlaceholder.Instance;
            if (placeholder == null)
                throw new InvalidOperationException("Story CharacterPlaceholder 尚未就绪");
            StoryCharacterConfig config = Traverse.Create(placeholder)
                .Field("_config").GetValue<StoryCharacterConfig>();
            if (config == null)
                throw new InvalidOperationException("StoryCharacterConfig 不可用");
            StoryCharacterData data = config.Get(character);
            if (data == null)
                throw new InvalidOperationException("原版人物不存在：" + character);

            string displayName = LocalizationManager.Instance.LocaleResolver.GetString(data.NameKey);
            if (string.IsNullOrWhiteSpace(displayName)
                || string.Equals(displayName, data.NameKey, StringComparison.Ordinal))
                throw new InvalidOperationException("原版人物名称未本地化：" + character);

            string idleAddress = "";
            StoryCharaterImageItem[] portraits = data.PortraitResourceList;
            if (portraits != null)
            {
                for (int i = 0; i < portraits.Length; i++)
                {
                    StoryCharaterImageItem item = portraits[i];
                    if (item == null || item.Mapping == null
                        || string.IsNullOrEmpty(item.AddressKey)) continue;
                    if (string.Equals(item.Mapping.Value, "normal", StringComparison.Ordinal))
                    {
                        idleAddress = item.AddressKey;
                        break;
                    }
                    if (idleAddress.Length == 0) idleAddress = item.AddressKey;
                }
            }
            GameplaySession.BindCombatCharacter(displayName, idleAddress);
            if (idleAddress.Length == 0)
                LuaManagerPatch.Log?.LogWarning(
                    "原版人物没有可回退的 Story 立绘，将优先使用专用 Combat 资源：" + character);
            LuaManagerPatch.Log?.LogInfo(
                "Combat 人物身份已从原版 StoryCharacterConfig 绑定："
                + character + " / " + displayName + " / " + idleAddress);
        }
    }
}
