# Aegle 与 GPUI 对比

状态：比较文档，2026-10-28。GPUI 事实以 crates.io 发布包 `gpui 0.2.2`（2025-10-22 发布）及其 docs.rs 公开 API 为准；Zed 仓库的 main 分支与 Zed 应用层可能领先于该发布包，文中涉及处单独标注。Aegle 事实以本仓库当前文档与源码为准，设计目标与已验证结果分开表述。本文不是设计契约，不改变[设计索引](README.md)中的架构决定。

## 结论摘要

**两者不在同一竞争层。** GPUI 是 Zed 的生产级应用框架，GPU 优先、混合即时/保留模式，随 Zed 在 macOS/Linux/Windows 上实际使用；但 API 仍 pre-1.0，0.2.2 发布包不含默认控件库、主题系统和无障碍接口。Aegle 是面向低资源设备、保留模式、可组合模块的 GUI 库，自带控件、主题、动画、文字编辑、系统文字栈、无障碍与标记语言；但项目处于 v0.1、未发布、没有外部用户，macOS 未实机运行，Linux 只支持 Wayland。

一句话选型：

- 低内存/无 GPU/ARM64 Linux/Wayland 面板、CJK 与 IME 优先、标记优先、需要嵌入现有宿主 → Aegle（接受 v0.1 风险）。
- 三平台桌面应用（尤其 macOS）、X11 与 Wayland 都要、需要异步、动作与快捷键、原生菜单、Inspector、愿意自建控件层或依赖 Zed 生态 → GPUI。

## 对比总表

| 维度 | Aegle | GPUI 0.2.2 | 判断 |
| --- | --- | --- | --- |
| 定位 | 资源受限的模块化保留模式 GUI 库 | Zed 的 GPU 加速应用框架 | 目标相反 |
| 界面模型 | 控件创建一次，弱句柄显式修改；保留控件树 | `Entity<T>` 保存状态，`Render` 每帧返回元素树；混合即时/保留 | 根本差异 |
| 主编写入口 | `.aegle` 标记（编译期或运行时）+ 命令式 Rust | 纯 Rust 的 Tailwind 风格 DSL | 各自取舍 |
| 布局 | Taffy 0.14，flex/block，Grid 可选 | Taffy 0.9，经 `Style`/`StyleRefinement` 暴露 | 都用 Taffy |
| 默认渲染 | CPU 软件渲染（无 GPU 可用） | 仅 GPU（blade-graphics） | Aegle 覆盖无 GPU 场景 |
| GPU 后端 | ash/Vulkan + 可选 wgpu（Vulkan/Metal/DX12） | blade：macOS Metal、Linux Vulkan、Windows D3D11 | GPUI 更统一 |
| 文字排版 | 三平台都用系统栈：Pango / DirectWrite / CoreText | Linux cosmic-text；macOS CoreText；Windows DirectWrite | Linux 路线不同 |
| 默认控件 | 20 余个（按钮、编辑器、列表、表格、菜单、对话框等） | 核心无通用控件；只有 `Div`、`List`、`UniformList`、图、SVG、Canvas | Aegle 开箱即用 |
| 主题 | 内建类型化 token、浅/深/高对比、局部覆盖、跟随系统 | 仅 `WindowAppearance` 与默认色板；主题在 Zed 的独立 crate | Aegle 内建 |
| 动画 | 过渡、弹簧、关键帧、减少动态效果 | `Animation`/`AnimationExt`、缓动函数 | 都有；Aegle 更系统 |
| 无障碍 | AccessKit + AT-SPI/UIA/NSAccessibility，feature 开关 | 0.2.2 公开 API 未见无障碍语义树/系统适配 | Aegle 明显更完整 |
| 平台 | Linux Wayland（无 X11）、Win32、AppKit（未实机） | macOS、Linux X11+Wayland、Windows（D3D11） | GPUI 更广 |
| layer-shell | 支持（面板/背景层） | 未提供 | Aegle 独有 |
| 桌面服务 | 文件对话框、通知、托盘、全局快捷键 | 0.2.2：文件对话框、原生菜单、屏幕捕获；托盘/通知/全局快捷键未见（main 有通知示例） | 方向不同 |
| 异步 | 无运行时；`UiProxy` 有界队列 | 内建 smol 执行器、`Task`、异步上下文 | GPUI 内建 |
| 测试工具 | 工作区测试、私有 compositor/GPU 像素对比 | `#[gpui::test]`、`TestAppContext`、可视化测试 | GPUI 更面向应用测试 |
| 调试工具 | 无可视检查器 | Inspector、screen-capture、leak-detection feature | GPUI 更强 |
| 模块化 | 29 个 crate，无窗口引擎可独立使用 | 1 个主 crate + 若干 gpui_* 辅助 crate，整体框架 | Aegle 更适合拆分/嵌入 |
| 发布状态 | 未发布到 crates.io | crates.io 0.2.2，约 40 万次下载 | GPUI 可用 |
| 许可 | MIT OR Apache-2.0 | Apache-2.0 | 都宽松 |
| 成熟度 | v0.1，单作者，无外部用户 | Zed 生产使用，API 仍不稳定 | GPUI 明显更成熟 |

