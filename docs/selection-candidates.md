# 第二轮选型候选

状态：历史候选取舍记录，已由用户授权设计者完成选型。最终决定见 README 中的专题；本文保留当时比较和证据，不作为待回答问题。核查日期：2026-10-05，无项目实测。

发布版核查纠正：Parley 0.11.1 已有可选 AccessKit integration；关闭 complex-scripts 不移除基础 CJK/UAX #14 换行，主要影响中日词级分段及部分东南亚文字的上下文分段。以 dependencies.md 和 text-input.md 为准。

## Q12：绘制后端

推荐 Linux / Windows 使用 ash 接 Vulkan，macOS 使用原生 Metal，以较小的二维绘制接口连接后端。macOS 也可以选择 MoltenVK 复用 Vulkan 实现；Vulkan 侧可用 vulkano 代替直接管理 ash。

ash 是薄绑定，不自动验证 Vulkan 调用，需要项目自行承担 unsafe、同步、分配、对象和交换链生命周期。vulkano 增加检查与资源管理，并不是现成 GUI 绘制器。Metal 减少携带 Vulkan→Metal 实现的需求，但增加另一套绘制与 shader 维护；MoltenVK 反之，并需考虑 portability subset。没有证据证明某一方案在所有负载下更快或更小。

此建议不引入 wgpu。具体 shader 工具、路径/特效算法、最低 GPU 能力和是否提供 CPU 绘制后端，在绘制路线确定后继续选择。

## Q13：窗口与平台输入

推荐各平台独立适配：Linux 采用 wayland-client + Smithay Client Toolkit（SCTK），Windows / macOS 通过 Rust 系统绑定接入原生窗口与文本输入。另可选择 winit 作为主要普通窗口层，或只在 Windows / macOS 使用 winit。

原生适配推荐基于首阶段 layer-shell 和完整文本输入需求，不是因为 winit 无法低功耗等待。winit 的 Wait / WaitUntil 可等待事件；它也可以禁用默认 features 后只启用 Wayland，排除 X11。但稳定版没有 layer-shell 创建接口，且 Wayland DeleteSurroundingText 明确未处理；基本 Preedit / Commit 不等于完成整个客户端文本输入协议。

SCTK 提供 layer-shell 封装，但其 seat::input_method 面向输入法程序自身，不是 GUI 文本框的客户端 IME。普通客户端需要结合 wayland-protocols 的 text-input-v3 绑定自行处理状态和时序。向 Vulkan 等 native FFI 提供 Wayland 指针需要 wayland-client 的 system backend，因此对应系统库应列入运行环境前提。

直接平台适配增加长期维护工作；它不自动带来更小发布物。不得把普通 xdg_toplevel 的 surface 假定为可直接切换成 layer surface，也不应无明确契约地维持两套 Linux 窗口生命周期。

## Q14：文字与字体

推荐候选为 Parley 的布局/基础编辑能力、Fontique 的字体匹配与回退、HarfRust 的 shaping，以及 Swash 的按需字形光栅化。职责可以分别封装；库内部的依赖关系依然存在，最终需要检查版本去重。推荐依据是已有文本能力与模块职责，而非已经证明占用最小。

替代候选是 COSMIC Text 的整体字体、排版和基础编辑栈，或自组底层库并自行实现更多排版/编辑行为。最后一种拆分最自由，但需要自行承担 bidi、断行、光标、选择等复杂性，不宜仅因极致模块化而从零重写。

核实到的具体边界：

- rustybuzz 上游已声明停止维护并建议 HarfRust；当前 Parley 和 COSMIC Text 主分支已采用 HarfRust。
- Fontique 按 script/locale 回退，对部分中日韩地区有显式分支；最终显示覆盖仍依赖真实字体。Linux 系统字体路径依赖 Fontconfig。
- COSMIC Text 经 fontdb 使用的 fontconfig-parser 是 Rust 配置解析器，不能与 Fontique 对 libfontconfig 的依赖混淆。
- Skrifa 提供轮廓、metrics 等，不能单独当成完整像素栅格器；当前 Swash 本身已依赖 Skrifa。
- Parley 的 PlainEditor 可以承接预编辑，但不替代平台 IME；其 AccessKit 示例也不代表自动接好系统无障碍。
- Parley 的 complex-scripts 影响包括 CJK 在内的字典式分段。关闭后的按字行为与数据体积存在取舍，不能静默裁剪。
- COSMIC Text 的 SwashCache 未见自动容量淘汰；Fontique 的 SourceCache::prune 也不是字节硬上限。无论选哪种栈，字形、布局、字体和 GPU 图集预算都需要单独设计。

字体数据可共享或映射不意味着不占 RAM。默认不预烘焙整个 CJK 字符集；按需生成、缓存上限、淘汰和在用资源保护需在后续确定。

## Q15：系统无障碍适配

推荐按目标平台使用 AccessKit 的独立 adapter，组件提供一致语义并接收动作。另一方案是自行实现 NSAccessibility、Windows UI Automation 和 Linux AT-SPI。

