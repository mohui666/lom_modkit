using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using UnityEngine;

namespace MortalModHost
{
    // Sprite and Texture identity stay with their existing owners. Only the
    // texture pixels change, so native portrait lookups and fades keep working.
    internal static class GifImageRuntime
    {
        [StructLayout(LayoutKind.Sequential)]
        private struct Info { internal int Width, Height, Count, Loops; }
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private unsafe delegate IntPtr OpenGif(byte* bytes, int length, out Info info);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate IntPtr GetFrame(IntPtr handle, int index, out float seconds);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate void DropGif(IntPtr handle);

        private static class Decoder
        {
            internal static readonly OpenGif Open = NativeHostKernel.Bind<OpenGif>("lom_gif_open_v1");
            internal static readonly GetFrame Frame = NativeHostKernel.Bind<GetFrame>("lom_gif_frame_v1");
            internal static readonly DropGif Drop = NativeHostKernel.Bind<DropGif>("lom_gif_drop_v1");
        }

        private static readonly List<Animation> Active = new List<Animation>();

        internal static unsafe bool Load(Texture2D texture, byte[] bytes, bool markNonReadable = false)
        {
            if (!IsGif(bytes)) return texture.LoadImage(bytes, markNonReadable);
            if (!NativeHostKernel.IsAvailable) return false;
            Info info;
            IntPtr handle;
            fixed (byte* data = bytes) handle = Decoder.Open(data, bytes.Length, out info);
            if (handle == IntPtr.Zero) return false;
            bool keep = false;
            try
            {
                if (!texture.Resize(info.Width, info.Height, TextureFormat.RGBA32, false)) return false;
                var animation = new Animation(texture, handle, info);
                animation.Upload(0, info.Count == 1 && markNonReadable);
                if (info.Count > 1)
                {
                    Active.Add(animation);
                    keep = true;
                }
                return true;
            }
            finally
            {
                if (!keep) Decoder.Drop(handle);
            }
        }

        private static bool IsGif(byte[] bytes)
        {
            return bytes != null && bytes.Length >= 6 && bytes[0] == 'G' && bytes[1] == 'I'
                && bytes[2] == 'F' && bytes[3] == '8' && (bytes[4] == '7' || bytes[4] == '9') && bytes[5] == 'a';
        }

        internal static void Tick()
        {
            float seconds = Time.deltaTime;
            for (int i = Active.Count - 1; i >= 0; i--)
            {
                Animation animation = Active[i];
                if (animation.Advance(seconds)) continue;
                Decoder.Drop(animation.Handle);
                Active.RemoveAt(i);
            }
        }

        internal static void Clear()
        {
            foreach (Animation animation in Active) Decoder.Drop(animation.Handle);
            Active.Clear();
        }

        private sealed class Animation
        {
            internal readonly IntPtr Handle;
            private readonly Texture2D _texture;
            private readonly int _bytes, _loops;
            private readonly double[] _frameEnds;
            private double _elapsed;
            private int _displayed;

            internal Animation(Texture2D texture, IntPtr handle, Info info)
            {
                _texture = texture;
                Handle = handle;
                _bytes = checked(info.Width * info.Height * 4);
                _loops = info.Loops;
                _frameEnds = new double[info.Count];
                double end = 0;
                for (int index = 0; index < info.Count; index++)
                {
                    Decoder.Frame(Handle, index, out float seconds);
                    _frameEnds[index] = end += seconds;
                }
            }

            internal void Upload(int index, bool markNonReadable = false)
            {
                _texture.LoadRawTextureData(Decoder.Frame(Handle, index, out _), _bytes);
                _texture.Apply(false, markNonReadable);
                _displayed = index;
            }

            internal bool Advance(float seconds)
            {
                if (_texture == null) return false;
                _elapsed += seconds;
                int last = _frameEnds.Length - 1;
                double duration = _frameEnds[last];
                if (_loops != 0 && _elapsed >= duration * _loops)
                {
                    if (_displayed != last) Upload(last);
                    return false;
                }
                int index = Array.BinarySearch(_frameEnds, _elapsed % duration);
                index = index < 0 ? ~index : index + 1;
                if (index != _displayed) Upload(index);
                return true;
            }
        }
    }
}
