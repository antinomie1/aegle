# 模块、依赖与构建组合

状态：v0.1 模块设计。types、core、layout、scene、text、glyph、controls、access、theme、motion、app、markup、macros、便捷入口、软件/Vulkan renderer 与 Wayland/Win32/macOS 平台已建立；实际覆盖范围见下文。其余模块及已建立模块的完整职责仍是设计目标，验证记录见[实现状态](implementation.md)。

## 拆分尺度

按可独立使用的能力和真实后端差异拆分。每个模块需有自己的 Interface：类型、所有权、调用顺序、错误和资源成本。模块不通过全局服务定位器寻找其他模块，由调用者显式传入能力。线程级共享状态只有两处，都是按名称登记的表：`aegle-ui` 的 token 注册表与 `aegle-loader` 的宿主动作共享表。

| crate | 责任及独立用途 | Aegle 内部依赖 |
| --- | --- | --- |
| aegle-types | 几何、颜色、阴影值、光标形状及 `Preferences`、`TouchPhase` 等两个平台共用的小词汇；`std` feature 增加拖放数据 `DragData` 与 `file://` URI 转换；无平台依赖 | 无 |
| aegle-core | 槽位树、句柄、属性变更、事件路由、焦点 | types |
| aegle-layout | Taffy 低层树适配、Flex/Block 与可选 Grid；经过校验的布局值（`Length`、`Insets`、`Align`、`Justify`、`Direction`、`LayoutDirection`、`Wrap`，`Length::Calc` 为“百分比 + 像素”，grid 另有 `Track`、`Placement`、`Flow`、`TemplateItem`/`Repeat`/`template`、`areas`、`GridLine`/`GridLines`）；叶节点测量回调与可选的首基线回调（`compute_with_baselines`）；透明的 contents 节点；不依赖应用 | types、core |
| aegle-segment | 向系统分词器要词边界：Linux/Windows 动态加载系统 ICU（`libicui18n`/`icu.dll`），macOS 用 `CFStringTokenizer`；不链接任何词典数据，也没有回退，找不到即报告不可用 | 无内部依赖 |
| aegle-text | 字体、保留段落布局、纯文本编辑/组合状态与有界撤销；排版交给平台系统栈（Linux 为 Pango、Windows 为 DirectWrite、macOS 为 CoreText），按词移动与选词取 `aegle-segment` 的边界 | types、segment；scene 按 feature 接入；glyph 在 Windows/macOS 上注册字形后端；`text-parley` opt-in 为不再维护的回退 |
| aegle-glyph | 字形光栅化（Linux 为系统 FreeType、Windows 为 DirectWrite alpha 纹理、macOS 为 CoreGraphics alpha 位图，都由系统抗锯齿）、有界 CPU 缓存、共用字形身份/变换策略；系统字体句柄经注册的 `SystemRasterizer` 解析 | image、scene 按 feature 接入；`swash` opt-in 为不再维护的回退 |
| aegle-scene | 二维绘制命令、裁剪、共享图像/路径资源及可选字形记录；字形身份是后端无关的 `FontSource`（共享字节或平台字体句柄） | types |
| aegle-gpu | GPU 后端共用、与图形 API 无关的部分：图元/裁剪记录、场景遍历、货架装箱、图像与路径 mask 放置、WGSL 着色器 | types、scene；vector feature 增加 zeno |
| aegle-render-vulkan | 几何、可选字形图集、裁剪、离屏读回与可选原生 swapchain | types、scene；glyph 按 text feature 接入 |
| aegle-render-cpu | 两个 CPU 后端共用的合成器：线性光混合、裁剪 mask、图层/模糊、渐变/阴影、图像与字形采样 | types、scene；glyph 按 text feature 接入 |
| aegle-render-tiny-skia | 无 GPU 栅格绘制（CPU 合成器 + tiny-skia 覆盖率），与 GPU 共用 scene/文字资源 | render-cpu、tiny-skia、scene |
| aegle-render-cairo | Linux 上同一 CPU 合成器加系统 cairo 覆盖率（运行时加载） | render-cpu、scene、libloading |
| aegle-render-wgpu | 可选的最小跨平台 GPU 后端：几何、字形图集、裁剪、离屏读回与原生 surface 呈现 | types、scene；glyph 按 text feature 接入 |
| aegle-dbus | Linux 会话总线：认证、Hello、消息编解码（无 Unix fd 与 16 位整数），任意线程发送，阻塞或非阻塞读取 | 无 |
| aegle-desktop | 桌面服务：文件选择对话框、通知、托盘及其菜单、全局快捷键；Linux 走会话总线（portal FileChooser/GlobalShortcuts、`org.freedesktop.Notifications`、StatusNotifierItem + dbusmenu），Windows 走 `IFileDialog`、`Shell_NotifyIconW`、`RegisterHotKey`；结果经一个线程安全回调送出，不依赖窗口或 UI | Linux 为 dbus |
| aegle-platform | 平台契约：`Platform`/`SurfacePlatform` trait 与共用的事件、指针、IME、窗口选项与几何、seat、wake 和错误；不含实现 | types |
| aegle-platform-wayland | 以契约提供 Wayland 窗口、可选 layer-shell 表面、事件、IME、剪贴板与拖放、输出与平台偏好 | types、platform、dbus |
| aegle-platform-win32 | 以契约提供 Win32 窗口、TSF 文本存储输入法、DPI、OLE 拖放、GDI 软件与 GPU 句柄、外观偏好 | types、platform |
| aegle-platform-macos | 以契约提供 AppKit 顶部窗口、`NSTextInputClient` 输入法、`NSView` 上的拖放、`CGImage` 软件呈现与 GPU 视图租约、外观偏好；`NSApplication` 由平台自己驱动 | types、platform、objc2 |
| aegle-access | 原生回调排队/唤醒与可选 AccessKit adapter；宿主派生语义更新 | 无内部依赖；schema 为 AccessKit，`platform` 按目标启用 AT-SPI/UIA/NSAccessibility 之一 |
| aegle-theme | 无分配的 Theme、控件类型 `ControlKind`（默认皮肤与可接受样式组）、视觉状态、Appearance/Style 和纯函数 Skin；局部主题继承、按字段的 `ThemeOverride`；类型化 `Token<T>` 与内置 token（注册表与绑定在 aegle-ui） | types |
| aegle-motion | 补间、过渡、关键帧动画与 Bézier/弹簧曲线；可无窗口独立推进 | types |
| aegle-controls | 可复用控件行为、语义动作与基础组合；无默认皮肤 | types；text feature 接 text，树与路由由宿主提供 |
| aegle-widgets | 默认控件库，包含全部默认控件：Label、Button、TextField（单/多行）、CheckBox、Switch、Radio、Slider、Progress、NumberField、Separator、Splitter、Tabs、Tooltip、ImageView、Canvas、ScrollView、ListView、Table、Popup、Dropdown、Menu、MenuBar，以及它们的类型与中性皮肤（`kinds`）和纯函数绘制（`paint`）；通过 `Control` trait 与 `Hooks` 接入 aegle-ui，创建入口是 `Widgets` trait | ui、controls、text、scene、theme、core、layout、types；access 按 feature |
| aegle-image | 有界图像解码：PNG（始终可用，字体位图用 `decode_into`）与可选 JPEG、WebP、GIF 首帧、静态 SVG 栅格化，以及可选的渐变/阴影图像（`effects`）；不依赖任何 UI | scene；解码器按 feature |
| aegle-markup | 有界解析、跨度、节点属性表与元素规格 `ElementSpec`（不含具体元素）、多文件导入与 state/表达式/事件/块/组件的类型检查 | 无 |
| aegle-macros | ui! 文件编译与有类型 View 生成，仅编译期运行；动态文档生成已检查程序的构造代码 | markup |
| aegle-loader | 动态标记执行引擎：绑定、事件、if/for、组件实例、运行时加载与显式重载；元素契约 `Element` 与内置元素 | ui、widgets、markup、macros（`element!`）；app 按 feature |
| aegle-ui | 无窗口保留式 UI 引擎：节点树、布局、输入路由与焦点、滚动与滚动条、光标、触摸/惯性、主题、过渡、IME 与语义导出；控件通过开放的 `Control` trait（经 `InputCx`/`MeasureCx`/`PaintCx`/`SemanticsCx` 上下文）和 `Hooks` 接入；引擎自带的只有无外观容器（`Plain`、grid 的 `Stack`）与滚动条，其余控件都在控件库中 | types、core、layout、scene、text、controls、theme、motion；access 按 feature |
| aegle-app | 原生应用宿主：App/Window、事件循环、平台与 renderer 选择、系统偏好、后台代理（UiProxy）；每窗口一个 `Ui`；只按平台契约编写，一处选定目标平台；`desktop-services` 时 `App::desktop` 把桌面服务事件经代理交回 UI 线程 | ui、types、platform、scene、text；平台实现、renderer、access、desktop 按 feature；widgets 仅用于测试 |
| aegle | 应用便捷入口与重导出（app、ui、widgets、image），不提供另一套实现 | app、ui、widgets、image；其他按 feature 重导出 |

