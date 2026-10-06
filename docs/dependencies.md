# 依赖与版本基线

状态：v0.1 选定设计基线。自身 crate 使用 Rust 2024，目标 MSRV 1.88；这是依赖发布声明支持的目标，尚未构建验证完整依赖闭包。依赖可以使用其他 edition。

| 职责 | 基线 | 说明 |
| --- | --- | --- |
| 布局 | Taffy 0.14.0 | Flex/Block 默认，Grid 可选；低层树适配 |
| 软件覆盖率栅格化 | tiny-skia 0.12.0 | 仅 std/simd，关闭默认 PNG；只用几何覆盖率，线性颜色合成由小型自有实现完成 |
| Vulkan | ash 0.38.0+1.3.281 | 当前离屏/原生窗口几何及可选文字；loaded/std 动态加载，资源与同步由小型封装管理，不采用 vulkano；wgpu 只用于下一行的可选后端 |
| 跨平台 GPU | wgpu 30.0.1、pollster 1.0.1 | 仅 `aegle-render-wgpu`：关闭默认 feature，启用 std、parking_lot、wgsl、vulkan、dx12、metal；不启用 GLES（无顶点存储缓冲）；pollster 阻塞式等待适配器与设备请求 |
| GPU 数据布局 | bytemuck 1.25（当前锁定 1.25.2） | Pod/Zeroable 与安全字节转换；shader 布局按显式契约对应 |
| Vulkan shader 编译 | Naga 30.0.1 | 仅构建期 wgsl-in/spv-out，生成 Vulkan 1.1 SPIR-V，不进入发布运行依赖 |
| Wayland | wayland-client 0.31.15、SCTK 0.21.1 | 软件独立构建用 Rust client backend；gpu feature 启用 system/dlopen 获取 libwayland 原生句柄，保留同一连接 |
| Windows | windows 0.62.2 | 按模块启用所需 Win32/GDI/IMM/UIA 能力，TSF 尚未实现 |
| macOS | objc2 0.6.4 | AppKit 系统绑定，尚未实现；原生 Metal 方案已放弃，Metal 只经 wgpu 使用，不采用已弃用 metal crate 或 MoltenVK |
| 文本 | Parley/Fontique 0.11.1 | 基础排版、字体回退及纯文本编辑 |
| grapheme 分段 | icu_segmenter 2.3.0 | 直接复用 Parley 已锁定的包及 compiled_data，编辑删除不另带分段引擎 |
| shaping / 字体解析 | HarfRust 0.12.0、Skrifa 0.44.0 | 按 Parley 兼容版本线，避免追最新产生双份依赖 |
| 字形 | Swash 0.2.10 | 按需光栅化；有界缓存由 aegle-glyph 管理 |
| 共享字体资源 | linebender_resource_handle 0.1.1 | scene/text 仅借助该轻量句柄共享字体字节，不引入 shaping |
| 字形缓存索引 | hashbrown 0.17.1、lru-slab 0.1.3 | 哈希索引和 LRU 槽位复用现成实现，命中不分配 |
| 语义 | AccessKit 0.24.1 | 与 Parley 可选 text-a11y 使用同一 schema |
| 系统语义 adapters | macOS 0.26.3、Windows 0.34.0、Unix 0.22.1 | 当前 Unix/Windows 通过可选 feature 接入；三者属于上述 AccessKit 兼容线 |
| Unix 无障碍传输 | accesskit_atspi_common 0.19.1、atspi 0.29.0、zbus 5.19.0 | 复用 AccessKit 适配；采用 async-io，无 Tokio |
| 路径 | 软件复用 tiny-skia，Vulkan 复用 zeno 0.3.3（swash 已依赖） | CPU 覆盖率光栅，不引入 Lyon 或三角细分 |
| PNG | png 0.18.1 | aegle-glyph 用于有界字体位图解码；纯几何 renderer 仅示例使用，不引入整个 image crate |
| SVG | resvg/usvg 0.48.1 | 仅 `aegle-image/svg` 与 `aegle-glyph/svg`：关闭 text、system-fonts 等默认 feature，glyph 侧启用 svgz；不处理 SVG 文字、外部文件或网络资源 |
| 其他图像格式 | zune-jpeg 0.5、image-webp 0.2、gif 0.14 | 仅 `aegle-image` 的 `jpeg`/`webp`/`gif` feature；都先检查尺寸与字节预算再分配；`image` crate 只作为测试编码器的开发依赖 |
| COLRv1 字形 | tiny-skia 0.12.0（已在 workspace） | `aegle-glyph/colrv1` 光栅 skrifa 的绘制回调，不增加新包 |
| 标记编译宏 | syn 2、quote 1、proc-macro2 1、proc-macro-crate 3.5 | 仅编译期；Rust 语法/生成与 facade 重命名识别复用现成库 |

设计版本来自 crates.io 发布记录及发布包 manifest 的核查。Taffy、tiny-skia、Parley/Fontique/HarfRust、Swash/Skrifa、字体句柄、缓存/PNG、Wayland 及独立 Vulkan 几何/文字依赖已进入 Cargo.lock 并在当前工具链构建验证；可选 Parley AccessKit 文本桥与 Unix adapter 已构建，并通过私有总线上的 AT-SPI 协议验证。Windows 编译与运行证据、GPU 原生呈现验证另见实现状态，不能将其等同于全部实机验收；Unix 当前能力与限制见[无障碍](accessibility.md)，具体验证见[实现状态](implementation.md)。tiny-skia 使用 BSD-3-Clause，不引入原生 Skia、图形驱动或窗口系统。特别保留 Parley/HarfRust/AccessKit 的兼容版本组，不把各库最新版随意组合。实现时检查完整传递依赖、许可、feature 合并与 MSRV；这是实现验收，不是尚待用户选择的架构问题。

