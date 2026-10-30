# Windows 快速开始 · v1.3.3

## 打开软件

完整解压 `lom_modkit-v1.3.3_windows_x64.zip`，进入 `lom_modkit` 文件夹，双击 `lom-editor.exe`。保留 `lomc.exe`、`runtime`、`assets` 和 `asset-extractor`；不要只移动编辑器 EXE。工具端为 1.3.3，随附 Host 为 1.1.4。

本次为 2026-10-30 修复版。已有安装请先退出游戏，用「游戏与 MOD → 高级维护 → 安装 / 更新 C# 宿主」更新本包旁的完整 `runtime`，再重启游戏。修复时间等节点被误判为坏包的问题，现有作品无需删除时间节点或重新导出；仅替换编辑器不能更新游戏中的 Host。完整变化见随包 `RELEASE_NOTES_v1.3.2.md`。官方预览图片仍从作者自己的游戏中提取，不随包分发。

## 后续更新编辑器

打开「设置 → 检查更新」。发现 GitHub 最新正式版后，会显示版本和更新说明；选择「更新并重启」即可后台下载、校验、覆盖程序文件并自动重启，重新打开当前项目。未保存的修改会先提示保存、放弃或取消；没有新版则显示「当前已是最新版本」。

用户内容库默认在 `%APPDATA%\lom_modkit\repository`，配置在 `%APPDATA%\lom_modkit\rust`，项目素材随项目保存。内置更新只替换发布包中的程序文件，保留这些数据和安装目录里的其他文件。手动更新也可退出编辑器后用新包覆盖旧程序目录；不要删除其中自行保存的项目或素材。首次从旧版升级到 v1.3.3 仍需手动解压覆盖一次。编辑器更新不会自动替换游戏中的 Host。

## 配置自己的游戏

1. 先退出游戏，打开“游戏与 MOD”。
2. 点击“一键定位游戏目录”。它读取 Steam 及其他盘的 Steam 游戏库；找到多处安装时选择要使用的目录。找不到时点“选择游戏目录”，选择包含 `Mortal.exe` 与 `Mortal_Data` 的文件夹。
3. 核对路径和显示的 x64／x86，再点“一键配置游戏”。程序从官方来源下载固定版本、匹配架构的加载器，完成 Steam 启动修复及随包 Host 安装。
4. 等待报告完成，确认“配置就绪”。已就绪的环境可点“重新配置游戏环境”；下载失败时，从“高级维护 → 使用官方离线压缩包配置…”选择匹配版本的官方归档。
5. 用“安装当前作品”或“安装 .lommod 文件…”安装自己的作品。现有 MOD 的启用状态保持不变。

安装检查只说明文件和配置状态，不代表游戏中的剧情、任务、奖励及存档已实测。旧 v1/v2 作品包需要通过兼容编辑器重新导出。“安装／更新 C# 宿主”显示 `changed:false` 时，表示所选源文件与已安装文件相同；更新应使用本包旁的完整 `runtime` 目录。

## 创建与演示

从“文件”选择“新建原版附加剧情”或“新建自由剧本”。在“剧情内容”写对白和分支，在“剧情安排”配置日期事件及任务。Ctrl+S 保存项目目录，“导出 Mod”生成 `.lommod`。

右侧“播放”只在软件内预览；顶栏“试玩”／F5 会安装临时试玩包并启动游戏。只看软件演示时使用右侧预览。

[中文核心教程](https://github.com/mohui666/lom_modkit/blob/master/docs/chs/core_tutorial.md) · [软件使用](https://github.com/mohui666/lom_modkit/blob/master/docs/chs/software_usage.md) · [事件与任务](https://github.com/mohui666/lom_modkit/blob/master/docs/chs/events_and_tasks.md)

[11 分钟中文演示](https://github.com/mohui666/lom_modkit/releases/download/v1.3.0/lom_modkit-v1.3.0-core-tutorial-zh-CN.mp4) · [跟做项目](https://github.com/mohui666/lom_modkit/releases/download/v1.3.0/lom_modkit-v1.3.0-tutorial-follow-along.zip) · [发布与验证记录](https://github.com/mohui666/lom_modkit/releases/tag/v1.3.3)

预览素材提取助手仅用于只读提取自己安装的游戏图片。官方资源不随本包分发；导入教程与作品请使用自己有权使用的素材。第三方组件许可位于对应的 `asset-extractor` 与 `assets/doorstop` 目录；本项目许可为根目录 `LICENSE`。
