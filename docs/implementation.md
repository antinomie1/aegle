# 实现状态

完整目标保持不变：模块化保留模式 GUI、GPU 与无 GPU 软件绘制、CJK/原生 IME、无障碍、组件/主题/动画、命令式与标记入口、跨平台及 API 文档。

## 已实现

- Rust 2024 workspace，LGPL-3.0-only；目标 MSRV 1.88，实际验证工具链为 1.96.1。
- aegle-types：no_std 几何与紧凑 RGBA 颜色，无第三方依赖。
- aegle-core：代数 ID、可复用槽位、保留树、索引子节点、结构变更与三通道失效；无第三方依赖。叶节点不分配子节点数组，删除不递归。
- aegle-layout：Taffy 0.14.0 直接适配同一棵保留树，无第二份拓扑；共享默认样式、测量缓存、Flex/Block、可选 Grid。它不依赖应用、字体、窗口或 renderer。
- aegle-scene：no_std + alloc 的局部绘制记录；实色矩形、圆角、居中边框、仿射变换、嵌套裁剪。几何与配对作用域在记录时校验；完成的记录只读，重新录制可复用命令分配。
- aegle-render-software：借用 RGBA8 缓冲，tiny-skia 负责抗锯齿覆盖率；小型线性光 SourceOver 合成器处理透明颜色。mask 显式预算与复用，跨帧复用路径/遍历空间。无需 GPU、窗口系统、字体、core 或 Taffy 依赖。

## 可运行的组合

`cargo run -p aegle-layout --example retained --release` 演示无窗口布局。

`cargo run -p aegle-render-software --example software_scene --release` 将同一棵保留树的 Taffy 结果接到每节点 Scene，再写出 `target/aegle-software.png`，也可传入输出路径。示例中的图形是形状演示，尚无文字、可交互控件或窗口；不是 GUI Hello world。PNG 编码器仅为 dev-dependency。

示例首次建立 12 个记录，随后仅改变按钮背景颜色，重建 1 个记录；后续重绘复用这些记录和缓冲。布局位置由 draw 的根变换应用，保留记录不存另一份节点树。

## 本阶段验证

- workspace all-features 测试通过：共 8 个集成场景，全部位于各 crate/tests；软件覆盖嵌套裁剪与恢复、边框、旋转、透明线性合成、mask 复用/尺寸变化/预算拒绝。
- Rustdoc 在 warnings-as-errors 下通过；格式与 diff 检查通过。默认软件 feature 组合及 release 示例已编译运行，图片已目视检查。
- 实现 1843 行、测试 345 行（15.8%，不含 examples）；最大源文件 308 行，均不超过 500 行。
- 本机 480×300 形状示例：命令缓冲保留 1152 B；mask 288000 B；调用方颜色缓冲 576000 B。另有树/布局/样式、约 8 KiB 颜色转换表、路径/扫描线、分配器及程序本身的成本，以上不是进程 RAM/PSS。
- 本次 release 运行 100 次暖态离屏重绘共 17.52 ms；这是当前机器、12 节点形状示例的一次观察，不含窗口、文字、输入或呈现，不能代表嵌入式设备、完整 GUI 帧延迟或空闲 CPU。
- Clippy 未安装；MSRV、其他平台、实际窗口和 GPU 均未验证。

## 下一阶段与缺口

下一步接入真实文字排版、按需字形与有界字形缓存，使 CJK 文字使用同一 scene/软件输出；随后整合原生窗口和事件、IME/无障碍，再接 Vulkan/Metal、组件、主题、动画、标记语言及发布组合。

当前尚无可用 GUI、字体排版/图像命令、平台窗口、IME、无障碍 adapter、GPU renderer、主题动画或 DSL 实现。软件绘制目前只支持已列出的形状；跨节点祖先裁剪与绘制组尚需在应用组装阶段接通。设计文档是目标，不将其当作实现证据。