`aegle-text` 默认启用 Parley std，并直接使用已有 icu_segmenter/compiled_data 提供 extended grapheme 删除边界。此直接依赖没有向锁定图新增包；PlainEditor 已有的选择、bidi、点命中和组合布局继续复用，不另带 Unicode 或编辑框架。`system-fonts`、`text-dictionary`、`text-a11y`、`scene` 独立选择。默认关闭 Parley complex-scripts；基础 CJK 显示与 UAX #14 换行保留，中日词典分词和部分东南亚文字上下文分段通过 text-dictionary 显式启用。未来默认 desktop 组合启用 parley/accesskit，当前独立文字模块默认关闭它；该 feature 提供文本语义桥，系统接入另选 `aegle-access/unix`。

`aegle-access` 默认依赖 AccessKit schema 与标准库通道，不带原生 adapter、异步运行时或窗口库。`unix` 仅在非 macOS 的 Unix 目标启用 AccessKit Unix 0.22.1，关闭其默认 features 并显式选择 `async-io`；上游不允许同时启用 `async-io` 与 `tokio`。该闭包含 AccessKit consumer 0.38.0、AT-SPI、Serde、zbus 及其异步组件，不能描述为只有一个小型运行依赖。consumer 使用的 hashbrown 0.16 与字形缓存的 0.17 同时存在于锁定图，不为统一版本私自修改上游依赖。上游 adapter/consumer 声明 MSRV 1.85，zbus 5.19 声明 1.87；完整 MSRV 仍须实际工具链验收。

Wayland 库本身没有 AccessKit 正常依赖；`example-accessibility` 只为其示例组合 dev-dependencies 的 Unix adapter 和 `text-a11y`。构建时纳入 async-io/zbus 不等于启动即创建线程，首次构造 UnixAdapter 才启动上游 worker；此后 worker 的生命周期、无界队列和语义缓存成本见[资源](resources.md)。Unix 原生文字选择可用，但当前上游缺少 AT-SPI EditableText 接口；不能用依赖版本声明代替完整控件支持。

`aegle-glyph` 用 Swash std/render 和与 Parley 相同的 Skrifa 0.44 解析字体；使用 png 的有界解码接口处理嵌入 PNG，避免无上限的中间解码分配。`aegle-render-software/text` 与 `aegle-render-vulkan/text` 显式引入此依赖闭包和 scene/text，均不依赖 Parley；默认纯几何构建没有字体栈或 PNG。Vulkan 图集索引复用已有 hashbrown 0.17，不增加另一套字体解析或栅格库。Cargo 测试/示例的 dev-dependencies 不代表库的发布依赖，仍需核查最终应用的 feature 合并。

SVG 默认以路径图标/构建期资产为主；可选运行时 resvg（`aegle-image/svg`、OpenType-SVG 字形的 `aegle-glyph/svg`）不处理 SVG text、外部 URL 或网络资源。需要 SVG 文字时在构建期转轮廓。构建期转换为位图需要指定尺寸/缩放档位，不能宣称与任意动态缩放完全等价。

`aegle-markup` 的语法很小，采用直接流式词法分析和递归下降，表达式按优先级爬升解析，不引入通用脚本或表达式框架；`aegle-loader` 以小型树解释已检查的表达式，同样没有第三方依赖。`aegle-macros` 则复用 syn/quote 和 proc-macro-crate 的清单解析，避免重复实现 Rust 参数语法和重命名依赖规则；这些包只参与构建，不随应用运行。`markup` 的目标依赖闭包与发布体积须区分编译主机侧的宏依赖。

来源：[Parley 发布清单](https://docs.rs/crate/parley/0.11.1/source/Cargo.toml)、[Swash 发布清单](https://docs.rs/crate/swash/0.2.10/source/Cargo.toml)、[AccessKit](https://github.com/AccessKit/accesskit)、[resvg 发布清单](https://docs.rs/crate/resvg/0.48.1/source/Cargo.toml.orig)。

Wayland 软件后端使用 SCTK 0.21.1/calloop 0.14，键盘使用同一版本的 xkbcommon 0.8 包；不引入 winit、Tokio、softbuffer 或 wgpu。SCTK 自带自动重复计时器不提供完整取消接口，因此平台持有自身 repeat token，在焦点/设备/窗口销毁和 backend drop 时移除。按键翻译仍复用 SCTK，重复状态额外持有一份 XKB keymap/state 用于按键可重复性与 modifier 更新；这是实际额外内存，不宣称零成本封装。后续上游若提供借用 keymap 与取消 timer 接口，可移除此重复状态。

构建需 libxkbcommon 的开发链接与 pkg-config 信息，发布需对应 runtime；SDK 不打包进程序。当前验证机器仅安装 runtime，验证时在临时目录创建了指向已有系统库的开发链接与 `.pc` 并经 `PKG_CONFIG_PATH` 引用，未修改系统或项目构建配置。当前软件路径未启用 wayland-client/system，因此不将 libwayland-client 误写成该示例的实际动态依赖；最终以构建的依赖树和二进制链接结果为准。

原生 window 复用 raw-window-handle 0.6.2；不引入 winit/ash-window。Win32 平台仅启用 windows crate 所需 API；UIA 在 aegle-access/windows 中额外编译 accesskit_windows 0.34.0，与现有 schema/consumer 兼容。Windows 的 target-specific 依赖不会将 Unix D-Bus、Wayland 或 Fontconfig 链入 Windows 程序。
