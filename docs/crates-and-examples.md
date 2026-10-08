# Crate、示例与构建组合

状态：2026-10-06，自根 README 迁入并整理。能力与验证证据以[实现状态](implementation.md)为准；应用层用法见[开发者 API 指南](developer/api.md)。

## 当前能力概览

紧凑共享类型、保留树、Taffy 布局、保留式 Unicode 段落与编辑器、按需 CJK 字形、绘制记录和软件光栅化已可用。独立 Wayland 后端提供原生窗口、有界 SHM 呈现、键盘/指针输入与 text-input-v3。应用层提供窗口、行、列、透明分组（可选网格与叠放）及对齐、换行、外边距、最小/最大尺寸、百分比、绝对定位等 flex/grid 布局，滚动视图、标签、按钮、纯文本字段、复选框、开关、滑块和进度条，并带浅色/深色/高对比主题。编译型 `.aegle` 标记与简洁的命令式 Rust 创建相同的保留控件。局部颜色、字体排印和小型主题/状态皮肤函数让组件库复用这些控件。共享的控件行为把输入和焦点接到保留树与编辑器。可选的绘制过渡共享同一状态，以帧驱动采样、平滑重定向，并显式支持减少动态效果；缩放/旋转、惯性滚动、渐变/阴影图像、触摸手势、后台 `UiProxy`、按字段的主题覆盖与系统文本缩放也已提供，可选的 JPEG/WebP/GIF/SVG 解码在 `aegle-image`。

可选的 Unix 无障碍通过 AT-SPI 暴露控件、CJK 文本、选择、焦点与按钮动作；AT-SPI 文本替换仍不支持。Windows 增加 Win32 窗口、软件/Vulkan 呈现、IMM 组合与可选 UIA。TSF、macOS 和完整的辅助技术验收仍未完成。

标记当前支持静态字面量、绑定、事件块、组件（含 slot 与组件事件）、record 与带 key 的列表、`let` 与宿主动作、导入和运行时加载，细节见[标记语言](markup.md)。标记编译为直接的构造器与 setter；带 `id` 的字段返回类型化的弱句柄，可用于普通 Rust 回调；仅用 `ui!` 的可执行文件不带标记解析器或运行时注册表。

## 默认组合与平台前提

`aegle` 默认启用原生窗口（Linux Wayland / Windows Win32）、软件绘制、系统字体、标记（含动态标记引擎）与过渡。系统无障碍适配默认关闭：`--features unix-accessibility`（AT-SPI，需要会话 D-Bus，并引入 zbus 栈）或 `--features windows-accessibility`（UI Automation）。

- 系统字体必须覆盖所请求的文字；Linux 的系统字体发现还链接系统 Fontconfig 及其发行版相关的运行时依赖，显式提供应用字体可避免该依赖。
- Wayland IME 需要 text-input-v3；没有它时聚焦可编辑字段返回能力错误。
- Wayland 需要 xdg-shell 与版本不低于 4 的 wl_compositor；构建需要 libxkbcommon 的 pkg-config 元数据，运行需要 libxkbcommon 运行时。
- Windows 目前使用 IMM 兼容路径，不是 TSF 文本存储。macOS 仍在开发中。

## 示例与检查命令

发布版本最快。调试构建也保持可交互：workspace 的 dev profile 以 opt-level 2 编译依赖、软件 renderer 和平台像素转换（wgpu-core 除外，本机 rustc 1.96 优化它时栈溢出），其余代码仍可调试。

```sh
cargo run -p aegle --example hello --release
cargo run -p aegle --example controls --release
cargo run -p aegle --example hello_markup --release
cargo run -p aegle --example markup_controls --release
cargo run -p aegle --example components --release
cargo run -p aegle --example widgets --release
cargo run -p aegle --example scrolling --release
cargo run -p aegle --example showcase --release
cargo run -p aegle --features grid --example layout --release
cargo run -p aegle-widgets --features motion --example standalone --release
cargo run -p aegle-widgets --example custom_control --release
cargo test --workspace --all-features
cargo run -p aegle-layout --example retained --release
cargo run -p aegle-render-software --example software_scene --release
cargo run -p aegle-render-software --features text --example text_scene --release
cargo run -p aegle-render-software --features text --example editor_scene --release
cargo run -p aegle-platform-wayland --example editor --release
cargo doc --workspace --all-features --no-deps
```

`cargo run -p aegle --example gallery` 重新生成[控件参考](developer/controls.md)中的截图。

## 独立 crate

