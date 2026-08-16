# Editing / Release 体检

编辑器保留两种 F6 体检 profile：

- `F6`：**Editing**。适合日常创作，沿用原有编译、流程、内容引用、登场状态等检查；
- `Ctrl+F6`：**Release**。先运行完整 Editing 体检，再增加发布严格项。

Release 额外检查：

- 占位文字提升为 error；
- Manifest 的 id、name、version、author、description、entry 缺失为 error，并执行完整格式/版本校验；
- `min_host_version` 高于当前随附 MortalModHost 为 error；
- 已启用剧情本地化但缺少支持语言时为 warning，说明会走 fallback/default；
- 包或项目资产目录中未被引用的 PNG/JPG/WAV/OGG/MP3/FLAC 为 warning；
- Editing 已发现的缺失用户内容、缺图、非法路径、跨章节断链等 package reference 错误原样保留。

严格模式不会把所有 warning 升级为 error。后景静态登场、缺语言、未使用媒体、可能的 flag 读取顺序等仍保留其原严重级别；只有明确不能安全发布的项目才阻断。

两种窗口标题和摘要都会显示当前 profile。安全自动修复完成后，会用同一个 profile 重新检查，避免 Release 窗口修复一次后悄悄退回 Editing 口径。
