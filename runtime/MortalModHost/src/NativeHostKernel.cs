using System;
using System.ComponentModel;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Threading;

namespace MortalModHost
{
    // One embedded library per architecture, held for the process lifetime.
    // Optional availability is decided once; computation failures never fall back.
    internal static class NativeHostKernel
    {
        private sealed class Backend
        {
            internal readonly IntPtr Module;
            internal readonly string Path, Error;
            internal Backend(IntPtr module, string path, string error)
            { Module = module; Path = path; Error = error; }
        }
        private static readonly Lazy<Backend> Current = new Lazy<Backend>(
            Load, LazyThreadSafetyMode.ExecutionAndPublication);
        private static int _fallbackReported;
        internal static bool IsAvailable { get { return Current.Value.Module != IntPtr.Zero; } }
        internal static string LoadedPath { get { return Current.Value.Path; } }
        internal static string UnavailableReason { get { return Current.Value.Error; } }
        internal static bool FallbackNoticePending
        { get { return Current.Value.Error != null && Volatile.Read(ref _fallbackReported) == 0; } }
        internal static void ReportFallbackOnce(Action<string> warning)
        {
            if (warning != null && Current.Value.Error != null
                && Interlocked.CompareExchange(ref _fallbackReported, 1, 0) == 0)
                warning("原生计算组件不可用：" + Current.Value.Error + "；使用原有托管计算，校验要求不变。");
        }
        internal static T Bind<T>(string name) where T : class
        {
            if (!IsAvailable) return null;
            IntPtr entry = GetProcAddress(Current.Value.Module, name);
            if (entry == IntPtr.Zero) throw new EntryPointNotFoundException(name);
            return (T)(object)Marshal.GetDelegateForFunctionPointer(entry, typeof(T));
        }

        private static Backend Load()
        {
            IntPtr module = IntPtr.Zero;
            bool loaded = false;
            try
            {
                if (Environment.OSVersion.Platform != PlatformID.Win32NT)
                    throw new PlatformNotSupportedException("原生计算组件只提供 Windows x64/x86");
                string architecture = IntPtr.Size == 8 ? "x64" : "x86";
                Assembly assembly = typeof(NativeHostKernel).Assembly;
                string resource = "MortalModHost.Native." + architecture + ".dll";
                byte[] bytes;
                using (Stream input = assembly.GetManifestResourceStream(resource))
                {
                    if (input == null) throw new FileNotFoundException("未包含当前架构的资源 " + resource);
                    using (var buffer = new MemoryStream())
                    {
                        input.CopyTo(buffer);
                        bytes = buffer.ToArray();
                    }
                }
                string directory = System.IO.Path.GetFullPath(System.IO.Path.Combine(
                    System.IO.Path.GetTempPath(), "lom_modkit",
                    assembly.ManifestModule.ModuleVersionId.ToString("N"), architecture));
                Directory.CreateDirectory(directory);
                string path = System.IO.Path.Combine(directory, "lom_host_native.dll");
                PublishIfMissing(path, bytes);
                // Deny writes/deletes while comparing and loading the exact
                // absolute file. The loaded image remains held for process life.
                using (var cache = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read))
                {
                    if (cache.Length != bytes.Length) throw new IOException("原生计算缓存与嵌入资源长度不一致");
                    var block = new byte[4096];
                    int at = 0, count;
                    while ((count = cache.Read(block, 0, block.Length)) != 0)
                    {
                        for (int i = 0; i < count; i++)
                            if (block[i] != bytes[at + i]) throw new IOException("原生计算缓存与嵌入资源字节不一致");
                        at += count;
                    }
                    if (at != bytes.Length) throw new IOException("原生计算缓存未完整读取");
                    module = LoadLibrary(path);
                    if (module == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "无法加载原生计算组件");
                    IntPtr entry = GetProcAddress(module, "lom_integrity_verify_v1");
                    if (entry == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "原生身份校验入口缺失");
                    loaded = true;
                    return new Backend(module, path, null);
                }
            }
            catch (Exception error) when (error is IOException || error is UnauthorizedAccessException
                || error is Win32Exception || error is BadImageFormatException
                || error is NotSupportedException || error is ArgumentException
                || error is System.Security.SecurityException)
            {
                // Initialization is attempted once. A failed status from an
                // already loaded backend never reaches this fallback path.
                return new Backend(IntPtr.Zero, null, error.GetType().Name + ": " + error.Message);
            }
            finally
            {
                if (!loaded && module != IntPtr.Zero) FreeLibrary(module);
            }
        }

        private static void PublishIfMissing(string path, byte[] bytes)
        {
            if (File.Exists(path)) return;
            string temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                using (var output = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                    output.Write(bytes, 0, bytes.Length);
                try { File.Move(temporary, path); }
                catch (IOException) when (File.Exists(path))
                {
                    // Another process published this assembly/architecture
                    // cache first. Its bytes are still compared before loading.
                }
            }
            finally
            {
                if (File.Exists(temporary)) File.Delete(temporary);
            }
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr LoadLibrary(string fileName);
        [DllImport("kernel32.dll", CharSet = CharSet.Ansi, ExactSpelling = true, SetLastError = true)]
        private static extern IntPtr GetProcAddress(IntPtr module, string name);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool FreeLibrary(IntPtr module);
    }
}
