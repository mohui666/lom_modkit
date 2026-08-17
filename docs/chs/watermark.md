# 来源水印

协议、画面嵌入、截图检测与视频检测写在这一页。它**不是**签名、DRM 或官方认证。

## 协议 v1

28 字节 big-endian payload，嵌入时 MSB-first：

| Offset | 长度 | 字段 |
|---:|---:|---|
| 0 | 4 | magic `LOMW`（`4C4F4D57`） |
| 4 | 1 | protocol version `01` |
| 5 | 1 | algorithm version `01`–`FF` |
| 6 | 1 | flags，v1 必须 `00` |
| 7 | 1 | reserved，v1 必须 `00` |
| 8 | 16 | Mod ID 的 SHA-256 前 16 字节 |
| 24 | 4 | 对 0–23 的 IEEE CRC-32 |

```text
SHA-256( ASCII("lom_modkit:watermark:mod-id:v1") || 00 || ASCII(mod_id) )[0:16]
```

Mod ID 必须符合 `[a-z0-9_-]{1,64}`。黄金向量 `demo_mod`：

```text
mod_id_hash = 720435D441F942141A10BE8AA833C874
payload     = 4C4F4D5701010000720435D441F942141A10BE8AA833C8741C08EE6D
```

Python `lomc.watermark_protocol` 与 C# `ProvenanceWatermarkProtocol` 必须同时通过。
未知 protocol 拒绝；非零 flags/reserved 拒绝。

## 画面嵌入（algorithm v1）

224-bit → Hamming(7,4) → 392 bit；公开 domain key 的 XorShift32 做格置换。
tile `448×224`（`28×14` 个 `16×16` 格），亮度棋盘，alpha `4/255`，
`TextureWrapMode.Repeat` 铺满。只在 Host 确认的 MOD Story 启用；无法维持时
fail-closed 回 Free。

## 截图检测

```powershell
python -m pip install -r compiler/requirements-detector.txt
$env:PYTHONPATH = "compiler"
python -m lomc detect-watermark screenshot.png
python -m lomc detect-watermark screenshot.jpg --json
```

退出码：检出 `0`，未检出 `2`，输入/依赖错误 `1`。JSON 含 `detected`、
`confidence`、`mod_hash`、`checksum_status`、`ecc_status`。会试常见缩放。
`detected=true` 只说明恢复出公开格式载体，不能证明作者。

## 视频检测

需要 PATH 里的 FFmpeg。定时抽帧后做亮度相关累积，再走截图检测器：

```powershell
python -m lomc detect-watermark-video capture.mp4 --json
```

支持 MP4/MKV/MOV/WebM/AVI/M4V，输入上限 16 GiB。额外字段：
`frames_sampled`、`sample_interval_seconds`、`method`。
