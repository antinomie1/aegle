# Vulkan 离屏绘制后端

状态：独立离屏几何与可选文字已实现，2026-10-05。已在 AMD RX 6800 XT 和 Lavapipe 上执行综合测试及 Khronos 验证层检查。当前 App 仍使用 Wayland 软件呈现，尚无 Vulkan 窗口或 GPU 加速的完整 GUI。

## 范围与依赖

`aegle-render-vulkan` 消费现有不可变 Scene，与软件后端共用颜色、变换和裁剪语义；不拥有控件树、Taffy、字体系统或窗口循环。默认提供实色矩形、统一圆角、居中边框、二维仿射变换与嵌套裁剪。可选 `text` 接受同一 Scene 的定位字形，依赖 glyph/scene 和现有 hashbrown；不依赖 Parley。未启用 text 时文字命令返回不支持错误，不允许跳过字形后报告成功。

以 Vulkan 1.1 为基线，复用 ash 0.38 调用驱动，bytemuck 1.25 检查 CPU/shader 数据布局；Naga 30 仅在构建期将 WGSL 转为 SPIR-V 1.3，不引入 wgpu 或运行时 shader 编译器。使用传统 render pass、一个 graphics queue 和可复用 fence，不要求可选 GPU feature。

每个图元一条 draw，顶点由 shader 生成；目前没有相邻批次合并。CPU 只记录变换、颜色和裁剪链，不栅格化整帧。整数 scissor 缩小工作范围，shader 计算小数、圆角和仿射裁剪覆盖率，最多八层（含外部 clip）；共享父索引避免为每个图元复制整条裁剪链。局部几何等比归一化后传入 shader，避免极大/极小局部单位造成距离计算溢出。映射后的几何范围限定为 ±1,048,576，不能表示的逆变换返回错误。

绘制遵守记录顺序。解析覆盖率使用 shader 导数估计边缘，不承诺与软件覆盖率逐像素相同；内部实色、颜色混合和裁剪边界分别验证。

本阶段不包含原生 surface/swapchain、显示呈现、通用图像资源或特效。后续接入真实平台时复用既有事件循环、IME 与无障碍，不建立另一套 UI。

## 可选文字与图集

软件和 Vulkan 通过 `aegle-glyph/scene` 共用 RasterTransform：正轴向均匀缩放采用 hinting 与四分之一像素相位，其他仿射按最大列长选择光栅字号，再过滤采样。完整 GlyphKey 包含字体资源身份、face、glyph、字号、相位、hint、变化轴及必要前景；不把单个 hash 或 CPU LRU 槽位作为身份。灰度忽略前景色，彩色前景层使用不透明 run RGB，run alpha 在 GPU 上只应用一次。

图集先查询自身完整身份，命中不访问 CPU 字形缓存；缺失才通过共享 GlyphCache 按需光栅化。R8_UNORM 灰度与 RGBA8_SRGB 彩色页独立使用简单 shelf 排布，每个字形保留一像素透明边。新建/重置页由 GPU 清零，CPU 只上传紧密字形补丁，不保留页大小的 CPU 镜像。彩色上传先在线性空间预乘，再编码 RGB；纹理硬件解码、插值后得到正确线性预乘值，避免透明彩色边缘出现色晕。灰度乘前景，彩色保留调色板颜色。

页数或条目达到上限时淘汰最旧的未使用页；当前帧已用页固定，不能覆盖正在录制的字形。保守像素支持范围与祖先 clip 不相交的字形不入图集；任意变换/圆角的边界筛选仍为保守估计，不扫描字形像素来判断完全透明。工作集无法容纳返回 AtlasFull，单字形连透明边超过页尺寸返回 GlyphTooLarge，均不静默漏字。单页放不下的大字号需调用方显式调大页尺寸。

