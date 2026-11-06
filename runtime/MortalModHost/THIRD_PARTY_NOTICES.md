# 存档界面来源

MOD 存档界面中的布局与展示代码移植自 **kevinnie2003 / 咖啡伴侣（LOMAssistant）**：

- 源文件：[`LOMAssistant.Features.Extras/SaveBrowser.cs`](https://github.com/kevinnie2003/LOMAssistant/blob/v3.4.0/LOMAssistant.Features.Extras/SaveBrowser.cs)
- 版本：v3.4.0，提交 `5b2749bdc37e2dba312feab694c1617f151e1ff5`
- 本地改动：接入 Host 的 MOD 栏位、读写与备注归属，移除原插件配置、剧情分析与 DLL 依赖。

上游该版本未提供覆盖此模块的许可证；本项目不将这部分上游代码声明为 MIT。上游 README 中对其他组件的 MIT 标注不作为此文件的授权声明。

游戏外框、字体、按钮与立绘由本机已安装游戏提供，不在源码中分发游戏资源。

# 编辑器音频与 GIF 解码

Rust 编辑器与编译器使用 Symphonia 0.5.5 及其 core、metadata、bundle-mp3 组件，将导入的旧 MP3 转为 WAV。作者为 Philip Deljanov，许可证为 MPL-2.0。对应未修改源码和许可证随上游版本提供。[Symphonia 0.5.5 源码](https://github.com/pdeljanov/Symphonia/tree/v0.5.5)

GIF 解码使用 gif 0.14.2，Copyright (c) 2015 nwin，采用 MIT 许可证。许可证正文如下。

```text
The MIT License (MIT)

Copyright (c) 2015 nwin

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
