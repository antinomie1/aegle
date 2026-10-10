# Vulkan 绘制与原生呈现

状态：独立离屏几何与可选文字已实现，2026-10-05。已在 AMD RX 6800 XT（Linux RADV 与 Windows AMD 专有驱动）和 Lavapipe 上执行综合测试，Khronos 验证层检查仅在 Linux。记录构建、着色器与图像/路径放置来自共享的 `aegle-gpu`。可选 window 已连接 Wayland/Win32 swapchain，App 可显式选择 Vulkan；Windows 实机验收范围见实现状态。

## 范围与依赖

`aegle-render-vulkan` 消费现有不可变 Scene，与软件后端共用颜色、变换和裁剪语义；不拥有控件树、Taffy、字体系统或窗口循环。默认提供实色矩形、统一圆角、居中边框、渐变填充、柔和阴影、二维仿射变换与嵌套裁剪。渐变与阴影仍走几何管线：图元 `header[3]` 选择效果，色标两个一行写入裁剪缓冲，不新增绑定、管线或离屏纹理。可选 `text` 接受同一 Scene 的定位字形，依赖 glyph/scene 和现有 hashbrown；不依赖 Parley。未启用 text 时文字命令返回不支持错误，不允许跳过字形后报告成功。

以 Vulkan 1.1 为基线，复用 ash 0.38 调用驱动，bytemuck 1.25 检查 CPU/shader 数据布局；Naga 30 仅在构建期将 WGSL 转为 SPIR-V 1.3，不引入 wgpu 或运行时 shader 编译器。使用传统 render pass、一个 graphics queue 和可复用 fence，不要求可选 GPU feature。

图元记录写入 host-visible storage buffer，顶点由 shader 按 instance 生成；相邻且 pipeline/图集页相同的图元合并为一次 instanced draw，无逐图元 push constant 或 scissor。CPU 只记录变换、颜色和裁剪链，不栅格化整帧。图元四边形先与裁剪作用域的整数范围相交，覆盖的像素中心与同尺寸 scissor 一致；shader 计算小数、圆角和仿射裁剪覆盖率，最多八层（含外部 clip）；共享父索引避免为每个图元复制整条裁剪链。局部几何等比归一化后传入 shader，避免极大/极小局部单位造成距离计算溢出。映射后的几何范围限定为 ±1,048,576，不能表示的逆变换返回错误。

绘制遵守记录顺序。解析覆盖率使用 shader 导数估计边缘，不承诺与软件覆盖率逐像素相同；内部实色、颜色混合和裁剪边界分别验证。

window feature 仅增加 raw-window-handle 0.6.2，复用现有平台循环、IME 与无障碍；图像、路径与应用图像见下文。

## 可选文字与图集

软件和 Vulkan 通过 `aegle-glyph/scene` 共用 RasterTransform：正轴向均匀缩放采用 hinting、整像素基线与水平四分之一像素相位，灰度覆盖率经共用对比曲线，其他仿射按最大列长选择光栅字号，再过滤采样。完整 GlyphKey 包含字体资源身份、face、glyph、字号、相位、hint、变化轴及必要前景；不把单个 hash 或 CPU LRU 槽位作为身份。灰度忽略前景色，彩色前景层使用不透明 run RGB，run alpha 在 GPU 上只应用一次。

图集先查询自身完整身份，命中不访问 CPU 字形缓存；缺失才通过共享 GlyphCache 按需光栅化。R8_UNORM 灰度与 RGBA8_SRGB 彩色页独立使用简单 shelf 排布，每个字形保留一像素透明边。新建/重置页由 GPU 清零，CPU 只上传紧密字形补丁，不保留页大小的 CPU 镜像。彩色上传先在线性空间预乘，再编码 RGB；纹理硬件解码、插值后得到正确线性预乘值，避免透明彩色边缘出现色晕。灰度乘前景，彩色保留调色板颜色。

页数或条目达到上限时淘汰最旧的未使用页；当前帧已用页固定，不能覆盖正在录制的字形。保守像素支持范围与祖先 clip 不相交的字形不入图集；任意变换/圆角的边界筛选仍为保守估计，不扫描字形像素来判断完全透明。本帧剩余字形装不下时先提交已记录部分、解除页固定，再从下一个字形继续（见“分段提交”）；只有空提交仍放不下一个条目才返回 AtlasFull。单字形连透明边超过页尺寸返回 GlyphTooLarge，均不静默漏字。单页放不下的大字号需调用方显式调大页尺寸。

