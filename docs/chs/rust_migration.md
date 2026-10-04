# Rust 工具端迁移与验收

编辑器与编译器现由 `rust/lom-editor`、`rust/lomc` 和 `rust/lom-core` 提供。C# `MortalModHost` 保留，包契约仍为 `package_format=3`、`story_schema=2`、`content_schema=1`。编辑、编译、测试、打包不再调用 Python、Qt 或 PyInstaller。

## 功能对应

| 原工具功能 | Rust 实现 | 验证方式 |
| --- | --- | --- |
| 63 类节点、字段与默认值、Lua 生成、错误拒绝 | core `validate`、`codegen`；editor `forms` | 旧版输出与非法输入黄金案例；全部节点预览 |
| 受控脚本新增/修改/删除/重命名/移动，登场保护、骰子与死亡节点 | core `story_api`；CLI `author`、`edit` | 181 个旧版 API 成功/拒绝案例 |
| 多章节项目、原子保存、旧格式迁移、完整性校验、确定性 ZIP | core `project`、`migration`、`package`；editor `persistence` | 原始路径、未知字段、非本项目文件、失败与恶意包测试 |
| 撤销/重做、多选、批量编辑、模板、分组、自动恢复 | editor `workspace`、`authoring` | 编辑器状态、资产字节和恢复所有者测试 |
| 全局分类查找、精确引用、跨章节范围复制及边界告警 | core `editing`；editor `advanced` | 大小写折叠、嵌套引用、链接重映射与失败不修改 |
| 变量/Flag、条件检查、路径模拟、离线测试 | core `analysis`；editor `advanced` | CFG、跨章节、循环、未知状态、断言与非零退出 |
| 内容库导入/替换/删除、立绘表情、介绍卡、战斗四图、音频绑定 | core `content`、`content_edit`；editor `content_panel` | metadata、引用保护、事务失败、共享库备份与符号链接边界 |
| `.lomcontent` 导入/导出、版本/依赖/冲突 | core `content_library`；editor `tools_panel` | 内容包黄金案例与安全测试 |
| 试听与停止 | editor `audio` | 拥有所启动的播放器子进程；关闭编辑器时停止。WAV 用系统音频，OGG 需要 FFplay，可设 `LOM_FFPLAY` |
| 发布身份、自由模式位置/月份/旬/旗标/好感度条件 | editor `manifest_panel` | 空操作保留未知字段与未来值；现有格式校验 |
| 四语言 UI、对白翻译/回退、覆盖统计、导入/导出/停用 | editor `i18n`、`workspace`；core `localization` | 字典、回退、字段和 ID 保持测试 |
| 舞台/流程/Lua/人物预览与三栏布局 | editor `preview`、`workspace`、`shell`、`macos_window` | 全样例 133 节点离屏预览与 macOS 标题栏、布局检查 |
| 包检查器、文件预览、校验摘要、报告定位 | core `package::inspect_package`；editor `tools_panel` | 损坏包只读预览、危险路径拒绝、文件摘要与报告跳转目标测试 |
| 统计、语音覆盖、编辑/发布体检、安全修复、发布构建、脱敏诊断包 | core `release`；editor `tools_panel` | 统计与发布黄金案例、诊断包白名单、修复测试 |
| 水印与视频水印检测 | core `watermark`；CLI 与工具面板 | 离线图片测试；视频/外部素材语料测试单独标注 |
| 游戏安装管理、诊断/回滚、启用 Mod、F5 请求、已读状态工具 | core `game_tools`；editor `tools_panel` | 仅合成目录与协议离线测试；未做 Windows 或游戏实机验收 |
| 全节点样例生成器 | `rust/lomc/examples/build_showcase3.rs` | 独立 source.json 与旧生成器 5 章输出一致；校验 63 类并导出包 |

新增工具入口集中在“创作工具”；全局查找快捷键为 `Ctrl/⌘+Shift+F`。受控 API 已迁到 Rust/JSON CLI，旧 Python `import story_api` 不再是受支持入口。运行时相关结果仍可能为 `unknown` / `unsupported`，不会把静态推断当作实机结果。

