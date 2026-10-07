# wgpu 后端

状态：最小实现，2026-10-06。离屏几何、裁剪、字形、图像、路径与原生 surface 呈现已实现，只在 Linux 上经 Vulkan 验证；Metal 与 Direct3D 12 经 wgpu 编译进该 crate，没有在这些平台构建或运行过。

## 定位与取舍

`aegle-render-wgpu` 是全平台通用的第二条 GPU 路径，原生 Metal 方案已放弃。它不替代 ash/Vulkan：体积、分配预算和空闲行为仍以 [Vulkan](vulkan.md) 为准，wgpu 换来的是同一份代码覆盖 Vulkan、Metal 与 Direct3D 12。调用方通过 `aegle-app` 的 `wgpu` feature 与 `RendererBackend::Wgpu` 显式选择，失败不会切换后端。

依赖见 [依赖版本](dependencies.md)：wgpu 30.0.1 关闭默认 feature，只启用 std、parking_lot、wgsl、vulkan、dx12、metal；pollster 1.0.1。不启用 GLES：它没有顶点阶段存储缓冲，而记录以存储缓冲提供。适配器缺少 `DownlevelFlags::VERTEX_STORAGE` 或每阶段存储缓冲少于两个时，创建返回 `Unsupported`；设备只请求两个存储缓冲。

## 范围

消费与软件、Vulkan 相同的不可变 Scene：实色矩形、统一圆角、居中描边、二维仿射、最多八层嵌套裁剪（含外部裁剪），可选 `text` 提供灰度与彩色字形、RGBA 图像和填充/描边路径（含填充规则、端点与连接）。未启用 `text` 时这些命令以及任何未来命令返回 `UnsupportedCommand`，使该帧失败，不跳过后报告成功；失败帧不能 `finish`，也不会呈现。

颜色契约与其他后端一致：输入为非预乘 sRGB，在 RGBA16F 线性图像中预乘混合，第二遍编码为预乘 sRGB 写入输出；离屏输出为 `Rgba8Unorm`，窗口用 surface 提供的第一个非 sRGB 格式，因为该遍已自行编码。

## 绘制与资源

`Renderer::new` 通过 `WGPU_BACKEND`、`WGPU_ADAPTER_NAME`、`WGPU_POWER_PREF` 选择适配器，无环境变量时取 wgpu 默认。`begin_frame` 返回 `Frame`；`draw_clipped` 的外部裁剪位于设备坐标。`finish` 提交并（窗口）呈现；离屏结果只经显式 `read_pixels` 读回，参数与 Vulkan 相同：紧密、自上而下的预乘 sRGB RGBA8。没有常驻线程或轮询。

记录是每图元 112 B 与每裁剪 64 B 的存储行，由实例化绘制按相邻同类图元合并。每次提交最多 `aegle_gpu::MAX_PRIMITIVES`（16,384）个图元（1.75 MiB），满了就提交并继续，不分配更大的缓冲，Vulkan 后端用同一上限分段；裁剪行保留整帧，后续提交仍可引用，其数量随场景的裁剪命令增长。存储缓冲只增不减，替换时才重建绑定组。队列顺序保证后写入的缓冲与图集内容不会被先提交的绘制看到。

字形图集有一个 R8 灰度页和一个 RGBA8 sRGB 彩色页，各为 `Options::atlas_size`²（默认 1024），首次需要才分配，每个字形带一像素透明边，货架式装入，每页最多 4096 个条目。页或条目满时先提交本帧已记录的图元，再整页清空并继续；没有按页 LRU，也没有 `AtlasFull`。字形身份、变换与对比曲线复用 `aegle-glyph`，与另两个后端一致。

图像按 Image id 缓存，非预乘 sRGB 像素在线性空间预乘后以 RGBA8 sRGB 上传，着色器钳制到边缘纹素并乘以解析的边缘覆盖率，没有 mipmap。路径由 zeno 在 CPU 光栅为 R8 覆盖率 mask，键为路径 id、线性 2×2 矩阵、两轴四分之一像素相位和描边样式：整像素平移命中原条目，新的缩放/旋转再光栅一次。与字形相同的页放不下时，图像和路径 mask（含带边框超过 `atlas_size` 的）获得恰好其尺寸的专用纹理，不带边框，在最后一次使用后的下一帧仍保留、再下一帧释放，所以动画重绘不会反复上传，消失的图像会被回收。单个字形超过页尺寸，或图像/路径 mask 超过设备纹理上限，返回 `TooLarge`。上传前的像素转换缓冲超过 1 MiB 即释放；不像 Vulkan 后端那样限制 CPU 上传量，图像大小只受设备纹理上限约束。`Renderer::resident_entries` 报告当前驻留条目数。

