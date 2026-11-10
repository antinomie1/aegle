# 平台与渲染后端

Aegle 的窗口系统接入（平台）与图形绘制（渲染器）分属不同职责。平台负责原生连接、窗口生命周期、输入分发和事件等待；渲染器负责消费不可变 `Scene` 绘制记录并输出像素。

---

## 一、 平台支持矩阵

| 平台 | 窗口后端 | 默认状态 | 绘制方式 | 特色能力与限制 |
|---|---|---|---|---|
| **Linux** | **`aegle-platform-sdl3`** | **默认** | 软件 / Vulkan / wgpu | 运行时加载 `libSDL3.so.0`；自动选 Wayland/X11；无 layer-shell；弹出菜单/提示（只滑动不翻转）；列出显示器；只收拖放 |
| **Linux** | **`aegle-platform-wayland`** | 可选 (`wayland`) | 软件 / Vulkan / wgpu | 原生 Wayland 客户端（SCTK）；layer-shell 面板（指定输出、原地改放置）、xdg_popup、输出枚举、双向拖放、text-input-v3 周边文字 |
| **Windows** | **`aegle-platform-win32`** | 原生默认 | 软件 (GDI) / Vulkan / wgpu | Win32 消息循环、DPI 感知、TSF 文本存储输入法、OLE 拖放；弹出层与输出枚举返回 `Unsupported` |
| **macOS** | **`aegle-platform-macos`** | 原生默认 | 软件 (CGImage) / wgpu (Metal) | AppKit 窗口、NSTextInputClient、NSPasteboard、NSDragging；子窗口弹出层（翻转与滑动）、屏幕即输出；无层 |

> **Linux X11 说明**：Aegle 自身不实现 X11 客户端协议。默认的 SDL3 后端会让 SDL 自行选择 Wayland 或 X11 驱动，因此同一二进制可附带在纯 X11 桌面上运行。

---

## 二、 平台抽象契约 (`aegle-platform`)

宿主 `aegle-app` 针对 `Platform` 和 `SurfacePlatform` 两个 trait 编写，单一 `Native` 类型完成绑定：

```rust
pub trait Platform: Sized {
    fn connect() -> Result<Self>;
    fn preferences(&self) -> Preferences;
    fn wake_handle(&mut self) -> Result<WakeHandle>;
    fn ime_available(&self) -> bool;
    fn create_window(&mut self, options: WindowOptions<'_>) -> Result<WindowId>;
    fn set_layer(&mut self, window: WindowId, layer: LayerOptions, size: PixelSize) -> Result<()>; // 默认 Unsupported
    fn outputs(&self) -> Result<Vec<OutputInfo>>;                                                  // 默认 Unsupported
    fn remove_window(&mut self, window: WindowId) -> Result<()>;
    fn window_info(&self, window: WindowId) -> Result<WindowInfo>;
    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<()>;
    fn next_event(&mut self) -> Option<Event>;
    fn request_redraw(&mut self, window: WindowId) -> Result<()>;
    fn set_title(&mut self, window: WindowId, title: &str) -> Result<()>;
    fn set_cursor(&mut self, window: WindowId, cursor: Cursor) -> Result<()>;
    // 窗口管理，默认 Unsupported
    fn set_size(&mut self, window: WindowId, size: PixelSize) -> Result<()>;
    fn set_size_limits(&mut self, window: WindowId, min: Option<PixelSize>, max: Option<PixelSize>) -> Result<()>;
    fn set_resizable(&mut self, window: WindowId, resizable: bool) -> Result<()>;
    fn set_mode(&mut self, window: WindowId, mode: WindowMode) -> Result<()>;
    fn position(&self, window: WindowId) -> Result<Point>;
    fn set_position(&mut self, window: WindowId, position: Point) -> Result<()>;
    fn set_always_on_top(&mut self, window: WindowId, on_top: bool) -> Result<()>;
    fn set_decorations(&mut self, window: WindowId, decorated: bool) -> Result<()>;
    fn set_areas(&mut self, window: WindowId, areas: &[(Rect, WindowArea)]) -> Result<()>;
    fn set_icon(&mut self, window: WindowId, icon: Icon<'_>) -> Result<()>;
    fn configure_ime(&mut self, window: WindowId, request: Option<ImeRequest>) -> Result<()>;
    fn set_clipboard(&mut self, window: WindowId, seat: SeatId, text: &str) -> Result<()>;
    fn request_clipboard(&mut self, window: WindowId, seat: SeatId) -> Result<()>;
    fn accept_drag(&mut self, window: WindowId, seat: SeatId, accept: bool) -> Result<()>;
    fn start_drag(&mut self, window: WindowId, seat: SeatId, data: &DragData) -> Result<()>;
    fn present(
        &mut self,
        window: WindowId,
        damage: Option<Region<PixelRect>>,
        draw: impl FnOnce(&mut [u8], PixelSize, &Region<PixelRect>) -> DrawResult,
    ) -> Result<bool>;
}

pub trait SurfacePlatform: Platform {
    type Surface: 'static; // 实现 raw-window-handle 0.6 的句柄租约
    fn window_surface(&self, window: WindowId) -> Result<Self::Surface>;
    fn present_external(
        &mut self,
        window: WindowId,
        present: impl FnOnce(PixelSize) -> DrawResult<bool>,
    ) -> Result<bool>;
}
```

