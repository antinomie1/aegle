# 实现状态

完整目标保持不变：模块化保留模式 GUI、GPU 与无 GPU 软件绘制、CJK/原生 IME、无障碍、组件/主题/动画、命令式与标记入口、跨平台及 API 文档。

## 已实现：底层保留状态与布局

- Rust 2024 workspace，LGPL-3.0-only；目标 MSRV 1.88，实际验证工具链为 1.96.1。
- aegle-types：no_std 几何与紧凑 RGBA 颜色，无第三方依赖。
- aegle-core：代数 ID、可复用槽位、保留树、索引子节点、结构变更与三通道失效；无第三方依赖。叶节点不分配子节点数组，删除不递归。
- aegle-layout：Taffy 0.14.0 直接适配同一棵保留树，无第二份拓扑；共享默认样式、测量缓存、Flex/Block、可选 Grid。它依赖小型 core 树，不依赖应用、字体、窗口或 renderer，可独立使用。
- 可运行示例：`cargo run -p aegle-layout --example retained --release`，只演示无窗口布局，不是 GUI Hello world。

## 本阶段验证

- 默认与 all-features workspace 测试通过：5 个集成场景，测试全部位于各 crate/tests。
- Rustdoc 在 warnings-as-errors 下通过；格式与 diff 检查通过。
- 实现 834 行、测试 161 行（16.2%），最大源文件 308 行；均不超过 500 行。
- 本机示例输出：三次布局过程共 8 次叶测量，修改后的宽度 120 dp；树/子索引数组保留 1088 字节。这个数不包含 Rc 样式等额外分配，不是进程 RAM 或完整 GUI 基准。
- Clippy 未安装；MSRV、其他平台、实际窗口和 GPU 均未验证。

## 下一阶段

从同一布局结果建立 scene，并接入独立软件 renderer，先完成无需 GPU 的可见绘制路径。随后整合文字/字形与原生窗口、IME/无障碍，再接 Vulkan/Metal、组件、主题、动画、标记语言及发布组合。

当前尚无可用 GUI、字体排版、平台窗口、IME、无障碍 adapter、GPU/软件 renderer、主题动画或 DSL 实现。设计文档是目标，不将其当作实现证据。
