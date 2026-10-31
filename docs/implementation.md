# 实现状态与验证证据

状态：2026-11。记录事实实现的模块、跨平台验证证据和已知缺口。不包含中间调试日志与历史反复细节。

---

## 一、 当前已实现能力

| 模块 / Crate | 已实现功能与契约 | 依赖与技术基线 |
|---|---|---|
| **aegle-types** | 几何（Point/Rect/Size/Region）、紧凑 RGBA 颜色、阴影、光标、输入修饰键、系统偏好、拖放数据结构 | `no_std`，无外部依赖 |
| **aegle-core** | 槽位树（NodeId 带槽位与代数）、属性变更与三通道失效标记（布局/绘制/语义）、焦点遍历、事件路由快照 | 纯 Rust，无外部依赖 |
| **aegle-layout** | Taffy 0.14.0 树适配（Flexbox/Block，可选 Grid）；测量与首基线回调；contents 虚拟节点；自适应滚动视口约束（容器自动收缩） | Taffy 0.14.0 |
| **aegle-segment** | 词边界断词（中日韩泰分词），供按词移动与选词 | 运行时 `libloading` 系统 ICU（Linux/Windows）或 `CFStringTokenizer`（macOS），无内置词典数据 |
| **aegle-text** | 段落成形与断行、段落块多行编辑器（`Editor` 分块增量重排、有界撤销、预编辑保持提交值稳定、密码模式）、空闲文字内存预算回收 | 默认系统文字栈（Linux Pango/Fontconfig、Win DirectWrite、macOS CoreText）；Parley 为 opt-in 回退 |
| **aegle-glyph** | 8-bit 灰度字形覆盖率、LRU 限制内存条目与字节预算、COLRv1/SVG 颜色字形分离支持 | 默认系统字形（Linux FreeType、Win DirectWrite 纹理、macOS CoreText 位图）；Swash 为 opt-in 回退 |
| **aegle-scene** | 局部绘制记录（Scene）、矩形/圆角/边框/图像/路径/仿射变换/最多八层嵌套裁剪、图层与盒式模糊、字形记录 | `no_std` + `alloc` |
| **aegle-gpu** | GPU 图元与裁剪存储行（Recording，112B 图元 / 64B 裁剪）、图集货架装箱、WGSL 着色器 | 纯逻辑，无图形 API 依赖 |
| **aegle-render-cpu** | CPU 合成器（线性光混合、裁剪 mask 预算、离屏图层模糊、渐变与解析式阴影、字形/图像双线性采样） | 纯 Safe/Unsafe Rust 内存操作 |
| **aegle-render-tiny-skia** | 结合 `render-cpu`，由 tiny-skia 0.12 提供抗锯齿几何覆盖率（直角几何由合成器直接解析计算） | tiny-skia 0.12 |
| **aegle-render-cairo** | 结合 `render-cpu`，由系统 cairo 提供几何覆盖率（Linux 专用，运行时动态加载） | 运行时加载 `libcairo.so.2` |
| **aegle-render-vulkan** | Vulkan 1.1 硬件加速渲染：storage buffer 图元合并、按需灰度/彩色字形图集、最多八层裁剪、应用纹理登记、原生 swapchain 呈现 | ash 0.38, bytemuck 1.25 |
| **aegle-render-wgpu** | 全平台通用 GPU 渲染：WGSL 着色器、多层裁剪、字形/图像专用纹理、FIFO 呈现 | wgpu 30.0.1 |
| **aegle-platform** | 统一平台契约：`Platform` 与 `SurfacePlatform` trait、标准事件（`Event`）、窗口选项、IME 事务、剪贴板与拖放契约 | 纯契约抽象，无特定窗口库依赖 |
| **aegle-platform-sdl3** | Linux 默认后端：普通窗口生命周期、键盘/指针/触摸/滚轮、系统光标、文本输入与 IME、进程剪贴板、拖入、软件呈现与 GPU 句柄租约 | 运行时加载 `libSDL3.so.0` |
| **aegle-platform-wayland** | Linux 原生客户端：xdg-shell 顶层窗口、wlr layer-shell 表面、text-input-v3（周边文字与选区）、双向拖放、SHM 呈现与 GPU 租约 | wayland-client 0.31, SCTK 0.21, libxkbcommon |
| **aegle-platform-win32** | Windows 原生后端：Win32 窗口、DPI 适配、TSF 文本存储输入法、OLE 拖放、GDI 软件呈现与 GPU HWND 租约 | windows 0.62 |
| **aegle-platform-macos** | macOS 原生后端：AppKit 窗口、NSTextInputClient、NSPasteboard、NSDragging、CGImage 软件呈现与 CAMetalLayer 租约 | objc2 0.6 系列 |
| **aegle-portal** | Linux 桌面 portal（`org.freedesktop.portal.Settings`）外观偏好读取与 `SettingChanged` 监听 | aegle-dbus |
| **aegle-dbus** | Linux 会话总线轻量客户端：认证、Hello、消息编解码、阻塞/非阻塞读写 | Unix 域套接字，无外部依赖 |
| **aegle-desktop** | 桌面级服务：文件选择对话框、系统通知、托盘图标与菜单、全局快捷键（Linux 经 portal/D-Bus，Windows 经 Win32 API） | Linux: aegle-dbus; Windows: Win32 API |
| **aegle-access** | 无障碍语义树导出与平台适配器（Linux AT-SPI、Windows UI Automation、macOS NSAccessibility） | AccessKit 0.24 |
| **aegle-theme** | 无分配主题配置、类 Metro 中性默认样式、`ThemeOverride` 局部覆盖、类型化 Token（颜色、尺寸、字体、阴影） | aegle-types |
| **aegle-motion** | 补间动画（Tween/Transition）、关键帧动画、Bézier 与阻尼弹簧曲线、属性动画（位移/缩放/旋转/透明度/阴影/颜色/渐变） | 纯数学计算，无定时器依赖 |
| **aegle-controls** | 可复用交互行为（点击、悬停、范围、开关、单选、文本编辑） | 行为逻辑，无默认皮肤依赖 |
| **aegle-widgets** | 完整默认控件库（Label、Button、TextField、CheckBox、Switch、Slider、Progress、ScrollView、ListView、Table、Dialog、Drawer 等） | 基于 aegle-ui |
| **aegle-markup** | 简洁花括号标记语言有界解析器、类型与作用域检查、诊断跨度报告 | 纯 Rust 解析器 |
| **aegle-macros** | `ui!` 声明宏：将标记直接编译为零开销 Rust 控件树调用代码 | 过程宏 |
| **aegle-loader** | 运行时动态标记执行引擎：响应式数据绑定、条件与循环渲染、组件插槽、热重载 | 解释执行引擎 |
| **aegle-ui** | 无窗口保留式 UI 核心引擎：树生命周期、Taffy 布局调度、输入捕获冒泡路由、焦点管理、动画调度、IME 与语义同步 | 组合核心 |
| **aegle-app** | 原生应用宿主：单 UI 线程事件循环、平台驱动、单一渲染器生命周期管理、后台代理消息传递（`UiProxy`） | 宿主调度层 |
| **aegle** | 顶层便捷 Facade：统一默认导出、一键 `App::run_ui` | 门面 Crate |

