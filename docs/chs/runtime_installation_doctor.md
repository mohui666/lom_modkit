# Runtime Installation Doctor

在“文件 → 安装管理”点击“安装诊断…”可以离线检查游戏侧的 Mod 运行环境。诊断只读取文件、目录、PE 头与 SHA-256，不加载或执行任何 DLL。

检查范围：

- 《活侠传》目录和 `Mortal.exe` 架构；
- BepInEx 6 Core、Unity Mono 与 Harmony；
- `MortalModHost.dll` 是否缺失或与编辑器内置版本不一致；
- 当前 Runtime 实际依赖 `NVorbis.dll` 是否缺失或过期；
- `mods` / `mods_disabled` 是否存在；
- BepInEx plugins 下是否还有旧布局或其他目录中的 `MortalModHost.dll` / `NVorbis.dll` 重复副本。

“应用安全修复”只会复制编辑器随附且哈希可核对的 Runtime/依赖，并创建缺失的两个 Mod 目录。它不会下载 BepInEx、删除重复 DLL、修改第三方插件或启动游戏。BepInEx 安装仍使用现有的单独确认流程；重复文件会列出完整路径，留给用户核对来源后手工处理。