- `aegle-types`：no_std 的几何与颜色。
- `aegle-dbus`：无依赖的阻塞式 D-Bus 客户端（仅 Linux）：会话总线认证、调用/返回/错误/信号与常用值类型，可从任意线程发送；Wayland 平台读设置 portal 与桌面集成共用它。
- `aegle-core`：保留状态，无第三方依赖。
- `aegle-layout`：该树上的 Taffy 布局，经过校验的布局值词汇、透明 contents 节点，`grid` feature 增加网格。
- `aegle-scene`：经过校验的绘制记录。
- `aegle-text`：段落、字体回退、纯文本编辑、组合与有界 delta 撤销。
- `aegle-glyph`：按需字形光栅化与有界图像缓存，含合成粗体/斜体；`colrv1`、`svg` feature 增加 COLRv1 与 OpenType-SVG 字形。
- `aegle-image`：有界图像解码，PNG 加可选 JPEG、WebP、GIF 首帧与静态 SVG 栅格化。
- `aegle-render-software`：借用帧缓冲与线性光合成。
- `aegle-gpu`：Vulkan 与 wgpu 后端共用、与图形 API 无关的图元/裁剪记录、场景遍历、图集装箱、图像与路径 mask 放置及 WGSL 着色器。
- `aegle-render-vulkan`：几何与文字、原生 swapchain、有界分配、显式离屏读回。
- `aegle-render-wgpu`：可选的全平台几何、文字、图像与路径，离屏或原生 surface。
- `aegle-controls`：无皮肤的 Button/Toggle/Slider 行为、共享数值 Range 与可选 TextField。
- `aegle-widgets`：默认控件库，包含全部默认控件（标签、按钮、单/多行编辑、复选/开关/单选、滑块、进度、图像/画布、滚动视图、虚拟列表、表格、弹出层、下拉框、菜单与菜单栏）及其纯函数皮肤。
- `aegle-access`：UI 线程回调邮箱与可选的 Unix/Windows 无障碍。
- `aegle-platform-wayland`、`aegle-platform-win32`：窗口与原生输入，不依赖绘制。
- `aegle-theme`：无分配的调色板、基于状态的皮肤与局部样式值。
- `aegle-motion`：无分配的按时间补间。
- `aegle-ui`：无窗口保留式 UI 引擎（树、布局、输入、焦点、滚动、主题、过渡、IME、语义），通过 `Control` trait 接入控件。
- `aegle-app`：原生宿主（窗口、事件循环、renderer、系统偏好、UiProxy）。
- `aegle-markup`：有界解析与静态组件检查；`aegle-macros` 生成编译型视图；`aegle-loader` 是运行时引擎。

模块关系与依赖方向见[模块与构建组合](modules.md)。

## 选择渲染后端

仅软件、系统字体，不带标记与过渡：

```sh
cargo run -p aegle --no-default-features --features native,software,system-fonts --example controls --release
```

仅 Vulkan，运行时依赖里没有软件渲染器：

```sh
cargo run -p aegle --no-default-features --features native,vulkan,system-fonts,markup --example scrolling --release
```

同时编译多个渲染器时，把 `AppOptions.renderer` 显式设为 `RendererBackend::Vulkan` 或 `RendererBackend::Wgpu`；软件仍是默认。驱动、feature 或预算失败返回错误，不会切换后端。只带 Vulkan 的最小构建默认使用 Vulkan。原生 Vulkan 目前为每个窗口创建一个设备，大窗口可能需要调大 `AppOptions.vulkan.memory_budget`。

可选的 wgpu 后端（`wgpu` feature、`RendererBackend::Wgpu`）绘制几何、文字、图像与路径；用 `WGPU_BACKEND` 和 `WGPU_ADAPTER_NAME` 选择适配器。契约见 [wgpu](wgpu.md)。

## 嵌入已有宿主

已有绘制/窗口宿主可以只使用 `aegle-ui` 与 `aegle-widgets`，并用 `Ui::with_fonts` 与显式共享的字体集合构造界面；依赖、feature 组合、宿主职责与两个可执行示例见[不经 facade 使用控件库](developer/standalone.md)。同样的控件暴露 scene 访问、规范化输入、有界 IME 状态和可选的语义更新。控件句柄是弱引用：丢弃句柄保留控件，删除子树或关闭窗口则使其句柄失效。

## 文字支持与示例输出

scene、软件和 Vulkan 渲染的文字支持是可选的。仅几何的构建没有字体栈；应用字体字节保持共享，不内置字体图集。软件渲染示例是无窗口演示，写出 `target/aegle-software.png`、`target/aegle-text.png` 与 `target/aegle-editor.png`；editor 示例验证预编辑、提交与撤销，并把选择/光标装饰记录进同一 scene。PNG 编码器只是开发依赖；可选的 glyph 模块也用 PNG 解码器读取字体内嵌的彩色位图。可移植的测试字体及其 OFL 声明位于 `tests/assets/`，库构建不内置任何字体。

## Vulkan 独立示例

独立 Vulkan 示例需要 Vulkan 1.1 loader 与兼容驱动。可选的 `text` 使用按需的 R8/RGBA8_SRGB 图集页与共享字形光栅化，渲染器不依赖 Parley。示例只用标准库写出 PPM。绘制在 RGBA16F 线性附件中混合，再由 GPU 遍编码为预乘 sRGB RGBA8；读回是显式的。限制与设备验证见 [Vulkan 契约](vulkan.md)；CPU Vulkan 驱动不等于硬件加速。

```sh
cargo run -p aegle-render-vulkan --example geometry --release -- target/aegle-vulkan.ppm
cargo run -p aegle-render-vulkan --features text --example vulkan_text_scene --release -- target/aegle-vulkan-text.ppm
```

wgpu 的对应示例与测试命令见 [wgpu](wgpu.md#验证)。

## Wayland 平台示例

`editor` 示例在一棵保留 Taffy 树中组合 CJK 文本字段与按钮，带路由动作、Tab 焦点和指针捕获；键盘编辑与 IME 使用共享控件和原子文本事务 API。原生组合另需 text-input-v3 与输入法。这个独立平台示例保持为低层组合；应用层 API 的演示是 `aegle --example controls`。

启用原生无障碍示例：

```sh
cargo run -p aegle-platform-wayland --example editor --features example-accessibility --release
```

这需要会话 D-Bus 与 AT-SPI 服务。适配器共享现有的控件/编辑器状态并无需轮询地唤醒 UI，会增加一个进程级工作线程和语义缓存；当前限制（包括缺少 EditableText 与 Wayland 屏幕定位）见[无障碍](accessibility.md)。