## 图像与路径（需 text）

图像和路径复用字形图集与 pipeline，不新增 descriptor 或绘制通道；未启用 text 时这两类命令返回 UnsupportedCommand。图像以 RGBA8_SRGB color 条目上传（上传前线性预乘），按 Image id 缓存，shader 钳制到边缘纹素并乘以矩形解析覆盖率。路径由 zeno 在 CPU 生成 R8 覆盖率，键为路径 id、线性 2×2 矩阵、两轴四分之一像素相位、描边样式与宽度：整像素平移命中原条目，新的缩放/旋转再光栅化一次并占用新条目，平移动画不会逐帧重建。超过页尺寸的条目获得恰好其尺寸的专用页，仍计入页数、条目和设备预算；每次提交的 CPU 上传受 `upload_bytes`（默认 1 MiB，约 512×512 RGBA）限制，一帧累计超过时分段提交，单个条目超过才返回 Budget，超出设备图像尺寸返回 InvalidSize，不静默跳过。没有 mipmap。

新增字形在成功提交后才视为已上传。取消或失败帧的脏页在下一帧清掉对应条目并重置，避免命中未上传内容；此前同页的有效条目也会失效。GPU 上传缓冲在 fence 完成时释放，CPU 上传容量保留复用；页图像在安全等待后才替换。空字形无需图集页，可复用 CPU 缓存。

## 应用图像（需 text）

`Renderer::register_texture(view, extent)`（unsafe，`SharedDevice` 上同名方法供多窗口共用）把应用创建的 `vk::ImageView` 登记为 `TextureId`，场景用 `SceneBuilder::texture` 绘制，复用图像管线与采样器。`raw_device()` 返回实例、物理设备、逻辑设备、图形队列与队列族（`RawDevice`，`aegle_render_vulkan::ash` 重导出所用 ash 版本），应用在其上创建并渲染图像。调用方保证：视图属于该设备、单采样二维颜色、带 SAMPLED 用途、可过滤的浮点格式且采样值为线性预乘 RGBA；执行引用它的帧时处于 `SHADER_READ_ONLY_OPTIMAL`，并以先于该帧提交到同一队列的屏障使写入对片段着色器可见；视图在注销且使用它的帧完成（`wait`）之前保持有效。

每个渲染器在描述符池中为应用图像额外保留 16 个描述符集。每帧开始（已等待上一提交的 fence）清空映射，本帧首次画某个纹理时取一个空闲集合并写入描述符，所以从不改写在途命令使用的集合；一次提交最多绑定 16 个不同纹理，更多时分段提交（`TooManyTextures` 只在空提交仍无空位时返回，正常不会出现）；未登记的 id 返回 `UnknownTexture`，使帧失败。

## 调用与生命周期

```rust
let mut renderer = Renderer::new(Options::default())?;
let mut frame = renderer.begin_frame(width, height, Color::WHITE)?;
frame.draw(&scene, Affine::IDENTITY)?;
frame.finish()?;
// 仅离屏导出或验证需要读回；正常绘制不强制复制到 CPU。
renderer.read_pixels(&mut rgba)?;
```

Renderer 持有一个可复用离屏目标及帧资源，同时至多一个提交。`begin_frame` 等待前次提交并建立本帧清屏颜色；`draw` 可重复调用，Scene 自身的作用域只影响本次调用。`finish` 提交 GPU 工作，不等待或读回；调用方通过 `wait` 显式等待，或由 `read_pixels` 等待并读取。没有常驻线程、轮询或自动重绘循环。

尺寸改变时先等待并释放旧目标及读回缓冲，再申请替换，因此 resize 失败会使旧图像失效；字形图集仍保留。同尺寸复用分配。`release_images` 等待并释放目标、clip buffer、读回、CPU 记录，以及文字图集/上传/CPU 字形缓存，保留设备及 pipeline 供后续使用。

`draw_clipped(scene, transform, Option<Rect>)` 的外部矩形位于设备坐标，不受节点 transform 再次变换；它与 Scene 内部裁剪相交，且不泄漏到下一次 draw。对接 Ui::visit_scenes 时，宿主需要将窗口逻辑平移和祖先裁剪一起转换为设备坐标。

输入颜色是非预乘 sRGB；混合在线性空间完成。`read_pixels` 交付紧密排列、自顶向下的预乘 sRGB RGBA8，尺寸必须恰好等于 `width × height × 4`。带透明度的结果不能直接当作透明 PNG 的非预乘像素。示例使用不透明底色，输出 PPM 时可直接取 RGB。

