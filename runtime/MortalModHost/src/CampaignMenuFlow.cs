using System;
using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>
    /// 官方战役面板使用的纯状态机。选择战役只改变页面/最近记录；只有随后显式点击
    /// “继续”或“开始新战役”才会返回动作，防止点 MOD 名称就直接进游戏。
    /// </summary>
    internal sealed class CampaignMenuFlow
    {
        private readonly Dictionary<string, ModPackage> _packages =
            new Dictionary<string, ModPackage>(StringComparer.Ordinal);

        internal CampaignMenuFlow(IEnumerable<ModPackage> packages, string recentCampaignId)
        {
            if (packages != null)
            {
                foreach (ModPackage package in packages)
                {
                    if (package == null || package.Campaign == null
                        || !CampaignIdentity.IsValid(package.CampaignId)) continue;
                    if (_packages.ContainsKey(package.CampaignId))
                        throw new InvalidOperationException("重复 campaign_id：" + package.CampaignId);
                    _packages.Add(package.CampaignId, package);
                }
            }
            if (!string.IsNullOrEmpty(recentCampaignId)
                && _packages.ContainsKey(recentCampaignId))
                RecentCampaignId = recentCampaignId;
        }

        internal string SelectedCampaignId { get; private set; }
        internal string RecentCampaignId { get; private set; }

        internal ModPackage RecentPackage
        {
            get { return Find(RecentCampaignId); }
        }

        internal ModPackage SelectedPackage
        {
            get { return Find(SelectedCampaignId); }
        }

        internal bool Select(string campaignId)
        {
            if (!_packages.ContainsKey(campaignId)) return false;
            SelectedCampaignId = campaignId;
            RecentCampaignId = campaignId;
            return true;
        }

        internal void Back()
        {
            SelectedCampaignId = null;
        }

        private ModPackage Find(string campaignId)
        {
            ModPackage package;
            return !string.IsNullOrEmpty(campaignId)
                && _packages.TryGetValue(campaignId, out package) ? package : null;
        }
    }
}
