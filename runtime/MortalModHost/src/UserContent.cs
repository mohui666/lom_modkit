using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>
    /// 包内用户素材及玩法定义。按所属 <see cref="ModPackage"/> 隔离，
    /// 两个 Mod 即使 ID 相同也不会串包。
    /// </summary>
    internal sealed class UserContent
    {
        public string Id;
        public string Type;
        public string Name;
        public string Title;
        public string Biography;
        public bool InitiallyKnown;
        public List<CharacterBiographyPage> BiographyPages = new List<CharacterBiographyPage>();
        public string AvatarFile;
        public int InitialAffinity;
        public float Scale = 100f;
        public string Extends;
        public string ArtFacing;
        public int MoodX = 50;
        public int MoodY = 90;
        public string AudioKind;
        public string MainFile;
        public string PackagePath;
        public byte[] Bytes;
        internal bool IsStoryPicture
        {
            get { return Type == "image" || Type == "item" || Type == "martial"; }
        }
        public Dictionary<string, string> Portraits;
        public string PortraitReference;
        public Dictionary<string, PortraitCalibration> PortraitLayouts = new Dictionary<string, PortraitCalibration>();
        public PortraitCalibration PortraitLayout(string portrait)
        {
            PortraitCalibration layout;
            return portrait != null && PortraitLayouts.TryGetValue(portrait, out layout) ? layout : new PortraitCalibration();
        }
        public Dictionary<string, string> CombatFrames;
        public Dictionary<string, byte[]> Files;
        public CustomGameplayDefinition Gameplay;
    }

    internal sealed class CharacterBiographyPage
    {
        public string Text;
        public int UnlockLevel;
    }

    internal sealed class PortraitCalibration
    {
        public float X;
        public float Y;
        public float Scale = 100f;
    }
}