第一遍在 RGBA16_SFLOAT 中按预乘线性 SourceOver 混合；第二遍用 textureLoad 读取，解预乘、编码 sRGB、再次预乘，写入 RGBA8_UNORM。直接使用 sRGB attachment 不能产生约定的透明预乘 sRGB 字节，因此本阶段保留这两张目标图像。软件每次绘制量化，GPU 在输出阶段量化，两者允许少量内部颜色差异。

## 分段提交

一次提交最多 `aegle_gpu::MAX_PRIMITIVES`（16,384）个图元，即 1.75 MiB 图元记录；图元数到达上限、图集或纹理绑定在本次提交内用尽、或 CPU 上传达到 `upload_bytes` 时，帧在中途提交已记录部分，CPU 等待 fence 后清空图元、解除图集页固定与纹理绑定，再继续录制，与 wgpu 后端相同。第一次提交照常清屏，后续提交使用只在 load 操作与初始布局上不同的兼容 render pass 载入线性目标（直接 sRGB 窗口则载入 swapchain 图像），只有最后一次运行编码通道。窗口帧在第一次提交时获取 swapchain 图像，最后一次提交后才呈现，因此合成器只看到完整帧。裁剪行保留整帧，后续提交仍可引用；其数量随场景的裁剪命令增长，不受图元上限约束。拆分不改变像素：各段按记录顺序写入同一目标。中途失败的帧使 swapchain 重建，弃帧不呈现。

非法尺寸、非有限外部裁剪、裁剪超过后端上限、未支持命令及资源预算不足均返回错误。失败或未提交的 Frame 丢弃后必须能开始新的帧；不能提交失败帧中的部分绘制作为完整结果。设备丢失须报告，不能静默切换到软件后端。

## 预算与可观察性

Options 包含可选 `device_index` 和 `memory_budget`；默认分别自动选择设备与 16 MiB 显式设备分配上限。CPU 图元记录由分段提交约束在 1.75 MiB 以内，不再单独配置。统计由 `stats().device_bytes` 与 `stats().recording_bytes` 提供，设备名通过 `device_name()` 查询。

设备预算包含两张目标图像、clip buffer、文字图集/上传和按需读回缓冲的实际 VkMemoryRequirements 分配大小，包括驱动要求的对齐；无读回时不分配读回 buffer。`recording_bytes` 报告图元/裁剪两个 Vec 的 capacity，清空帧时保留容量复用。预算不足明确报错，不靠额外在途帧扩大分配。

启用 text 后，`Options.text` 独立配置默认512×512页、最多8页/4096字形条目、最多1 MiB CPU上传 capacity，以及原有 GlyphCache 限额。文字页按需分配，8页不是初始化时分配8张图；mask和color共用页数上限。上传 region 数量不超过字形条目上限；哈希表有空槽，变化轴最多64个i16/条目。`text_stats` 报告页/条目数、实际图像与staging分配、上传capacity、表容量、CPU缓存和raster_requests计数；它与总体stats部分重叠，不能直接相加。空白/未入图集字形可再次查询CPU缓存，raster_requests不等于实际重新光栅化次数。

这两个计量不是进程总内存：pipeline、command pool 等驱动内部对象、loader/驱动映射、调用方持有的 Scene、读回目标 Vec 和 allocator 元数据仍是额外成本。不能以显式设备分配或发布文件大小代替 PSS/峰值验收。

## 执行验证

综合测试位于 `crates/aegle-render-vulkan/tests/render.rs`，默认 ignored，必须在明确选择的 Vulkan ICD/设备上主动运行：

```sh
cargo test -p aegle-render-vulkan --test render -- --ignored --nocapture
cargo run -p aegle-render-vulkan --example geometry -- target/aegle-vulkan.ppm
cargo test -p aegle-render-vulkan --features text --test text -- --ignored --nocapture
cargo run -p aegle-render-vulkan --features text --example vulkan_text_scene -- target/aegle-vulkan-text.ppm
```

测试覆盖不透明/透明线性混合、外部裁剪与嵌套旋转、居中描边、draw 作用域隔离、非法尺寸、设备/记录预算、过深裁剪、不支持的文字、失败帧禁止提交，以及 resize/释放/重新创建。抽样避开后端抗锯齿边缘，颜色允许两级 RGBA8 量化误差；透明混合另与现有软件路径对照。

