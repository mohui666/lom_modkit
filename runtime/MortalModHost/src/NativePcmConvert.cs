using System;
using System.Runtime.InteropServices;
using System.Threading;

namespace MortalModHost
{
    // Pure sample conversion. The decoder owns framing/cursors and retains the
    // existing PCM16 BlockCopy. Native scaling preserves Unity Mono's double
    // intermediate before conv.i2; .NET's stricter float32 intermediates can
    // differ at thresholds. Managed methods retain the original C# loops.
    // An available native backend never retries a rejected conversion.
    internal static unsafe class NativePcmConvert
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int FloatCallback(float* samples, int sampleCount,
            byte* destination, int destinationCapacity);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int WavCallback(byte* source, int sourceLength, int bytesPerSample,
            byte* destination, int destinationCapacity);

        private static readonly Lazy<FloatCallback> Floats = new Lazy<FloatCallback>(
            () => NativeHostKernel.Bind<FloatCallback>("lom_pcm_f32_to_i16_v1"),
            LazyThreadSafetyMode.ExecutionAndPublication);
        private static readonly Lazy<WavCallback> Wav = new Lazy<WavCallback>(
            () => NativeHostKernel.Bind<WavCallback>("lom_pcm_wav_to_i16_v1"),
            LazyThreadSafetyMode.ExecutionAndPublication);

        internal static bool IsAvailable { get { return Floats.Value != null && Wav.Value != null; } }

        internal static int Float32ToPcm16(float[] samples, int sampleCount, byte[] destination, int offset)
        {
            ValidateFloats(samples, sampleCount, destination, offset);
            if (sampleCount == 0) return 0;
            FloatCallback convert = Floats.Value;
            if (convert == null) return Float32ToPcm16Managed(samples, sampleCount, destination, offset);
            int written;
            fixed (float* source = samples)
            fixed (byte* output = destination)
                written = convert(source, sampleCount, output + offset, destination.Length - offset);
            return RequireWritten(written, sampleCount);
        }

        internal static int WavToPcm16(byte[] source, int sourceOffset, int sampleCount,
            int bytesPerSample, byte[] destination, int destinationOffset)
        {
            ValidateWav(source, sourceOffset, sampleCount, bytesPerSample, destination, destinationOffset);
            if (sampleCount == 0) return 0;
            WavCallback convert = Wav.Value;
            if (convert == null) return WavToPcm16Managed(source, sourceOffset, sampleCount,
                bytesPerSample, destination, destinationOffset);
            int written;
            fixed (byte* input = source)
            fixed (byte* output = destination)
                written = convert(input + sourceOffset, sampleCount * bytesPerSample, bytesPerSample,
                    output + destinationOffset, destination.Length - destinationOffset);
            return RequireWritten(written, sampleCount);
        }

        internal static int Float32ToPcm16Managed(float[] samples, int sampleCount, byte[] destination, int offset)
        {
            ValidateFloats(samples, sampleCount, destination, offset);
            for (int i = 0; i < sampleCount; i++)
            {
                float f = samples[i];
                if (f > 1f) f = 1f;
                if (f < -1f) f = -1f;
                short s = (short)(f * 32767f);
                destination[offset + i * 2] = (byte)(s & 0xFF);
                destination[offset + i * 2 + 1] = (byte)((s >> 8) & 0xFF);
            }
            return sampleCount * 2;
        }

        internal static int WavToPcm16Managed(byte[] source, int sourceOffset, int sampleCount,
            int bytesPerSample, byte[] destination, int destinationOffset)
        {
            ValidateWav(source, sourceOffset, sampleCount, bytesPerSample, destination, destinationOffset);
            if (bytesPerSample == 2)
            {
                Buffer.BlockCopy(source, sourceOffset, destination, destinationOffset, sampleCount * 2);
                return sampleCount * 2;
            }
            int src = sourceOffset, written = 0;
            for (int f = 0; f < sampleCount; f++)
            {
                float sample;
                if (bytesPerSample == 1) sample = (source[src++] - 128) / 128f;
                else { sample = BitConverter.ToSingle(source, src); src += 4; }
                if (sample > 1f) sample = 1f;
                if (sample < -1f) sample = -1f;
                short s = (short)(sample * 32767f);
                destination[destinationOffset + written] = (byte)(s & 0xFF);
                destination[destinationOffset + written + 1] = (byte)((s >> 8) & 0xFF);
                written += 2;
            }
            return written;
        }

        private static int RequireWritten(int written, int samples)
        {
            if (written != samples * 2)
                throw new InvalidOperationException("原生 PCM 转换拒绝输入或返回了错误的字节数：" + written);
            return written;
        }

        private static void ValidateDestination(byte[] destination, int offset, int samples)
        {
            if (destination == null) throw new ArgumentNullException("destination");
            if (samples < 0 || samples > int.MaxValue / 2) throw new ArgumentOutOfRangeException("samples");
            if (offset < 0 || offset > destination.Length - samples * 2)
                throw new ArgumentOutOfRangeException("offset");
        }

        private static void ValidateFloats(float[] samples, int count, byte[] destination, int offset)
        {
            if (samples == null) throw new ArgumentNullException("samples");
            if (count < 0 || count > samples.Length) throw new ArgumentOutOfRangeException("sampleCount");
            ValidateDestination(destination, offset, count);
        }

        private static void ValidateWav(byte[] source, int sourceOffset, int samples, int width,
            byte[] destination, int destinationOffset)
        {
            if (source == null) throw new ArgumentNullException("source");
            if (width != 1 && width != 2 && width != 4) throw new ArgumentOutOfRangeException("bytesPerSample");
            if (sourceOffset < 0 || sourceOffset > source.Length || samples < 0
                || samples > (source.Length - sourceOffset) / width)
                throw new ArgumentOutOfRangeException("sampleCount");
            ValidateDestination(destination, destinationOffset, samples);
        }
    }
}
