# 自定义音频测试

本目录只有剧情，**不含音频文件**。请用你自己的 `.ogg` / `.wav`。

## 导入编号（必须一致）

| 用途 | 用户内容库里的编号 | 剧情引用 |
| --- | --- | --- |
| 音乐 | `test.bgm` | `user:test.bgm` |
| 音效 | `test.sfx` | `user:test.sfx` |
| 环境音 | `test.env` | `user:test.env` |

命名空间填 `test`，内部名称填 `bgm` / `sfx` / `env`，用途分别选音乐 / 音效 / 环境音。

## 怎么跑

1. 用当前源码或新打的 `lom_editor.exe` 打开 `samples/audio_test/story/main.json`。
2. 「文件 → 用户内容库」按上表导入三首（环境音可省略，导出时会因缺文件失败）。
3. 「文件 → 安装管理」更新运行时（需带上新的 `MortalModHost.dll` 和 `NVorbis.dll`）。
4. **完全退出并重启游戏**，再 F5 或导出后演出。

编辑器预览本身不发声，只在游戏里听。
