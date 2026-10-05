# Vulkan 几何后端

状态：独立离屏几何已实现，2026-10-05。已在 AMD RX 6800 XT 和 Lavapipe 上执行综合测试及 Khronos 验证层检查。当前 App 仍使用 Wayland 软件呈现，尚无 Vulkan 窗口、GPU 文字或 GPU 加速的完整 GUI。

## 范围与依赖

`aegle-render-vulkan` 消费现有不可变 Scene，与软件后端共用颜色、变换和裁剪语义；不拥有控件树、Taffy、字体系统或窗口循环。当前范围是实色矩形、统一圆角、居中边框、二维仿射变换与嵌套裁剪。文字命令必须返回不支持错误，不允许跳过字形后报告成功。

以 Vulkan 1.1 为基线，复用 ash 0.38 调用驱动，bytemuck 1.25 检查 CPU/shader 数据布局；Naga 30 仅在构建期将 WGSL 转为 SPIR-V 1.3，不引入 wgpu 或运行时 shader 编译器。使用传统 render pass、一个 graphics queue 和可复用 fence，不要求可选 GPU feature。

每个图元一条 draw，顶点由 shader 生成；目前没有相邻批次合并。CPU 只记录变换、颜色和裁剪链，不栅格化整帧。整数 scissor 缩小工作范围，shader 计算小数、圆角和仿射裁剪覆盖率，最多八层（含外部 clip）；共享父索引避免为每个图元复制整条裁剪链。局部几何等比归一化后传入 shader，避免极大/极小局部单位造成距离计算溢出。映射后的几何范围限定为 ±1,048,576，不能表示的逆变换返回错误。

绘制遵守记录顺序。解析覆盖率使用 shader 导数估计边缘，不承诺与软件覆盖率逐像素相同；内部实色、颜色混合和裁剪边界分别验证。

本阶段不包含原生 surface/swapchain、显示呈现、字形图集、图像资源或特效。后续接入真实平台时复用既有事件循环、IME 与无障碍，不建立另一套 UI。GPU 文字应复用 aegle-glyph 的按需缓存，避免预装 CJK 字库或新建字体栅格器。

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

尺寸改变时先等待并释放旧目标及读回缓冲，再申请替换，因此 resize 失败会使旧图像失效。同尺寸复用分配。`release_images` 等待并释放目标、clip buffer、读回和 CPU 记录容量，保留设备及 pipeline 供后续使用。

`draw_clipped(scene, transform, Option<Rect>)` 的外部矩形位于设备坐标，不受节点 transform 再次变换；它与 Scene 内部裁剪相交，且不泄漏到下一次 draw。对接 Ui::visit_scenes 时，宿主需要将窗口逻辑平移和祖先裁剪一起转换为设备坐标。

输入颜色是非预乘 sRGB；混合在线性空间完成。`read_pixels` 交付紧密排列、自顶向下的预乘 sRGB RGBA8，尺寸必须恰好等于 `width × height × 4`。带透明度的结果不能直接当作透明 PNG 的非预乘像素。示例使用不透明底色，输出 PPM 时可直接取 RGB。

第一遍在 RGBA16_SFLOAT 中按预乘线性 SourceOver 混合；第二遍用 textureLoad 读取，解预乘、编码 sRGB、再次预乘，写入 RGBA8_UNORM。直接使用 sRGB attachment 不能产生约定的透明预乘 sRGB 字节，因此本阶段保留这两张目标图像。软件每次绘制量化，GPU 在输出阶段量化，两者允许少量内部颜色差异。

非法尺寸、非有限外部裁剪、裁剪超过后端上限、未支持命令及资源预算不足均返回错误。失败或未提交的 Frame 丢弃后必须能开始新的帧；不能提交失败帧中的部分绘制作为完整结果。设备丢失须报告，不能静默切换到软件后端。

## 预算与可观察性

Options 包含可选 `device_index`、`memory_budget` 和 `recording_budget`；默认分别自动选择设备、16 MiB 显式设备分配上限、1 MiB 记录上限。统计由 `stats().device_bytes` 与 `stats().recording_bytes` 提供，设备名通过 `device_name()` 查询。

设备预算包含两张目标图像、clip buffer 和按需读回缓冲的实际 VkMemoryRequirements 分配大小，包括驱动要求的对齐；无读回时不分配 transfer buffer。记录预算计算 draw/clip 两个 Vec 的 capacity，清空帧时保留容量复用。预算不足明确报错，不靠额外在途帧扩大分配。

这两个计量不是进程总内存：pipeline、command pool 等驱动内部对象、loader/驱动映射、调用方持有的 Scene、读回目标 Vec 和 allocator 元数据仍是额外成本。不能以显式设备分配或发布文件大小代替 PSS/峰值验收。

## 执行验证

综合测试位于 `crates/aegle-render-vulkan/tests/render.rs`，默认 ignored，必须在明确选择的 Vulkan ICD/设备上主动运行：

```sh
cargo test -p aegle-render-vulkan --test render -- --ignored --nocapture
cargo run -p aegle-render-vulkan --example geometry -- /tmp/aegle-vulkan.ppm
```

测试覆盖不透明/透明线性混合、外部裁剪与嵌套旋转、居中描边、draw 作用域隔离、非法尺寸、设备/记录预算、过深裁剪、不支持的文字、失败帧禁止提交，以及 resize/释放/重新创建。抽样避开后端抗锯齿边缘，颜色允许两级 RGBA8 量化误差；透明混合另与现有软件路径对照。

geometry 示例只用标准库写 PPM，不增加 PNG 运行依赖。示例能生成图片不等于 native swapchain 接入，也不证明嵌入式帧耗时、空闲 CPU 或 PSS 达标。Lavapipe 的结果属于 Vulkan 软件驱动验证，硬件 GPU 必须单列设备与执行结果；正式证据汇总见[实现状态](implementation.md)。
