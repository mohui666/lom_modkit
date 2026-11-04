namespace MortalModHost
{
    internal enum ModPlayModeKind
    {
        Original,
        Addon,
        Campaign
    }

    /// <summary>The selected game experience, independent of whether the Host is installed.</summary>
    internal static class ModPlayModeSession
    {
        internal static ModPlayModeKind Selected { get; private set; }

        internal static void Select(ModPlayModeKind mode)
        {
            Selected = mode;
        }

        internal static bool Allows(ModPackage package)
        {
            if (package == null || Selected == ModPlayModeKind.Original) return false;
            if (Selected == ModPlayModeKind.Addon)
                return package.PlayMode == "addon";
            return ModCampaignState.Active
                && string.Equals(package.Id, ModCampaignState.ActiveModId,
                    System.StringComparison.Ordinal);
        }
    }
}