### 契约约定与坐标系
* **逻辑坐标**：API 输入、点击、命中、滚轮和 IME 候选框矩形均使用以左上角为原点的逻辑 dp 坐标。
* **物理像素与比例**：`WindowInfo.buffer` 为物理像素，`scale = buffer.width / size.width`。
* **Seat 机制**：输入事件带有 `SeatId`。多 seat 平台以协议对象区分；单 seat 平台（如 Windows、SDL3）恒为 `SeatId(0)`。
* **跨线程唤醒**：`WakeHandle` 为可克隆无锁发送句柄，向平台事件循环发送 `Event::Wake`，无轮询定时器。
* **窗口角色**：`WindowOptions` 的 `layer` 与 `popup` 至多设置一个（否则 `InvalidRole`）。`LayerOptions` 含层级、锚定边、保留区、键盘策略、`margin` 与 `output`；`set_layer` 原地改变层的放置与建议尺寸，几何变化时随后发 `Configure`，换输出或对非层窗口调用返回 `InvalidRole`。`PopupOptions` 含父窗口、父窗口逻辑坐标中的锚定矩形（须非空）、`Side` 与可选的抓取 seat；被系统撤销时发 `Close`。有未移除弹出层的窗口不能移除（`InvalidRole`），宿主须由内向外先移除弹出层。
* **窗口管理**：只作用于普通窗口，层与弹出层返回 `InvalidRole`。尺寸为内容区逻辑尺寸，空或超出平台上限返回 `InvalidSize`；尺寸约束经 `check_size_limits` 校验（不能为空、最小值不大于最大值），设置后当前尺寸被移入约束，程序调整尺寸同样受约束。位置为内容区左上角在桌面逻辑坐标中的位置，非有限值返回 `InvalidPosition`。`set_size` 与 `set_position` 只用于 `WindowMode::Normal`，其他模式返回 `InvalidRole`。`set_mode` 是请求：尚未显示的窗口在首次显示时进入该模式，过渡可跨多次调度，到达的模式由 `WindowInfo::mode` 随 `Configure` 报告。
* **无边框窗口与窗口区域**：`set_decorations(false)` 去掉系统标题栏与边框，内容区的尺寸与位置不变；全屏窗口在离开全屏时才应用。无边框窗口只有内容，应用自绘标题栏并用 `set_areas` 指明哪些部分移动窗口。`set_areas` 以窗口逻辑坐标的矩形列表给出 `WindowArea`（`Client`、`Move`、`Resize(ResizeEdge)`），由共享的 `area_at` 解析：后面的矩形盖住前面的，因此标题栏 `Move` 矩形之后的 `Client` 矩形让其中的按钮仍可按下。主键在 `Move` 或 `Resize` 区域按下时开始窗口系统自己的交互式移动或改尺寸，该次按下不作为指针事件上报；`Resize` 只在窗口可调整尺寸且处于 `Normal` 时生效，全屏时都不生效；带边框的窗口同样可以设置区域。矩形非有限或尺寸为负返回 `InvalidSize`。无边框、可调整尺寸且处于 `Normal` 的窗口仍可由用户拖边缘改尺寸：窗口系统不为它保留边框时，平台按共享的 `border_edge` 把距边缘 `RESIZE_BORDER`（6 dp）以内的按下当作改尺寸（沿边距角 16 dp 以内为对角），优先于窗口区域。
* **窗口图标**：`set_icon` 接受非预乘 RGBA8 的 `Icon`（与托盘图标同一类型，定义在 `aegle-types`），为空或像素数不符返回 `InvalidSize`。窗口图标取自应用本身的窗口系统（macOS 取应用包，多数 Wayland 合成器取 `app_id` 对应的 desktop 文件）返回 `Unsupported`。
* **输出**：`outputs()` 返回 `OutputInfo`（`OutputId`、名称、描述、逻辑位置与尺寸、缩放）；输出增删或几何、缩放变化发合并后的 `Event::Outputs`。

