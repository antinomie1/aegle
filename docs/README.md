# Aegle 设计与文档索引

Aegle 是基于 Rust 2024 的低资源保留模式 GUI 框架，采用单 UI 线程、模块化分工、多渲染后端与原生输入法支持。

---

## 开发者文档

* **[开发者 API 指南](developer/api.md)**：依赖、Feature、应用与窗口、布局、样式、事件处理、过渡动画、标记语言与嵌入。
* **[默认控件参考](developer/controls.md)**：内置全部控件的创建、属性、事件与状态展示。
* **[不经 Facade 使用控件库](developer/standalone.md)**：脱离 `aegle` 便捷入口，仅使用 `aegle-ui` 与控件库构建自定义宿主。
* **[编写自定义控件库](developer/library.md)**：实现 `Control` trait、静态 `ControlKind`、样式句柄宏 `handle!` 与标记宏 `element!`。

---

## 架构与设计规范

* **[需求、决策与验收标准](requirements.md)**：R01–R23 核心需求、架构关键决策与工程验收边界。
* **[整体架构与运行契约](architecture.md)**：状态拓扑、生命周期、更新循环管线、线程模型与失效机制。
* **[模块、依赖与构建组合](modules.md)**：29 个工作区 Crate 职责表、系统运行时依赖基线、Feature 互斥规则与示例运行命令。
* **[平台与渲染后端](platform-rendering.md)**：平台抽象契约（SDL3 默认、Wayland 原生、Win32、macOS AppKit）与渲染器架构（CPU tiny-skia/cairo、Vulkan 1.1、wgpu）。
* **[文本、排版与输入法](text-input.md)**：系统文字栈、词边界断词（`aegle-segment`）、多行增量编辑器（`Editor`）与平台 IME 事务。
* **[系统无障碍](accessibility.md)**：语义树模型、AccessKit 平台适配（Linux AT-SPI、Windows UIA、macOS NSAccessibility）。
* **[组件、主题与动画系统](components-theme-animation.md)**：默认控件行为与皮肤、有类型 Token 与局部覆盖、连续过渡与弹簧物理曲线。
* **[界面标记语言](markup.md)**：花括号语法、单向响应式数据绑定、编译期 `ui!` 展开与动态解释加载引擎。
* **[资源预算与性能策略](resources.md)**：内存预算、字形图集与 Mask 限制、零周期唤醒与发布包体测量。
* **[实现状态与验证证据](implementation.md)**：事实实现的模块能力、跨平台真机与无头环境测试证据、已知缺口。

---

## 附录与参考

* **[术语表](GLOSSARY.md)**：统一术语定义。
* **[架构决策记录 (ADR)](adr/)**：[ADR-0001 无障碍](adr/0001-accessibility-and-custom-controls.md)、[ADR-0002 标记双路径](adr/0002-dual-markup-execution.md)、[ADR-0003 模块独立性](adr/0003-independent-modules-and-imperative-ui.md)、[ADR-0004 元素契约](adr/0004-element-contract.md)。
* **[Aegle 与 GPUI 对比](gpui-comparison.md)**：横向架构与资源定位对比。
