# lom_modkit v1.1.1

这是 1.1.0 的补丁版本，重点修复首次使用者无法通过 F5 建立试玩战役，以及发布体检无法说明项目级错误的问题。Editor、Compiler 与 MortalModHost 统一升级为 `1.1.1`；Runtime 游戏功能契约不变。

## 修复

- **F5 改由 Steam 启动游戏**：编辑器不再直接执行 `Mortal.exe`，而是调用 `steam://rungameid/1859910`，确保原版 `SaveSystem.NewGameData()` 调用 `SteamUser.GetSteamID()` 时已有 Steamworks 上下文。若 Steam 协议不可用会明确报错，不再回退到必然产生坏试玩进程的直启方式。
- **发布体检可定位项目资料**：缺失的 Mod 标识、名称、版本、作者和简介合并为一条可读错误；双击即可打开发布信息页。空版本不再同时出现“缺失”和“格式错误”两条重复报告。
- **发布信息独立入口**：文件菜单新增“编辑发布信息…”，无需先进入导出流程即可维护 Manifest。
- **冻结版启动稳定性**：PyInstaller 每次执行干净构建，并排除从构建机 PATH 误识别的 Poppler 私有 ICU DLL，避免成品导入 QtCore 时出现 WinError 127。

## 维护

- 保留 v1.1.0 之后的仓库瘦身和无界面 smoke 测试稳定性修复。
- Steam App ID 改为安装与诊断共用的单一常量。
- 增加 Steam URI 启动、发布体检聚合与定位的回归测试。

## 验证

- Gameplay API 当前安装版本：43 个契约探针通过，并实时反编译复核 `Mortal.Core.SaveSystem`。
- Editor：337 项 unittest 与完整 smoke 测试通过。
- Runtime：Release 构建与 SmokeTest 通过。
- 冻结版：GUI Lua 预览自检和 `story_api_cli` 校验通过；Windows ZIP 内容及 SHA-256 由发布脚本复核。

## 发布文件

- `lom_modkit-v1.1.1_windows_x64.zip`
- `lom_modkit-v1.1.1_windows_x64.zip.sha256`