平台文字后端的文件归属与依赖方向：

- `aegle-text/src/backend_directwrite/`：`sys.rs`（DirectWrite 工厂单例、CSS→DWRITE 映射与宽字符串）、`system.rs`（字体集合、text format 与 generic family）、`fonts.rs`（从注册字节建字体集合，含临时文件）、`layout.rs`（`IDWriteTextLayout` 与度量）、`metrics.rs`（命中/光标/范围/Uniscribe 分段/诊断）、`runs.rs`（`Draw` 回调把 face 变成 `SystemFont` 并登记 `FaceSource`）、`paint.rs`（run→scene）。依赖 windows crate 的 DirectWrite/GDI/Globalization/Com 模块与 `aegle-glyph`。
- `aegle-text/src/backend_coretext/`：同样的分工，`sys.rs`（CF 字符串、属性字典、`CTParagraphStyle`）、`system.rs`（字体与 generic family）、`fonts.rs`（`CTFontManagerRegisterGraphicsFont`）、`layout.rs`（`CTFramesetter`/`CTFrame` 与度量）、`metrics.rs`（`CTLineGetStringIndexForPosition` 等）、`runs.rs`（`CTRun` → `SystemFont`）、`paint.rs`。依赖 objc2-core-foundation/core-text/core-graphics 与 `aegle-glyph`。
- `aegle-glyph/src/directwrite.rs` 与 `cg.rs`：系统直接出覆盖率，前者用 DirectWrite glyph-run analysis 的灰度 alpha 纹理，后者用 CoreText 画进 alpha-only `CGBitmapContext`；两者都把彩色/位图字形报告为 `UnsupportedGlyph`。`system.rs` 拥有 `SystemRasterizer` 注册表、`FaceSource` 注册表与 `shared_face`。两个后端各自拥有原生对象缓存与释放点，只在 `cfg(windows)`/`cfg(target_os = "macos")` 且未启用 `swash` 时编译。
- `aegle-text/src/offsets.rs`：三个后端共用的 UTF-16↔UTF-8 换算、段落方向与行高换算，公开给写自己平台适配器的宿主。

