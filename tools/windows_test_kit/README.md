# Windows 回归测试包

先看 `Windows测试清单.md`。建议按 **A → B → C → D → E → F** 测，游戏回归 G 和环境维护 H 按需单独记录。

这个包保留 v1.2.1 时期的回归基线，包含 6 个可编辑项目、对应 lommod、19 组重要人物画廊、64 类节点综合样例、11 个错误样例、自制图片/音频与离线检查脚本，**不含 Windows 程序**。64 类是本测试包的历史样例范围，当前全节点样例见 [samples/showcase3](../../samples/showcase3/README.md)。

仓库中的本目录保存说明与检查脚本；`projects`、`fixtures`、`packages` 等数据由 [测试包生成器](../build_windows_test_kit.py) 写入独立输出目录。下列命令在已生成的测试包目录中执行。

1.6.0 使用内置组件的单个 `lom-editor.exe`，普通使用不需要 Python 或旁置编译器。用当前 exe 可在测试包目录逐项检查：

```powershell
& 'D:\LoM\lom-editor.exe' --cli check .\projects\04_editor\story\main.json --json
& 'D:\LoM\lom-editor.exe' --cli test .\projects\04_editor --json
& 'D:\LoM\lom-editor.exe' --smoke-preview .\projects\04_editor
```

`Run-OfflineChecks.ps1` 仍使用旧目录包接口：`-BundleDirectory` 必须同时包含 `lomc.exe`、`lom-editor.exe` 和 `asset-extractor/lom-asset-extractor.exe`，不能直接传入只有 1.6.0 单 exe 的目录。保留兼容目录时可继续运行旧脚本；当前单 exe 的打包、检查和工具命令见 [CLI 参数](../../docs/chs/cli_parameters.md)。

然后用编辑器打开 `projects` 中项目的副本。测试修改请另存，结果填 `测试结果模板.md`。

`validation` 是生成时的 Mac 本地校验记录，不是 Windows 通过证明。Windows 脚本的新结果会单独写到 `results`。本包不包含官方游戏图片，也不会自动安装 MOD 或启动游戏。
