using System;
using System.Runtime.InteropServices;

namespace MortalModHost
{
    internal static unsafe class NativeArchivePath
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate uint ValidateCallback(char* path, int length);
        private static readonly ValidateCallback Validate = NativeHostKernel.Bind<ValidateCallback>("lom_archive_path_v1");
        internal static bool IsAvailable { get { return Validate != null; } }

        internal static string Check(string path)
        {
            if (Validate == null) return CheckManaged(path);
            uint error;
            fixed (char* text = path) error = Validate(text, path == null ? 0 : path.Length);
            switch (error)
            {
                case 0: return path;
                case 1: throw new FormatException("包内包含空路径或 NUL 字符");
                case 2: throw new FormatException("包内路径必须使用正斜杠：" + path);
                case 3: throw new FormatException("包内包含绝对路径：" + path);
                case 4: throw new FormatException("包内包含根目录条目");
                case 5: throw new FormatException("包内包含不安全或非规范路径：" + path);
                case 6: throw new FormatException("包内路径段禁止冒号（Windows ADS 风险）：" + path);
                case 7: throw new FormatException("包内路径段禁止尾随点或空格：" + path);
                case 8: throw new FormatException("包内路径使用 Windows 保留设备名：" + path);
                default: throw new InvalidOperationException("原生 ZIP 路径检查返回无效状态：" + error);
            }
        }

        internal static string CheckManaged(string path)
        {
            if (string.IsNullOrEmpty(path) || path.IndexOf('\0') >= 0)
                throw new FormatException("包内包含空路径或 NUL 字符");
            if (path.IndexOf('\\') >= 0) throw new FormatException("包内路径必须使用正斜杠：" + path);
            if (path[0] == '/' || path.Length >= 2 && IsAsciiLetter(path[0]) && path[1] == ':')
                throw new FormatException("包内包含绝对路径：" + path);
            bool directory = path.EndsWith("/", StringComparison.Ordinal);
            string body = directory ? path.Substring(0,path.Length-1) : path;
            if (body.Length == 0) throw new FormatException("包内包含根目录条目");
            foreach (string part in body.Split('/'))
            {
                if (part.Length == 0 || part == "." || part == "..")
                    throw new FormatException("包内包含不安全或非规范路径：" + path);
                if (part.IndexOf(':') >= 0) throw new FormatException("包内路径段禁止冒号（Windows ADS 风险）：" + path);
                if (part.EndsWith(".",StringComparison.Ordinal) || part.EndsWith(" ",StringComparison.Ordinal))
                    throw new FormatException("包内路径段禁止尾随点或空格：" + path);
                string stem = part;
                int dot = stem.IndexOf('.');
                if (dot >= 0) stem = stem.Substring(0,dot);
                stem = stem.ToUpperInvariant();
                if (stem == "CON" || stem == "PRN" || stem == "AUX" || stem == "NUL"
                    || stem.Length == 4 && (stem.StartsWith("COM",StringComparison.Ordinal) || stem.StartsWith("LPT",StringComparison.Ordinal)) && stem[3] >= '1' && stem[3] <= '9')
                    throw new FormatException("包内路径使用 Windows 保留设备名：" + path);
            }
            return directory ? body + "/" : body;
        }
        private static bool IsAsciiLetter(char value) { return value >= 'A' && value <= 'Z' || value >= 'a' && value <= 'z'; }
    }
}
