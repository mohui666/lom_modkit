# 来源水印

协议、画面嵌入、截图检测与视频检测写在这一页。它**不是**签名、DRM 或官方认证。

1.6.0 普通试玩和视频导出均停用可见、隐藏的画面水印，不创建标签、来源纹理或逐帧水印载体，也不加载神经水印模型；真实的作品身份、逐帧会话完整性与存档归属检查保持执行。下述协议和检测器为旧版媒体保留，检测入口的存在不代表当前输出仍在嵌入水印。

旧版隐形载荷不保证能从压缩视频恢复：2026-11-15 本机原生视频样片（720p30、H.264 CRF18、9.667 秒）按每秒一帧抽取 10 帧后，检测器未恢复出通过协议与 CRC 校验的来源码。不能据此宣称旧版视频可可靠追溯；没有检出也不能证明画面没有 MOD 内容。

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

Rust `lom_core::watermark` 与 C# `ProvenanceWatermarkProtocol` 必须通过相同黄金向量。
未知 protocol 拒绝；非零 flags/reserved 拒绝。

## 旧版画面嵌入（algorithm v1）

224-bit → Hamming(7,4) → 392 bit；公开 domain key 的 XorShift32 做格置换。
tile `448×224`（`28×14` 个 `16×16` 格），亮度棋盘，alpha `4/255`，
`TextureWrapMode.Repeat` 铺满。这是旧版 Host 的载体规则；当前水印策略为关闭，不再进入该纹理或画布创建／维护路径。旧载体代码与检测器保留用于兼容既有媒体协议。

## 原版整章来源判定（1.4.9）

整章导入保存原文与基准步骤。Host 在每次加载作品后首次进入该章时核对本机原文，再缓存步骤来源。未改原版步骤与新增／改写内容分别标记来源；人物、音乐、数值或流程产生持续影响，以及删除、重排或改写原生逻辑时，按已有规则判断后续内容来源。旧版用此判断显隐画面标记；1.6.0 只保留来源分类，不显示或嵌入任何水印。

来源会话仍归当前作品，只有到达 Title／Free 的可信边界才解除。包指纹从 Host 实际读取的整包字节生成，跨包嵌套和存档归属继续校验；停用画面标记没有放宽这些规则。来源判定由包内章节与本机原文生成，运行中的节点切换查询缓存，无需逐帧比较整章文字。

## 截图检测

```powershell
cargo build --locked --release -p lomc
target/release/lomc detect-watermark screenshot.png
target/release/lomc detect-watermark screenshot.jpg --json
```

退出码：检出 `0`，未检出 `2`，输入/依赖错误 `1`。JSON 含 `detected`、
`confidence`、`mod_hash`、`checksum_status`、`ecc_status`。会试常见缩放。
`detected=true` 只说明恢复出公开格式载体，不能证明作者。

## 视频检测

需要 PATH 里的 FFmpeg。定时抽帧后做亮度相关累积，再走截图检测器：

```powershell
target/release/lomc detect-watermark-video capture.mp4 --json
```

支持 MP4/MKV/MOV/WebM/AVI/M4V，输入上限 16 GiB。额外字段：
`frames_sampled`、`sample_interval_seconds`、`method`。