---

## 三、 平台窗口后端实现

### 1. SDL3 后端 (`aegle-platform-sdl3`，Linux 默认)
* **动态加载**：运行时通过 `libloading` 打开 `libSDL3.so.0`，无构建期头文件依赖。
* **呈现与帧节流**：借出 `SDL_GetWindowSurface` 的紧密 BGRA8 缓冲（非 BGRA8 时经 `SDL_ConvertPixels` 暂存转换），以 `SDL_UpdateWindowSurfaceRects` 上报损伤。因缺少逐表面帧回调，软件帧以 `SDL_GetDesktopDisplayMode` 刷新率的截止时间在 `dispatch` 中节流。
* **文本输入**：获焦时 `SDL_StartTextInput`，失焦时停止并发送 `ImeEvent::Left`；光标区域传给 `SDL_SetTextInputArea`。SDL3 不支持周边文本，`ImeRequest.surrounding` 校验后丢弃。
* **拖放与剪贴板**：仅支持接收拖入（`start_drag` 返回 `Unsupported`）；剪贴板为进程级全局同步读写。
* **弹出层与输出**：`SDL_CreatePopupWindow` 创建弹出层，抓取时为弹出菜单、否则为提示；SDL 只接受偏移，弹出层放在锚定矩形的指定一侧并由 SDL 滑回屏幕内，不翻转；不支持弹出层的驱动（dummy、offscreen）返回 `Unsupported`。`SDL_GetDisplays` 列出显示器，名称为驱动所报告的名称；显示器事件合并为 `Event::Outputs`。
* **窗口管理**：`SDL_SetWindowSize`、最小/最大尺寸、`SDL_SetWindowResizable`、`SDL_MaximizeWindow`/`MinimizeWindow`/`RestoreWindow`/`SetWindowFullscreen`、位置与置顶；模式取自窗口标志，由 SDL 的窗口事件报告。SDL 的 Wayland 驱动不能定位或置顶窗口，这两类请求返回 `Unsupported`，而不是交给 SDL 静默忽略。
* **无边框、窗口区域与图标**：`SDL_SetWindowBordered`；SDL 只能在命中测试回调（`SDL_SetWindowHitTest`）里开始移动或改尺寸，回调在按下上报之前同步调用，按窗口区域与无边框窗口的改尺寸边框回答，改尺寸光标由 SDL 显示。回调读取的区域放在窗口的共享分配里，只经共享引用访问。没有命中测试的驱动（dummy、offscreen）对非空区域与去边框返回 `Unsupported`。图标经 `SDL_CreateSurfaceFrom` 与 `SDL_SetWindowIcon`，驱动拒绝时（如其 Wayland 驱动遇到没有 `xdg-toplevel-icon-v1` 的合成器）返回 `Unsupported`。
* **GPU 租约**：从 SDL 窗口属性中读取 `wl_display`/`wl_surface` 或 X11 `Display`/`Window`，构造 `raw-window-handle` 租约。

