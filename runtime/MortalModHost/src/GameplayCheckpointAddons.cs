namespace MortalModHost
{
    internal static partial class GameplayCheckpointStore
    {
        internal static bool TryLoadAddon(string saveSlot,
            out ModPackage package, out GameplayCheckpoint checkpoint)
        {
            return TryLoadAddon(saveSlot, Plugin.LoadedMods, out package, out checkpoint);
        }
    }
}
