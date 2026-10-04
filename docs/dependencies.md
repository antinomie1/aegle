# 依赖与版本基线

状态：v0.1 选定设计基线。自身 crate 使用 Rust 2024，目标 MSRV 1.88；这是依赖发布声明支持的目标，尚未构建验证完整依赖闭包。依赖可以使用其他 edition。

| 职责 | 基线 | 说明 |
| --- | --- | --- |
| 布局 | Taffy 0.14.0 | Flex/Block 默认，Grid 可选；低层树适配 |
| 软件覆盖率栅格化 | tiny-skia 0.12.0 | 仅 std/simd，关闭默认 PNG；只用几何覆盖率，线性颜色合成由小型自有实现完成 |
| Vulkan | ash 0.38.0+1.3.281 | 自行封装资源、同步与 unsafe；不采用 wgpu/vulkano |
| Wayland | wayland-client 0.31.15、SCTK 0.21.1 | system backend；客户端 IME 由平台层补齐 |
| Windows | windows 0.62.2 | 只启用所需 Win32/COM/TSF/UIA 能力 |
| macOS | objc2 0.6.4、objc2-metal 0.3.2 | AppKit/Metal 系统绑定；不采用已弃用 metal crate 或 MoltenVK |
| 文本 | Parley/Fontique 0.11.1 | 基础排版、字体回退及纯文本编辑 |
| shaping / 字体解析 | HarfRust 0.12.0、Skrifa 0.44.0 | 按 Parley 兼容版本线，避免追最新产生双份依赖 |
| 字形 | Swash 0.2.10 | 按需光栅化；有界缓存由 aegle-glyph 管理 |
| 共享字体资源 | linebender_resource_handle 0.1.1 | scene/text 仅借助该轻量句柄共享字体字节，不引入 shaping |
| 字形缓存索引 | hashbrown 0.17.1、lru-slab 0.1.3 | 哈希索引和 LRU 槽位复用现成实现，命中不分配 |
| 语义 | AccessKit 0.24.1 | 与 Parley 可选 text-a11y 使用同一 schema |
| 系统语义 adapters | macOS 0.26.3、Windows 0.34.0、Unix 0.22.1 | 三者均属于上述 AccessKit 兼容线，只编译目标平台 |
| 路径 | lyon_tessellation 1.0.22 | 可选，不依赖完整通用图形框架 |
| PNG | png 0.18.1 | aegle-glyph 用于有界字体位图解码；纯几何 renderer 仅示例使用，不引入整个 image crate |
| SVG | resvg/usvg 0.48.1 | 构建期优先；运行时可选，关闭 text/system-fonts 等默认 feature |

设计版本来自 crates.io 发布记录及发布包 manifest 的核查。Taffy、tiny-skia、Parley/Fontique/HarfRust、Swash/Skrifa、字体句柄及缓存/PNG 依赖已进入 Cargo.lock 并在当前工具链构建验证；可选 Parley AccessKit 接口也已通过全 features 构建。平台 adapters、GPU 与其余待建模块仍未据此宣称可用，具体验证见[实现状态](implementation.md)。tiny-skia 使用 BSD-3-Clause，不引入原生 Skia、图形驱动或窗口系统。特别保留 Parley/HarfRust/AccessKit 的兼容版本组，不把各库最新版随意组合。实现时检查完整传递依赖、许可、feature 合并与 MSRV；这是实现验收，不是尚待用户选择的架构问题。

`aegle-text` 默认只启用 Parley std；`system-fonts`、`text-dictionary`、`text-a11y`、`scene` 独立选择。默认关闭 Parley complex-scripts；基础 CJK 显示与 UAX #14 换行保留，中日词典分词和部分东南亚文字上下文分段通过 text-dictionary 显式启用。未来默认 desktop 组合启用 parley/accesskit，当前独立文字模块默认关闭它，启用也不等于已有系统无障碍 adapter。

`aegle-glyph` 用 Swash std/render 和与 Parley 相同的 Skrifa 0.44 解析字体；使用 png 的有界解码接口处理嵌入 PNG，避免无上限的中间解码分配。`aegle-render-software/text` 才引入此依赖闭包；默认纯几何构建的正常依赖树没有 Parley、Swash 或 PNG。Cargo 测试/示例的 dev-dependencies 不代表库的发布依赖，仍需核查最终应用的 feature 合并。

SVG 默认以路径图标/构建期资产为主；可选运行时 resvg 不处理 SVG text、外部 URL 或网络资源。需要 SVG 文字时在构建期转轮廓。构建期转换为位图需要指定尺寸/缩放档位，不能宣称与任意动态缩放完全等价。

来源：[Parley 发布清单](https://docs.rs/crate/parley/0.11.1/source/Cargo.toml)、[Swash 发布清单](https://docs.rs/crate/swash/0.2.10/source/Cargo.toml)、[AccessKit](https://github.com/AccessKit/accesskit)、[resvg 发布清单](https://docs.rs/crate/resvg/0.48.1/source/Cargo.toml.orig)。其他原始调查来源保留在[选型记录](selection-candidates.md)。
