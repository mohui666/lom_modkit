# lom_modkit 来源水印协议 v1

> 状态：协议已冻结并有 Python/C# 黄金向量；Host 已实现并强制启用 algorithm v1 画面嵌入器。离线截图检测器属于下一阶段。

## 目的与非目标

协议用于让后续的传统图像水印嵌入器和离线检测器交换同一个、可校验的“这是 lom_modkit MOD 内容”标记，并携带一个稳定的 Mod ID 摘要。

它**不是**数字签名、作者认证、官方认证、DRM 或不可移除承诺。任何人都能生成协议 payload；CRC-32 只检测误码，不抵抗伪造。截图中检出该协议，只能说明检测到了 lom_modkit 格式标记，不能证明作者身份或内容真实性。

## 固定 payload

协议 v1 固定为 28 字节（224 bit），多字节整数使用 big-endian，嵌入时逐字节按 MSB-first 展开：

| Offset | 长度 | 字段 | v1 规则 |
|---:|---:|---|---|
| 0 | 4 | magic | ASCII `LOMW`，十六进制 `4C4F4D57` |
| 4 | 1 | protocol version | `01` |
| 5 | 1 | algorithm version | `01`–`FF`；为未来不同嵌入算法保留 |
| 6 | 1 | flags | v1 必须为 `00` |
| 7 | 1 | reserved | v1 必须为 `00` |
| 8 | 16 | mod ID hash | 下述 SHA-256 的前 16 字节 |
| 24 | 4 | checksum | 对 offset 0–23 计算 IEEE CRC-32，big-endian |

Mod ID 必须先通过运行时同款规则 `[a-z0-9_-]{1,64}`。摘要计算为：

```text
SHA-256(
  ASCII("lom_modkit:watermark:mod-id:v1")
  || 00
  || ASCII(mod_id)
)[0:16]
```

128-bit 截断值用于离线核对，不包含明文 ID。它不是秘密：Mod ID 空间较小时仍可通过字典枚举，因此不得把它当作匿名化保证。

## 黄金向量

输入：

```text
mod_id = demo_mod
protocol_version = 1
algorithm_version = 1
flags = 0
```

输出：

```text
mod_id_hash = 720435D441F942141A10BE8AA833C874
payload = 4C4F4D5701010000720435D441F942141A10BE8AA833C8741C08EE6D
```

Python `lomc.watermark_protocol` 与 C# `ProvenanceWatermarkProtocol` 必须同时通过这条向量。改变 magic、版本、保留位或长度属于结构错误；改变 payload 内容但不更新 checksum 会保留“结构可读、CRC 失败”的解析结果，供检测器明确区分。

## 版本演进

- `protocol version` 改变 payload 字段或解释；未知协议必须拒绝。
- `algorithm version` 标识如何把同一 payload 嵌入画面；v1 解析器可保存非零的未来算法号，但检测器只有实现对应算法后才能从图像中提取它。
- v1 不使用 flags/reserved；非零必须拒绝，避免两个实现对同一字节产生不同解释。
- ECC、重复嵌入、置信度和图像变换鲁棒性属于嵌入/检测层，不写进本基础 payload，也不能修改本页黄金向量。

## Algorithm v1 画面嵌入

Algorithm v1 是一个面向截图检出的传统图像载体，不训练模型：

- 先将 224-bit payload 按 4 bit 分组，用 Hamming(7,4) 编成 392 bit。每个码字最多能纠正 1 个翻转；这不等于整幅截图必然可恢复。
- 使用固定公开 domain key 派生的 XorShift32 PRNG，对 392 个载体格进行 Fisher–Yates 置换，并为每个格生成 PN 极性。key 用于稳定布局，不是密码学秘密。
- 一个载体 tile 为 `448 × 224` 像素，由 `28 × 14` 个 `16 × 16` 格组成。每格承载一个 ECC bit，以 2 像素块的平衡黑白棋盘纹理调制亮度；RGBA alpha 固定为 `4/255`。
- tile 通过 `TextureWrapMode.Repeat` 铺满屏幕，形成重复空间嵌入。载体只改变亮度方向，不编码作者自报名称、版本或“官方”字段。
- Python `lomc.watermark_codec` 和 C# `ProvenanceWatermarkCodec` 共享布局、ECC、tile SHA-256 黄金向量；改变任一算法常量必须提升 algorithm version。

这只是截图来源提示的辅助证据。公开算法可以被复刻，压缩、裁剪、缩放、滤镜或主动去除也可能降低或破坏检出，因此不得宣传为不可移除、DRM、作者认证或官方认证。

## Host 强制边界

- 仅 Host 注册表确认的 MOD Story 会话启用；原版 Story 不启用。
- 启停方法是 Host 内部实现，没有注册为 MOD Lua API。
- 水印根、Canvas、纹理引用、透明度、排序和铺满几何会在 Update、LateUpdate 与渲染前检查并修复。
- 载体无法创建或维持时，沿用强制披露的 fail-closed 路径：停止当前 MOD Lua、显示安全遮罩并返回 `Free`。只有实际抵达可信 `Title/Free` 边界才解除会话标记。

## 验收边界

自动化测试已经验证：协议黄金向量、Hamming 单码字纠错、PRNG 载体映射、Python/C# tile 字节完全一致，以及 Host `net48` 编译。

这些测试**没有**证明游戏画面中的视觉强度或截图鲁棒性。需要在《活侠传》实机中另行验证：MOD Story 启用、官方 Story 禁用、不同分辨率铺满、Lua 破坏后的自愈，以及原图/JPEG/缩放/轻裁剪/亮度/对比度截图的检测率。离线检测器和变换语料属于下一阶段。
