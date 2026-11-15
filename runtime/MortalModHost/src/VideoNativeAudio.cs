using System;
using System.IO;

namespace MortalModHost
{
    // The installed Wwise integration exposes the final output device's float
    // samples directly. Counts include all channels; GetCaptureSamples consumes
    // whole native chunks, so read the complete count in one call.
    internal sealed class VideoNativeAudio : IDisposable
    {
        private BinaryWriter _writer;
        private float[] _samples = new float[8192];
        private byte[] _bytes = new byte[8192 * 4];
        private long _written;
        private bool _capturing;
        private readonly int _rate, _channels;

        internal VideoNativeAudio(string path)
        {
            _rate = checked((int)AkSoundEngine.GetSampleRate());
            using (AkChannelConfig config = AkSoundEngine.GetSpeakerConfiguration())
            {
                _channels = checked((int)config.uNumChannels);
                if (!config.IsValid() || _rate <= 0 || _channels <= 0)
                    throw new InvalidOperationException("游戏原生音频设备尚未就绪");
            }
            _writer = new BinaryWriter(new FileStream(path, FileMode.CreateNew, FileAccess.Write));
            VideoAudioMixer.Header(_writer, 0, _rate, _channels);
            AkSoundEngine.StartDeviceCapture(0);
            _capturing = true;
        }

        internal void Frame()
        {
            uint count = AkSoundEngine.UpdateCaptureSampleCount(0);
            if (count == 0) return;
            int length = checked((int)count);
            if (_samples.Length < length) { _samples = new float[length]; _bytes = new byte[checked(length * 4)]; }
            uint read = AkSoundEngine.GetCaptureSamples(0, _samples, count);
            if (read != count) throw new InvalidOperationException("游戏原生音频采样未完整读取");
            int bytes = checked(length * 4);
            Buffer.BlockCopy(_samples, 0, _bytes, 0, bytes);
            _writer.Write(_bytes, 0, bytes);
            _written += bytes;
        }

        internal void End(bool requireSamples)
        {
            if (_writer == null) return;
            AkSoundEngine.StopDeviceCapture(0);
            _capturing = false;
            try { Frame(); }
            finally { Dispose(); }
            if (requireSamples && _written == 0) throw new InvalidOperationException("原生音频设备没有输出采样，视频未导出");
        }

        public void Dispose()
        {
            if (_writer == null) return;
            if (_capturing) AkSoundEngine.StopDeviceCapture(0);
            _capturing = false;
            var writer = _writer; _writer = null;
            try { writer.BaseStream.Position = 0; VideoAudioMixer.Header(writer, checked((uint)_written), _rate, _channels); }
            finally { writer.Dispose(); }
        }
    }
}