层次：`aegle-ui` 是引擎，`aegle-widgets` 是默认控件库，`aegle-app` 是原生宿主；三者单向依赖（widgets→ui，app→ui），ui 不依赖 widgets 或任何平台/renderer。自带控件的库可只依赖 `aegle-ui` 实现 `Control`；自带窗口/绘制宿主的程序可只依赖 `aegle-ui` 与 `aegle-widgets`。`Hooks` 让虚拟列表、弹出层、单选组等需要跨节点协作的行为留在控件库内，引擎只提供调用点；各库的节点外数据存于 `State::ext`。

表中的简称指同名前缀 crate。文字无障碍为 `aegle-text/text-a11y`：系统栈下由 Aegle 自己导出 run 与选择位置，`text-parley` 回退下转发 Parley 的 AccessKit 支持。基础文字模块不强制启用它。平台 adapters 按 target 编译，不能把三平台实现都塞进一个程序。

当前 `aegle-scene` 使用 no_std + alloc，默认只依赖 types；`text` 仅增加轻量的 `linebender_resource_handle`，通过共享 `FontData` 及独立 run 旁表保存字形记录，不引入 Parley 或 Swash。一个 run 的字体身份是 `FontSource::Bytes(FontData)` 或 `FontSource::System(SystemFont { backend, id })`：后者只是后端注册表键，scene 从不保存原生指针，`backend` 由 `SystemFont::DWRITE`/`CORETEXT` 区分。渲染器与图集把它当不透明身份。纯几何命令不携带完整字体/run 数据。图像与路径以 `Arc` 共享句柄保存在 scene 旁表，路径光栅化留在各 renderer 内部，不另设路径 crate。

