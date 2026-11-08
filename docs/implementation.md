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
| **aegle-gpu** | GPU 图元与裁剪存储行（Recording，112B 图元 / 64B 裁剪）、图集货架装箱、WGSL 着色器；`shaders` 下拼装并校验自定义片元程序（naga），错误定位到程序行列 | 纯逻辑，无图形 API 依赖；naga 30（可选） |
| **aegle-render-cpu** | CPU 合成器（线性光混合、裁剪 mask 预算、离屏图层模糊、渐变与解析式阴影、字形/图像双线性采样） | 纯 Safe/Unsafe Rust 内存操作 |
| **aegle-render-tiny-skia** | 结合 `render-cpu`，由 tiny-skia 0.12 提供抗锯齿几何覆盖率（直角几何由合成器直接解析计算） | tiny-skia 0.12 |
| **aegle-render-cairo** | 结合 `render-cpu`，由系统 cairo 提供几何覆盖率（Linux 专用，运行时动态加载） | 运行时加载 `libcairo.so.2` |
| **aegle-render-vulkan** | Vulkan 1.1 硬件加速渲染：storage buffer 图元合并、按需灰度/彩色字形图集、最多八层裁剪、应用纹理登记、原生 swapchain 呈现；`shaders` 下首次使用时把自定义片元程序编成 SPIR-V 管线（lavapipe 上与同色填充逐像素一致） | ash 0.38, bytemuck 1.25；naga 30（可选） |
| **aegle-render-wgpu** | 全平台通用 GPU 渲染：WGSL 着色器、多层裁剪、字形/图像专用纹理、FIFO 呈现；`shaders` 下首次使用时编译自定义片元程序 | wgpu 30.0.1 |
| **aegle-platform** | 统一平台契约：`Platform` 与 `SurfacePlatform` trait、标准事件（`Event`）、窗口选项与角色（层、弹出层）、输出枚举、IME 事务、剪贴板与拖放契约 | 纯契约抽象，无特定窗口库依赖 |
| **aegle-platform-sdl3** | Linux 默认后端：普通窗口与弹出菜单/提示窗口、显示器枚举、键盘/指针/触摸/滚轮、系统光标、文本输入与 IME、进程剪贴板、拖入、软件呈现与 GPU 句柄租约 | 运行时加载 `libSDL3.so.0` |
| **aegle-platform-wayland** | Linux 原生客户端：xdg-shell 顶层窗口、指定输出且可原地改放置的 wlr layer-shell 表面、xdg_popup（父为顶层、弹出层或 layer）、输出枚举、text-input-v3（周边文字与选区）、双向拖放、SHM 呈现与 GPU 租约 | wayland-client 0.31, SCTK 0.21, libxkbcommon |
| **aegle-platform-win32** | Windows 原生后端：Win32 窗口、DPI 适配、TSF 文本存储输入法、OLE 拖放、GDI 软件呈现与 GPU HWND 租约 | windows 0.62 |
| **aegle-platform-macos** | macOS 原生后端：AppKit 窗口、子窗口弹出层（翻转与滑动）、屏幕输出枚举、NSTextInputClient、NSPasteboard、NSDragging、CGImage 软件呈现与 CAMetalLayer 租约 | objc2 0.6 系列 |
| **aegle-portal** | Linux 桌面 portal（`org.freedesktop.portal.Settings`）外观偏好读取与 `SettingChanged` 监听 | aegle-dbus |
| **aegle-dbus** | Linux 会话总线轻量客户端：认证、Hello、消息编解码、阻塞/非阻塞读写 | Unix 域套接字，无外部依赖 |
| **aegle-desktop** | 桌面级服务：文件选择对话框、系统通知、托盘图标与菜单、全局快捷键（Linux 经 portal/D-Bus，Windows 经 Win32 API，macOS 经 AppKit 面板、`NSStatusItem`、Carbon 热键与应用包内的 `UNUserNotificationCenter`） | Linux: aegle-dbus; Windows: Win32 API; macOS: objc2 0.6 系列、Carbon |
| **aegle-access** | 无障碍语义树导出与平台适配器（Linux AT-SPI、Windows UI Automation、macOS NSAccessibility） | AccessKit 0.24 |
| **aegle-theme** | 无分配主题配置、类 Metro 中性默认样式、`ThemeOverride` 局部覆盖、类型化 Token（颜色、尺寸、字体、阴影） | aegle-types |
| **aegle-motion** | 补间动画（Tween/Transition）、关键帧动画、Bézier 与阻尼弹簧曲线、属性动画（位移/缩放/旋转/透明度/阴影/颜色/渐变） | 纯数学计算，无定时器依赖 |
| **aegle-controls** | 可复用交互行为（点击、悬停、范围、开关、单选、文本编辑） | 行为逻辑，无默认皮肤依赖 |
| **aegle-widgets** | 完整默认控件库（Label、Button、TextField、CheckBox、Switch、Slider、Progress、ScrollView、ListView、Table、Dialog、Drawer 等） | 基于 aegle-ui |
| **aegle-markup** | 简洁花括号标记语言有界解析器、类型与作用域检查、诊断跨度报告 | 纯 Rust 解析器 |
| **aegle-macros** | `ui!` 声明宏：将标记直接编译为零开销 Rust 控件树调用代码 | 过程宏 |
| **aegle-loader** | 运行时动态标记执行引擎：响应式数据绑定、条件与循环渲染、组件插槽、热重载；`layer` 窗口文档直接打开为面板并原地重载；`State::on_change` 状态变化处理器（随重载转移）；弹簧/贝塞尔过渡、缩放/旋转/透明度关键帧动画、`Shape`（SVG 路径）与 `Arc` 元素 | 解释执行引擎 |
| **aegle-ui** | 无窗口保留式 UI 核心引擎：树生命周期、Taffy 布局调度、输入捕获冒泡路由、焦点管理、动画调度、IME 与语义同步 | 组合核心 |
| **aegle-app** | 原生应用宿主：单 UI 线程事件循环、平台驱动、单一渲染器生命周期管理、后台代理消息传递（`UiProxy`）、输出列表与变化回调、`Window::set_layer`、原生弹出层（随父窗口关闭） | 宿主调度层 |
| **aegle** | 顶层便捷 Facade：统一默认导出、一键 `App::run_ui` | 门面 Crate |

