# lom_modkit 来源水印协议 v1

> 状态：协议已冻结并有 Python/C# 黄金向量；本阶段只定义 payload，不代表画面嵌入器已经启用。嵌入算法与离线截图检测器分别属于后续阶段。

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
