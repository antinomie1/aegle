# 模块、依赖与构建组合

Aegle 采用多 Crate 拆分设计，按可独立使用的能力和后端差异分工。每个模块拥有清晰的类型、所有权、错误和调用边界。底层模块不反向依赖上层抽象。

---

## 一、 工作区 Crate 职责表

| Crate | 职责与独立用途 | 内部依赖 |
|---|---|---|
| **aegle-types** | 几何坐标、RGBA 颜色、阴影、光标、输入修饰键、系统偏好与拖放数据结构（`no_std`，`std` 下增拖放） | 无 |
| **aegle-core** | 槽位树（32位索引+32位代数 ID）、属性变更、结构树维护、三通道失效打脏（布局/绘制/语义）、焦点遍历 | types |
| **aegle-layout** | Taffy 0.14.0 适配（Flexbox/Block，可选 Grid）；首基线与尺寸测量回调；虚拟 contents 节点；自适应滚动视口约束 | types, core |
| **aegle-segment** | 词边界断词（供按词移动与双击选词）；运行时加载系统 ICU 或 `CFStringTokenizer`，无内置词典数据 | 无 |
| **aegle-text** | 字体管理、保留段落排版、多行分块增量编辑器（`Editor`）、有界历史撤销、空闲段落文字内存预算回收 | types, segment, (scene) |
| **aegle-glyph** | 8-bit 灰度字形光栅覆盖率、LRU 限制内存与条目预算；可选 COLRv1/SVG 颜色字形分离支持 | (scene, image) |
| **aegle-scene** | 局部绘制记录（Scene）；矩形/圆角/边框/图像/路径/仿射变换/最多八层嵌套裁剪；图层与盒式模糊 | types |
| **aegle-gpu** | GPU 图元与裁剪行记录（Recording，112B 图元 / 64B 裁剪）、图集货架装箱算法、通用 WGSL 着色器 | types, scene |
| **aegle-render-cpu** | CPU 共享合成器：线性光混合、裁剪 mask 预算与复用、离屏图层模糊、渐变与解析式阴影、双线性采样 | types, scene, (text) |
| **aegle-render-tiny-skia** | CPU 渲染器：`render-cpu` 合成器 + tiny-skia 0.12 几何覆盖率（直角几何由合成器直接解析） | render-cpu, tiny-skia, scene |
| **aegle-render-cairo** | CPU 渲染器：`render-cpu` 合成器 + 发行版 `libcairo.so.2` 覆盖率（Linux 专享，运行时动态加载） | render-cpu, scene |
| **aegle-render-vulkan** | Vulkan 1.1 硬件加速渲染器：storage buffer 图元合并、按需灰度/彩色字形图集、最多八层裁剪、应用纹理登记 | types, scene, (text, window) |
| **aegle-render-wgpu** | 全平台通用 GPU 渲染器：WGSL 着色器、专用字形/图像纹理、多层裁剪、surface 呈现 | types, scene, (text, window) |
| **aegle-platform** | 跨平台统一契约：`Platform` 与 `SurfacePlatform` trait、统一事件模型（`Event`）、窗口配置、IME 事务、剪贴板与拖放 | types |
| **aegle-platform-sdl3** | Linux 默认窗口后端：窗口生命周期、输入分发、系统光标、文本输入、剪贴板、拖入、软件呈现与 GPU 租约 | types, platform, portal |
| **aegle-platform-wayland** | Linux 原生窗口客户端：xdg-shell、wlr layer-shell、text-input-v3（周边文字与选区）、双向拖放、SHM 与 GPU 租约 | types, platform, portal |
| **aegle-platform-win32** | Windows 原生窗口后端：Win32 窗口、DPI 动态感知、TSF 文本存储输入法、OLE 拖放、GDI 呈现与 HWND 租约 | types, platform |
| **aegle-platform-macos** | macOS 原生窗口后端：AppKit 窗口、NSTextInputClient、NSPasteboard、NSDragging、CGImage 呈现与 Metal 租约 | types, platform |
| **aegle-portal** | Linux 桌面 portal（`org.freedesktop.portal.Settings`）外观偏好读取与 `SettingChanged` 监听 | types, dbus |
| **aegle-dbus** | Linux 会话总线轻量级客户端：认证、Hello、消息编解码，支持跨线程发送与阻塞/非阻塞读取 | 无 |
| **aegle-desktop** | 桌面级服务：文件选择对话框、系统通知、托盘图标与菜单、全局快捷键（Linux 经 portal/D-Bus，Windows 经 Win32 API） | Linux: dbus; Windows: Win32 |
| **aegle-access** | 无障碍语义树导出与平台适配（Linux AT-SPI、Windows UI Automation、macOS NSAccessibility） | AccessKit 0.24 |
| **aegle-theme** | 无分配主题配置、类 Metro 中性默认样式、`ThemeOverride` 局部覆盖、类型化 Token（颜色、尺寸、字体、阴影） | types |
| **aegle-motion** | 补间动画、关键帧动画、Bézier/阻尼弹簧曲线、属性动画（位移/缩放/旋转/透明度/阴影/渐变），无自身定时器 | types |
| **aegle-controls** | 可复用交互行为（点击、悬停、范围、开关、单选、文本编辑），无默认皮肤 | types |
| **aegle-widgets** | 完整默认控件库（Label、Button、TextField、CheckBox、Switch、Slider、ScrollView、ListView、Table、Dialog 等） | ui, controls, theme, text |
| **aegle-markup** | 花括号标记语言解析器、类型与作用域检查、诊断跨度报告 | 纯 Rust 解析器 |
| **aegle-macros** | `ui!` 声明宏：将标记编译为零开销 Rust 控件树调用代码 | 过程宏 |
| **aegle-loader** | 运行时动态标记执行引擎：响应式数据绑定、条件与循环渲染、组件插槽、热重载 | ui, widgets, markup, macros |
| **aegle-ui** | 无窗口保留式 UI 核心引擎：树生命周期、Taffy 布局调度、输入捕获冒泡路由、焦点管理、动画调度、IME 与语义同步 | core, layout, scene, text, theme, motion |
| **aegle-app** | 原生应用宿主：单 UI 线程事件循环、平台驱动、单一渲染器生命周期管理、后台代理消息传递（`UiProxy`） | ui, platform, scene, text |
| **aegle** | 顶层便捷 Facade：统一默认导出、一键 `App::run_ui` | app, ui, widgets, image |