---

## 二、 跨平台实测与验证证据

### 1. 自动化测试套件
* **工作区测试**：`cargo test --workspace` 共计 **213 项通过、0 项失败、22 项 ignored**（需独立显示环境/硬件的集成场景）。
* **代码规范**：`cargo fmt --all --check` 与 `cargo clippy --workspace --all-targets`（`-D warnings`）零告警。
* **Feature 组合矩阵**：CI 矩阵包含极简构建、markup 独立、渲染器 4 选 1（tiny-skia / cairo / vulkan / wgpu）、Linux 窗口后端 2 选 1（sdl3 / wayland）及无障碍组合，均通过编译与单测。
* **发布前检查（2026-11-08，macOS 主机）**：`cargo package -p aegle-types` 打包成功（其余 crate 要等依赖先上 crates.io 才能解析）；`aegle`、`aegle-ui`、`aegle-types`、`aegle-core`、`aegle-text`、`aegle-app` 在 crates.io 上未被占用。每个可在 macOS 编译的 crate 单独 `cargo test -p <crate> --no-run` 均编译通过（此前 `aegle-ui` 的 `shader` 测试在未开 `motion` 时编译失败）。交叉 `cargo check --all-targets`：`x86_64-pc-windows-msvc` 下 Win32、桌面服务、无障碍与 facade（tiny-skia、vulkan、wgpu）通过；`x86_64-unknown-linux-gnu` 下除需要 libxkbcommon pkg-config 的 `aegle-platform-wayland` 外的工作区，以及 facade 的四种渲染器（sdl3）通过。macOS 上 `cargo test --workspace`（排除 Linux 专用 crate 与 facade/宿主）此前因 `aegle-portal` 在非 Linux 编译失败而从未运行；现在 194 项通过、12 项 ignored、1 项失败（`aegle-render-tiny-skia --test text` 的彩色字形部分，见第三节）；两项控件测试此前把快捷键写死为 Ctrl，现按平台用 Cmd/Ctrl。docs.rs 目标配置已写入各平台 crate，尚未在 docs.rs 上实际构建。

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
* **桌面 Shell 表面（层、弹出层、输出）**：在无头 `kwin_wayland --virtual`（1280×800 虚拟输出，`WAYLAND_DISPLAY=aegle-test`）中验证：
  * `aegle-platform-wayland` native 测试：首次调度前即列出输出（名称 `Virtual-0`）；在该输出上创建顶部面板并拉伸至输出宽度，`set_layer` 原地移到左下角后宽度为 200 dp；layer 上的 xdg_popup 配置为 120×80 并呈现；换输出、对普通窗口 `set_layer`、层与弹出层同设、先移除父窗口均返回 `InvalidRole`。
  * `aegle-platform-sdl3` 会话测试（SDL Wayland 驱动）：显示器枚举、弹出层配置为请求尺寸、父窗口先移除被拒；dummy 驱动下弹出层返回 `Unsupported`。
  * `aegle-app` native 套件（wayland 后端 5 项、sdl3 后端新增 1 项）：弹出层随父窗口关闭、控件句柄失效；`App::output(name)`；`on_outputs` 回调；面板 `set_layer` 后控件存活。
  * `aegle` window 测试：同一 `layer` 文档经 `Program::open` 与 `ui!` 打开为整宽面板；重载为左下角 300 dp 时窗口句柄、state 值与 `on_change` 处理器保持；去掉 `layer` 或改指输出的重载在构建前被拒；`open_on` 为每个输出各开一个面板。
  * 未覆盖：弹出层抓取（需真实输入 serial）、输出热插拔事件、多输出场景，以及真实 KDE/Sway 桌面上的外观。