新增字形在成功提交后才视为已上传。取消或失败帧的脏页在下一帧清掉对应条目并重置，避免命中未上传内容；此前同页的有效条目也会失效。GPU 上传缓冲在 fence 完成时释放，CPU 上传容量保留复用；页图像在安全等待后才替换。空字形无需图集页，可复用 CPU 缓存。

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

非法尺寸、非有限外部裁剪、裁剪超过后端上限、未支持命令及资源预算不足均返回错误。失败或未提交的 Frame 丢弃后必须能开始新的帧；不能提交失败帧中的部分绘制作为完整结果。设备丢失须报告，不能静默切换到软件后端。

## 预算与可观察性

Options 包含可选 `device_index`、`memory_budget` 和 `recording_budget`；默认分别自动选择设备、16 MiB 显式设备分配上限、1 MiB 记录上限。统计由 `stats().device_bytes` 与 `stats().recording_bytes` 提供，设备名通过 `device_name()` 查询。

设备预算包含两张目标图像、clip buffer、文字图集/上传和按需读回缓冲的实际 VkMemoryRequirements 分配大小，包括驱动要求的对齐；无读回时不分配读回 buffer。记录预算计算 draw/clip 两个 Vec 的 capacity，清空帧时保留容量复用。预算不足明确报错，不靠额外在途帧扩大分配。

启用 text 后，`Options.text` 独立配置默认512×512页、最多8页/4096字形条目、最多1 MiB CPU上传 capacity，以及原有 GlyphCache 限额。文字页按需分配，8页不是初始化时分配8张图；mask和color共用页数上限。上传 region 数量不超过字形条目上限；哈希表有空槽，变化轴最多64个i16/条目。`text_stats` 报告页/条目数、实际图像与staging分配、上传capacity、表容量、CPU缓存和raster_requests计数；它与总体stats部分重叠，不能直接相加。空白/未入图集字形可再次查询CPU缓存，raster_requests不等于实际重新光栅化次数。

这两个计量不是进程总内存：pipeline、command pool 等驱动内部对象、loader/驱动映射、调用方持有的 Scene、读回目标 Vec 和 allocator 元数据仍是额外成本。不能以显式设备分配或发布文件大小代替 PSS/峰值验收。

## 执行验证

综合测试位于 `crates/aegle-render-vulkan/tests/render.rs`，默认 ignored，必须在明确选择的 Vulkan ICD/设备上主动运行：

```sh
cargo test -p aegle-render-vulkan --test render -- --ignored --nocapture
cargo run -p aegle-render-vulkan --example geometry -- /tmp/aegle-vulkan.ppm
cargo test -p aegle-render-vulkan --features text --test text -- --ignored --nocapture
cargo run -p aegle-render-vulkan --features text --example text_scene -- /tmp/aegle-vulkan-text.ppm
```

测试覆盖不透明/透明线性混合、外部裁剪与嵌套旋转、居中描边、draw 作用域隔离、非法尺寸、设备/记录预算、过深裁剪、不支持的文字、失败帧禁止提交，以及 resize/释放/重新创建。抽样避开后端抗锯齿边缘，颜色允许两级 RGBA8 量化误差；透明混合另与现有软件路径对照。

geometry 示例只用标准库写 PPM，不增加 PNG 运行依赖。示例能生成图片不等于 native swapchain 接入，也不证明嵌入式帧耗时、空闲 CPU 或 PSS 达标。Lavapipe 的结果属于 Vulkan 软件驱动验证，硬件 GPU 必须单列设备与执行结果；正式证据汇总见[实现状态](implementation.md)。

文字综合场景另验证CJK、四相位、几何/文字顺序、仿射/clip、透明COLRv0和PNG字形，与软件像素比较允许3级通道量化误差；覆盖小CPU缓存下的GPU命中、脏页取消恢复、整页淘汰、工作集/字号错误和释放重建。text_scene使用带OFL许可的测试子集展示三种CJK文字；库自身仍不内嵌字体。
