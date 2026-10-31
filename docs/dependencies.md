# 依赖与版本基线

状态：v0.1 选定设计基线。自身 crate 使用 Rust 2024，目标 MSRV 1.88；这是依赖发布声明支持的目标，尚未构建验证完整依赖闭包。依赖可以使用其他 edition。

| 职责 | 基线 | 说明 |
| --- | --- | --- |
| 布局 | Taffy 0.14.0 | Flex/Block 默认，Grid 可选；低层树适配 |
| 软件覆盖率栅格化 | tiny-skia 0.12.0 | 仅 std/simd，关闭默认 PNG；只用几何覆盖率，线性颜色合成由 `aegle-render-cpu` 完成 |
| Linux 系统覆盖率栅格化 | 运行时加载 `libcairo.so.2`（libloading 0.8.9） | 仅 `aegle-render-cairo`：可选 CPU 后端，用发行版的 cairo 光栅化几何覆盖率。不链接、不读头文件、不内嵌 cairo；只绑定图像 surface/路径/mask 入口 |
| Vulkan | ash 0.38.0+1.3.281 | 当前离屏/原生窗口几何及可选文字；loaded/std 动态加载，资源与同步由小型封装管理，不采用 vulkano；wgpu 只用于下一行的可选后端 |
| 跨平台 GPU | wgpu 30.0.1 | 仅 `aegle-render-wgpu`：关闭默认 feature，启用 std、parking_lot、wgsl、vulkan、dx12、metal；不启用 GLES（无顶点存储缓冲）；适配器与设备请求由 `gpu.rs` 中约 20 行的 std `block_on`（`Wake` + `thread::park`）阻塞等待 |
| GPU 数据布局 | bytemuck 1.25（当前锁定 1.25.2） | Pod/Zeroable 与安全字节转换；shader 布局按显式契约对应 |
| Vulkan shader 编译 | Naga 30.0.1 | 仅构建期 wgsl-in/spv-out，生成 Vulkan 1.1 SPIR-V，不进入发布运行依赖 |
| Wayland | wayland-client 0.31.15、SCTK 0.21.1 | 软件独立构建用 Rust client backend；gpu feature 启用 system/dlopen 获取 libwayland 原生句柄，保留同一连接 |
| Linux 窗口（默认） | 运行时加载 `libSDL3.so.0`（libloading 0.8.9） | `aegle-platform-sdl3`：窗口、输入、IME、剪贴板、拖放接收与 GPU 原生句柄都交给发行版的 SDL3；不链接、不读头文件、不内嵌 SDL3。缺失该库时 `Sdl3::connect()` 以 `Backend` 错误失败，不静默换成另一个窗口后端（构建里也没有） |
| Windows | windows 0.62.2 | 按模块启用所需 Win32/GDI/TSF/UIA 能力（`Win32_UI_TextServices` 需要 `Win32_System_Variant`）。`aegle-text` 额外启用 `Win32_Graphics_DirectWrite`（排版与字体集合）、`Win32_Globalization`（Uniscribe `ScriptItemize`/`ScriptBreak` 的 grapheme 边界）、`Win32_System_Com`（`DWriteCreateFactory` 与 COM 生命周期）与 `Win32_Graphics_Gdi`；`aegle-glyph` 只启用 `Win32_Foundation` 与 `Win32_Graphics_DirectWrite`（glyph-run analysis 的灰度 alpha 纹理）。COM 对象用 windows 的 RAII 包装，不手写 vtable；`implement` 宏直接命名 `windows-core`，因此该包也是直接依赖 |
| 系统分词 | libloading 0.8.9（Linux/Windows 动态加载系统 ICU）、objc2-core-foundation 0.3.2（macOS 的 `CFStringTokenizer`） | 不链接任何词典数据；找不到分词器时 `aegle-segment` 报告不可用，按词操作关闭 |
| macOS | objc2 0.6.4 + objc2-app-kit / objc2-foundation / objc2-core-foundation / objc2-core-graphics / objc2-core-text 0.3.2、block2 0.6.2 | AppKit 窗口、输入与 `NSTextInputClient`；软件呈现用 Core Graphics 的 `CGImage`，Metal 只经 wgpu 使用；不采用已弃用 metal crate 或 MoltenVK。`objc2-core-text` 供 `aegle-text` 的 CoreText 排版（`CTFramesetter`/`CTLine`/`CTRun`/`CTParagraphStyle`）与 `aegle-glyph` 的 `CTFontCreatePathForGlyph`；`objc2-core-graphics` 供 `CGPath` 遍历与 `CGFont` 注册。两者都只在 macOS target 编译 |
| 系统文字栈（Linux） | 运行时加载 `libpango-1.0.so.0`、`libpangoft2-1.0.so.0`、`libgobject-2.0.so.0`、`libfontconfig.so.1`（libloading 0.8.9） | 排版、bidi、换行、字体回退与字形光栅都交给发行版的系统库；不链接、不内嵌任何字体引擎或数据 |
| 系统文字栈（Windows） | `dwrite.dll`、`usp10.dll`（windows crate 静态链接的导入库，系统自带） | 排版、字体集合与字形覆盖率都是 DirectWrite（`IDWriteFactory2::CreateGlyphRunAnalysis` 灰度抗锯齿、`DWRITE_TEXTURE_ALIASED_1x1` 纹理），grapheme 边界是 Uniscribe；不链接、不内嵌任何字体引擎或数据 |
| 系统文字栈（macOS） | CoreText、CoreGraphics（系统框架） | 排版来自 CoreText，字形由 `CTFontDrawGlyphs` 画进只有 alpha 的 `CGBitmapContext` 得到覆盖率；不链接、不内嵌任何字体引擎或数据 |
| 系统字形（Linux） | 运行时加载 `libfreetype.so.6`（libloading 0.8.9） | 轮廓、hinting、内嵌位图与 COLR 彩色层；有界缓存仍由 aegle-glyph 管理 |
| 文本回退（不再维护） | Parley/Fontique 0.11.1、HarfRust 0.12.0、Skrifa 0.44.0、Swash 0.2.10、icu_segmenter 2.3.0 | 仅 `aegle-text/text-parley` 与 `aegle-glyph/swash` opt-in。Linux、Windows、macOS 的默认构建都不包含这些包（各自走系统栈）；只有既无系统后端也未启用 feature 的 target 才无条件编译回退实现，避免默认构建静默失去文字与字形 |
| 共享字体资源 | linebender_resource_handle 0.1.1 | scene/text 仅借助该轻量句柄共享字体字节，不引入 shaping |
| 字形缓存索引 | hashbrown 0.17.1、lru-slab 0.1.3 | 哈希索引和 LRU 槽位复用现成实现，命中不分配 |
| 语义 | AccessKit 0.24.1 | 与 Parley 可选 text-a11y 使用同一 schema |
| 系统语义 adapters | macOS 0.26.3、Windows 0.34.0、Unix 0.22.1 | 由单个 `platform` feature 接入，每个依赖只为自己的目标声明，因此只构建一个适配器；macOS 适配器仍基于 objc2 0.5，与平台 crate 的 objc2 0.6 在同一锁定图中共存 |
| Unix 无障碍传输 | accesskit_atspi_common 0.19.1、atspi 0.29.0、zbus 5.19.0 | 复用 AccessKit 适配；采用 async-io，无 Tokio |
| 路径 | 软件复用 tiny-skia，Vulkan 复用 zeno 0.3.3（swash 已依赖） | CPU 覆盖率光栅，不引入 Lyon 或三角细分 |
| PNG | png 0.18.1 | aegle-glyph 用于有界字体位图解码；纯几何 renderer 仅示例使用，不引入整个 image crate |
| SVG | resvg/usvg 0.48.1 | 仅 `aegle-image/svg` 与 `aegle-glyph/svg`：关闭 text、system-fonts 等默认 feature，glyph 侧启用 svgz；不处理 SVG 文字、外部文件或网络资源。`aegle-glyph/svg` 需要 `skrifa` 定位 OpenType-SVG 表，但**不启用 `swash`**：它与 `colrv1` 都是按字形的颜色表示，不是整套字形光栅器 |
| 其他图像格式 | zune-jpeg 0.5、image-webp 0.2、gif 0.14 | 仅 `aegle-image` 的 `jpeg`/`webp`/`gif` feature；都先检查尺寸与字节预算再分配；`image` crate 只作为测试编码器的开发依赖 |
| COLRv1 字形 | tiny-skia 0.12.0（aegle-glyph 的可选依赖，只由此 feature 引入）与 `skrifa` | `aegle-glyph/colrv1` 用 `skrifa` 解析绘制图、tiny-skia 光栅化，不增加新包，也**不启用 `swash`** |
| 标记编译宏 | syn 2、quote 1、proc-macro2 1 | 仅编译期；Rust 语法与代码生成复用现成库；facade 与 loader 的 `macro_rules!` 包装把 `$crate` 传给 proc macro，依赖改名后路径仍然正确，不读 Cargo 清单 |