### 2. Wayland 原生客户端 (`aegle-platform-wayland`)
* **协议支撑**：基于 SCTK 0.21 与 `wayland-client` 0.31，管理单个连接与事件队列。
* **Layer Shell**：`WindowOptions.layer` 在指定 `wl_output` 上创建 `zwlr_layer_shell_v1` 面板或覆盖层，支持边缘锚定、exclusive zone、margin 与键盘策略；`set_layer` 原地更新（改层级需协议 v2）。
* **弹出层**：xdg_popup，父窗口可为 toplevel、弹出层或 layer surface（经 `get_popup`）；positioner 按 `Side` 设锚点与重力，允许翻转与滑动；抓取引用该 seat 最近一次按键、按钮或触摸按下的 serial。
* **输出**：SCTK 输出状态（含 xdg-output 名称）；连接时做两次往返，首次调度前即可列出；缩放由当前模式与逻辑尺寸求得（可为小数）。
* **IME 事务**：为每个 seat 绑定 `zwp_text_input_v3`，支持完整周边文字切片、选区偏移与变更原因。
* **双向拖放**：基于 `wl_data_device` 支持非阻塞管道读写的拖入与拖出发起。
* **窗口管理**：浮动的 toplevel 自定尺寸，`set_size` 直接以新尺寸呈现；尺寸约束为 `xdg_toplevel` 的 min/max 提示，不可调整尺寸时两者都钉在当前尺寸；模式为 `set_maximized`/`set_fullscreen`/`set_minimized` 请求，configure 状态报告最大化与全屏（协议不告诉客户端最小化）；合成器把尺寸留给客户端时（如离开最大化）回到浮动尺寸。协议没有位置与置顶，返回 `Unsupported`。
* **无边框、窗口区域与图标**：去边框即向 `xdg-decoration` 请求客户端装饰而不绘制任何装饰；合成器没有该协议时窗口本来就没有装饰。凡是合成器未给服务端装饰的可调整尺寸 toplevel（按最近一次 configure），平台都提供改尺寸边框。主键在边框、`Move` 或 `Resize` 区域按下时以该次按下的 serial 发 `xdg_toplevel.move`/`.resize`，之后由合成器接管指针；指针在边框与 `Resize` 区域上显示对应方向的改尺寸光标。图标经 `xdg-toplevel-icon-v1`：协议只接受正方形 `wl_shm` 缓冲，图标居中放进透明正方形并预乘，指定后立即释放图标对象与缓冲；合成器没有该协议时返回 `Unsupported`。
* **SHM 呈现**：有界双缓冲 SlotPool 借出，提交后以协议 frame callback 推进动画。

### 3. Windows 后端 (`aegle-platform-win32`)
* **消息循环**：标准 Win32 消息泵；支持 per-monitor DPI 感知缩放。
* **软件呈现**：GDI `SetDIBitsToDevice` 上传 32 位 DIB，等待 `DwmFlush` 同步。
* **TSF 文本服务**：为每个窗口实现 `ITextStoreACP`，支持输入法读取周边文字并重转换已提交内容。
* **OLE 拖放**：注册 `IDropTarget`，支持 `CF_HDROP` 与 `CF_UNICODETEXT`。
* **窗口管理**：尺寸约束在 `WM_GETMINMAXINFO` 中换算为边框尺寸；最大化与最小化用 `ShowWindow`；全屏为覆盖所在显示器的无边框窗口，保存原样式与 `WINDOWPLACEMENT` 以便还原；窗口在首帧前隐藏，此前请求的模式成为首次显示的命令。位置按窗口当前缩放在物理像素与逻辑坐标间换算，跨不同缩放的显示器时只是近似。