`aegle-render-cpu` 是 CPU 后端的共享合成器：它借用调用方像素缓冲，不依赖 core、Taffy 或窗口，也不依赖任何光栅库。线性光 SourceOver、裁剪 mask 预算、离屏图层与背景模糊、渐变/阴影、图像与字形采样都在这里；抗锯齿几何覆盖率经 `Rasterizer` trait 交给具体后端。`text` 显式增加 aegle-glyph，其 PNG 解码器用于字体内嵌位图。

`aegle-render-tiny-skia` 是该合成器加 tiny-skia 0.12（仅 std/simd）的覆盖率光栅器；默认纯几何构建没有字体栈和 PNG 运行依赖。`aegle-render-cairo` 是同一合成器加发行版 `libcairo.so.2` 的覆盖率光栅器（仅 Linux，运行时用 libloading 加载，不链接、不读头文件），缺失时返回明确的 `BackendUnavailable`。两个 CPU 后端不依赖 aegle-text，其他 shaping 宿主也可提供 scene 字形记录；它们对正立直角几何的输出逐字节相同，差异只在光栅器的边缘采样。

`aegle-render-vulkan` 通过 ash 0.38 和 bytemuck 1.25 消费相同 Scene，支持几何、最多八层裁剪及显式 RGBA8 读回。默认纯几何不带字体；可选 `text` 用 glyph、scene/text 和 hashbrown 管理按需 R8/RGBA8_SRGB 图集，复用下述字形身份与光栅策略，不依赖 Parley。没有 core、平台窗口库或软件 renderer 的正常依赖；可选 window 仅增加 raw-window-handle；Naga 30 仅在构建期生成 SPIR-V。swapchain 与 App 显式后端选择已接入，详见 [Vulkan 契约](vulkan.md)。

`aegle-gpu` 不接触任何图形 API：`Walker` 把 Scene 逐命令转成 112 B 图元行与 64 B 裁剪行（`Recording`，带字节上限），几何、渐变与阴影命令就地记录（渐变色标两个一行附在裁剪行中），字形、图像、路径交回后端处理；`aegle-gpu` 因此是两个 GPU 后端行为一致的单一来源，也是它们的 WGSL 的来源（Vulkan 的 build.rs 以它为构建依赖）。

`aegle-render-wgpu` 消费相同 Scene，经 wgpu 30 在 Vulkan、Metal、Direct3D 12 上绘制几何、最多八层裁剪与 mask/color 字形，不依赖 Parley、Vulkan 后端或 ash；可选 `window` 只增加 raw-window-handle。图像与 CPU 光栅的路径 mask 共用同一图集，放不下的得到专用纹理。与 Vulkan 后端共用的部分见 [wgpu 契约](wgpu.md)。

