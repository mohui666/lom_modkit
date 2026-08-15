using System;
using System.Collections.Generic;
using System.Text.RegularExpressions;

namespace MortalModHost
{
    /// <summary>
    /// Host 自有任务状态机；不调用 MissionManager，不占用或修改任何官方 Mission ID。
    /// 当前阶段为战役会话态：同一 MOD 战役跨 Story/Free 保留，Title/新开局/插件卸载时清空。
    /// </summary>
    internal static class ModQuestSession
    {
        private static readonly Regex QuestId = new Regex(
            "^[A-Za-z0-9_-]{1,64}$", RegexOptions.CultureInvariant);
        private static readonly Dictionary<string, string> States =
            new Dictionary<string, string>(StringComparer.Ordinal);
        private static string _owner = "";

        internal static void Apply(ModPackage package, string questId, string operation)
        {
            EnsureOwner(package);
            RequireQuestId(questId);
            string current;
            bool exists = States.TryGetValue(questId, out current);
            switch (operation)
            {
                case "start":
                    if (exists && !string.Equals(current, "inactive", StringComparison.Ordinal))
                        throw new InvalidOperationException("任务已经开始或结束：" + questId);
                    if (States.Count >= 256 && !exists)
                        throw new InvalidOperationException("单个 MOD 战役最多允许 256 个任务");
                    States[questId] = "active";
                    break;
                case "update":
                    RequireActive(questId, exists, current);
                    States[questId] = "active";
                    break;
                case "complete":
                    RequireActive(questId, exists, current);
                    States[questId] = "completed";
                    break;
                case "fail":
                    RequireActive(questId, exists, current);
                    States[questId] = "failed";
                    break;
                default:
                    throw new ArgumentException("任务操作必须是 start/update/complete/fail");
            }
        }

        internal static string Read(ModPackage package, string questId)
        {
            EnsureOwner(package);
            RequireQuestId(questId);
            string state;
            return States.TryGetValue(questId, out state) ? state : "inactive";
        }

        internal static void Reset()
        {
            _owner = "";
            States.Clear();
        }

        private static void EnsureOwner(ModPackage package)
        {
            string owner = Owner(package);
            if (ModCampaignState.Active
                && !string.Equals(ModCampaignState.ActiveModId, package.Id, StringComparison.Ordinal))
                throw new InvalidOperationException("活动 MOD 战役只能读写当前战役包的任务状态");
            if (_owner.Length == 0)
            {
                _owner = owner;
                return;
            }
            if (string.Equals(_owner, owner, StringComparison.Ordinal)) return;
            if (ModCampaignState.Active)
                throw new InvalidOperationException("活动 MOD 战役期间拒绝其他包接管任务状态");
            Reset();
            _owner = owner;
        }

        private static string Owner(ModPackage package)
        {
            if (package == null || string.IsNullOrEmpty(package.Id)
                || string.IsNullOrEmpty(package.PackageFingerprint)
                || package.PackageFingerprint.Length != 64)
                throw new InvalidOperationException("MOD 任务缺少可信的包 id / 完整 SHA-256 身份");
            return package.Id + "\n" + package.PackageFingerprint;
        }

        private static void RequireQuestId(string questId)
        {
            if (string.IsNullOrEmpty(questId) || !QuestId.IsMatch(questId))
                throw new ArgumentException("任务 id 必须是 1~64 位字母、数字、下划线或短横线");
        }

        private static void RequireActive(string questId, bool exists, string current)
        {
            if (!exists || !string.Equals(current, "active", StringComparison.Ordinal))
                throw new InvalidOperationException("任务不是 active，不能更新或结算：" + questId);
        }
    }
}