* **无边框、窗口区域与图标**：无边框窗口保留 overlapped 样式（系统仍提供贴靠、动画与阴影），在 `WM_NCCALCSIZE` 中让客户区覆盖整个窗口（最大化时取所在显示器的工作区，因为系统会让最大化窗口的边框伸出屏幕）。`WM_NCHITTEST` 把改尺寸边框与窗口区域回答为 `HTLEFT` 等与 `HTCAPTION`，移动、改尺寸、光标、贴靠与双击最大化都由系统完成；带边框的窗口先取系统的回答，只在客户区内再看窗口区域。切换装饰时按原客户区重新计算窗口矩形。图标经 `CreateIcon`（非预乘 BGRA，全零掩码）与 `WM_SETICON` 同时设为大、小图标，随窗口销毁。

### 4. macOS 后端 (`aegle-platform-macos`)
* **AppKit 循环**：以截止时间驱动 `nextEventMatchingMask:`（无截止时间时等到 `distantFuture`），不进入 `NSApplicationMain`；每次分发至少运行一轮 run loop，持续重绘也不会饿死 AppKit。等待期间 run loop 回调（跨线程唤醒、输入法异步提交）排入的首个事件补发一个应用自定义事件结束等待。
* **呈现与输入**：软件呈现时宿主画入一块紧密画布，只把损伤区域标为需重画；视图的 `drawRect:` 把 AppKit 正在重画的每个矩形从画布拷成一张自有像素的小 `CGImage` 画入 Core Animation 的后备存储（由 GPU 渲染，只重画标记的区域），因此一帧只上传变化的像素；事件每轮与每次呈现各有自动释放池；`NSTextInputClient` 协议映射文本编辑与预编辑；`CAMetalLayer` 原生租约。Metal 在窗口不可见时（包括刚排到前台、窗口服务器尚未确认时）不给 drawable，窗口随即等待遮挡回调再重绘。
* **输出**：`NSScreen.screens` 即输出，ID 为 `CGDirectDisplayID`，名称为系统本地化的显示器名（无连接器名，`description` 为空）；位置由 AppKit 左下原点换算为主屏左上原点的逻辑坐标。`NSApplicationDidChangeScreenParametersNotification` 合并为一个 `Event::Outputs`。
* **弹出层**：无边框 `NSWindow`，作为父窗口的子窗口随其移动、排序与最小化。创建时一次定位：锚定矩形换算到屏幕后由 `popup_origin` 放在请求一侧，该侧越出父窗口所在屏幕的可见区域且对侧放得下时翻转，再沿两轴滑入可见区域（与 `xdg_positioner` 的 flip + slide 一致，规则在 `tests/placement.rs` 测试）。抓取弹出层显示时成为 key 窗口；AppKit 没有弹出抓取，平台自己撤销：在弹出链（自身及由它打开的弹出层）之外的窗口按下，或应用失去激活时，对其发一次 `Close`，由内向外；该次按下仍正常投递。被动弹出层从不成为 key。父窗口有未移除的弹出层时不能移除（`InvalidRole`）。
* **窗口管理**：`setContentSize`、`contentMinSize`/`contentMaxSize`、样式位 `Resizable`、窗口级别（置顶为 `NSFloatingWindowLevel`）；最大化即缩放（`zoom:`），最小化进入 Dock，全屏为独立空间（窗口带 `FullScreenPrimary`）。模式请求保存为目标，每次过渡结束的委托回调在下一轮 run loop 推进一步（AppKit 拒绝在结束上一次全屏过渡的回调里开始下一次），因此全屏到最小化会先退出全屏；到达目标的那一步清除目标，被 AppKit 拒绝的请求不会稍后重试。
* **缩放状态**：`isZoomed` 对位于最大尺寸约束上的窗口和用户不可调整尺寸的窗口也返回真，因此 `Maximized` 由平台自己的标记决定：一次缩放（程序请求，或用户经 `windowShouldZoom:toFrame:`）把窗口带到屏幕框架，且可调整尺寸的窗口此刻仍被 AppKit 认为处于该框架。AppKit 只缩放可调整尺寸的窗口，不可调整尺寸的窗口在缩放期间临时带上 `Resizable`。改样式掩码后重新把视图设为第一响应者，否则按键不再送达视图。
* **无边框与窗口区域**：普通窗口是允许成为 key 与 main 窗口的 `NSWindow` 子类；去边框只去掉 `Titled`，因此缩放、最小化与全屏照常，切换时保持内容区矩形，全屏期间的请求在退出全屏的回调里应用。主键在 `Move` 区域按下时调用 `performWindowDragWithEvent:`（立即返回，由窗口服务器拖动）。AppKit 没有开始改尺寸的调用，`Resize` 区域与无边框窗口的改尺寸边框由平台跟踪：按下时记下屏幕上的指针位置与窗口框架，每个拖动事件按位移设置框架（受尺寸约束限制，对边不动），不进入模态循环；指针在其上显示 `frameResizeCursorFromPosition:inDirections:` 光标（macOS 15 之前只有四边有光标，四角保持窗口自己的光标）。实测中 AppKit 对去掉 `Titled` 的可调整尺寸窗口有时自己接管边缘内侧约 2 px 处的按下并运行原生改尺寸（结果相同），有时把按下交给视图；平台边框覆盖后一种情况。
* **图标**：macOS 的窗口没有自己的图标，`set_icon` 返回 `Unsupported`。
* **透明**：`WindowOptions::transparent` 使窗口非不透明、背景透明，wgpu 以非不透明 `CAMetalLayer`（wgpu 称 `PostMultiplied`，Core Animation 按预乘合成）呈现预乘 alpha。

