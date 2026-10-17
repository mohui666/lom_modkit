# Unity Doorstop 代理

编辑器按 `Mortal.exe` 的 PE Machine 选择代理，与编辑器自身的位数无关。
`win-x86-doorstop.dll` 保留仓库已有的 4.3.0 文件；新增的
`win-x64-doorstop.dll` 是上游 4.3.0 Windows Release 压缩包内
`x64/winhttp.dll` 的原始字节，安装时命名为 `version.dll`。

- [上游 v4.3.0 发布页](https://github.com/NeighTools/UnityDoorstop/releases/tag/v4.3.0)
- [Windows Release 下载](https://github.com/NeighTools/UnityDoorstop/releases/download/v4.3.0/doorstop_win_release_4.3.0.zip)
- [对应源代码](https://github.com/NeighTools/UnityDoorstop/tree/v4.3.0)
- 许可：上游 GNU LGPL 2.1，见 [LICENSE.UnityDoorstop](LICENSE.UnityDoorstop)。

2026-10-17 从上述官方来源取得并核对：

| 文件 | SHA-256 |
| --- | --- |
| `doorstop_win_release_4.3.0.zip` | `e23fec7a6adfa07ba26a3e209b617584d0319a1145506ae4e1b892c44e86f679` |
| `win-x64-doorstop.dll` | `d3b62f4c9c3e2196ef82603c52d6b98c043a0d6c125c081fd33d7ca3798b41b8` |
| 既有 `win-x86-doorstop.dll` | `6d33639f3c330efa0afd9c48801091d8acae43eed606d2b64ab49db5fea7c308` |

这里只记录加载器分发物；不计算游戏程序集哈希。文件校验与 PE 架构检查不能代替 Windows／游戏启动验收。