## 1. 定位与设计目标

Aegle 的[需求](requirements.md)把资源约束写成了硬目标：512 MB～1 GB 的 Linux ARM64 设备、静止应用每秒零主动唤醒、静止 CPU ≤ 0.5%、800×480/100 控件 PSS ≤ 48 MiB、最小 Hello world 发布文件 ≤ 8 MiB，并要求保留模式、Taffy、多后端、CJK/IME、系统无障碍与标记语言。它明确不引入 ECS、虚拟 DOM、通用脚本 VM、全局异步运行时、winit 或通用三维 RHI；系统文字栈与软件渲染是“无 GPU、少依赖”策略的一部分。见[架构](architecture.md)、[资源](resources.md)、[模块](modules.md)。

GPUI 的公开目标是“支持广泛应用的、GPU 加速的混合即时/保留 UI 框架”，由 Zed 编辑器驱动；README 明确说仍处于 pre-1.0、版本间常有破坏性变更，并把“阅读 Zed 源码、Discord、后续博客”列为学习途径。它没有低内存、无 GPU、嵌入式或发布体积目标，依赖里包含 `image`、`resvg`/`usvg`、`serde`、`schemars`、`smol`、`lyon`、`etagere` 等常规桌面应用栈。两者对“框架应该自带多少东西”的答案也完全不同：Aegle 自带应用套件，GPUI 只提供运行框架与底层元素，控件与主题留在 Zed 的其他 crate。

## 2. 界面模型与数据流（核心差异）

### Aegle：真保留模式

控件创建一次，之后通过类型化弱句柄修改；没有要求应用每帧重述界面的入口。属性修改打脏标记，刷新时只重算失效的布局、文字与绘制记录，空闲时不重绘也不轮询。状态在控件树与宿主代码中，事件处理函数直接改其他控件。见 [Rust API](rust-api.md)。

```rust
let label = window.text("等待操作");
let button = window.button("完成");
button.on_click(move |_| label.set_text("已完成"));
```

同一界面也可以写成标记，`ui!` 在编译期把它变成直接创建/设置控件的代码：

```text
Column {
    Text { id: status; text: "等待" }
    Button { id: done; text: "完成" }
}
```

