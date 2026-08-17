# 维护者手册

编辑器工具、体检、发布、Runtime 安装与离线测试。作者日常用法见
[软件使用](software_usage.md)。

## 编辑器

### 自动恢复

未保存修改时，每 30 秒把整个多章节项目写入
`%APPDATA%\lom_modkit\recovery\<会话 ID>\snapshot.json`。
快照不含 Lua，也不会写回 `story.json` / `.lommod`。写入先落临时文件再
`os.replace`，上限 64 MiB。

启动时扫描 `active + snapshot` 且 PID 已死的会话。用户可只读检查、载入内存
（标为未保存，不覆盖原文件）、丢弃或稍后处理。恢复后的正式路径被清空，必须
另存或导出。

### 项目模板

「文件 → 从模板新建…」：空项目、线性对白、分支、自定义人物、用户内容展示。
生成结果就是普通 `story_schema` JSON。后两个模板里的 `user:template.*` 是故意
不存在的占位 ID，导出前必须换成真实内容。

### 项目统计 / 语音覆盖

「运行 → 项目统计…」只读汇总章节、节点、对白、选项、结尾、人物、图片、音频、
语音覆盖、不可达节点、未使用资产。不替代 F6。

「运行 → 语音覆盖…」按项目 / Story / 人物统计 `say.voice`；未配音行可定位到
节点。填写了错误引用仍算“已填”，合法性由体检和打包负责。

## 体检

- `F6`：**Editing**。编译、流程、内容引用、登场状态。
- `Ctrl+F6`：**Release**。先跑完整 Editing，再加发布项：占位文字、Manifest
  格式/版本、`min_host_version`、缺语言、未引用媒体。不会把所有 warning 升成
  error。自动修复后用同一 profile 再检。

## 发布构建

「文件 → 构建发布包…」只在本机：校验 Manifest → Release 体检 → 临时目录打包
→ 整包 SHA-256 与 `.sha256` 旁路文件。失败不留半包。不安装、不启动游戏、
不 Git、不上传。日常「导出 Mod」仍可选用。`.sha256` 只证明字节一致，不是签名。

## Runtime 安装与回滚

「文件 → 安装管理 → 安装诊断…」只读检查游戏目录、`Mortal.exe` 架构、BepInEx 6、
Harmony、`MortalModHost.dll` / `NVorbis.dll` 哈希、`mods` 目录、重复 DLL。
「应用安全修复」只复制编辑器内置且哈希可对的 Runtime，并创建缺失的 mods 目录。
不下载 BepInEx、不删第三方插件。

每次更新前在宿主目录 `.runtime_rollback` 保存上一版 DLL（SHA-256 文件名）。
替换失败自动恢复；也可点「恢复上一版」（游戏运行中拒绝）。第一次全新安装没有
回滚点。回滚不碰 `Mortal.exe`、Managed、存档或第三方插件。

## 离线测试矩阵

```powershell
python tools/test_matrix.py
python tools/test_matrix.py --full --report out/test-matrix.json
python tools/test_matrix.py --step compiler-tests --step runtime-build
```

不带参数只列出矩阵。覆盖 Compiler、Editor unit/smoke/stress、Runtime Release
build 与 SmokeTest。离线串行，一步失败也继续收集；最终有失败则非零退出。
不访问网络、不改游戏目录。