---

## 四、 渲染后端架构

一个 Aegle 二进制**只链接一个渲染器**：启用 `cairo`、`vulkan` 或 `wgpu` 之一时用它（两个同时启用为编译错误），否则用默认的 `tiny-skia`。四个渲染器实现同一个 `Renderer` trait：

```rust
pub(crate) trait Renderer {
    fn add_window(&mut self, window: WindowId, setup: &mut Setup<'_>) -> Result<()>;
    fn remove_window(&mut self, window: WindowId);
    fn present(&mut self, window: WindowId, present: &mut Present<'_>) -> Result<bool>;
}
```

### 渲染器特性对比

| 特性 | tiny-skia / cairo (CPU) | Vulkan 1.1 | wgpu (通用 GPU) |
|---|---|---|---|
| **执行载体** | CPU 多线程/单线程光栅 | GPU 硬件着色器 | GPU 硬件 (Vulkan/Metal/D3D12) |
| **覆盖率计算** | tiny-skia 算法 / cairo 动态库 | 片元着色器导数估计边缘 | 片元着色器导数估计边缘 |
| **混合空间** | 预乘线性光混合 | 预乘线性光混合 (RGBA16F) | 预乘线性光混合 (RGBA16F) |
| **外部应用纹理** | 不支持 (`UnsupportedCommand`) | 支持 (`register_texture`) | 支持 (`register_texture`) |
| **自定义片元程序** | 不支持 (`UnsupportedCommand`) | 首次使用编成 SPIR-V (`shaders`) | 首次使用编译 WGSL (`shaders`) |
| **多层嵌套裁剪** | 覆盖率 mask 栈 (最多 64 层) | 硬件着色器解析 (最多 8 层) | 硬件着色器解析 (最多 8 层) |
| **字形与图像图集** | CPU 灰度/彩色缓存 | 按需 R8/RGBA8 货架图集 | 按需 1024² 灰度/彩色图集 |
| **呈现范围** | 仅重绘并上报有变化的区域 | 整幅重绘，支持增量呈现上报 | 整幅重绘并提交整幅 surface |
| **批次拆分** | 整帧绘制不分段 | 每 16,384 图元或图集满分段提交 | 每 16,384 图元分段提交 |

### 1. CPU 软件渲染器 (`aegle-render-tiny-skia` / `aegle-render-cairo`)
* **共享合成器 (`aegle-render-cpu`)**：像素缓冲直接借用平台的 BGRA8 内存切片。正立直角填充和边框直接由合成器按精确像素覆盖率解析绘制，不进入光栅库。
* **光栅器插件**：
  * `tiny-skia`（默认）：纯 Rust 静态编译，无外部运行依赖。
  * `cairo`：运行时加载系统 `libcairo.so.2`，适合追求极小发布体积的 Linux 发行版环境。