geometry 示例只用标准库写 PPM，不增加 PNG 运行依赖。示例能生成图片不等于 native swapchain 接入，也不证明嵌入式帧耗时、空闲 CPU 或 PSS 达标。Lavapipe 的结果属于 Vulkan 软件驱动验证，硬件 GPU 必须单列设备与执行结果；正式证据汇总见[实现状态](implementation.md)。

`--features text --test vector` 在 page_size 64 下对照软件渲染器：图像 1:1 纹素精确，缩放图像、偶奇填充星形与圆头二次/三次描边的平均通道差小于 0.5；同时检查整像素平移复用、旋转后新增条目和专用大页。

文字综合场景另验证CJK、水平相位、几何/文字顺序、仿射/clip、透明COLRv0和PNG字形，与软件像素比较允许3级通道量化误差；覆盖小CPU缓存下的GPU命中、脏页取消恢复、整页淘汰、工作集/字号错误和释放重建。vulkan_text_scene使用带OFL许可的测试子集展示三种CJK文字；库自身仍不内嵌字体。

## 原生窗口生命周期与预算

`unsafe WindowRenderer::new(owner, options)` 接受实现 raw-window-handle 的原生租约；调用方必须保证同一 window/display 在 renderer 销毁前一直有效，并遵守平台线程规则。Wayland/Win32 的 `WindowSurface` 保留原生对象，平台逻辑关闭不提前破坏 GPU 句柄。构造窗口 renderer 不创建新事件循环。

`begin_frame(width, height, clear)` 在零尺寸时释放尺寸相关目标并返回 None；同尺寸复用，否则等待设备/呈现队列后释放旧 swapchain 并重建。Frame::extent 返回实际 extent。帧的第一次提交（通常是 Frame::finish，大帧为第一段）才获取图像并等待 acquire fence，最后一次提交后 present；未提交过的弃帧不获取图像，已分段提交后失败或丢弃的帧使 swapchain 在下一帧重建。SurfaceOutOfDate 保持 dirty 等待重试，DeviceLost/SurfaceLost 返回错误，不隐式切换后端。当前 graphics/present 必须为同一 queue。 `Frame::set_damage(&[PixelRect])` 声明本帧相对上一次呈现改变的设备像素矩形：设备提供 `VK_KHR_incremental_present` 时窗口渲染器启用它，并在 present 时附上 `VkPresentRegionsKHR`，合成器只需更新这些区域；帧本身仍整幅绘制。新 swapchain 的第一次呈现、没有该扩展或未调用时上报整个 surface。

呈现使用 FIFO 与 SRGB_NONLINEAR。默认 `Options::transparent = false`：表面提供 BGRA8/RGBA8_SRGB 时，几何与文字直接在 sRGB swapchain 图像上由硬件线性混合，没有 RGBA16F 目标和编码 pass；优先 OPAQUE composite alpha，并要求不透明清屏色。`transparent = true` 或表面只有 UNORM 时，保留 RGBA16F 线性目标并由编码 pass 写入 UNORM swapchain，优先 PRE_MULTIPLIED，否则选择 OPAQUE 并拒绝非不透明清屏色。两条路径都不保留第二张 RGBA8 离屏目标、不经 CPU 读回或 SHM 转送。未指定设备时优先集成 GPU，其次独显、虚拟设备和 CPU 驱动。同一应用的窗口共用一个 `SharedDevice`（实例与逻辑设备），一次 graphics submission 在途；render-finished semaphore 按 swapchain image 保存，重建等待 device idle，不能把 graphics fence 当作 presentation 完成证明。

`Stats.swapchain_bytes` 为 extent×4×驱动返回的实际图像数估计，只单列报告，不计入 memory_budget：swapchain 图像由驱动/窗口系统分配，应用无法控制或缩小，WSI 也不暴露其实际 memory requirements，不声称此值包含驱动分配/压缩/对齐。memory_budget 只约束 device_bytes 报告的显式分配（目标图像、缓冲、读回与图集）；驱动内部资源和线程另计。各窗口渲染器的图集与目标仍独立，尚无跨窗口共享 GPU 缓存。

原生综合场景位于 tests/native.rs，默认 ignored；需隔离 Wayland compositor，设置 AEGLE_TEST_COMPOSITOR=private。App 的同一个 native 场景通过 AEGLE_TEST_VULKAN=1 切换渲染器，覆盖两个 CJK 窗口、回调、关闭和句柄失效。具体设备证据见实现状态。