---

## 二、 跨平台实测与验证证据

### 1. 自动化测试套件
* **工作区测试**：`cargo test --workspace` 共计 **207 项通过、0 项失败、19 项 ignored**（需独立显示环境/硬件的集成场景）。
* **代码规范**：`cargo fmt --all --check` 与 `cargo clippy --workspace --all-targets`（`-D warnings`）零告警。
* **Feature 组合矩阵**：CI 矩阵包含极简构建、markup 独立、渲染器 4 选 1（tiny-skia / cairo / vulkan / wgpu）、Linux 窗口后端 2 选 1（sdl3 / wayland）及无障碍组合，均通过编译与单测。

### 2. Linux 平台（真实桌面与无头会话）
* **SDL3 默认后端验证**：
  * **真实桌面环境**：运行于 Linux x86_64 KDE Plasma Wayland 会话。
  * **像素对齐与正确性**：`hello` 与 `showcase` 窗口截图与原生 Wayland 客户端对比，ImageMagick `compare -metric AE` 为 **0 差异像素**。
  * **二进制体积**：默认 release 构建的 `hello` 仅为 **1,538,024 字节（~1.47 MiB）**，比原生 Wayland 静态依赖构建（2.50 MiB）减少 38%，`ldd` 仅依赖基础 C 运行时（libc, libm, libgcc）。
  * **空闲开销**：静止 5 秒期间实测 CPU 消耗为 0 ticks，无空闲轮询。
  * **GPU 呈现**：在真实会话中，基于 SDL3 租约的 Vulkan 与 wgpu 硬件加速呈现均正常提交，绘图层、背景模糊与 swapchain 表现正确。
* **原生 Wayland 后端验证**：
  * **无头隔离环境**：在专用无头 Sway 会话（Pixman 软件渲染）中完成。
  * **核心场景通过**：覆盖生命周期配置、多窗口管理、虚拟指针拖放（坐标精度与源释放正确）、`text-input-v3` 中日韩输入法事务、分式缩放及 portal 偏好更新。
* **系统文字栈收益**：Linux 默认切换至系统文字栈（Pango + FreeType + Fontconfig，动态加载）后，去除了内置 ICU 词典数据，排版与分词直接委托系统库。

### 3. Windows 平台
* **验证环境**：Windows 11 Pro 64-bit，实机桌面环境。
* **已验证功能**：
  * Win32 窗口生命周期、消息循环分发、DPI 动态感知。
  * GDI 32 位 DIB 软件呈现，Vulkan 原生 HWND 交换链呈现。
  * TSF（Text Services Framework）原生文本存储接入：周边文本读取、微软拼音输入法组合交互。
  * OLE 拖放（`CF_UNICODETEXT` 与 `CF_HDROP`）及剪贴板交互。
  * Windows UI Automation 无障碍客户端节点属性查询。

### 4. macOS 平台
* **状态**：已完成代码编写并在 Linux 宿主上完成交叉编译检查（`cargo check`、`cargo clippy`、`cargo test --no-run`）。
* **已实现边界**：AppKit 窗口、`NSTextInputClient`、CoreText + CoreGraphics 字形渲染、`CAMetalLayer` 租约。
* **未实机运行**：尚未在 Apple 硬件实机上执行渲染、输入与 VoiceOver 验收。

---

## 三、 当前已知限制与未决缺口

1. **输入与交互**：
   * SDL3 后端受限于上游 API，不支持拖动发起（只能作为放置目标）、无触摸板松手惯性（缺少滚动结束事件），输入法仅支持候选框定位与提交，不支持传递周边文字。
   * 真实触摸屏多点手势与压感尚未在物理硬件上验收。
2. **多平台实机**：
   * macOS 平台目前仅为交叉编译通过，需要实机环境补齐渲染、高分屏缩放与输入验收。
   * Windows ARM64 尚未完成实机编译与驱动兼容性核验。
3. **桌面服务**：
   * `aegle-desktop` 目前在 macOS 上各服务方法直接返回 `Unavailable`（需要主线程消息循环深度适配）。
