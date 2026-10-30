# v1.3.5 — 更新检查避开 GitHub API 限流

2026-10-30。编辑器、CLI、Host 均为 **1.3.5**，旧作品兼容条件不变。

真实在线回验中发现 GitHub 匿名 REST API 返回 403 限流。本版改为读取最新正式 Release 附带的 `update.json`，无需 GitHub 登录或匿名 API 配额。更新仍直接下载四个独立 EXE／DLL，校验后覆盖并重启，不使用 ZIP。

手动按钮位于 **游戏与 MOD → 检查更新**。默认每次启动后台检查一次，只有新版本才弹窗；无新版或联网失败时静默。开关位于「设置 → 更新」，设置还包括「常规」「素材与路径」。更新后显示完成提示和当前版本，项目、用户内容库、预览库与配置均保留。

## 发布约定

每次正式 Release 使用递增的 `vX.Y.Z` Tag，上传 `update.json` 与 `lom-editor.exe`、`lomc.exe`、`MortalModHost.dll`、`NVorbis.dll`。清单记录版本、更新说明和四个文件的大小／SHA-256；文件名、安装位置及下载仓库由更新器固定。发布脚本会生成这五个附件，完整 ZIP 另供首次安装。

旧版若遇到 API 限流，可先退出编辑器，直接把本版 `lom-editor.exe`、`lomc.exe` 覆盖到原目录。游戏内 Host 仍需从「游戏与 MOD」另行安装／更新。第三方 `NVorbis.dll` 保留其自身版本。

[Windows 快速开始](docs/chs/windows_quick_start.md) · [v1.3.4 设置与版本说明](RELEASE_NOTES_v1.3.4.md)