设计版本来自 crates.io 发布记录及发布包 manifest 的核查。Taffy、tiny-skia、Parley/Fontique/HarfRust、Swash/Skrifa、字体句柄、缓存/PNG、Wayland 及独立 Vulkan 几何/文字依赖已进入 Cargo.lock 并在当前工具链构建验证；可选 Parley AccessKit 文本桥与 Unix adapter 已构建，并通过私有总线上的 AT-SPI 协议验证。Windows 编译与运行证据、GPU 原生呈现验证另见实现状态，不能将其等同于全部实机验收；Unix 当前能力与限制见[无障碍](accessibility.md)，具体验证见[实现状态](implementation.md)。tiny-skia 使用 BSD-3-Clause，不引入原生 Skia、图形驱动或窗口系统。特别保留 Parley/HarfRust/AccessKit 的兼容版本组，不把各库最新版随意组合。实现时检查完整传递依赖、许可、feature 合并与 MSRV；这是实现验收，不是尚待用户选择的架构问题。

`aegle-text` 默认走平台系统文字栈：Linux 用 Pango（含 bidi、UAX #14 换行与字体回退）、Fontconfig 与 FreeType，全部在运行时按 `.so` 名称加载，找不到时 `TextSystem::system()` 报告不可用而不是退回第二套引擎；Windows 用 DirectWrite 排版、Uniscribe 取 grapheme 边界；macOS 用 CoreText。grapheme 删除边界同样来自系统栈，因此默认构建不再携带 `icu_segmenter` 的 UAX #14 数据。`system-fonts`、`text-a11y`、`scene` 独立选择。按词移动与双击选词仍由独立的 `aegle-segment` 在运行时向系统分词器要边界（Linux/Windows 的 `libicui18n`/`icu.dll`，macOS 的 `CFStringTokenizer`），不链接任何词典数据；找不到时这两个操作停用。Parley/HarfRust/Skrifa/Swash（以及它们需要的 `icu_segmenter/compiled_data`）只在显式启用 `text-parley` 时参与构建，作为不再维护的回退路径保留，默认关闭。

