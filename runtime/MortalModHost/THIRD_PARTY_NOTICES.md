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


# Host 原生计算与 OGG 解码

`lom_host_native.dll` 使用以下未经修改的 Rust crates。MIT OR Apache-2.0 组件在此选择 MIT 分发；上游源码可按相同版本从 crates.io 获取。

| 组件 | 版本 | 许可证／版权声明 |
| --- | --- | --- |
| hmac、digest | 0.12.1、0.10.7 | MIT；Copyright (c) 2017 Artyom Pavlov |
| sha2 | 0.10.9 | MIT；Copyright (c) 2006-2009 Graydon Hoare；2009-2013 Mozilla Foundation；2016 Artyom Pavlov |
| block-buffer | 0.10.4 | MIT；Copyright (c) 2018-2019 The RustCrypto Project Developers |
| crypto-common | 0.1.7 | MIT；Copyright (c) 2021 RustCrypto Developers |
| cpufeatures | 0.2.17 | MIT；Copyright (c) 2020-2025 The RustCrypto Project Developers |
| generic-array | 0.14.7 | MIT；Copyright (c) 2015 Bartłomiej Kamiński |
| typenum | 1.20.1 | MIT；Copyright (c) 2014 Paho Lurie-Gregg |
| cfg-if | 1.0.5 | MIT；Copyright (c) 2014 Alex Crichton |
| subtle | 2.6.1 | BSD-3-Clause；版权及条款见下文 |
| lewton | 0.10.2 | MIT；Copyright (c) 2016 est31 <MTest31@outlook.com> and contributors |
| arrayvec | 0.7.8 | MIT；Copyright (c) Ulrik Sverdrup "bluss" 2015-2023 |
| bitflags、log | 1.3.2、0.4.34 | MIT；Copyright (c) 2014 The Rust Project Developers |
| bytemuck | 1.25.2 | MIT；Copyright (c) 2019 Daniel "Lokathor" Gee. |
| byteorder | 1.5.0 | MIT；Copyright (c) 2015 Andrew Gallant |
| core_detect | 1.0.0 | MIT；Copyright (c) 2017-2020 The Rust Project Developers |
| encoding_rs | 0.8.44 | MIT AND BSD-3-Clause；Copyright Mozilla Foundation；另见下文 WHATWG 条款 |
| multiversion_no_op | 1.0.0 | MIT；Copyright Mozilla Foundation |
| scopeguard | 1.2.0 | MIT；Copyright (c) 2016-2019 Ulrik Sverdrup "bluss" and scopeguard developers |
| lazy_static、simdutf8、tinyvec | 1.5.1、0.1.5、1.13.3 | MIT；采用上游所附 MIT 正文 |

OGG 容器读取另使用未经修改的 Symphonia 0.5.5 及其 core、metadata、format-ogg、utils-xiph 组件，作者 Philip Deljanov，采用 MPL-2.0。对应源码及许可证可从 [Symphonia v0.5.5](https://github.com/pdeljanov/Symphonia/tree/v0.5.5) 获取。Vorbis 音频解码使用 [Lewton 0.10.2](https://github.com/RustAudio/lewton/tree/v0.10.2)，在此选择 MIT 分发。

以上 MIT 组件的许可正文：

```text
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

subtle 的 BSD-3-Clause 许可正文：

```text
Copyright (c) 2016-2017 Isis Agora Lovecruft, Henry de Valence. All rights reserved.
Copyright (c) 2016-2024 Isis Agora Lovecruft. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

encoding_rs 所含 WHATWG 数据的 BSD-3-Clause 条款：

```text
Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

# Windows 输入法处理

编辑器使用 egui-winit 0.33.3（MIT OR Apache-2.0），本地回补输入法已处理按键的过滤逻辑，来源：[emilk/egui #7967](https://github.com/emilk/egui/pull/7967)。对应源码与修改说明位于 `vendor/egui-winit/`，该组件采用 MIT 许可证分发，正文如下。

```text
Copyright (c) 2018-2021 Emil Ernerfeldt <emil.ernerfeldt@gmail.com>

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```
