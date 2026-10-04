# 测试矩阵与验证边界

本文列出当前 Rust／C# 验证入口。2026-10-04 本轮仅核对文档和源码，没有重跑测试、构建或实机流程。已记录的上一轮 Apple Silicon Mac 结果为：Rust **179 项通过、0 失败、2 项默认跳过**；Showcase 3.0 为 5 章、133 节点、63 类型，预览检查 **133 / 133**。详细记录见 [迁移验收表](rust_migration.md)。

## 验证层次

| 层次 | 当前入口 | 覆盖与限制 |
| --- | --- | --- |
| Rust 格式 | `cargo fmt --all -- --check` | 源码格式，不证明运行正确 |
| Rust workspace | `cargo test --locked --workspace`；发行配置另加 `--release` | 编译、包格式、受控创作 API、内容管理、编辑状态、表单、保存／恢复、模态与语言等单元／集成回归 |
| 样例预览 | `cargo run --locked -p lom-editor -- --smoke-preview samples/showcase3` | 逐节点校验、编译与预览状态模拟；不是逐场游戏实机验收 |
| 样例生成与包检查 | `lomc` 的 `build_showcase3` example 和 `inspect` | 源 JSON 校验、63 类型覆盖、打包、包内源码／Lua 一致性 |
| Mac 应用构建 | `sh scripts/build-macos.sh` | 原生程序、应用资源、签名检查与内置预览烟测；GUI 仍需另外打开检查 |
| C# 离线回归 | `dotnet run --project runtime/MortalModHost/test/SmokeTest/SmokeTest.csproj --configuration Release` | 使用离线测试工程；不启动 Unity，也不验证真实游戏生命周期 |
| Windows 原生工具 | `scripts/build-windows.ps1 -NoArchive` | 保留构建入口，需要 Windows 及已构建 C# DLL；本次迁移未执行 |
| 游戏实机 | [Showcase 手动流程](../../samples/showcase3/README.md#手动测试顺序) | 只在另行明确授权后执行；本次迁移未执行 |

受控 API 的一个对照测试内部检查 181 个旧版案例，不能把它再加到 179 项 Rust 测试上重复计数。旧 JSON／Lua fixture 是历史对照数据，当前测试不启动已退役的 Python 编辑器或编译器。

## 常用命令

在仓库根目录执行。`sh scripts/test-native.sh` 串联格式、workspace 测试、预览、生成器和包检查。下列独立步骤便于区分结果：

```sh
cargo fmt --all -- --check
cargo test --locked --workspace --release
cargo run --locked -p lom-editor -- --smoke-preview samples/showcase3
cargo run --locked -p lomc --example build_showcase3 -- out/showcase3-check
cargo run --locked -p lomc -- inspect out/showcase3-check/showcase3.lommod --json
sh scripts/build-macos.sh
```

样例生成应使用尚未存在的输出目录。当前生成器创建新 Project，不能凭命令中的输出路径认领已有章节文件；若 `out/showcase3-check` 已有生成内容，改用新的目录名。固定使用 `out/showcase3-native` 的 `scripts/test-native.sh` 也受此限制，不应通过删除未知文件或绕过所有权检查继续。

## 需要外部依赖的案例

当前默认跳过两项：

- `ffmpeg_real_video_extraction`：需 `LOM_FFMPEG` 指定可执行程序，并显式选择该 ignored 测试。
- 历史截图水印语料测试：需 `LOM_WATERMARK_CORPUS` 指向事先生成的外部语料。

此前 FFmpeg 合成视频验证有通过记录；它不表示本轮重跑了视频或外部图片语料。缺少依赖、默认跳过和实际失败应分别记录。

## GUI 与 CI

上一轮 Mac 窗口检查包括：系统应用菜单的设置入口、四语言字形、命名和输入法、模态确认／取消、素材导入与名称编辑，以及保存后撤销章节、删除素材后重开的持久化行为。该记录不覆盖 Windows UI，也不代表 Swift 或 Liquid Glass 已完成。

当前 [CI 配置](../../.github/workflows/ci.yml) 包含 `Native Rust tools (macOS)` 与 Ubuntu 上的 `Runtime tests`。前者运行格式、Rust 测试、预览和样例生成；后者运行 C# 离线测试。CI 没有 Windows 或游戏实机任务，也没有替代本机窗口的视觉检查。

维护者操作与恢复目录说明见 [维护者手册](maintainers.md)。