`aegle-text` 的排版与字形遍历默认来自平台系统栈，`layout.rs` 的 `PlatformSystem`/`Layout` 是唯一按 target 分派的位置：Linux 走 `backend_pango`（Pango + PangoFT2 + Fontconfig，全部运行时加载），Windows 走 `backend_directwrite`（DirectWrite 排版与字形覆盖率 + Uniscribe 分段），macOS 走 `backend_coretext`（CoreText 排版 + CoreGraphics 字形位图）。三者都把字体身份记录成 `FontSource::System`，由 `aegle-glyph` 的 `SystemRasterizer` 注册表按 backend 分派光栅化；未注册的 backend 返回 `GlyphError::UnsupportedGlyph`，不猜、不回退到字节路径。`offsets.rs` 提供三者共用的 UTF-16↔UTF-8 边界换算、段落方向判定与行高换算。段落与 Editor 共用 TextSystem 的字形系统，Editor 按硬换行分段排版（编辑只重新成形触及的段落），并补充稳定提交值、可取消组合和有界 delta 历史；不引入另一个编辑框架或转发 crate。scene 桥接共用字形绘制，额外记录选择、预编辑和光标；平台 IME、剪贴板及系统语义由平台/应用层连接。

`aegle-glyph` 独立接受共享字体句柄，字形覆盖率由平台直接给出：Linux 是系统 FreeType（`libfreetype.so.6`，运行时加载），Windows 是 DirectWrite glyph-run analysis（`DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE` 的 1x1 alpha 纹理），macOS 是 `CTFontDrawGlyphs` 画进 alpha-only `CGBitmapContext`，三者输出同格式的 8-bit mask。合成粗体在 Windows 是 DirectWrite 的粗体模拟、在 macOS 是填充加 `size / 32` 宽的描边，合成斜体都是变换矩阵。`opentype_name.rs` 是有界的 `name`/`OS/2` 解析器，给 CoreText 后端注册应用字体时提供 family/weight/italic。索引与 LRU 用 hashbrown 与 lru-slab，不自建字体解析器或通用缓存框架。缓存不保留字体字节；段落、编辑器及 scene 的字体句柄维持各自资源寿命。`swash`/`skrifa` 路径只在显式启用该 feature 时编译。

该模块公开共用的借用/拥有字形缓存身份；可选 `scene` feature 依赖 `aegle-scene/text`，提供 renderer 共用的字体缩放、整像素基线与水平四相位、灰度对比曲线和 bitmap 仿射策略。默认字形缓存仍不依赖 scene。