* **Pango 行高**：`LineHeight` 改用 Pango 1.50 行高属性后，单行段落的高度等于所设行高（此前 `line_spacing` 只作用于行间，12 px 字号设 30 px 行高的单行仍量得 18 px）。`aegle-text/tests/paragraph.rs` 覆盖绝对、字号倍数与字体度量倍数三种形式的单行与两行高度及基线位置；本机 Pango 1.58.2 验证。
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
* **状态**：已在 macOS 15（x86_64，Metal）实机运行：`gpu_texture` 示例与 Axo 终端经 wgpu 正常显示、Retina 2× 缩放、空闲 CPU 0%；`opacity` 下的透明窗口（`WgpuOptions::transparent`）合成正确。
* **已实现边界**：AppKit 窗口、`NSTextInputClient`、CoreText + CoreGraphics 字形渲染（`aegle-text/tests/coretext.rs` 覆盖通用字族、行基线、CJK 回退与光栅化）、`CAMetalLayer` 租约与透明窗口。
* **CoreText 编辑语义**：`cargo test -p aegle-text --features system-fonts,scene` 全部通过（已加入 macOS CI）。行尾不含硬换行符；末尾硬换行后与空文本各有一条空行承载光标（CoreText 不生成，按前一行或字体度量补出）；单行编辑器不软换行；`TextSystem::new()` 只匹配已注册字体；CoreText 以 `LastResort` 代替缺字，其字形计入缺字数。
* **输出与弹出层**：`cargo test -p aegle-platform-macos --test native -- --ignored` 在实机 GUI 会话通过（该目标 `harness = false`，在主线程运行；此前的 libtest 形式不在主线程，从未能运行）：屏幕列为输出（主屏位于原点、ID 互异）；抓取弹出层取得键盘焦点；弹出层上的被动弹出层；空锚点、层与弹出层同设被拒；在被动子弹出层内按下不撤销，在父窗口按下只对抓取弹出层发一次 `Close` 且父窗口仍收到该按下；父窗口与有子弹出层的弹出层先移除被拒。`tests/placement.rs` 覆盖四个方向、翻转、滑动与超出屏幕的弹出层。
* **桌面服务**：`cargo test -p aegle-desktop --test macos -- --ignored` 在实机通过（`harness = false`，主线程，由 `aegle-platform-macos` 的循环送达事件；连续 3 次通过）：主线程外创建被拒；保存面板按请求显示标题、文件名与扩展名过滤，在面板就绪后经 HID 输入 Return 返回所选文件夹下的 `notes.txt`；打开面板允许多选、无过滤，Escape 返回 `None`；状态栏按钮带图标与更新后的提示，单击报 `TrayActivated`；在菜单栏位置经 HID 右键弹出菜单，键盘选第一项报 `TrayMenu { item: 7 }`（菜单在事件投递中嵌套跟踪，输入由另一线程发出）；Ctrl+Alt+Shift+F19 经 HID 真实按下（含 Fn，与 Mac 键盘一致）报 `Shortcut`，F24 报 `Unavailable`，重绑与清空后无事件；无应用包时 `notify` 报含 “bundle” 的 `Unavailable`。通知另在临时应用包（`services` 示例、`CFBundleIdentifier` 为 `org.aegle.DesktopExample`、ad-hoc 签名、放在 `~/Applications` 并经 LaunchServices 登记；放在 `/private/tmp` 下时 `usernoted` 找不到应用记录而直接拒绝授权）中实测：首次通知弹出系统授权提示，允许后横幅显示标题与正文；经通知中心的 AX 操作按 “Open” 报 `NotificationAction { action: "open" }` 与 `NotificationClosed`，点正文报 `"default"` 与关闭，“关闭” 只报 `NotificationClosed`。请求标识带进程前缀：同一标识的新请求会静默替换通知中心里已送达的旧通知，且上次运行留下的通知不应归到本次同号请求。进程外面板在显示后需片刻才接收按键，测试等待约 2 秒。
* **软件呈现与自动释放池**：tiny-skia 下 `showcase` 经 CGImage 呈现正确（Retina 2×、中日韩文字、全部控件，截屏核对）。平台循环不经 `-[NSApplication run]`，此前从不清空自动释放池，绘制保留的每帧图像永不释放：`showcase`（含一直转动的不定进度条）物理内存每秒增长约 300 MB，十余秒到 9.4 GB。现在每轮事件与每次呈现各用一个自动释放池，物理内存稳定在约 113 MB（`vmmap` 物理占用）。release 构建下无动画的 `hello` 空闲 CPU 0%、43 MB。软件帧不再每帧把整幅缓冲拷入新 `CFData`：两块缓冲轮换，CGImage 经数据提供者原地读取，宿主只在 CoreGraphics 归还的那块上补拷自其上次显示以来变化的区域再绘制；`showcase` 动画时 CPU 由约 30% 降到约 24%（同机 wgpu 约 23%），物理内存由约 113 MB 降到约 96 MB，`sample` 中不再出现整帧 `memmove`。
* **NSAccessibility**：`controls`（`accessibility` 特性）由外部进程经 AX API（VoiceOver 所用的同一接口）检查：首次查询得到占位、发布后得到完整树（静态文本、`AXTextArea`、各按钮及标题）；`AXPress` 走与指针相同的路径（“Clear text”清空编辑器）；多行编辑器经 `AXNumberOfCharacters`、`AXStringForRange`、`AXSelectedTextRange`、`AXInsertionPointLineNumber` 读出含中日韩文字的全文、光标与行号。此前文字 run 的 `character_lengths` 按每字符 1 字节填写，而 AccessKit 以 UTF-8 字节切分，读取含多字节字符的编辑器时 panic 使应用崩溃（各平台同一缺陷）；现按字节长度填写，`aegle-text/tests/accessibility.rs` 检查其与 run 文字一致。未用 VoiceOver 语音实测。
* **输入法**：`controls` 中以系统简体拼音（`com.apple.inputmethod.SCIM.ITABC`）经 HID 键入 `nihao`：预编辑 `ni hao` 带下划线显示在光标处，候选窗紧贴光标下方（截屏核对），空格上屏 `你好`；经 AX 读回的字符数随预编辑与提交变化（23 → 29 → 25）。日文、韩文输入法未测。
* **拖放**：`aegle-platform-macos` 的 native 场景用真实指针（HID）在两个窗口间拖动：源窗口按下后 `start_drag`，目标收到 `Drag`、宿主回答接受后收到 `Drop`，文字（含中文）与文件各一次，落点位于目标窗口内（会话自行采样指针，略滞后于合成轨迹）。AppKit 在事件投递中嵌套跟踪拖动，宿主循环在拖动期间停在 `dispatch` 内；连续 3 次通过。拖出到其他应用、其他应用拖入未测。
* **未验收**：应用失去激活时撤销抓取弹出层未自动化测试。

