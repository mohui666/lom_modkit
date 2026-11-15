# FFmpeg video encoder

The Windows editor invokes FFmpeg as a separate executable to encode locally
rendered game frames and audio. The encoder is not linked into LoM Modkit.

The bundled encoder input is Gyan Doshi's FFmpeg 9.0.2 essentials build,
distributed under GNU GPL version 3. Its complete upstream license is supplied
beside the executable as `LICENSE`. LoM Modkit's MIT license does not replace
FFmpeg's license.

- FFmpeg: https://ffmpeg.org/
- Binary build and build information: https://www.gyan.dev/ffmpeg/builds/
- Exact binary release: https://github.com/GyanD/codexffmpeg/releases/tag/9.0.2
- Corresponding FFmpeg source commit: https://github.com/FFmpeg/FFmpeg/commit/946fcce07b
- Build repository: https://github.com/GyanD/codexffmpeg

The upstream build includes third-party encoding libraries, including x264.
Their applicable licenses remain in effect. Distribution of the encoder must
follow the supplied GPL license and provide the corresponding source required
by that license.