`aegle-access` 默认依赖 AccessKit schema 与标准库通道，不带原生 adapter、异步运行时或窗口库。`unix` 仅在非 macOS 的 Unix 目标启用 AccessKit Unix 0.22.1，关闭其默认 features 并显式选择 `async-io`；上游不允许同时启用 `async-io` 与 `tokio`。该闭包含 AccessKit consumer 0.38.0、AT-SPI、Serde、zbus 及其异步组件，不能描述为只有一个小型运行依赖。consumer 使用的 hashbrown 0.16 与字形缓存的 0.17 同时存在于锁定图，不为统一版本私自修改上游依赖。上游 adapter/consumer 声明 MSRV 1.85，zbus 5.19 声明 1.87；完整 MSRV 仍须实际工具链验收。

Wayland 库本身没有 AccessKit 依赖；Unix adapter 由 aegle-app 的 `accessibility` 经 `aegle-access/platform` 引入，Windows 与 macOS 的适配器在同一 feature 下只为各自目标构建。构建时纳入 async-io/zbus 不等于启动即创建线程，首次构造 UnixAdapter 才启动上游 worker；此后 worker 的生命周期、无界队列和语义缓存成本见[资源](resources.md)。Unix 原生文字选择可用，但当前上游缺少 AT-SPI EditableText 接口；不能用依赖版本声明代替完整控件支持。

