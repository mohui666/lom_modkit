# Runtime Rollback

每次 MortalModHost 更新前，安装器会先保存当前受管 Runtime 的最近版本。回滚范围固定为：

- `BepInEx/plugins/MortalModHost/MortalModHost.dll`
- 当前声明的受管依赖（现为 `NVorbis.dll`）

备份位于同一宿主目录的 `.runtime_rollback` 中，扩展名为 `.rollback`，不会被 BepInEx 当成插件 DLL 加载。文件以 SHA-256 内容哈希命名，`previous.json` 只接受固定文件名和严格的 64 位十六进制摘要；恢复前会重新核对每个副本。

更新采用“全部暂存并校验，再逐个替换”的顺序。替换或安装后校验失败时，安装器会自动尝试恢复刚才保存的上一版。用户也可以在“安装管理”点击“恢复上一版”，游戏运行时会拒绝手工恢复。

第一次全新安装没有旧 Runtime，因此不会制造一个“空版本”回滚点。回滚不会读取或修改 `Mortal.exe`、`Mortal_Data/Managed`、BepInEx core、游戏存档或第三方插件；如果发现备份缺失、metadata 越界或 SHA-256 不一致，会拒绝恢复而不是使用可疑文件。
