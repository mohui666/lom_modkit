# 1.4.12 路线图与流程图连线优化

- 流程图参考 ELK 分层布局规则，双向调整分支顺序以减少交叉，对齐连续步骤并居中汇合。
- 为跨层连线预留狭窄通道，减少绕到整张流程图外侧的长线；连接端口按实际通道顺序排列。
- 原版路线图的相邻节点优先直连，其他连线避开节点；自循环就近绕回，返回路线不再强制绕到全图边缘。
- 两种图统一使用圆角折线、实心箭头和交叉处的间隔，突出选中路线，时间顺序保留虚线。

## 升级

编辑器、编译器和随包 Host 统一为 1.4.12，可从完整 1.4.9 或更新版本使用内置更新。首次安装或从更早版本升级，请下载完整 Windows x64 包。作品格式及原有最低 Host 版本要求保持不变。

## 验证范围

Windows 编辑器实际检查分支、汇合、回路、节点点击定位，以及本机原版路线图加载。图形相关定向测试 28 项通过，Windows 发布构建通过。本版修改编辑器图形布局，不新增游戏运行接口；未重复进行全部原版分支、macOS GUI 或 Combat／Battle 专项实机验收。

布局规则参考 [ELK Layered](https://eclipse.dev/elk/reference/algorithms/org-eclipse-elk-layered.html)、[React Flow 多端口示例](https://reactflow.dev/examples/layout/elkjs-multiple-handles)与 [X6 避障路由](https://x6.antv.antgroup.com/en/api/registry/router)，以原生 Rust 实现适合本编辑器的布局与绘制。
