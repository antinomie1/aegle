# wgpu 后端

状态：最小实现，2026-10-06。离屏几何、裁剪、字形与原生 surface 呈现已实现，只在 Linux 上经 Vulkan 验证；Metal 与 Direct3D 12 经 wgpu 编译进该 crate，没有在这些平台构建或运行过。

## 定位与取舍

`aegle-render-wgpu` 是全平台通用的第二条 GPU 路径，原生 Metal 方案已放弃。它不替代 ash/Vulkan：体积、分配预算和空闲行为仍以 [Vulkan](vulkan.md) 为准，wgpu 换来的是同一份代码覆盖 Vulkan、Metal 与 Direct3D 12。调用方通过 `aegle-app` 的 `wgpu` feature 与 `RendererBackend::Wgpu` 显式选择，失败不会切换后端。

依赖见 [依赖版本](dependencies.md)：wgpu 30.0.1 关闭默认 feature，只启用 std、parking_lot、wgsl、vulkan、dx12、metal；pollster 1.0.1。不启用 GLES：它没有顶点阶段存储缓冲，而记录以存储缓冲提供。适配器没有至少两个顶点可见的存储缓冲时，创建返回 `Unsupported`。

## 范围

消费与软件、Vulkan 相同的不可变 Scene：实色矩形、统一圆角、居中描边、二维仿射、最多八层嵌套裁剪（含外部裁剪），可选 `text` 提供灰度与彩色字形。不支持的命令（图像、路径、未来命令）返回 `UnsupportedCommand`，使该帧失败，不跳过后报告成功；失败帧不能 `finish`，也不会呈现。图像与路径需要新的图集条目与 CPU 光栅路径，未实现。

颜色契约与其他后端一致：输入为非预乘 sRGB，在 RGBA16F 线性图像中预乘混合，第二遍编码为预乘 sRGB 写入输出；离屏输出为 `Rgba8Unorm`，窗口用 surface 提供的第一个非 sRGB 格式，因为该遍已自行编码。

## 绘制与资源

`Renderer::new` 通过 `WGPU_BACKEND`、`WGPU_ADAPTER_NAME`、`WGPU_POWER_PREF` 选择适配器，无环境变量时取 wgpu 默认。`begin_frame` 返回 `Frame`；`draw_clipped` 的外部裁剪位于设备坐标。`finish` 提交并（窗口）呈现；离屏结果只经显式 `read_pixels` 读回，参数与 Vulkan 相同：紧密、自上而下的预乘 sRGB RGBA8。没有常驻线程或轮询。

记录是每图元 112 B 与每裁剪 64 B 的存储行，由实例化绘制按相邻同类图元合并。每次提交最多 16,384 个图元（1.75 MiB），满了就提交并继续，不分配更大的缓冲；裁剪行保留整帧，后续提交仍可引用。存储缓冲只增不减，替换时才重建绑定组。队列顺序保证后写入的缓冲与图集内容不会被先提交的绘制看到。

字形图集有一个 R8 灰度页和一个 RGBA8 sRGB 彩色页，各为 `Options::atlas_size`²（默认 1024），首次需要才分配，每个字形带一像素透明边，货架式装入，每页最多 4096 个条目。页或条目满时先提交本帧已记录的图元，再整页清空并继续；没有按页 LRU，也没有 `AtlasFull`。单个字形连边框超过页尺寸返回 `GlyphTooLarge`。字形身份、变换与对比曲线复用 `aegle-glyph`，与另两个后端一致。

窗口：`WindowRenderer::new` 为 unsafe，与 Vulkan 版有相同的句柄寿命要求；FIFO 呈现。`Options::transparent` 且合成器提供预乘 alpha 时保留透明，否则要求不透明清屏色。`begin_frame` 在零尺寸、被遮挡或超时时返回 `None`，surface 过期返回 `SurfaceOutOfDate` 要求调用方重绘，surface 丢失返回 `SurfaceLost`。wgpu 默认把未捕获的设备与验证错误当作致命错误并 panic（已在其源码确认）；本后端没有安装自己的处理器，设备丢失尚未转为可恢复错误。

## 与 Vulkan 后端的重复

记录构建（`records.rs`）与 WGSL 是 Vulkan 版的简化副本，不是共享代码：wgpu 的裁剪空间 Y 轴向上，着色器顶点阶段需要翻转，所以不能直接复用同一文件。两者共享的纯 CPU 逻辑（图元与裁剪几何、边界计算）以后应提取为独立 crate，等第二个后端稳定后再做，避免在没有验证面时重构已验证的 Vulkan 路径。

## 验证

`crates/aegle-render-wgpu/tests/render.rs` 默认 ignored，必须选择适配器运行：

```sh
WGPU_BACKEND=vulkan WGPU_ADAPTER_NAME=RADV cargo test -p aegle-render-wgpu --features text --test render -- --ignored --nocapture
WGPU_BACKEND=vulkan WGPU_ADAPTER_NAME=llvmpipe cargo test -p aegle-render-wgpu --features text --test render -- --ignored --nocapture
cargo run -p aegle-render-wgpu --features text --example scene --release -- target/aegle-wgpu.ppm
```

场景对照软件渲染器：像素对齐的圆角填充、半透明叠加、描边与裁剪的采样点误差不超过 2 级；CJK 段落在 40 像素的小图集上逐字节误差不超过 3 级，并经过一次中途整页清空；第九层裁剪使帧失败，之后同一渲染器重新绘制得到与失败前相同的字节。
