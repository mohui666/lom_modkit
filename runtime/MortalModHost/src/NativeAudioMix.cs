using System;
using System.Runtime.InteropServices;

namespace MortalModHost
{
    internal static unsafe class NativeAudioMix
    {
        internal const uint Done = 1, NeedRead = 2, Stopped = 3;

        [StructLayout(LayoutKind.Sequential)]
        internal struct State
        {
            internal double Phase;
            internal float Left, Right, NextLeft, NextRight;
            internal uint HasNext, Advance;
        }

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate uint MixCallback(byte* input, int inputFrames, int channels,
            double step, float volume, float* output, int outputFrames, State* state,
            int* consumed, int* produced);

        private static readonly MixCallback Mix = NativeHostKernel.Bind<MixCallback>("lom_audio_mix_pcm16_v1");
        internal static bool IsAvailable { get { return Mix != null; } }

        internal static uint Process(byte[] input, int inputAt, int inputFrames, int channels,
            double step, float volume, float[] output, int outputAt, int outputFrames,
            ref State state, out int consumed, out int produced)
        {
            if (input == null || inputAt < 0 || inputFrames < 0 || channels < 1 || channels > 2
                || inputAt > input.Length || (long)inputFrames * channels * 2 > input.Length - inputAt
                || output == null || outputAt < 0 || outputFrames < 0 || outputAt > output.Length
                || (long)outputFrames * 2 > output.Length - outputAt)
                throw new ArgumentOutOfRangeException("音频混音缓冲范围无效");
            if (Mix == null) throw new InvalidOperationException("原生音频混音未加载");
            uint status;
            int read = 0, written = 0;
            fixed (byte* source = input)
            fixed (float* target = output)
            fixed (State* current = &state)
                status = Mix(source + inputAt, inputFrames, channels, step, volume,
                    target + outputAt, outputFrames, current, &read, &written);
            if ((status != Done && status != NeedRead && status != Stopped)
                || read < 0 || read > inputFrames || written < 0 || written > outputFrames)
                throw new InvalidOperationException("原生音频混音拒绝了请求或返回无效状态：" + status);
            consumed = read;
            produced = written;
            return status;
        }
    }
}