### 2. Vulkan 渲染器 (`aegle-render-vulkan`)
* **基线要求**：Vulkan 1.1，标准 graphics queue，单个 descriptor pool。
* **数据流**：图元记录直接写入 host-visible storage buffer，顶点在顶点着色器中按 instance 展开，相邻同管线图元自动合并单次 instanced draw。
* **多层裁剪**：着色器内解析最多 8 层矩形、圆角与仿射裁剪链。
* **挖洞**：`Cut(shape)` 以 destination-out 合成（目标保留 1 − 覆盖率）。GPU 后端为其单独建一条混合状态不同的几何管线（种类 4，按批切换）；软件渲染器把像素四个分量同乘 1 − 覆盖率，对预乘 sRGB 存储是精确的。
* **自定义片元程序**（`shaders` 特性）：`Shader { shape, shader, time, values }` 用 WGSL 程序的颜色填充形状。`aegle-gpu` 把共享前导（记录、存储、实例四边形、覆盖率与裁剪，`common.wgsl`）、全局 `time`/`values` 与程序的 `shade` 拼成模块，入口 `fs_shader` 先算覆盖率再调用程序，结果由 sRGB 转线性预乘后按覆盖率合成；`check_shader` 用 naga 校验并把错误位置换算到程序自己的行列。记录为种类 5，页槽放后端的程序管线号，`values` 占一行裁剪行。Vulkan 首次使用时经 naga 编成 SPIR-V（顶点阶段复用构建期的 geometry.vert），wgpu 直接交给 wgpu；两者都按源码哈希缓存管线，超过 8 条时在帧开始丢弃上一帧未用的。编译失败返回 `Error::Shader`；软件渲染器返回 `UnsupportedCommand`。
* **内阴影**：`InsetShadow { shape, hole, color, blur }` 在 `shape` 内画颜色乘以（1 − `hole` 的高斯阴影）。GPU 把 `shape` 作为该图元的一层裁剪压入裁剪链，着色器取 1 减阴影；软件渲染器用 `shape` 的覆盖遮罩逐像素取同一阴影公式。
* **逐角圆角**：`RoundedRect::with_radii` 给四个角各自的半径（左上、右上、右下、左下，相邻两角之和超过边长时按同一比例缩小，同 CSS）。GPU 把四个半径以 16 位定点数放进图元行未用的 w 分量，按点所在象限取角；阴影逐行取该行一侧的两个角，软件渲染器用同样公式。
* **字形图集与路径**：灰度 R8 与彩色 RGBA8 分页管理；路径由 CPU（zeno）生成 R8 覆盖率后上传图集。
* **应用纹理**：`Renderer::register_texture` 接受外部 `vk::ImageView`，分配独立描述符集接入渲染管线。

### 3. wgpu 渲染器 (`aegle-render-wgpu`)
* **定位**：全平台通用的跨 API 路径，同一份代码覆盖 Vulkan、Metal 与 Direct3D 12。
* **着色器**：采用 WGSL 源码（来自共享的 `aegle-gpu`），在运行时由 wgpu 编译。
* **资源管理**：单图元 112 B、单裁剪 64 B 存储行；超出图集的大尺寸图像和路径 mask 分配独立临时纹理，使用两帧后自动回收。

---

## 五、 高级视觉效果（阴影、渐变与图层）

* **渐变填充**：线性与径向渐变支持 2–16 个色标，在预乘线性空间中插值；无需离屏纹理，直接在几何图元中计算。
* **解析式柔和阴影**：基于高斯卷积闭式解在像素中心直接解析求值（x 方向 erf 积分，y 方向分段加权），三倍标准差之外自动剔除，无需离屏高斯模糊 pass。
* **离屏图层与模糊**：
  * 节点设置 `opacity` 或 `backdrop_blur` 时触发离屏层绘制。
  * 背景模糊仅对当前窗口已绘制内容生效，采用三次盒式模糊逼近高斯模糊。
  * 超出预算的背景模糊将被自动跳过并记录（`skipped_blurs()`），不会丢弃界面基础图元。