当前先完成 Rust 通用外壳，统一浅色字体、控件、紧凑步骤列表与内容库，保留三栏布局。状态栏仅显示保存状态及实际操作结果。Mac 原生玻璃叠层因遮挡标题栏/正文已移除，Swift 玻璃外壳延后；不能把它计为已完成。功能对应表及自动化案例也不代表已逐项完成全部 GUI、Windows 和游戏实机验收。

## 构建与检查

以下样例生成命令使用的 `out/showcase3-native` 必须是全新输出目录；已存在时换一个新目录，避免覆盖现有项目。

```sh
cargo build --locked --release --workspace
cargo test --locked --workspace
cargo run --locked -p lom-editor -- --smoke-preview samples/showcase3
cargo run --locked -p lomc --example build_showcase3 -- out/showcase3-native
cargo run --locked -p lomc -- inspect out/showcase3-native/showcase3.lommod --json
scripts/build-macos.sh
```

Mac 产物：`out/LoM Modkit Rust.app`，包含 `lom-editor` 与 `lomc`。现有 Qt 应用、venv 和个人文件不会被清理脚本删除。Windows 构建入口是 `scripts/build-windows.ps1`；它使用 Cargo 和已构建的 C# DLL，支持 `-NoArchive`，Rust 迁移阶段仅更新脚本，未执行 Windows 构建或测试。

`source.json` 是样例的独立创作源；修改样例时先更新它，Rust 生成器重新校验并写出章节及包。全节点检查随嵌入的 authoring schema 自动变化。

## 删除与保留范围

旧 `compiler/lomc`、Qt 编辑器 `.py`、旧 Qt/Python 测试、PyInstaller spec/依赖、Python 样例生成器和旧测试矩阵退役。用于历史对照的黄金 JSON/Lua 保留，运行测试无需 Python。

`tools/` 中的 UnityPy 游戏资源提取、反编译接口研究与其独立测试仍有用途，予以保留；它们不是编辑器或编译器运行依赖，也不随应用发布。GitHub 语言统计仍可能显示少量 Python，不代表工具运行时未迁移。完整反编译源码和官方资源不会提交。

## 分支保护

远端默认主分支为 `master`。要求通过 Pull Request 合并，并且 `Native Rust tools (macOS)` 与 `Runtime tests` 成功、分支跟上主分支、讨论已解决；规则对管理员生效，禁止强推和删除。当前不要求额外审批人数（0），避免单人仓库无法自审；CI 不包含 Windows 或游戏实机测试。

## 已记录的实现验证（2026-10-04，Apple Silicon macOS）

以下结果来自 [PR #4](https://github.com/mohui666/lom_modkit/pull/4) 与 [PR #5](https://github.com/mohui666/lom_modkit/pull/5) 及前序迁移，最近代码合并为 `6e61602`。随后文档维护只核对文档、命令与链接，没有重新运行本机软件或游戏验收。

- `cargo test --locked --workspace --release`：179 通过、0 失败，2 个外部依赖案例默认跳过。其中受控 API 对照测试内部逐项检查 181 个旧版案例；新增回归覆盖 63 类表单不因展示而改值、模态与输入法、统一保存/撤销、自动恢复、删除后保存重开、四语言字形、资源名称补译、同名选项对应原 ID 及系统设置菜单事件。最后的预览名称显示调整另行通过 7 项定向预览测试。
- FFmpeg 合成视频抽帧测试在此前迁移验收中通过；这些 UI 修改未重跑外部截图/视频语料测试。
- Rust 生成器：5 章、133 节点、63 类型；原生应用内嵌预览：133 / 133。
- Mac `.app` 构建及签名校验通过；实际窗口检查了四语言设置与字形、新建取消、命名输入保持、回车复制、保存后撤销章节、素材导入与改名、删除后保存重开。重开确认已撤销章节与已删除素材没有复活；保留三栏布局，常规表单不再显示启用字段的复选框或技术信息块。后续实际窗口确认资源译名、同名编号、系统菜单“设置…”与 `⌘,`、语言切换后菜单标题同步。
- 删除 157 个旧 Python 源文件及 5 个 spec/依赖文件。保留 10 个资源研究/提取及配套测试脚本。
- Windows 构建、Windows 测试及游戏实机测试均未执行。上述测试不能替代这些平台的验收。
