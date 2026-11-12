# 测试入口与验证边界

本版变化见 [更新说明](../../RELEASE_NOTES_v1.5.12.md)；历史 1.4.19 验证记录见 [当前能力](current_capabilities.md#1419-验证范围)。

## 验证层次

| 层次 | 当前入口 | 覆盖与限制 |
| --- | --- | --- |
| Rust 格式 | `cargo fmt --all -- --check` | 源码格式，不证明运行正确 |
| Rust workspace | `cargo test --locked --workspace`；发行配置另加 `--release` | 编译、包格式、受控创作 API、内容管理、编辑状态、表单、保存／恢复、模态与语言等单元／集成回归 |
| 样例预览 | `cargo run --locked -p lom-editor -- --smoke-preview samples/showcase3` | 逐节点校验、编译与预览状态模拟；不是逐场游戏实机验收 |
| 样例生成与包检查 | `lomc` 的 `build_showcase3` example 和 `inspect` | 源 JSON 校验、73 类型覆盖、打包、包内源码／Lua 一致性 |
| Mac 应用构建 | `sh scripts/build-macos.sh` | 原生程序、应用资源、签名检查与内置预览烟测；GUI 仍需另外打开检查 |
| C# 离线回归 | `dotnet run --project runtime/MortalModHost/test/SmokeTest/SmokeTest.csproj --configuration Release` | 使用离线测试工程；不启动 Unity，也不验证真实游戏生命周期 |
| Windows 原生工具 | `scripts/build-windows.ps1 -NoArchive` | 使用正式 Host DLL 构建；构建、自动化与实际 UI 结果分开记录 |
| Windows 单 exe | 发布版 `lom-editor.exe --standalone-smoke <项目> <游戏目录> <空输出目录>` | 使用内嵌组件完成项目导出重开、完整游玩包导出及 13 项音频播放／停止／取消；组件缓存必须与旧安装隔离 |
| Rust 资源读取 | `lom-assets` 与 `vanilla_routes` 定向测试；本机游戏对照另行显式执行 | 合成音频包与路线 fixture 不含官方素材；全量游戏图片、动画、剧情及 45 分支对照不进入仓库，不等于游戏运行验收 |
| 游戏实机 | [Showcase 手动流程](../../samples/showcase3/README.md#手动测试顺序) | 按具体场景记录结果，本版范围见当前能力 |

受控 API 对照测试内部的旧版案例不能再加到 Rust 测试总数上重复计数；不同阶段重跑、预览状态、C# 冒烟、构建与 UI 操作也分别记录。旧 JSON／Lua fixture 是历史对照数据，当前测试不启动已退役的 Python 编辑器或编译器。

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

用 `cargo test --locked --workspace -- --list --ignored` 查看当前忽略清单。下列用例按需准备依赖后单独执行。

- 官方加载器 ZIP 安装测试：需 `LOM_BEPINEX_ARCHIVES` 指向固定版本的官方归档，仅写临时合成安装目录。

- `ffmpeg_real_video_extraction`：需 `LOM_FFMPEG` 指定可执行程序，并显式选择该 ignored 测试。
- 历史截图水印语料测试：需 `LOM_WATERMARK_CORPUS` 指向事先生成的外部语料。
- 公开版本的实际更新下载测试，需要网络和已发布附件。

手工性能测试使用合成大项目，单独运行并记录面板耗时。缺少依赖、默认忽略与实际失败分别记录。

## GUI 与 CI

当前 [CI 配置](../../.github/workflows/ci.yml) 仅保留 Ubuntu 上的 `Runtime tests`，运行 C# 离线测试。Rust 格式、测试、预览与样例生成按上面的本地命令验证。macOS CI 任务已移除；CI 不承担 Windows 或游戏实机验证，也不替代本机窗口的视觉检查。

维护者操作与恢复目录说明见 [维护者手册](maintainers.md)。
