# FFmpeg video encoder

The Windows editor invokes FFmpeg as a separate executable to encode locally
rendered game frames and audio. The encoder is not linked into LoM Modkit.

The default bundled encoder is built from unmodified FFmpeg 9.0.2 and x264
sources at the fixed commits below. It is distributed under GNU GPL version 3
or later. The complete GPL text is supplied beside the executable as `LICENSE`,
with x264's license in `LICENSE.x264`. LoM Modkit's MIT license does not replace
these licenses.

- FFmpeg: https://ffmpeg.org/
- FFmpeg source commit: https://github.com/FFmpeg/FFmpeg/commit/946fcce07b6dcd0331c8cc609192aeff5e1924f8
- FFmpeg source archive: https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz
- x264 source commit: https://code.videolan.org/videolan/x264/-/commit/0480cb05fa188d37ae87e8f4fd8f1aea3711f7ee
- x264 source archive: https://code.videolan.org/videolan/x264/-/archive/0480cb05fa188d37ae87e8f4fd8f1aea3711f7ee/x264-0480cb05fa188d37ae87e8f4fd8f1aea3711f7ee.tar.gz
- Build recipe: `scripts/build-video-encoder.ps1` in the LoM Modkit repository.

The build enables the native-video profile: raw RGBA and PCM input, H.264
(libx264), AAC, MP4, audio mixing, pixel conversion and resampling. Assembly
optimizations, runtime CPU detection and multithreading remain enabled. The
legacy media detector continues to use its separately configured FFmpeg.

The build recipe records the configuration and retains the fixed source
archives. Distribution of the encoder must follow the supplied GPL license and
provide the corresponding source required by that license.