渲染路径是 `Ui` 保留树 → Taffy 布局 → 每控件局部 `Scene` → 软件/Vulkan/wgpu。布局或外观变化只重建相关记录；软件路径还维护最多四个矩形的损伤区域（[局部重绘](resources.md#低成本更新)）。

### GPUI：Entity 保留状态 + 每帧 Render

GPUI 把应用状态放在 `App` 拥有的 `Entity<T>` 中，所有句柄都只是 `App` 内的标识；读取/修改状态需要 `App`/`Context`。视图是实现了 `Render` 的 `Entity`，GPUI 在需要绘制的帧调用根视图的 `render`，返回新的元素树，再由元素树布局、绘制。用 `cx.notify()` 通知观察者并请求下一帧。见 [GPUI 所有权与数据流](https://docs.rs/gpui/0.2.2/gpui/_ownership_and_data_flow/index.html) 与 [`Render`](https://docs.rs/gpui/0.2.2/gpui/trait.Render.html)。

```rust
struct Counter { count: i32 }

impl Render for Counter {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap_2()
            .child(format!("{}", self.count))
            .on_click(cx.listener(|this, _, _, cx| {
                this.count += 1;
                cx.notify();
            }))
    }
}
```

### 差异的实际后果

| 方面 | Aegle | GPUI |
| --- | --- | --- |
| 状态位置 | 控件树 + 应用代码；句柄弱引用 | `App` 拥有的 `Entity`；句柄需 `App` 访问 |
| UI 定义 | 一次创建，之后命令式修改 | 每次 Render 重新返回元素树 |
| 更新粒度 | 属性脏标记；只重建失效部分 | 帧级重建元素树；布局/元素状态按 ID 缓存 |
| 动态内容 | 应用显式增删控件；`ListView`/标记 `for` 管行生命周期 | 直接按状态生成子元素；列表元素/ID 管理状态 |
| 组合方式 | 函数/标记组件；第三方 `Control`/`element!` | Rust 函数/组件；`Element` 底层逃生口 |
| 每帧分配 | 空闲与稳态输入路径接近零分配 | 每帧构造元素树与样式，设计上接受分配 |
| 嵌入现有宿主 | `aegle-ui` 无窗口引擎可直接嵌入 | 以 `Application`/`Window` 为中心，嵌入需自实现平台层 |
| 心智模型 | Qt/Tkinter 式直接操作 | SwiftUI/React 式状态到 UI |

两者都保留状态、都需要显式失效，区别在于“UI 是保留的对象”还是“UI 是状态的每帧投影”。Aegle 的 API 让事件回调直接改控件，容易写传统桌面表单；GPUI 的 Render 让复杂动态布局更自然，也更容易把状态和 UI 分层，但每次刷新要重建元素树，且需要理解 `Entity`/`Context`/`Window` 的所有权规则。

GPUI 的“保留”不只在实体里：元素状态通过 `ElementId`/`GlobalElementId` 跨帧保留（`Stateful`、`DivFrameState`、`ScrollHandle`、`ListState` 等），Taffy 布局结果按 `LayoutId` 缓存，`notify`/`emit` 不是同步回调而是入队效果、在更新结束后统一派发（run-to-completion，监听器不可重入）。所以“每帧重建”的准确含义是重建元素树，不是重建全部状态；而 Aegle 连元素树本身都是保留的。这个区别决定了两者在每帧分配、动态布局表达力和调试心智模型上的取舍。

## 3. 编写入口、布局与样式

Aegle 有两个等价入口：`.aegle` 标记（构建期 `ui!` 直接生成控件代码；运行时 `Program::load`/`reload` 用同一个检查器与元素规格）和命令式 Rust。静态标记的发布程序不带解析器；动态标记才链接引擎。第三方控件用 `element!` 声明元素规格，与内置元素走同一条检查与构建路径。代价是自有语言、检查器、两套执行路径和额外的文档/工具成本，收益是表单类界面短、可热重载、对 AI 生成更友好。见[标记语言](markup.md)、[ADR 0002](adr/0002-dual-markup-execution.md)、[ADR 0004](adr/0004-element-contract.md)。

GPUI 只有 Rust 一种入口：`div()` 加 Tailwind 风格方法（`.flex()`、`.gap_2()`、`.p_4()`、`.w_full()` 等），`Style`/`StyleRefinement` 是底层样式类型；没有标记语言，也没有运行时 UI 热重载。好处是完全类型安全、没有额外语言与解析器、与 Rust 生态和 `cargo` 一致；代价是界面代码更长，动态样式和主题通常要靠函数组合。

布局两者都用 Taffy，但版本与暴露方式不同：Aegle 是 Taffy 0.14 的“低层树适配”，对保留树提供逐轴尺寸、margin/padding/gap、flex、绝对定位、Grid（可选 feature）等经过校验的 setter；GPUI 锁 Taffy 0.9.0，用 `Style` 与 `StyleRefinement` 表达。Aegle 的 Grid/Stack 是可选 feature（release 约 244 KiB），GPUI 的布局能力以发布包的公开类型为准。

主题是更大的区别：Aegle 在 `aegle-theme` 中提供无分配的类型化颜色/尺寸、浅/深/高对比配色、按控件的 `Skin`、按子树的局部主题和类型化 token（全局或子树覆盖、可绑定到属性），原生 App 跟随系统深浅色/高对比/减少动态效果；GPUI 0.2.2 只有窗口外观（Light/Dark）和 `colors::Colors` 默认色板，没有 token 或主题系统，Zed 的完整主题在 Zed 的独立 `theme` crate 中。

## 4. 渲染与绘制

| 方面 | Aegle | GPUI 0.2.2 |
| --- | --- | --- |
| CPU 软件渲染 | 有，且是默认；无 GPU 可运行 | 无；需要 GPU 驱动 |
| GPU | 直接 ash/Vulkan；可选 wgpu（Vulkan/Metal/DX12） | blade-graphics：Metal（macOS）、Vulkan（Linux）、D3D11（Windows） |
| 绘制抽象 | `Scene` 绘制记录 + 三后端 | 元素树 → 图元 → GPU |
| 路径 | CPU 光栅化成覆盖率 mask（tiny-skia/zeno/cairo），各后端一致 | lyon 三角化 + GPU；`etagere` 图集 |
| 文字 | 灰度字形图集，彩色字形按平台能力 | 字形图集；`cosmic-text`/CoreText/DirectWrite |
| 局部重绘 | 软件路径按损伤矩形；GPU 整窗，Vulkan 可增量上报 | 公开 API 无损伤区域抽象；以帧为单位 |
| 效果 | 渐变、解析阴影、组透明度、背景模糊图层、最多 8 层裁剪 | 渐变背景、box shadow、窗口背景外观；裁剪以矩形 `ContentMask` |
| 资源约束 | 明确预算：字形缓存、显存、mask、SHM、编辑历史等 | 有图像缓存与图集；未以低内存为设计目标 |

Aegle 的软件路径不是“降级演示”：它支持文字、图像、路径、渐变、阴影、图层与裁剪，并和 GPU 后端做像素对比；代价是软件路径上的大面积模糊明显更贵。GPU 路径以 Vulkan 为主（Windows/Linux），wgpu 作为 macOS 与跨平台通用路径；Vulkan 每个窗口一个设备，图集按窗口独立。这些取舍见[平台与绘制](platform-rendering.md)、[Vulkan](vulkan.md)、[wgpu](wgpu.md)、[资源](resources.md)。

GPUI 是纯 GPU 路线：Zed 的公开材料描述其 120 FPS 目标是基于 SDF/实例化矩形、字形图集与 8.33 ms 帧预算，但没有公开可复现的基准表；README 与依赖里也没有软件光栅器。0.2.2 的公开 API 没有损伤区域类型或接口，从架构上推断它是按帧整窗重绘（不是 Aegle 软件路径那种按矩形局部重绘）。对无 GPU 或资源受限设备，这是硬性差异，不只是性能差异；Zed 自己的 Linux 文档就要求 Vulkan 设备并列出 `NoSupportedDeviceFound` 等失败模式，Windows 文档要求 DirectX 11 兼容 GPU。

## 5. 文字、CJK 与输入法

| 方面 | Aegle | GPUI 0.2.2 |
| --- | --- | --- |
| Linux 排版 | Pango + Fontconfig + FreeType，运行时 `dlopen` | cosmic-text 0.14（Rust 栈） |
| Windows 排版 | DirectWrite 排版与字形覆盖率 + Uniscribe 分段 | DirectWrite |
| macOS 排版 | CoreText + CoreGraphics 轮廓 | CoreText |
| 字体发现 | Fontconfig / DirectWrite / CoreText | zed-font-kit / 平台 API |
| 按词/选词 | 系统分词器（ICU/CFStringTokenizer），无词典数据 | cosmic-text/unicode 段边界（未见系统词典接入） |
| 内建编辑器 | 有：单/多行、选择、撤销、密码、预编辑、IME 事务 | 核心无通用文本编辑控件；提供 `ElementInputHandler`/`EntityInputHandler` 原语 |
| IME | Wayland text-input-v3、Win32 TSF、macOS NSTextInputClient | 平台输入处理器：macOS NSTextInputClient、Windows IMM/TSF、Linux XIM（X11）与 text-input-v3（Wayland）；编辑控件需自建 |
| 验证 | Linux 实机/协议；Windows 11 实机 TSF；macOS 仅交叉编译 | Zed 在 macOS/Linux/Windows 的实际使用；独立 gpui 的输入控件仍需自建 |
| 词典/字体打包 | 不内嵌 CJK 字体或词典；缺字/缺分词器如实报告 | 不内嵌字体；cosmic-text 自带 shaping/断行数据 |

Aegle 的文字策略是“操作系统会做的都交给系统栈，Aegle 只保留段落、光标、选择、编辑与字形记录”。优点是 Linux 上的 bidi、UAX #14 换行、字体回退与系统分词与桌面一致，二进制不需要 Parley/ICU 词典数据（`hello` 默认构建 2.46 MiB）；缺点是把 Pango/Fontconfig/FreeType 变成 Linux 运行前提，且三个系统的缺字、彩色字形、行距行为并不完全一致，需要逐平台验证。Windows 的 TSF 文本存储（周边文字、重转换）和 Wayland 的 text-input-v3 已经做了真实协议验证；macOS 的 `NSTextInputClient` 映射只在 Linux 上编译/单测过，未实机。

GPUI 在 Linux 上用 cosmic-text，macOS/Windows 用系统栈，因此三平台行为同样不完全一致，但 Linux 不依赖 Pango。GPUI 核心提供 shaped line、`TextLayout`、输入处理器抽象，不提供 Aegle 那样的完整 `Editor`；Zed 的代码编辑器在独立 `editor` crate 中，不能简单当作 gpui 自带控件。对“只要一个能用中文输入法的文本框”的应用，Aegle 的开箱程度明显更高；对“要写代码编辑器”的应用，GPUI/Zed 生态更成熟。

## 6. 控件、主题、动画与无障碍

| 方面 | Aegle | GPUI 0.2.2 |
| --- | --- | --- |
| 默认控件 | Label、Button、TextField/TextArea、CheckBox、Switch、Radio、Slider、Progress、NumberField、Dropdown、Tabs、Splitter、TreeView、Table、ListView、ScrollView、Popup/Menu/MenuBar、Dialog、Drawer、Tooltip、ImageView、Canvas、FilePicker 等 | 核心仅有 `Div`、`List`、`UniformList`、`Img`、`Svg`、`Canvas`、`TextLayout` 等；没有通用按钮/复选框/文本框 |
| 控件扩展 | `Control` trait + `handle!` + `element!`；第三方控件库只依赖所需 crate | `Element` trait/自定义元素；通常还需自己管理状态与样式 |
| 主题 | 类型化 token、浅/深/高对比、子树局部主题、系统偏好 | `WindowAppearance` + `colors::Colors`；无框架级主题系统 |
| 动画 | 过渡、弹簧、关键帧、完成回调、减少动态效果 | `Animation`/`AnimationExt`、`ease_in_out`、`bounce`、`pulsating_between` |
| 键盘与动作 | 控件库键绑定、`Ui::on_key`、捕获/冒泡、焦点；无统一 Action 层 | `Action` derive、`KeyBinding`、keymap、上下文断言、原生菜单 |
| 无障碍 | AccessKit + 三平台 adapter，语义树/角色/动作/文本 run；feature 开关；AT-SPI 与 UIA 有验证，macOS 仅交叉编译 | 0.2.2 发布包公开 API 与依赖中均未见无障碍语义树/系统适配 |
| 调试工具 | 无可视检查器；计划 debug-only 语义快照 | Inspector、screen-capture、leak-detection |

这一节是“应用套件”和“框架内核”的差别。Aegle 的目标是让第三方只写业务和自定义控件，默认控件、主题、动画、无障碍直接可用；代价是 29 个 crate 的维护面与更长文档。GPUI 的目标是给 Zed 这类应用提供高性能底座，控件与主题由应用层（Zed 的 `ui`/`theme`/`editor`）或社区实现；因此独立使用 gpui 时，做一套按钮/输入框/主题往往就是额外工程量。

无障碍是 Aegle 明确承诺而 GPUI 0.2.2 发布包未覆盖的能力。需要注意 Aegle 的无障碍默认不编译系统 adapter（用户决定），启用单个 `accessibility` feature 才接入 AT-SPI/UIA/NSAccessibility；AT-SPI 与 UIA 有协议/客户端验证，macOS 只做了交叉编译，真实屏幕阅读器仍未验收。GPUI 的 0.2.2 发布包没有公开无障碍接口，这不等于 Zed 应用层或 main 分支永远没有：上游资料对 GPUI 的无障碍状态说法并不一致，main 的示例列表已有 `a11y` 示例，但这些能力没有进入 0.2.2（0.2.2 的示例列表没有 `a11y`，依赖清单也没有 AccessKit）。按发布版评估时，不能把“GPUI 已内建无障碍”当作事实；按 main 评估时，也需要重新核实其平台覆盖与成熟度。

## 7. 平台与桌面集成

| 方面 | Aegle | GPUI 0.2.2 |
| --- | --- | --- |
| Linux 窗口 | Wayland（xdg-shell）；**不实现 X11** | X11 + Wayland（默认 features 同时启用） |
| Linux 特有 | layer-shell 面板/背景层、text-input-v3、分数缩放、portal 主题偏好 | 未见公开的 layer-shell 表面 API；常规桌面窗口 |
| Windows | Win32、TSF、GDI/Vulkan/wgpu-DX12；Windows 11 实机验证 | Windows 后端：DirectX 11 + DirectWrite；Zed 于 2025-10-15 宣布 Windows 正式支持，README 仍写 macOS/Linux |
| macOS | AppKit + 软件/wgpu-Metal；**未在 Mac 实机运行** | Metal；Zed 的主力平台，生产验证最充分；Zed 最低 macOS 10.15.7 |
| 移动/Web | 无 | 0.2.2 无官方目标（src/platform 只有 mac/linux/windows/blade/test） |
| 文件对话框 | `aegle-desktop`：Linux portal、Windows IFileDialog；macOS 返回 Unavailable | `PathPromptOptions`/`PromptHandle` 平台文件对话框 |
| 通知/托盘/全局快捷键 | `aegle-desktop` 提供（Linux portal/StatusNotifierItem；Windows Shell；macOS Unavailable） | 0.2.2 公开 API 未见托盘/通知/全局快捷键；main 有 `system_notifications` 示例，未进入发布包 |
| 原生菜单 | 自绘 MenuBar；不接入系统菜单栏 | `Menu`/`OsMenu` 与系统菜单集成 |
| 剪贴板/拖放 | 有；Wayland/Win32 已验证 | 有；Zed 实际使用 |
| 屏幕捕获 | 无 | `screen-capture` feature |
| 高 DPI/分数缩放 | Wayland 分数缩放 + wp-viewporter；Windows DPI | 支持缩放（平台层） |
| 最低系统 | Linux glibc 2.36+ / 内核 6.1+（目标）；Windows 11；macOS 13+（目标） | Zed 文档：macOS 10.15.7+；Linux glibc 2.31+（x86_64）/ 2.35+（aarch64）；Windows DX11 GPU |

平台是 GPUI 最明显的现实优势：Zed 在 macOS、Linux、Windows 上线，macOS 是长期主力，Linux 同时覆盖 X11 与 Wayland；Aegle 的 Linux 只做 Wayland，macOS 只有交叉编译，Windows 做了真实桌面验证但 ARM64 与其他 GPU 驱动未验收。反过来，Aegle 的 Wayland layer-shell、text-input-v3 与 portal 集成是 Quickshell 类面板/覆盖层项目的直接能力，GPUI 未见对应抽象；Aegle 的 `aegle-desktop` 也补上了 GPUI 发布版缺少的托盘/通知/全局快捷键。GPUI 的 Windows 支持虽然已经正式化，但历史短于 macOS/Linux，Linux 侧的 Vulkan 驱动兼容性与 Wayland IME 仍在持续修复，这些都是采用时要评估的风险。

## 8. 模块化、嵌入与构建资源

Aegle 把能力拆成 31 个 crate：`types/core/layout/scene/text/glyph/segment/render-*/platform-*/access/theme/motion/controls/widgets/markup/macros/loader/app/ui/facade`，依赖方向单向，`aegle-ui` 是无窗口引擎，第三方控件库可只依赖 `aegle-ui`；`aegle-render-tiny-skia` 与 Linux 的 `aegle-render-cairo` 共用 `aegle-render-cpu` 的合成器、都只做几何而不带字体栈；没有全局服务定位器、通用异步运行时或 winit。默认组合是“目标平台窗口 + 软件绘制 + 系统字体 + 标记 + 过渡”，Vulkan/wgpu/无障碍/Grid 按 feature 选择。文档记录 `hello` 默认 release 构建 2,580,056 B（2.46 MiB，另一工具链上的绝对测量，本机同构建为 2,494,112 B），目标 8 MiB；1000 控件无窗口场景首帧 3.69–3.88 ms、空闲刷新 23–25 µs/4 次分配，2000 行文档编辑器单字符输入 22 µs。整机 PSS、空闲 CPU、启动时间等仍是待验收目标，不是已完成结果。见[实现状态](implementation.md)、[资源](resources.md)、[crates 与示例](crates-and-examples.md)。

GPUI 是 1 个主 crate（`gpui`，63k 行 Rust、127 个文件，crate 发布包约 5.29 MB、约 47 个常规依赖）加若干 `gpui_*` 辅助 crate，平台后端在 0.2.2 中还是 `src/platform/{mac,linux,windows,blade,test}` 内部模块，main 分支已开始拆成 `gpui_macos`/`gpui_linux`/`gpui_windows`/`gpui_platform`。它提供完整应用生命周期（`Application`、`Window`、`Entity`、异步执行器、动作、菜单、提示框），但**没有** `aegle-ui` 那样的独立无窗口引擎。要嵌入已有宿主，需要自己实现 `Platform`/窗口层，成本远高于使用 `aegle-ui`。GPUI 的依赖与功能面也大得多：blade、cosmic-text、taffy、lyon、etagere、resvg/usvg、image、serde、schemars、smol 等；它没有以二进制体积、空闲唤醒或低内存为目标，也不提供软件渲染回退。这些不是缺陷，而是面向不同产品形态的取舍。

## 9. 验证、成熟度与风险

| 证据 | Aegle | GPUI 0.2.2 |
| --- | --- | --- |
| 发布/用户 | 未发布；仓库 v0.1；无外部用户 | crates.io 0.2.2；约 40 万次下载；Zed 生产使用；0.2.2 之后约一年没有新发布，main 明显领先 |
| 代码规模 | 510 个 Rust 文件、约 9.3 万行（含测试/示例）、29 个 crate | 127 个 Rust 文件、约 6.3 万行、1 个主 crate；约 47 个常规依赖 |
| 测试/CI | 工作区测试、clippy/fmt/doc、feature 矩阵、MSRV 1.88、Windows/macOS runner、cargo-deny；实现状态记录 188 项通过 | `#[gpui::test]`、`TestAppContext`、可视化测试；Zed 自身测试体系 |
| 示例/脚手架 | 31 个示例、控件参考截图、可执行 standalone 示例 | 0.2.2 约 30 个官方示例；`create-gpui-app` 脚手架；gpui.rs 示例站 |
| Linux | Plasma Wayland 实机、私有 headless Sway、软件/Vulkan/wgpu、像素对比 | 生产使用；X11/Wayland；Vulkan 驱动兼容性（如 amdvlk）与 Wayland IME 仍在修 |
| Windows | Windows 11 实机：窗口、TSF 输入法、UIA、OLE 拖放、桌面服务、Vulkan/wgpu | Zed 自 2025-10-15 正式支持；DX11 + DirectWrite |
| macOS | 仅 Linux 上交叉编译/单测；未在 Mac 运行 | 主力平台，生产验证（最低 10.15.7） |
| 文档 | 约 3.6k 行中文文档（设计 2.4k + 开发者 1.2k）；设计目标与实现证据分离 | rustdoc 覆盖率 100%，~30 示例与示例站，但指南仍少，README 建议读 Zed 源码 |
| API 稳定性 | 未发布，无兼容承诺 | pre-1.0，版本间常有破坏性变更；未声明 MSRV |
| 无障碍 | AccessKit 三平台，AT-SPI/UIA 有验证，macOS 未验收 | 0.2.2 发布包无；main 有 `a11y` 示例，但未进入发布版 |
| 默认控件/主题 | 有 | 无（在 Zed 其他 crate） |

性能数字不能直接对照：Aegle 的 1000 控件 3.69–3.88 ms 是无窗口、软件绘制/本地字体场景的 microbench（另有 2000 行文档编辑器的 22 µs 单字符输入），不是整机交互延迟；GPUI 的 120 FPS 是 Zed 在 GPU 上渲染编辑器的公开叙述，没有 benchmark 表。两者都缺少可复现的端到端对照（相同窗口、字体、内容、GPU/CPU、功耗），本文不拿这些数字做排名。

除技术差异外，两者的**治理模型**也不同：Aegle 是单仓库、单作者、按本仓库文档推进的独立项目；GPUI 绑定 Zed，官网明确说“近期 GPUI 与 Zed 绑定，贡献需在 Zed 仓库提交并保持同步”，主分支优先服务 Zed。GPUI 的发布节奏因此也是采用风险：0.2.2 停留在 2025-10-22，约一年没有新的 crates.io 版本，而 main 已推进数百个提交并把平台拆成新 crate；社区出现了 `gpui-unofficial`、`gpui-pre`、`gpui-ce` 等镜像/分支来补发布缺口（其中部分信息来自社区文章，未由 Zed 官方确认）。这意味着“用 GPUI 0.2.2”与“跟 GPUI main”是两种不同的依赖策略，前者稳定但落后，后者能力新但需要跟仓库。

Aegle 的主要风险是采用风险而非技术空白：项目年轻、单作者、未发布、API 会变；Linux 不支持 X11；macOS 未实机；资源目标尚未整机验收；系统文字栈让 Linux 运行依赖变多；文档虽然有大量证据，但仍是自述，需要外部复现。GPUI 的主要风险是生态与产品匹配：API 不稳定，升级要跟 Zed 节奏；必须 GPU；核心缺少控件、主题、无障碍，独立应用要自建或依赖未随 gpui 发布的 Zed crate；Windows/Linux 的质量历史上落后 macOS；无官方 1.0 时间表。

## 10. 选型建议

| 场景 | 建议 | 理由 |
| --- | --- | --- |
| 512 MB～1 GB ARM64 Linux、无 GPU 或 GPU 不可靠 | Aegle | 软件渲染是默认路径；有明确资源预算与有界缓存 |
| Wayland 面板/覆盖层/多显示器 shell（Quickshell 类） | Aegle | layer-shell、text-input-v3、portal 主题、Wayland 原生实现 |
| 以 CJK/IME 表单为主，希望开箱有文本框/编辑器/控件 | Aegle | 自带 Editor、IME 事务、默认控件与主题；需接受 v0.1 |
| 标记优先、需要编译期检查或运行时热重载、AI 生成 UI | Aegle | `.aegle` + `ui!`/`loader` 的双路径是明确设计目标 |
| 要嵌入现有窗口/渲染宿主，只用 UI 引擎 | Aegle | `aegle-ui` 无窗口，可单独组合 renderer/platform |
| macOS/Linux/Windows 三平台 GPU 桌面应用 | GPUI | Metal/Vulkan/D3D11 + Zed 生产验证 |
| 必须同时支持 Linux X11 与 Wayland | GPUI | Aegle 明确不实现 X11 |
| 需要异步、Action/KeyBinding、原生菜单、Inspector | GPUI | GPUI 内建；Aegle 需自行搭建 |
| 严格的无障碍采购要求（AT-SPI/UIA/VoiceOver） | 两者都不够；Aegle 起点更高 | Aegle 有语义树与 AT-SPI/UIA 验证，但 macOS/真实屏幕阅读器未验收；GPUI 0.2.2 无公开接口 |
| 今天就要生产稳定、长期维护承诺 | 两者都不满足 1.0；GPUI 更接近 | GPUI 有 Zed 生产使用但 API 会破坏；Aegle 未发布 |

不建议把两者混用：都拥有窗口、事件循环与渲染所有权，模型（句柄保留 vs 每帧 Render）也不同；把 Aegle 的 `aegle-ui` 嵌入 GPUI 窗口在理论上要自写宿主，但没有现成集成，成本高于收益。

## 11. 待核实与边界

- GPUI 的 main 分支已经出现 0.2.2 没有的能力（例如 `a11y`、`system_notifications` 示例，平台 crate 拆分）；本文对发布版的结论不能直接套到 main。若选择 GPUI，需先决定锁 0.2.2 还是跟 main/镜像。
- GPUI 的 Windows/Linux 成熟度应以 Zed 当前发布说明为准；README 的“macOS or Linux”与 0.2.2 依赖中的 Windows 后端不一致，说明文档滞后。
- GPUI 的二进制体积、常驻内存、启动时间与空闲 CPU 没有公开的对照数据，不能拿 Zed 编辑器体积推断框架开销；需要实测。
- GPUI 的无障碍状态在上游资料中说法不一致（网站/主分支提到 AccessKit 与 `a11y` 示例，但 0.2.2 发布包没有）；采用前必须按锁定版本核实，尤其不能把 Zed 应用层的能力当作 gpui 的能力。
- Aegle 的整机资源目标（PSS、空闲唤醒、启动、嵌入式）尚未完成验收；`hello` 体积与 1000 控件微基准是当前可引用的测量，不等于整机结论。
- Aegle 的 macOS 平台、IME、着色器/字形像素与无障碍都只做了交叉编译或部分协议验证；上 Mac 实机前不能宣传等价支持。
- Aegle 的系统文字栈在 Linux 上依赖 Pango/Fontconfig/FreeType 的运行时存在；这与“发布依赖少”不矛盾（动态加载，不计入二进制），但会增加部署前提。
- Aegle 的标记语言是自有语法，第三方工具、编辑器支持与人才储备都不如纯 Rust DSL；这是可维护性成本。

## 来源

Aegle：

- [README](../README.md)、[设计索引](README.md)、[需求](requirements.md)、[架构](architecture.md)、[模块](modules.md)
- [Rust API](rust-api.md)、[标记语言](markup.md)、[实现状态](implementation.md)
- [文字与 IME](text-input.md)、[无障碍](accessibility.md)、[平台与绘制](platform-rendering.md)
- [资源与预算](resources.md)、[crate 与示例](crates-and-examples.md)

GPUI：

- crates.io：<https://crates.io/crates/gpui/0.2.2>（0.2.2，2025-10-22，Apache-2.0）
- docs.rs：<https://docs.rs/gpui/0.2.2/gpui/>
- 官网与示例：<https://gpui.rs/>、<https://gpui.rs/examples>
- README：<https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md>
- 所有权与数据流：<https://docs.rs/gpui/0.2.2/gpui/_ownership_and_data_flow/index.html>、<https://zed.dev/blog/gpui-ownership>
- `Render`：<https://docs.rs/gpui/0.2.2/gpui/trait.Render.html>
- 依赖与 feature：<https://crates.io/api/v1/crates/gpui/0.2.2/dependencies>、<https://docs.rs/crate/gpui/0.2.2/source/Cargo.toml.orig>
- 平台要求：<https://zed.dev/docs/linux>、<https://zed.dev/docs/macos>、<https://zed.dev/docs/windows>
- Windows 正式支持与 DX11：<https://zed.dev/blog/zed-for-windows-is-here>
- 120 FPS 渲染说明：<https://zed.dev/blog/videogame>
- 脚手架：<https://github.com/zed-industries/create-gpui-app>
- blade-graphics：<https://github.com/kvark/blade>
- cosmic-text：<https://github.com/pop-os/cosmic-text>
