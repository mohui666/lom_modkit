# 截图来源水印检测器

`lomc detect-watermark` 是独立离线工具，用于检查 PNG/JPG 截图中是否存在 lom_modkit algorithm v1 载体。它不联网、不运行 Mod，也不需要安装《活侠传》。

## 安装与使用

剧情编译/检查/打包仍只用 Python 标准库。只有截图检测命令需要额外的 Pillow 与 NumPy：

```powershell
python -m pip install -r compiler/requirements-detector.txt
$env:PYTHONPATH = "compiler"
python -m lomc detect-watermark screenshot.png
python -m lomc detect-watermark screenshot.jpg --json
```

检测到时退出码为 `0`，未检测到时为 `2`，输入或依赖错误为 `1`。JSON 输出字段固定包括：

- `detected`：是否恢复出结构、版本和 CRC 均有效的 v1 payload；
- `confidence`：`0..1` 的载体相关度评分，只用于同一算法内排序，不是法律或统计概率；
- `protocol_version` / `algorithm_version`；
- `mod_hash`：协议中的 128-bit Mod ID 摘要，不是明文 Mod ID；
- `checksum_status`：`valid`、`invalid` 或 `unavailable`；
- `ecc_status`：`clean`、`corrected` 或 `uncorrectable`，并附 `ecc_corrections`；
- `scale_factor` / `sync_score`：诊断检测器采用的缩放候选和同步分数。

检测器会搜索原尺寸及常见的 50%、2/3、75%、80%、120%、125%、150% 缩放。任意比例、强裁剪、重压缩、锐化、模糊、降噪、再拍屏幕或主动去水印都可能无法检出。

## 结果含义

`detected=true` 只说明图像中恢复出了公开 lom_modkit 格式的载体和 Mod ID 摘要。算法、key、ECC 和 CRC 都是公开的，任何人都可以生成同格式图像；结果不能证明作者身份、发布时间、内容真实性或“官方认证”，也不能替代可见的“玩家制作 MOD｜非官方内容”披露。

`detected=false` 也不证明截图来自官方内容：水印可能从未嵌入，也可能已被图像变换或主动处理破坏。

## 自动化与实机验收

版本库的确定性 corpus 在临时目录生成同一场景的原图、JPEG 85、75% 缩放、轻裁剪、亮度、对比度和无水印对照图，并核对协议版本、Mod 哈希、CRC/ECC 与负样本。

这套 synthetic corpus 是回归基线，不是《活侠传》实机证据。发布前仍应采集不同场景、分辨率、UI 缩放和截图工具的真实 MOD/官方 Story 对照集，记录检测率和误报率；在完成这项实机测试前，不宣称特定鲁棒率。
