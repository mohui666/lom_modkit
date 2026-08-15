using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>
    /// 包内一条用户内容（audio / character）。按所属 <see cref="ModPackage"/> 隔离，
    /// 两个 Mod 即使 ID 相同也不会串包。
    /// </summary>
    internal sealed class UserContent
    {
        public string Id;
        public string Type;
        public string Name;
        public string AudioKind;
        public string MainFile;
        public string PackagePath;
        public byte[] Bytes;
        public Dictionary<string, string> Portraits;
        public Dictionary<string, byte[]> Files;
    }
}
