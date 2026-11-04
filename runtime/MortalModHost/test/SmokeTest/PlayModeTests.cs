using System;

namespace MortalModHost
{
    internal static class PlayModeTests
    {
        internal static void Run()
        {
            var addon = new ModPackage { Id = "addon_one", PlayMode = "addon" };
            var other = new ModPackage { Id = "other", PlayMode = "addon" };
            var story = new ModPackage { Id = "story", PlayMode = "sandbox" };
            try
            {
                ModCampaignState.Active = true;
                ModCampaignState.ActiveModId = story.Id;
                ModPlayModeSession.Select(ModPlayModeKind.Original);
                Require(!ModPlayModeSession.Allows(addon) && !ModPlayModeSession.Allows(story),
                    "纯原版不得触发已安装 MOD，即使存在上一场战役状态");
                ModPlayModeSession.Select(ModPlayModeKind.Addon);
                Require(ModPlayModeSession.Allows(addon) && ModPlayModeSession.Allows(other)
                    && !ModPlayModeSession.Allows(story), "原版＋MOD只启用附加剧情");
                ModPlayModeSession.Select(ModPlayModeKind.Campaign);
                Require(ModPlayModeSession.Allows(story) && !ModPlayModeSession.Allows(addon),
                    "纯MOD只允许当前作品，不得触发其他已安装包");
                ModCampaignState.ActiveModId = addon.Id;
                Require(ModPlayModeSession.Allows(addon) && !ModPlayModeSession.Allows(other)
                    && !ModPlayModeSession.Allows(story), "切换作品必须同时切换允许的包");
                ModCampaignState.Clear();
                Require(!ModPlayModeSession.Allows(addon), "尚未进入作品不得自动触发剧情");
            }
            finally
            {
                ModCampaignState.Clear();
                ModPlayModeSession.Select(ModPlayModeKind.Original);
            }
        }

        private static void Require(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }
    }
}
