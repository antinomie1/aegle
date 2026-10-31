# 平台与渲染后端

Aegle 的窗口系统接入（平台）与图形绘制（渲染器）分属不同职责。平台负责原生连接、窗口生命周期、输入分发和事件等待；渲染器负责消费不可变 `Scene` 绘制记录并输出像素。

---

## 一、 平台支持矩阵

| 平台 | 窗口后端 | 默认状态 | 绘制方式 | 特色能力与限制 |
|---|---|---|---|---|
| **Linux** | **`aegle-platform-sdl3`** | **默认** | 软件 / Vulkan / wgpu | 运行时加载 `libSDL3.so.0`；自动选 Wayland/X11；无 layer-shell；只收拖放 |
| **Linux** | **`aegle-platform-wayland`** | 可选 (`wayland`) | 软件 / Vulkan / wgpu | 原生 Wayland 客户端（SCTK）；支持 layer-shell 面板、双向拖放、text-input-v3 周边文字 |
| **Windows** | **`aegle-platform-win32`** | 原生默认 | 软件 (GDI) / Vulkan / wgpu | Win32 消息循环、DPI 感知、TSF 文本存储输入法、OLE 拖放 |
| **macOS** | **`aegle-platform-macos`** | 原生默认 | 软件 (CGImage) / wgpu (Metal) | AppKit 窗口、NSTextInputClient、NSPasteboard、NSDragging |

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
    fn remove_window(&mut self, window: WindowId) -> Result<()>;
    fn window_info(&self, window: WindowId) -> Result<WindowInfo>;
    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<()>;
    fn next_event(&mut self) -> Option<Event>;
    fn request_redraw(&mut self, window: WindowId) -> Result<()>;
    fn set_title(&mut self, window: WindowId, title: &str) -> Result<()>;
    fn set_cursor(&mut self, window: WindowId, cursor: Cursor) -> Result<()>;
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

---

## 三、 平台窗口后端实现

### 1. SDL3 后端 (`aegle-platform-sdl3`，Linux 默认)
* **动态加载**：运行时通过 `libloading` 打开 `libSDL3.so.0`，无构建期头文件依赖。
* **呈现与帧节流**：借出 `SDL_GetWindowSurface` 的紧密 BGRA8 缓冲（非 BGRA8 时经 `SDL_ConvertPixels` 暂存转换），以 `SDL_UpdateWindowSurfaceRects` 上报损伤。因缺少逐表面帧回调，软件帧以 `SDL_GetDesktopDisplayMode` 刷新率的截止时间在 `dispatch` 中节流。
* **文本输入**：获焦时 `SDL_StartTextInput`，失焦时停止并发送 `ImeEvent::Left`；光标区域传给 `SDL_SetTextInputArea`。SDL3 不支持周边文本，`ImeRequest.surrounding` 校验后丢弃。
* **拖放与剪贴板**：仅支持接收拖入（`start_drag` 返回 `Unsupported`）；剪贴板为进程级全局同步读写。
* **GPU 租约**：从 SDL 窗口属性中读取 `wl_display`/`wl_surface` 或 X11 `Display`/`Window`，构造 `raw-window-handle` 租约。

### 2. Wayland 原生客户端 (`aegle-platform-wayland`)
* **协议支撑**：基于 SCTK 0.21 与 `wayland-client` 0.31，管理单个连接与事件队列。
* **Layer Shell**：`WindowOptions.layer` 创建 `zwlr_layer_shell_v1` 面板或覆盖层，支持边缘锚定与 exclusive zone。
* **IME 事务**：为每个 seat 绑定 `zwp_text_input_v3`，支持完整周边文字切片、选区偏移与变更原因。
* **双向拖放**：基于 `wl_data_device` 支持非阻塞管道读写的拖入与拖出发起。
* **SHM 呈现**：有界双缓冲 SlotPool 借出，提交后以协议 frame callback 推进动画。

### 3. Windows 后端 (`aegle-platform-win32`)
* **消息循环**：标准 Win32 消息泵；支持 per-monitor DPI 感知缩放。
* **软件呈现**：GDI `SetDIBitsToDevice` 上传 32 位 DIB，等待 `DwmFlush` 同步。
* **TSF 文本服务**：为每个窗口实现 `ITextStoreACP`，支持输入法读取周边文字并重转换已提交内容。
* **OLE 拖放**：注册 `IDropTarget`，支持 `CF_HDROP` 与 `CF_UNICODETEXT`。

### 4. macOS 后端 (`aegle-platform-macos`)
* **AppKit 循环**：以截止时间驱动 `nextEventMatchingMask:`，不进入 `NSApplicationMain`。
* **呈现与输入**：`CGImage` 软件呈现；`NSTextInputClient` 协议映射文本编辑与预编辑；`CAMetalLayer` 原生租约。

---

## 四、 渲染后端架构

一个 Aegle 构建**恰好编入一个渲染器**。四个渲染器实现同一个 `Renderer` trait：

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
