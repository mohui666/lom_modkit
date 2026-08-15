# MortalModHost Runtime

MortalModHost 是 BepInEx 6 / Harmony 宿主，负责加载 `.lommod`、把编译后的 Lua 接入《活侠传》现有 Fungus/LuaManager 流程，并承载自定义媒体、战役入口和强制非官方来源披露。

## 接口原则

- 能调用原版系统的节点使用已经由反编译确认、并由 Host 构建/SmokeTest 覆盖的真实方法签名；完整映射以 `docs/zh_CN/mod_format.md` 为准。
- `enemy`、`battle_skill`、`goto_scene Combat/Battle` 与高层 `combat` 只调用原版战斗 API。`combat` 观察已核验的 `CombatManager.GameOver(bool)`，把 win/lose 一次性回流到同包同剧情；结果所有权绑定包 id + 完整 SHA-256。Runtime 不包含自定义 Battle Engine，也不伪造 draw/escape。
- 自定义角色和媒体是独立 Mod Runtime 对象，不伪造或覆盖原版 Addressables 条目。
- Manifest、路径、包大小、哈希和脚本注册名在 Runtime 再次验证，不能只信编辑器导出的包。

Gameplay / Combat 的程序集哈希、反编译签名、可实现范围与禁止猜测项见 [`research/gameplay_api.md`](../../research/gameplay_api.md)。`tools/verify_gameplay_api.py` 可对本机安装只读复验；游戏更新后旧哈希不会被静默接受。

## Fail-closed 边界

Mod 演出一旦开始即保持来源会话污染标记，只有真正进入官方 Title / Free 才解除。必需披露表面无法创建或恢复、Lua 加载/执行失败、或补丁不完整时，Host 停止 Mod 协程并返回 Free；不会继续无标播放。包指纹和画面水印用于文件/画面来源核对，不是官方签名或作者认证。

## 构建与测试

项目目标为 .NET Framework 4.8。正式构建见 `scripts/build_host.ps1`；纯 C# 契约测试在 `test/SmokeTest`。仓库级离线矩阵由 `tools/test_matrix.py` 统一调用。功能冲刺采用每 10 个 Phase 一次完整矩阵，中间阶段只跑改动对应的定向检查。
