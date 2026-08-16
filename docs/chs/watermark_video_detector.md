# 视频来源水印检测器 v1

`lomc detect-watermark-video` 使用外部 FFmpeg 定时抽取视频帧，把同尺寸帧逐帧归一化亮度后做空间对齐累积，再调用 screenshot detector 的传统相关检测。它不训练或使用 ML 模型。

## 使用

先安装 [FFmpeg](https://ffmpeg.org/) 并加入 `PATH`，以及截图检测器的 Python 可选依赖：

```powershell
python -m pip install -r compiler/requirements-detector.txt
$env:PYTHONPATH = "compiler"
python -m lomc detect-watermark-video capture.mp4 --json
python -m lomc detect-watermark-video capture.mkv `
  --ffmpeg C:\tools\ffmpeg\bin\ffmpeg.exe `
  --interval 2 --max-frames 12
```

支持 MP4、MKV、MOV、WebM、AVI 和 M4V；输入上限 16 GiB，抽帧间隔为 0.25–60 秒，最多 1–120 帧。FFmpeg 通过无 shell 的参数列表调用，抽帧目录是自动清理的临时目录。

输出在截图检测字段基础上增加：

- `frames_sampled` / `sample_interval_seconds`；
- `method=ffmpeg-frame-extraction+normalized-luminance-correlation`；
- 多帧累积后的 `confidence` 与 `sync_score`。

所有帧必须同尺寸。画面水印在同一屏幕坐标对齐时，多帧累积会增强固定载体、衰减变化的剧情画面；如果录制中途缩放、加黑边、改变裁剪、切换到不同 Mod 身份或后期移动画面，应拆分成稳定片段分别检测。

和截图检测一样，`detected=true` 不是作者/官方认证，`detected=false` 也不能证明内容来自官方。

## 验收状态

- 自动化：**VERIFIED**。确定性动态场景的四帧累积能恢复协议/Mod 哈希/CRC/ECC，无水印四帧不误报；FFmpeg 命令边界和缺失工具错误已覆盖。
- OBS 1080p 实际录屏：**NOT VERIFIED**。
- H.264 普通码率实际样本：**NOT VERIFIED**。

当前开发环境没有可用 FFmpeg，也没有真实 OBS/H.264 样本。因此本阶段没有通过加深可见纹理或降低画质来制造“通过”的视频。发布前应在实机录制中记录编码器、码率、关键帧间隔、游戏/输出分辨率、采样段和检测结果。
