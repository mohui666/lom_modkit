# Windows 临时目录拒绝访问修复（2026-10-20）

用户转述的错误为 `拒绝访问 (os error 5) at path "C:\Temp\.tmp…"`，涉及 BepInEx、Steam 启动修复及 C# 宿主安装。对方版本和游戏路径未知，不能据这段转述断定三个操作都是同一故障。

当前源码确实存在系统 TEMP 依赖：Steam 修复暂存 Doorstop、MOD 安装的校验快照、试玩导出、包内素材校验、预览编译等操作会创建工作文件。共享 TEMP 不可写时，这些步骤可能在操作完成前失败。BepInEx 和宿主安装还可能遇到真实游戏目标目录的权限或文件占用；后者不能通过更换工作目录解决。

## 改动

- 新增共用工作目录入口。Windows 使用 `%LOCALAPPDATA%\lom_modkit\cache\work\lom-modkit-*`，缺少 `LOCALAPPDATA` 时从 `USERPROFILE\AppData\Local` 取得用户缓存位置；每次独立暂存，正常结束后清理本次子目录。
- Steam 修复、MOD 校验快照、试玩导出、包资源检查、预览编译及相关发布检查使用此入口，不修改全局 TEMP/TMP。
- 文件写入失败说明是在创建临时工作目录、读取宿主源文件、读取已装宿主、创建目标临时文件、备份还是替换目标文件，并保留原始错误及实际路径。
- `changed:false` 且无错误时，结果提示“内容已一致，无需更新，未修改文件”；包含 `error` 时仍优先显示“操作未完成”。

## Windows 验证

在 Windows 11 `10.0.26300` 的本轮独立工作目录中重新构建。系统与真实游戏目录均未修改权限，未启动游戏；ACL 测试只操作本轮新建的合成目录。

| 检查 | 结果 |
| --- | --- |
| Release 工作区 | 238 通过、0 失败、3 原有忽略 |
| 样例离线检查 | 65 项通过 |
| 当前进程 TEMP 无法创建文件 | Win32 `CreateFileW` 实际返回错误 5 |
| TEMP 被拒绝时加载带自制素材的包并编译预览 | 通过 |
| TEMP 被拒绝时执行编辑器 Steam 修复入口、宿主安装/重复安装、试玩包导出、MOD 校验与安装、试玩请求 | 合成游戏目录通过；没有启动游戏 |
| TEMP 被拒绝时安装匹配的官方 x86/x64 BepInEx ZIP | 通过；错误位数与篡改包仍拒绝，用户文件保留 |
| 目标写入目录被拒绝 | 明确失败并指出目标路径，未创建目标文件 |
| 用户工作目录本身不可用 | 明确失败并指出工作目录，不伪报成功 |
| 测试 ACL 恢复 | 已恢复 |

ACL 复验曾发现预览编译还在使用系统 TEMP，已补齐并重新构建、通过完整验证。初次 SID 参数错误、仅探测目录创建而未探测文件创建等测试过程及首次预览失败日志均保留，未计为通过。

Mac 工作区回归为 239 通过、3 忽略；补齐预览编译后重跑编辑器 124 通过，并重新构建签名应用。这些不计入 Windows 结果。

## 路径与复现入口

- Mac 源码：`/Users/mohui666/Documents/ChatGPT/活侠传mod`。
- Mac 证据：`out/windows-temp-permission-fix-20261020/`。
- Windows 源码：`C:\Users\mohui666\Documents\ChatGPT\lom-postrelease-20261020`。
- Windows 完整程序：上述目录 `out\windows\lom_modkit-tempfix\lom-editor.exe`。
- Windows 证据：上述目录 `out\temp-permission-fix-20261020`，包括 `final-verification.json`、`acl-verification.json` 和构建/测试日志。
- 桌面“活侠传MOD编辑器-本轮测试20261020”已指向权限修复版；原完整程序目录保留。
- 本轮迁入 11 个修改/新增源码文件，原文件已备份；合并后的 304 个构建输入逐文件复核一致。使用预构建的 v1.2.1 Host，未计算游戏程序集哈希，未制作应用 Release ZIP。

从 Mac 运行的实际入口：

```sh
ssh -T -o BatchMode=yes -o ConnectTimeout=12 windows 'python.exe -X utf8 C:/Users/mohui666/Documents/ChatGPT/lom-postrelease-20261020/out/temp-permission-fix-20261020/build-driver.py'
ssh -T -o BatchMode=yes -o ConnectTimeout=12 windows 'python.exe -X utf8 C:/Users/mohui666/Documents/ChatGPT/lom-postrelease-20261020/out/temp-permission-fix-20261020/acl_reproduction_final.py'
```

不要直接再次运行第二条覆盖已有测试记录：它要求全新的合成目录，并会在 `finally` 恢复测试 ACL。需要复验时先为测试脚本选择新的输出目录。

## 对方复测

保存作品并退出旧编辑器，使用完整权限修复版目录，重新选择正确的游戏目录和本版 `runtime` 目录，再测试三项安装/修复操作。相同宿主第二次安装应提示无需更新。

若仍拒绝访问，保留完整报告：错误指向工作目录时检查该用户缓存路径；指向游戏目录/DLL 时检查该目标的写权限、只读属性和文件占用。对方机器尚未复测，不能把本轮合成目录结果当作其问题已经消失。