AccessKit 可以减少平台适配重复工作，但 adapter 保留无障碍树，Linux AT-SPI 路径还涉及 zbus 和异步任务；这些内存、任务及系统服务都要计入预算。独立平台 crate 不意味着所有平台实现都要进入同一发布物。

AccessKit 已发布适配支持单行/多行输入，但上游仍声明部分 schema、rich text / hypertext 的缺口。默认启用组合、具体控件覆盖和所选版本仍需确定；不能以“有该库”代替真实辅助技术验收。详见[无障碍专题](accessibility.md)。

## Q16：标记语言外形

推荐 QML 风格花括号、具名属性与子元素，但不使用 QML 或强制 JavaScript 引擎：

```text
Window {
    title: "Hello"
    Text { text: "你好，世界" }
}
```

替代方案为 XML 风格或缩进风格。此示例仅表示待选语法方向；类型、绑定、组件导入、条件和列表语义尚未选定。

## Q17：标记语言的逻辑能力

推荐受限表达式加小型命令式事件块，支持属性/状态读取、计算、赋值、条件和宿主动作调用；I/O 与系统服务交给 Rust 宿主。通用 JavaScript / Lua / Rhai VM 不进入基础依赖，是否另设脚本模块后续选择。

此方案仍需要语言执行逻辑；编译路径与运行时加载路径必须具有一致语义。替代方案是所有事件仅绑定 Rust 动作，或直接采用通用脚本引擎。具体算术、错误、依赖更新和执行限制需在路线确定后定义，不能用“小型”掩盖一个未受控的通用语言。

## Q18：Rust 调用与十行验收

推荐父对象直接创建子控件，返回可修改句柄，类似 Tkinter。另一选择是统一通过 ui.add / ui.set 操作句柄。推荐形态示例：

```rust
use aegle::prelude::*;
fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Hello")?;
    window.text("你好，世界");
    app.run()
}
```

这是 7 行候选源码，不是已实现或已编译接口。所有权、句柄和失败传播尚待细化；修改操作预期类似 label.set_text(...)。普通应用的便捷入口不成为其他独立模块的强制依赖。

建议 Rust 路径按 rustfmt 后的完整应用源码计算十行，包括导入和初始化；标记路径累计计算手写启动源码和标记文件。Cargo 清单与空行不计，必须手写的应用专用初始化辅助代码需要计入。该计数规则仍待用户选择。

## Q19：运行线程与调度

推荐 UI 状态、焦点和窗口生命周期由主线程拥有；后台工作通过明确消息返回 UI，动画只在活动期间请求必要帧，空闲时等待事件。允许接入外部任务系统，核心不强制选择 Tokio 等通用异步运行时。

这不代表所有依赖都不会创建线程或异步任务，例如 AccessKit Unix 的内部工作仍需单独核算。文字或图片工作如何放到后台、更新何时提交、动画暂停/销毁和任务取消，在运行模型确定后继续定义。

## 证据与未验证范围

绘制/窗口核查使用 ash 0.38、vulkano 0.35.2、winit 0.30.13、SCTK 0.21.1、wayland-client 0.31.15 的已发布文档/源码。这些是证据版本，不是选定版本。文字库调查依据当前上游主分支，尚未核对和锁定全部已发布版本，不将主分支能力自动当作现有 release 能力。

- [ash](https://docs.rs/ash/0.38.0+1.3.281/ash/)、[vulkano](https://docs.rs/vulkano/0.35.2/vulkano/)
- [winit features](https://docs.rs/crate/winit/0.30.13/features)、[Wayland IME 源码](https://docs.rs/crate/winit/0.30.13/source/src/platform_impl/linux/wayland/seat/text_input/mod.rs)、[外部事件循环约束](https://docs.rs/winit/0.30.13/winit/platform/pump_events/trait.EventLoopExtPumpEvents.html)
- [SCTK layer-shell](https://docs.rs/smithay-client-toolkit/0.21.1/smithay_client_toolkit/shell/wlr_layer/index.html)、[wayland-client](https://docs.rs/wayland-client/0.31.15/wayland_client/)
- [Metal](https://developer.apple.com/documentation/metal)、[MoltenVK](https://github.com/KhronosGroup/MoltenVK/blob/main/Docs/MoltenVK_Runtime_UserGuide.md)
- [rustybuzz 维护状态](https://github.com/harfbuzz/rustybuzz#readme)、[HarfRust](https://github.com/harfbuzz/harfrust#readme)
- [Parley](https://github.com/linebender/parley#readme)、[Fontique](https://github.com/linebender/parley/blob/main/fontique/Cargo.toml)、[Swash](https://github.com/dfrg/swash#readme)
- [COSMIC Text](https://github.com/pop-os/cosmic-text#readme)、[SwashCache](https://github.com/pop-os/cosmic-text/blob/main/src/swash.rs)、[fontdb](https://github.com/RazrFalcon/fontdb#readme)
- [AccessKit](https://github.com/AccessKit/accesskit#readme)、[Unix adapter 运行模型](https://github.com/AccessKit/accesskit/blob/main/adapters/unix/README.md)