---

## 三、 当前已知限制与未决缺口

1. **输入与交互**：
   * SDL3 后端受限于上游 API，不支持拖动发起（只能作为放置目标）、无触摸板松手惯性（缺少滚动结束事件），输入法仅支持候选框定位与提交，不支持传递周边文字。
   * 真实触摸屏多点手势与压感尚未在物理硬件上验收。
2. **多平台实机**：
   * macOS 平台已实机验证软件与 wgpu 呈现、CoreText 编辑语义、拼音输入法、窗口间拖放与 AX 无障碍接口；VoiceOver 语音、日韩输入法与跨应用拖放尚未实测。
   * Windows ARM64 尚未完成实机编译与驱动兼容性核验。
3. **桌面 Shell 表面**：
   * Win32 未实现弹出层（创建返回 `Unsupported`）与输出枚举；`set_layer` 仅 Wayland 后端可用（其他平台无层）。
   * 文本编辑器不处理辅助技术的 `SetValue` 与 `ReplaceSelectedText`（各平台相同）：读屏用户的键盘与输入法输入不受影响，但经辅助接口直接写值（如 macOS `AXValue` 赋值）被忽略。
   * macOS 弹出层只在创建时定位，父窗口移动时整体跟随，但不随锚点或屏幕变化重新翻转。
   * SDL3 弹出层不按锚定矩形翻转，只由 SDL 滑回屏幕内；显示器名称取决于 SDL 驱动，不保证为连接器名。
   * 控件库的 `Popup`/`Menu`/`Dropdown` 仍是窗口内浮层，尚未改用原生弹出层；标记语言没有弹出层元素，原生弹出层只能由 Rust 的 `Window::popup` 打开。
   * 输出热插拔时按输出增删面板需由宿主在 `App::on_outputs` 中完成；按 layer-shell 协议，输出断开时合成器关闭其上的层，宿主收到 `Close` 后移除该窗口（此路径尚未实测）。
4. **macOS 彩色字形**：CoreGraphics 光栅器按字体拒绝带彩色字形特征（sbix、COLR、SVG）的字体中的全部字形，返回 `UnsupportedGlyph`，渲染器因此整帧失败。实测：`TextSystem::system()` 排版 `"A 😀"`（回退到 Apple Color Emoji）后 tiny-skia 绘制返回 `Err(Glyph(UnsupportedGlyph))`；`aegle-render-tiny-skia/tests/text.rs` 的彩色位图字形部分在 macOS 上同样失败（Linux 由 FreeType 渲染）。即 macOS 上含 emoji 的文字目前无法绘制。
5. **桌面服务**：
   * `aegle-desktop` 的 macOS 通知只在应用包内可用（裸二进制报 `Unavailable`），通知验证为手动步骤、未进 CI；全局快捷键按 ANSI 键位匹配，非美式布局下字母键按位置而非字符。
