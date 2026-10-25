# Windows 回归测试包

先看 `Windows测试清单.md`。建议按 **A → B → C → D → E → F** 测，游戏回归 G 单独记录。

这个包包含 6 个可编辑项目、对应 lommod、19 组重要人物画廊、64 类节点综合样例、11 个错误样例、自制图片/音频与离线检查脚本，**不含 Windows 程序**。需要当前源码重新构建的完整 Windows 程序目录；网上原版 v1.2.1 没有这轮新功能。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\Run-OfflineChecks.ps1 -BundleDirectory 'D:\你的新构建\lom_modkit'
```

然后用编辑器打开 `projects` 中项目的副本。测试修改请另存，结果填 `测试结果模板.md`。

`validation` 是生成时的 Mac 本地校验记录，不是 Windows 通过证明。Windows 脚本的新结果会单独写到 `results`。本包不包含官方游戏图片，也不会自动安装 MOD 或启动游戏。
