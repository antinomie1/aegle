# Aegle GUI 设计

v0.1，2026-10-05。设计作为实现基线；当前已开始底层框架实现，实际能力和验证见 [实现状态](implementation.md)。设计中的完整 GUI 尚未实现。

Rust 2024、Taffy、保留模式。标记语言优先，提供十行内完整示例和直接的命令式 API。Linux/Windows 默认使用 Vulkan，另有可选的最小 wgpu 后端作为全平台通用 GPU 路径（含 macOS 的 Metal，原生 Metal 方案已放弃）；CJK、IME、系统无障碍、主题与动画共同设计，模块独立选用。

## 开发者文档

- [API 指南](developer/api.md)：依赖与 feature、应用与窗口、布局、样式、事件、动画、标记与嵌入宿主
- [控件参考](developer/controls.md)：每个默认控件的用法与各状态截图

## 阅读入口

- [需求](requirements.md)、[决策记录](design-tree.md)、[术语](GLOSSARY.md)
- [整体架构与生命周期](architecture.md)、[模块与构建组合](modules.md)
- [Rust API](rust-api.md)、[标记语言](markup.md)
- [默认组件、主题与动画](components-theme-animation.md)
- [文字与输入](text-input.md)、[系统无障碍](accessibility.md)
- [平台与绘制](platform-rendering.md)、[Vulkan 绘制与呈现](vulkan.md)、[wgpu 后端](wgpu.md)、[依赖版本](dependencies.md)
- [资源目标](resources.md)、[验收与交付边界](quality.md)

[早期候选取舍](selection-candidates.md)与[平台事实来源](platform-facts.md)保留作依据；最终决定以以上专题为准。

重要决策：[无障碍与自绘](adr/0001-accessibility-and-custom-controls.md)、[标记双执行路径](adr/0002-dual-markup-execution.md)、[独立模块和显式更新](adr/0003-independent-modules-and-imperative-ui.md)。

本文档的 API 是设计规范，预算是待测工程目标。具体实现的编译验证见状态文档；不能将“设计已完成”解读为“整个库已实现或达标”。
