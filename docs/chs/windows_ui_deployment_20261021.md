# 2026-10-21 UI 逻辑新版 Windows 测试部署

已在原 Windows 测试项目内构建、打包并部署新版编辑器。桌面原“活侠传MOD编辑器-本轮测试20261020”快捷方式已指向新版，继续打开原立绘测试项目。旧程序、原源码与测试资料保留；原快捷方式另有备份。本轮没有启动游戏或向实际游戏目录安装组件。

## 两端路径与文件范围

| 项目 | 绝对路径 |
| --- | --- |
| Mac 工作区 | `/Users/mohui666/Documents/ChatGPT/活侠传mod` |
| Mac 证据与 ZIP | `/Users/mohui666/Documents/ChatGPT/活侠传mod/out/windows-ui-20261021` |
| Windows 主机 | `MOHUI666`，Windows 11 Pro `10.0.26300`，SSH 别名 `windows` |
| Windows 原测试项目 | `C:\Users\mohui666\Documents\ChatGPT\lom-postrelease-20261020` |
| Windows 本轮源码快照 | 原测试项目 `out\ui-logic-20261021\source` |
| Windows 程序 | 原测试项目 `out\windows\ui-logic-20261021\lom_modkit\lom-editor.exe` |
| Windows ZIP | 原测试项目 `out\windows\ui-logic-20261021\lom_modkit-v1.2.1_windows_x64.zip` |
| Windows 证据 | 原测试项目 `out\ui-logic-20261021` |
| 用户说明 | 原测试项目 `新版UI测试说明20261021.txt` |

快照最终 308 个源码、配置、资源索引、自制样例与文档输入逐文件校验一致。不包含 Git 历史、游戏程序集、官方提取图片或整份反编译源码。旧测试源码未覆盖；新快照通过目录联接复用原 `target` 编译缓存，提取器复用原 `.extractor-venv`。使用原权限修复版 `runtime` 下的预构建 Host，复制后字节一致；没有计算游戏程序集哈希。

## 交付行为

- 顶栏直接进入剧情编辑、素材管理、游戏与 MOD、发布；管理页面使用完整工作区。
- 游戏目录选择、一键配置、MOD 安装和列表集中在“游戏与 MOD”；手动宿主选择、离线加载器、修复和回滚位于高级维护。
- MOD 列表独立保存；安装与启停后刷新，不受其他工具报告影响。
- 素材分作品素材、游戏预览素材、共享内容库；切页保护未完成输入，管理页不会通过步骤快捷键修改隐藏的剧情。
- 发布按检查、导出排列；保留原打包、统计和诊断能力。

## 本轮验证

| 检查 | 结果 |
| --- | --- |
| Windows Rust Release 工作区 | 247 通过，0 失败，3 项原有忽略 |
| 已打包程序的样例离线检查 | 65 / 65，通过；结果 `20261021-183505-5c7b94` |
| 编辑器、编译器、提取器 | 均为原生 x64 PE；提取器自检 `sprite_decoder_ready=true`、`game_accessed=false` |
| 新下载代码的 Windows PowerShell 本地回环检查 | 正常响应完整保存；超限 Content-Length 在写文件前拒绝；无长度响应最多写入 32 MiB 后拒绝 |
| ZIP | 262 个文件，逐项与部署目录字节一致；下载回 Mac 后整体 SHA-256 一致 |
| 桌面快捷方式 | 读取回验目标、工作目录及原样例参数；原 `.lnk` 备份保留 |
| 游戏与人工界面验收 | 本轮未启动游戏；自动化和离线结果不替代游戏或逐项人工界面验收 |

成品自动画面启动检查未完成：SSH 会话截图等待 30 秒超时；改用已登录桌面时，Windows 任务登记返回 `0x80070057 / UserId` 参数错误。两次记录均保留，未计为通过；没有留下已登记的临时启动任务。用户仍需从桌面快捷方式手动验证窗口、菜单和安装流程。

ZIP SHA-256：`67177f653ef93b88010be77671cc9beb1fe5f1cf95a94f4052ad924602eff440`。

首次完整打包因 archspec 自带 `json/tests` 第三方测试数据触发禁止夹带测试目录的规则而失败。新增 PyInstaller 收集钩子，仅排除该依赖测试数据，保留运行需要的 CPU 表；重新打包及全部样例离线检查通过。首次失败日志与暂存包保留，没有放宽整个安装包的排除规则。

## 实际远程入口

以下是本轮实际执行入口。复跑应选择新输出目录，不直接覆盖本轮证据；原驱动对已有目录明确拒绝。

```sh
ssh -T -o BatchMode=yes -o ConnectTimeout=12 windows 'python.exe -X utf8 C:/Users/mohui666/Documents/ChatGPT/lom-postrelease-20261020/out/ui-logic-20261021/build-driver.py'
ssh -T -o BatchMode=yes -o ConnectTimeout=12 windows 'python.exe -X utf8 C:/Users/mohui666/Documents/ChatGPT/lom-postrelease-20261020/out/ui-logic-20261021/resume-driver.py'
ssh -T -o BatchMode=yes -o ConnectTimeout=12 windows 'python.exe -X utf8 C:/Users/mohui666/Documents/ChatGPT/lom-postrelease-20261020/out/ui-logic-20261021/deploy.py'
```

驱动复用 Rust/MSVC 1.97.1、VS 2022 环境，执行工作区测试、`scripts/build-windows.ps1` 完整 ZIP 构建及既有测试资料的 `Run-OfflineChecks.ps1`。ZIP 属于本轮测试构建，没有发布到远端 Release。