`aegle-glyph` 默认用平台光栅器，三个平台都由系统直接给出 8-bit 覆盖率：Linux 是运行时加载的 FreeType，Windows 是 DirectWrite 的 glyph-run analysis，macOS 是 CoreText 画进 alpha-only 位图上下文。因此 aegle-glyph 只在 `colrv1` 下依赖 tiny-skia（`svg` 经 resvg 间接带入）。只有 `swash` feature 才用 Swash std/render 与 Skrifa 0.44 解析字体；`swash` 路径仍使用 png 的有界解码接口处理嵌入 PNG，避免无上限的中间解码分配。`aegle-render-tiny-skia/text` 与 `aegle-render-vulkan/text` 显式引入此依赖闭包和 scene/text，均不依赖 Parley；默认纯几何构建没有字体栈或 PNG。Vulkan 图集索引复用已有 hashbrown 0.17，不增加另一套字体解析或栅格库。Cargo 测试/示例的 dev-dependencies 不代表库的发布依赖，仍需核查最终应用的 feature 合并。

SVG 默认以路径图标/构建期资产为主；可选运行时 resvg（`aegle-image/svg`、OpenType-SVG 字形的 `aegle-glyph/svg`）不处理 SVG text、外部 URL 或网络资源。需要 SVG 文字时在构建期转轮廓。构建期转换为位图需要指定尺寸/缩放档位，不能宣称与任意动态缩放完全等价。

`aegle-markup` 的语法很小，采用直接流式词法分析和递归下降，表达式按优先级爬升解析，不引入通用脚本或表达式框架；`aegle-loader` 以小型树解释已检查的表达式，同样没有第三方依赖。`aegle-macros` 则复用 syn/quote，避免重复实现 Rust 参数语法；生成代码经包装宏传入的 `$crate` 指向 facade 或 loader，不解析调用方的 Cargo 清单；这些包只参与构建，不随应用运行。`markup` 的目标依赖闭包与发布体积须区分编译主机侧的宏依赖。

来源：[Parley 发布清单](https://docs.rs/crate/parley/0.11.1/source/Cargo.toml)、[Swash 发布清单](https://docs.rs/crate/swash/0.2.10/source/Cargo.toml)、[AccessKit](https://github.com/AccessKit/accesskit)、[resvg 发布清单](https://docs.rs/crate/resvg/0.48.1/source/Cargo.toml.orig)。

Wayland 软件后端使用 SCTK 0.21.1/calloop 0.14，键盘使用同一版本的 xkbcommon 0.8 包；不引入 winit、Tokio、softbuffer 或 wgpu。SCTK 自带自动重复计时器不提供完整取消接口，因此平台持有自身 repeat token，在焦点/设备/窗口销毁和 backend drop 时移除。按键翻译仍复用 SCTK，重复状态额外持有一份 XKB keymap/state 用于按键可重复性与 modifier 更新；这是实际额外内存，不宣称零成本封装。后续上游若提供借用 keymap 与取消 timer 接口，可移除此重复状态。

构建需 libxkbcommon 的开发链接与 pkg-config 信息，发布需对应 runtime；SDK 不打包进程序。当前验证机器仅安装 runtime，验证时在临时目录创建了指向已有系统库的开发链接与 `.pc` 并经 `PKG_CONFIG_PATH` 引用，未修改系统或项目构建配置。当前软件路径未启用 wayland-client/system，因此不将 libwayland-client 误写成该示例的实际动态依赖；最终以构建的依赖树和二进制链接结果为准。

原生 window 复用 raw-window-handle 0.6.2；不引入 winit/ash-window。macOS 平台按目标仅链接 objc2 0.6 族与 Core Graphics/Foundation；`aegle-access/macos` 另编译 accesskit_macos 0.26.3，其 objc2 0.5 依赖不会进入 Linux/Windows 目标。Win32 平台仅启用 windows crate 所需 API；UIA 在 aegle-access/windows 中额外编译 accesskit_windows 0.34.0，与现有 schema/consumer 兼容。Windows 的 target-specific 依赖不会将 Unix D-Bus、Wayland 或 Fontconfig 链入 Windows 程序。