---

## 二、 依赖基线与系统运行时库

项目基于 **Rust 2024**，目标 MSRV 1.88。Aegle 的原则是：**操作系统已有的能力委托给系统库运行时加载，不把庞大的第三方引擎和词典数据静态打包进二进制**。

| 职责领域 | 依赖库与版本 | 链接与加载方式 | 说明 |
|---|---|---|---|
| **界面布局** | Taffy 0.14.0 | 静态编译 | 默认 Flexbox/Block，可选 Grid |
| **CPU 几何覆盖率** | tiny-skia 0.12.0 | 静态编译 | 默认 CPU 渲染器覆盖率光栅（关闭默认 PNG） |
| **Linux 系统覆盖率** | `libcairo.so.2` | 运行时 `libloading` | 可选 CPU 后端，缺失时报错，不静态链接 |
| **Linux 默认窗口** | `libSDL3.so.0` | 运行时 `libloading` | 默认窗口、输入、IME、剪贴板；自动选择 Wayland 或 X11 |
| **Linux 原生 Wayland** | wayland-client 0.31, SCTK 0.21, libxkbcommon | 静态 Rust 绑定 + libxkbcommon 动态链接 | 可选原生 Wayland 客户端（layer-shell、拖动源、周边文字） |
| **Windows 窗口与输入** | windows 0.62.2 | 动态链接系统 DLL | Win32、GDI、TSF、UIA |
| **macOS 窗口与输入** | objc2 0.6 系列 | 动态链接系统框架 | AppKit、CoreText、CoreGraphics、CAMetalLayer |
| **Linux 文字栈** | Pango, FreeType, Fontconfig | 运行时 `libloading` | 排版、断行、回退、字体配置与字形覆盖率全走系统库 |
| **系统分词** | 系统 ICU (`libicui18n` / `icu.dll`) 或 `CFStringTokenizer` | 运行时 `libloading` | 中日韩泰按词移动与双击选词，无内置词典数据 |
| **硬件 GPU 渲染** | ash 0.38 (Vulkan 1.1) / wgpu 30.0.1 | 动态加载驱动 | Vulkan 通过 Naga 在构建期生成 SPIR-V；wgpu 运行时编译 WGSL |
| **无障碍语义** | AccessKit 0.24.1 | 平台按需选择 | Linux AT-SPI (zbus)、Windows UIA、macOS NSAccessibility |

---

## 三、 Feature 矩阵与构建组合

### 1. 核心构建规则
* **单一渲染器互斥**：一个构建**恰好编入一个渲染器**（`tiny-skia`、`cairo`、`vulkan`、`wgpu`）。同时启用两个会触发编译期错误。
* **Linux 窗口后端互斥**：Linux 上 `sdl3`（默认）与 `wayland` 互斥。同时启用会触发编译期错误。
* **默认组合**：`aegle` 默认 feature 为 `["desktop", "sdl3", "tiny-skia"]`。
  * `desktop` 包含：`native`、`system-fonts`、`markup`、`motion`、`effects`、`desktop-services`。
  * 换渲染器：`--no-default-features --features desktop,<renderer>`。
  * 换 Linux 窗口后端：`--no-default-features --features desktop,wayland,tiny-skia`。

### 2. 常用功能 Feature 说明
* `native`：目标平台的原生窗口宿主支持（Windows/macOS 自动匹配自身，Linux 搭配 `sdl3` 或 `wayland`）。
* `accessibility`：启用无障碍语义树，并挂载目标系统的无障碍适配器（隐含 `native`）。
* `system-fonts`：启用操作系统字体发现（关闭时可通过代码手动注册字体字节）。
* `markup`：启用编译期 `ui!` 宏与动态标记解释引擎 `aegle-loader`。
* `motion`：启用补间过渡、弹簧动画与手势惯性滚动。
* `grid`：启用 Taffy Grid 布局容器（`grid`、`stack`），增加约 244 KiB 体积。
* `jpeg` / `webp` / `gif` / `svg`：启用特定格式的图像解码支持。

### 3. 典型运行命令

```bash
# 1. 默认运行（SDL3 + tiny-skia + 系统字体）
cargo run -p aegle --example hello

# 2. 控件库全功能展示（默认 SDL3 + tiny-skia）
cargo run -p aegle --example showcase

# 3. 使用系统 cairo 渲染器（Linux 专用，需运行时有 libcairo.so.2）
cargo run -p aegle --no-default-features --features desktop,cairo --example showcase

# 4. 改用原生 Wayland 客户端后端（SCTK + layer-shell 支持）
cargo run -p aegle --no-default-features --features desktop,wayland,tiny-skia --example showcase

# 5. 使用硬件加速渲染器（Vulkan / wgpu）
cargo run -p aegle --no-default-features --features desktop,vulkan --example showcase
cargo run -p aegle --no-default-features --features desktop,wgpu --example showcase

# 6. 开启系统无障碍（Linux AT-SPI）
cargo run -p aegle --features accessibility --example controls
```
