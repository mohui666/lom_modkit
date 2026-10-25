# 2026-10-20 Windows 本轮测试构建

已将恢复 v1.2.1 修复并保留本轮新增功能的当前 Mac 工作区传到 Windows，完成本机原生 Release 构建。没有启动游戏、安装 MOD 或进行界面人工验收；后续由用户按[测试清单](windows_postrelease_test.md)操作。

## 两端位置

| 项目 | 绝对路径 |
| --- | --- |
| Mac 源码 | `/Users/mohui666/Documents/ChatGPT/活侠传mod` |
| Mac 传输与验证记录 | `/Users/mohui666/Documents/ChatGPT/活侠传mod/out/windows-build-20261020` |
| Windows 主机 | `MOHUI666`，SSH 别名 `windows`，Windows 11 `10.0.26300` |
| Windows 本轮独立源码目录 | `C:\Users\mohui666\Documents\ChatGPT\lom-postrelease-20261020` |
| Windows 可运行程序 | `C:\Users\mohui666\Documents\ChatGPT\lom-postrelease-20261020\out\windows\lom_modkit\lom-editor.exe` |
| Windows 测试资料 | `C:\Users\mohui666\Documents\ChatGPT\lom-postrelease-20261020\tests\windows-test-kit-20261020-ready` |
| Windows 说明与完整清单 | 本轮源码根目录 `开始测试.txt`、`Windows测试清单.md` |
| Windows 构建日志 | 本轮源码根目录 `out\windows-validation` |

桌面快捷方式：`活侠传MOD编辑器-本轮测试20261020`（自动打开立绘样例）、`活侠传MOD-测试资料20261020`（打开本轮根目录）。旧开发目录和已有未提交修改未覆盖。

## 传输与构建范围

- 传输当前工作区的 303 个构建源码/配置/自制样例文件，包含未提交的新模块；逐文件 SHA-256 与 Mac 一致，构建结束后再次核对一致。清单保存在 `transfer-manifest.json`。
- 排除 `.git`、Mac 构建产物、游戏程序集、官方提取图片、整份反编译源码和其他研究目录。测试包含 6 个项目、33 章，使用自制素材或官方资源 ID。
- 使用 Windows 现有 Rust/MSVC 1.97.1、VS 2022；直接调用已安装工具链，避免该机 Rustup 代理路径的装入点错误。
- 提取器使用独立 `.extractor-venv`，安装 `UnityPy==1.24.2`、`PyInstaller==6.22.3`；不更改原 Python 环境。
- 按项目规则使用预构建 Host。来源为 Windows 原 `lom-modkit-fix-1.2.1\out\windows\lom_modkit\runtime`；77 个相关 C# 源码/工程文件在换行归一化后与本轮源码一致。没有计算游戏程序集哈希。
- 构建采用 `-NoArchive`，只生成完整可运行目录，没有制作或发布应用 Release ZIP。

实际工作入口（从 Mac）：

```sh
ssh -T -o BatchMode=yes -o ConnectTimeout=12 windows 'python.exe -X utf8 C:/Users/mohui666/Documents/ChatGPT/lom-postrelease-20261020/out/windows-validation/build-driver.py'
```

驱动在 Windows 初始化 VS x64 环境后执行：

```powershell
cargo test --locked --release --workspace
.\scripts\build-windows.ps1 -NoArchive -RuntimeDirectory 'C:\Users\mohui666\Documents\ChatGPT\lom-modkit-fix-1.2.1\out\windows\lom_modkit\runtime' -ExtractorPython '.\.extractor-venv\Scripts\python.exe'
powershell -NoProfile -ExecutionPolicy Bypass -File .\tests\windows-test-kit-20261020-ready\Run-OfflineChecks.ps1 -BundleDirectory .\out\windows\lom_modkit
```

## 本轮 Windows 证据

| 检查 | 结果 |
| --- | --- |
| Rust Release 工作区 | 235 通过，0 失败，3 原有忽略；其中编辑器 127 通过 |
| Python 测试 | 11 通过 |
| 完整程序目录构建 | 退出码 0；编辑器、编译器、提取器均为 x64 PE |
| Windows 样例离线脚本 | 65 项通过，包括正例校验/打包/检查、6 项目预览、分支测试、11 个错误样例拦截、组件自检 |
| 提取器独立依赖 | PATH 仅保留 Windows System32，移除 Python 环境变量，自检通过；`sprite_decoder_ready=true`、`game_accessed=false` |
| GUI 人工操作 / 实际游戏素材提取 / 游戏运行 | 本轮未测，留给用户测试 |

Windows 原始结果为测试资料目录 `results\20261020-203145-d318df\summary.json`；构建汇总为 `out\windows-validation\final-verification.json`，完整程序目录也有 `BUILD_INFO.json`。Mac 对应副本保存在 `out/windows-build-20261020/remote-validation` 和 `windows-offline-summary.json`。

## 用户测试顺序

1. 1.2.1 图片导入、默认立绘防覆盖、表情编号、条件必填提示。
2. 一键提取预览素材、错误目录、取消与重试、重启恢复素材库。
3. 人物 → 外观 → 表情、入魔小梅、缺失表情、赵活跟随游戏外观。
4. 同时执行：两人不同耗时、音效、偏移旋转、退场及保存撤销。
5. 黑幕保持、两个方向、中途 F5 试玩。
6. 流程图、错误定位、同名章节、音频、帮助、工具结果、导入导出。
7. 19 组重要人物画廊；按需测试长路径与异常恢复。
8. 用户自行运行游戏样例和 64 类节点综合样例，分别记录编辑器与游戏结果。