`aegle-platform` 只有契约：两个 trait 和它们的值类型，宿主据此只写一遍事件循环与输入翻译，平台在各自 crate 内把原生值翻译成契约（见[平台契约](platform-rendering.md#平台契约)）。它有两个实际实现和一个宿主（aegle-app），不是转发层；契约之外的平台诊断（如 `buffer_bytes`）留在各平台类型上。

`aegle-platform-wayland` 复用 SCTK、wayland-client 与 calloop 管理同一连接、多个普通窗口和原生输入。平台只依赖 types、platform 与 dbus（读设置 portal）；TextSystem、Editor、Scene 和 renderer 在可执行示例中组合，不成为平台的发布依赖。软件呈现直接借出有界 SHM 像素；text-input-v3 以带 seat 身份的事务传递给宿主。gpu feature 提供带生命周期的原生 surface 租约，启用 libwayland system backend。layer-shell 表面与 xdg 窗口共用窗口表、输入、IME 和呈现路径，由 `WindowOptions::layer` 选择，不另设 crate 或 feature；剪贴板按 seat 使用 data device 与非阻塞管道。托盘、通知、文件对话框和全局快捷键在 `aegle-desktop`，它们与窗口无关，经会话总线完成。

`aegle-desktop` 每个 `Desktop` 一个后台线程：Linux 上它读会话总线（portal 请求按规范预测的 Response 路径登记，避免在 Response 早于返回时丢失；托盘对象由同一线程应答宿主的属性与菜单调用），Windows 上它拥有一个隐藏消息窗口并运行对话框、托盘和热键（命令经通道与 `PostMessageW` 送达）。应用经回调收到 `Event`；`aegle-app` 的 `App::desktop` 只是把该回调接到 `UiProxy`。服务不存在时报告 `Event::Unavailable`，不静默丢弃。

`aegle-controls` 默认提供无分配的 Button/Toggle/Slider、共享 Range 状态及借用 Input/Outcome；`text` 增加复用 Editor 的 TextField。它不依赖 core、布局、主题、renderer 或窗口。宿主在自己的树中保存行为状态，负责命中、焦点和 capture；键盘、指针及语义激活经过同一默认行为。可选 aegle-access/unix 已在示例接通 AT-SPI 的查询、焦点、按钮及文字选择，完整系统无障碍仍未完成。

`aegle-access` 的 Mailbox/Handlers 将原生线程上的请求交给宿主自己的 UI 线程，不引入另一棵应用树。UnixAdapter 复用 AccessKit 的系统协议与语义缓存，收到初次请求时完整导出，其后按脏标记更新。text-a11y 文本桥补充 run 身份/范围校验；平台、控件和文字依赖仍可分开选择。

`aegle-theme` 是 no_std、无分配的小型值类型，只依赖 types。`Theme` 提供浅色、深色和高对比配色以及正文、间距、圆角和控件高度；Appearance/Style 按控件状态解析独立于行为的外观，Skin 是纯函数指针；自定义值在宿主接受时验证。app 用稀疏表保存本地外观和字号，不把完整 Style 放进每个节点；局部主题由子树节点共享一份 `Rc<Theme>`。token 注册表、全局/子树 token 覆盖与属性绑定（Style、字号、字体、padding/gap、过渡时长）由 aegle-ui 保存（稀疏表，按索引）；系统偏好由平台 crate 报告，原生 App 据此选择主题；可选外观/位移过渡由 app 连接独立 motion 模块。

`aegle-ui` 不创建平台依赖。`Ui::with_fonts` 接受可共享的 TextSystem，拥有一棵控件树，提供容器（row/column、透明的 contents 分组，`grid` feature 下另有 grid/stack）与完整的 flex/grid 布局 setter、滚动、主题、过渡、输入、IME 与语义接口，具体控件由 `aegle-widgets` 通过 `Widgets` trait 创建。平台宿主可分别调用输入、刷新、scene 遍历、IME 和可选语义接口。`aegle-app` 的 `wayland` / `windows` / `macos` 按目标增加原生 App，`tiny-skia` / `cairo` / `vulkan` / `wgpu` 互斥，恰好选一个 renderer 编入（同时启用两个是编译错误），四者是同一个 `Renderer` trait 的实现，宿主不区分“软件”与“GPU”；各窗口独立拥有 Ui，共享平台和字体；CPU 后端的所有窗口共用一个合成器实例，GPU 窗口共享第一个窗口创建的设备，图集按窗口独立。

ScrollView 的偏移、嵌套滚轮传递和焦点显露由 ui 协调现有树与布局，不新增滚动 crate。renderer 仍不依赖控件树：scene 遍历给宿主传递平移和外部矩形裁剪，由宿主应用；绘制、输入、IME 和可选语义共享 ui 派生的滚动几何。

`aegle` 重导出 app、ui、widgets 与 image，不复制实现。当前默认 `desktop` 组合是 **目标平台原生窗口（Linux Wayland / Windows Win32 / macOS AppKit）+ 软件绘制 + 系统字体 + 编译型标记（含动态标记引擎） + 外观过渡**，不是下表的目标 GPU 组合；按词移动与选词所需的词典来自运行环境的系统分词器（`aegle-segment`），不链接数据、没有回退，系统无障碍适配需显式启用。系统无障碍适配默认不启用：Linux AT-SPI 引入 zbus 与异步运行时，由单个 `accessibility` 选择，它按编译目标接入 AT-SPI、UIA 或 NSAccessibility，并隐含 `native`；`aegle-ui` / `aegle-widgets` 的 `accessibility` 只启用语义树导出，`aegle-app` 在此基础上接系统 adapter；`system-fonts` 可关闭并改用显式字体。当前 facade 的 `default-features = false` 仍保留 Ui 的文字等基本依赖；需要更小的单一能力时直接选择底层 crate。Vulkan 可选且无需编译 CPU renderer；macOS 用 `tiny-skia` 或 `wgpu`（Vulkan 显式选择时返回错误）；动态标记与运行时加载都由 `markup` 中的 loader 执行。

`aegle-markup` 是无第三方依赖的有界解析器、schema 与类型检查器；不依赖 app 或任何平台，可供外部工具独立检查，I/O 由调用方的读取函数提供。`aegle-macros` 复用它，并用 syn/quote 处理 Rust 宏参数与代码生成，避免自建 Rust 语法处理；facade 的 `ui!` 与 loader 的 `element!` 以 `macro_rules!` 把 `$crate` 交给 proc macro，依赖别名由编译器解析。facade 的可选 `markup` 增加编译期宏与 `aegle-loader`：静态文档生成直接创建控件的代码，不链接引擎；动态文档生成构造已检查程序的代码并由引擎执行，发布程序不带解析器。引擎以 state 单元和效果（effect）记录绑定依赖，绑定与块随控件通过 `Node::keep_alive` 释放；运行时加载额外链接解析器。

`aegle-motion` 提供无时钟所有权的 Tween/Transition 与关键帧 Animation（延迟、循环、往返），支持 f32、Point、Color 与二次、Bézier、弹簧曲线；只有多关键帧动画分配一次共享帧数组；仅依赖 types 的可选 `color-math`。该 feature 需要 std，将软件合成与动画共用的 sRGB 转换表放在一个 OnceLock 中，types 默认仍为 no_std。app 的可选 `motion` 维护节点外观目标/呈现值并驱动失效；无需 motion 时不会编译其映射表或调度代码。

`aegle-theme` 与 `aegle-motion` 公开函数不多，仍各自成 crate：两者是只依赖 types、不接触控件树的值库。theme 为 no_std 且不分配，widgets 的纯函数皮肤、第三方控件库或 renderer 可只依赖它取得同一套配色、`Style` 与 `Appearance`，不必编译需要 std、Taffy 与 Parley 的引擎；motion 的 Tween/Animation 由调用者给时间，可在不用引擎的宿主中单独采样，引擎也只在 `motion` feature 下链接它。并入 aegle-ui 会让只要这些值的使用者编译整个引擎；再细分则没有对应的消费者。aegle-ui 重导出两者的类型，经引擎使用时无需另加依赖；独立使用的方式见[不经 facade 使用控件库](developer/standalone.md)。

没有独立的“每个控件 crate”或“每个颜色类型 crate”。当一个模块的多种选择只影响内部小函数时使用 feature，不为包装一个转发函数增加新的包。

## 依赖方向

```mermaid
flowchart TD
  Facade[aegle 便捷入口] --> App[aegle-app 原生宿主]
  Facade --> Widgets[aegle-widgets 默认控件]
  Facade --> Image[aegle-image]
  App --> Ui[aegle-ui 引擎]
  App --> Contract[platform 契约]
  Platform[目标平台 + 可选 shell] --> Contract
  App --> Platform
  App --> Render[scene + 一个 renderer]
  Widgets --> Ui
  Widgets --> Controls[controls]
  Ui --> Core[core]
  Ui --> Layout[layout / Taffy]
  Ui --> Text[text + glyph]
  Ui --> Visual[theme / motion / scene]
  Render --> Cpu[render-cpu 共享合成]
  Ui --> Access[可选 access]
  Facade --> Loader
  Loader[可选 loader] --> Ui
  Loader --> Widgets
  Loader --> Markup[markup]
  Loader --> Macros
  Macros[构建期 macros] --> Markup
```

这是一张依赖图，不是每次更新必须经过的多层调用链。markup 只定义语言、节点属性和元素规格 `ElementSpec`，不认识任何具体元素；loader 定义元素契约 `Element` 并用 macros 的 `element!` 声明内置元素，第三方控件库以同样方式声明自己的元素。core 不依赖 ui、app、widgets、loader、GPU 或操作系统库；renderer 不依赖控件或标记语言。

## 默认组合与可裁剪组合

| 组合 | 包含 | 不自动包含 |
| --- | --- | --- |
| `aegle` 默认 `desktop` | 目标平台窗口（`native`）、软件绘制、系统字体、`markup`（编译宏与 loader，含运行时加载）、`motion`（过渡、关键帧与弹簧）、`effects`、`colrv1`、`desktop-services` | 系统无障碍适配（`accessibility`）、Vulkan、wgpu、JPEG/WebP/GIF/SVG、Grid |
| `default-features = false` | 不自动选择窗口/renderer；使用者显式加所需功能或直接用独立模块 | 便捷默认组合 |
| 按词移动与选词 | 系统分词器（`aegle-segment` 运行时查找 libicu / `icu.dll` / `CFStringTokenizer`），不链接数据 | 内置词典数据（已移除）与任何近似替代（找不到时分词操作关闭） |
| `effects`（桌面默认）/ `colrv1`（桌面默认）/ `jpeg` / `webp` / `gif` / `svg` | 渐变与阴影图像、COLRv1 字形、各图像格式、静态 SVG（图像与 OpenType-SVG 字形） | 彼此不暗中全部开启；`colrv1`/`svg` 只增加按字形的颜色表示，**不替换平台字形光栅器**（那是 `swash` 的作用）；未启用时颜色字形返回 `UnsupportedGlyph` 而不画成灰度轮廓 |
| `grid` | Taffy Grid 算法、`grid`/`stack` 容器与标记 `Grid`/`Stack`（release 约 244 KiB） | 默认 desktop 不包含 |

默认组合不启用系统无障碍 adapter：`accessibility` 显式选择，它同时给出可检查的语义树与目标平台的 adapter；不能把未接 adapter 的构建宣传为完整无障碍构建，省掉平台 adapter 也不删除控件的基本语义定义。减少动态效果偏好由 `motion` 跟随系统。

Cargo feature 在依赖图中会统一：不能承诺同一程序里的某个控件使用有字典的 Parley，另一个控件因此完全不承担其静态数据成本。发布检查必须查看实际闭包和最终文件，而不是只查看 facade 清单。

## 扩展契约

第三方可以注册控件、主题 token、动作和新的 renderer/platform adapter，使用稳定 Rust 接口；不提供动态二进制插件 ABI。组件库依赖 core/controls/theme/scene 等实际需要的模块，不必依赖整个 aegle。只改外观的组件复用行为，只有新增交互时才实现新的行为。

首版只承诺文档所列 capability。扩展能力不存在时返回结构化错误或使用明确的组件降级方案，不依赖反射查找“可能存在”的隐藏服务。

`aegle/native` 是目标平台的便捷组合（内部再选 wayland/windows/macos 中与目标相符的一个），不编译另一个 OS 的平台依赖。最小命令式软件 hello 选择 native,software,system-fonts；仅 Vulkan 则选择 native,vulkan,system-fonts。两个 renderer 同时编译时默认软件，显式 AppOptions.renderer 切换；没有 renderer feature 时构造返回错误。标记与 motion 继续单独选择。系统 adapter 由 `accessibility` 单独选择，它不影响 `aegle-ui`/`aegle-widgets` 的语义定义。