窗口：`WindowRenderer::new` 为 unsafe，与 Vulkan 版有相同的句柄寿命要求；FIFO 呈现。wgpu 没有呈现区域接口，每帧整幅绘制并上报整个 surface（Vulkan 后端可上报变化区域）。`Options::transparent` 且合成器提供预乘 alpha 时保留透明，否则要求不透明清屏色。`begin_frame` 在零尺寸、被遮挡或超时时返回 `None`，surface 过期返回 `SurfaceOutOfDate` 要求调用方重绘，surface 丢失返回 `SurfaceLost`。wgpu 默认把未捕获的验证、内存与内部错误当作致命错误并 panic；本后端在设备上安装 `on_uncaptured_error` 与设备丢失回调，记下第一个故障，之后 `begin_frame`、提交与 `read_pixels` 返回 `Error::Gpu` 或 `Error::DeviceLost`，不再 panic。wgpu 在编码与提交时同步校验，所以出错的那一帧在 `finish` 即返回错误。故障不可恢复：同一设备上的渲染器（含共享设备的窗口）都拒绝后续工作，由调用方新建渲染器；Aegle 不自动重建设备或切换后端，与 Vulkan 后端一致。

## 应用纹理

`Renderer::register_texture(&wgpu::Texture)`（窗口共用设备时也可经 `SharedGpu`，原生 App 用 `App::wgpu()` 取得）把应用自己的纹理登记为 `TextureId`，场景用 `SceneBuilder::texture(id, rect)` 绘制，与图像一样双线性过滤、钳制到边缘纹素并带解析边缘覆盖率。要求：创建于 `Renderer::device()`、单采样二维、带 `TEXTURE_BINDING`、可过滤的浮点格式，采样结果须是线性预乘 RGBA（例如内容不透明或已预乘的 `Rgba8UnormSrgb`），取 mip 0；不满足时登记返回 `Unsupported`。登记表在共享设备上，所有窗口都能画同一纹理。录制时把该纹理的 bind group 复制进本次提交的列表，因此帧中途注销不影响已录制的绘制；之后的帧画未登记的 id 以 `UnknownTexture` 使帧失败。应用在 Aegle 帧之前把写纹理的命令提交到同一个 `queue()`，队列顺序保证采样时已完成。`aegle_render_wgpu::wgpu` 重导出所用 wgpu 版本。需要 `text` feature（纹理与图像共用带纹理管线）。

## 与 Vulkan 后端共享的部分

图元与裁剪记录、场景遍历、边界与裁剪几何、货架装箱、图像放置、路径 mask 的量化与光栅化，以及两个 WGSL 着色器，都在 `aegle-gpu`，两个后端共用，没有第二份副本。顶点阶段的 Y 轴方向由视口高度的符号选择：Vulkan 传正高度，wgpu 传负高度，所以同一份 WGSL 既在构建期编译为 SPIR-V，又在运行时交给 wgpu。两个后端各自保留的是与 API 相关的部分：资源与同步、图集页的上传与淘汰策略、呈现。

## 验证

`crates/aegle-render-wgpu/tests/render.rs` 默认 ignored，必须选择适配器运行：

```sh
WGPU_BACKEND=vulkan WGPU_ADAPTER_NAME=RADV cargo test -p aegle-render-wgpu --features text --test render -- --ignored --nocapture
WGPU_BACKEND=vulkan WGPU_ADAPTER_NAME=llvmpipe cargo test -p aegle-render-wgpu --features text --test render -- --ignored --nocapture
cargo run -p aegle-render-wgpu --features text --example scene --release -- target/aegle-wgpu.ppm
```

场景对照软件渲染器：像素对齐的圆角填充、半透明叠加、描边与裁剪的采样点误差不超过 2 级；CJK 段落在 40 像素的小图集上逐字节误差不超过 3 级，并经过一次中途整页清空；第九层裁剪使帧失败，之后同一渲染器重新绘制得到与失败前相同的字节。
