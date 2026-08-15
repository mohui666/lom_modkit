# Test Matrix

仓库根目录提供统一的离线矩阵入口：

```powershell
python tools/test_matrix.py
```

不带参数只列出矩阵，不会运行测试。完整检查点显式执行：

```powershell
python tools/test_matrix.py --full --report out/test-matrix.json
```

也可以只跑一个或多个 lane：

```powershell
python tools/test_matrix.py --step compiler-tests --step runtime-build
```

矩阵固定覆盖 Compiler（含 localization/watermark）、Editor unit（含 story_api/content/package/migration/localization）、Editor smoke、Editor stress、Runtime Release build 与 Runtime SmokeTest。所有步骤离线串行运行，即使前一步失败也会继续收集后续结果；最终只要有一步失败就返回非零退出码。JSON 报告记录每步的命令、工作目录、覆盖域、退出码和耗时。

工具不会执行 Git、网络、安装、发布或修改游戏目录。当前功能广度冲刺按每 10 个 Phase 执行一次完整矩阵：Phase 46 只固化矩阵入口并做定向测试，Phase 50 才实际运行 `--full`。
