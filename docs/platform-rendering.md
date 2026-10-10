# 平台与绘制

状态：v0.1 设计决定。平台窗口与绘制后端分别选择，通过显式窗口资源与生命周期契约连接。

## 支持目标

| 平台 | 首版目标 | 绘制与输入 |
| --- | --- | --- |
| Linux | x86_64/aarch64，glibc 2.36+、内核 6.1+；Wayland | 软件绘制（默认）、可选 ash/Vulkan 与 wgpu；SCTK + wayland-client；text-input-v3 |
| Windows | Windows 11，x86_64/aarch64 | 软件绘制（默认）、可选 ash/Vulkan 与 wgpu；原生窗口；TSF 文本存储输入法 |
| macOS | macOS 13+，x86_64/aarch64，存在可用 Metal 设备 | 绘制经可选 wgpu 使用 Metal，窗口平台未实现；AppKit、NSTextInputClient |

这些是验收目标，不是已经验证所有 OS/GPU 组合。Linux 不实现 X11；基础普通窗口要求 xdg-shell，shell 表面另要求 layer-shell。没有对应协议时报告明确能力缺失，不假造成功。无 GPU 设备通过独立软件 renderer 支持；与 GPU 共用 scene、布局、文字和输入，不建立第二套 UI。软件像素缓冲由各平台呈现，按可用后端显式选择。

Linux 使用一个连接和事件队列承载普通窗口及可选 layer-shell，补齐预编辑、提交、周边文字/删除、焦点和提交时序。SCTK 的 input_method 面向输入法程序，不代替普通客户端 text-input-v3。默认不会引入 winit 再维护另一套窗口循环。

各平台按事件等待；Linux 使用 FD readiness 和协议要求的读取流程，Windows/AppKit 保留原生消息循环。文本输入、系统偏好、剪贴板和无障碍动作归一化后交给 UI 主线程，不跨 FFI 展开 panic。

## 绘制契约

Vulkan 最低 API 1.1，采用传统 render pass、普通 descriptor、fence 与 binary semaphore；不要求 descriptor indexing、timeline semaphore 或 dynamic rendering。还必须检查 graphics/present queue、swapchain、surface format 和所用纹理格式能力。

可选 wgpu 后端是全平台通用的第二条 GPU 路径，经 wgpu 选择 Vulkan、Metal 或 Direct3D 12，shader 以 WGSL 在运行时交给 wgpu；原生 Metal 后端方案已放弃。Vulkan 的 shader 在构建期编译为 SPIR-V，开发工具不进入运行依赖。两个小型二维实现共用 Scene，不建立通用三维 RHI；契约见 [wgpu](wgpu.md)。

基础绘制目标包括文字图集、RGBA 图像、矩形/圆角/边框、透明度、二维仿射变换和裁剪。颜色在线性空间混合，输出转换遵循目标色彩格式。默认字形用灰度抗锯齿，不依赖 LCD 子像素排列。

具体颜色边界：调用方和 scene 的 `Color` 为非预乘 sRGB RGBA8；renderer 在合成时解码并预乘，输出遵循目标格式。软件缓冲为预乘 sRGB RGBA8，不能将其直接当作线性颜色或透明 PNG 的非预乘颜色使用。

GPU 路径的轴对齐裁剪用整数范围，圆角与小数裁剪由 shader 解析，最多 8 层，超过时返回明确错误。批次合并只处理相邻绘制，不做复杂全局重排。任意路径不做三角细分或 stencil：两个后端都在 CPU 上把路径光栅化为覆盖率 mask，软件直接用 tiny-skia，Vulkan 用 swash 已依赖的 zeno 生成 R8 mask 存入图集并缓存；路径不能作为裁剪。软件裁剪使用下文的覆盖率 mask。

高级效果包括渐变、阴影和区域模糊。当前已实现渐变填充与解析式柔和阴影（见下文 scene 支持范围），它们无需离屏纹理，三个后端始终可用；组透明度与背景模糊经离屏层实现（见下文“图层”）。模糊只作用于本窗口已绘制的内容，不承诺跨平台桌面背景模糊。超出特效预算的背景模糊被跳过并计数（`skipped_blurs()`），层本身超预算则返回错误，不会静默丢弃内容。默认组件不依赖这些特效保持可用。

原生 GPU 呈现目标为默认 FIFO、最多两帧在途；有像素变化才请求新帧；GPU 路径每次绘制完整窗口，Vulkan 在支持 `VK_KHR_incremental_present` 时只向合成器上报变化区域；软件路径只重绘并上报变化区域（见[资源](resources.md)）。四条路径的像素结果相同，差别只在重绘与上报的范围。当前离屏 Vulkan 只保留一次在途提交。后台上传和临时资源遵守[资源预算](resources.md)。DeviceLost 停止相关提交并报告，首版不隐藏地切换后端。

三个后端的能力差异如下，没有运行时能力查询：差异在编译期（feature）或创建时（所选后端）已确定，未支持的命令使帧返回错误，不静默跳过。

| | 软件 | Vulkan | wgpu |
| --- | --- | --- | --- |
| 应用纹理（`SceneBuilder::texture`） | 不支持，帧返回 `UnsupportedCommand` | 支持（独立使用需 `text`） | 支持（独立使用需 `text`） |
| 文字、图像、路径 | 支持 | 需 `text` feature | 需 `text` feature |
| 窗口透明 | 不支持：原生软件窗口不透明 | `Options::transparent` 且合成器支持预乘 alpha | 同 Vulkan |
| 重绘与上报范围 | 只重绘并上报变化区域 | 整幅重绘，有 `VK_KHR_incremental_present` 时上报变化区域 | 整幅重绘并上报整个 surface |
| 大帧 | 不分段 | 每 16,384 个图元或图集/纹理绑定/上传用尽时分段提交 | 每 16,384 个图元或图集用尽时分段提交 |
| 设备故障 | 无设备 | Vulkan 错误（如 `ERROR_DEVICE_LOST`）返回给调用方 | `Error::Gpu`/`Error::DeviceLost`，此后拒绝工作 |

原生 App 总是为 GPU renderer 启用 `text`。应用纹理是否可用即 `App::wgpu()` 或 `App::vulkan()` 是否返回设备。原生窗口的背景（根主题的 `background`）在软件后端必须不透明，App 在交给平台前检查一次并返回错误；GPU 后端由各自的 `begin_frame` 对不透明 surface 做同样检查。渐变与阴影在 WGSL（两个 GPU 后端共用）和软件 Rust 中各有一份公式，两个 GPU 后端的 `tests/effects.rs` 逐像素对照软件结果。

## 坐标与平台差异

内部使用逻辑 dp、统一二维变换；平台层转换为物理像素和平台坐标，整数边界向外取整。命中、IME 光标矩形和语义位置共享几何来源。

Wayland 通常不向普通客户端公开全局窗口位置。平台能力必须区分窗口局部坐标与可取得的屏幕原点，不能编造全局坐标；对应辅助技术的屏幕定位能力受 compositor 和 adapter 限制，但角色、文本、焦点和动作仍需正确。具体支持矩阵通过真实 compositor 验收。

首版 shell 只含面板/背景层、输出绑定、锚点、exclusive zone 与键盘交互策略；托盘、通知、全局快捷键等桌面服务不进入平台窗口模块，由独立的 `aegle-desktop` 提供（见[模块](modules.md)）。

## 当前软件绘制接口

已实现部分以 `SceneBuilder → Scene → Renderer::begin_frame → Frame::draw` 连接。录制时非有限、负尺寸或变换后超出 `f32` 范围的几何以及不配对的作用域是调用方的程序错误，`SceneBuilder` 的方法、`RoundedRect::new`、`Affine::translation`/`scale`、`PathBuilder::finish` 与 `GlyphRun::new` 以 `SceneError` 的消息 panic；校验外部数据的构造（`Image::new`、渐变、`Affine::new`、`Layer::new`）仍返回 `SceneError`。`Scene` 为不可变局部绘制记录；重新构建时可以取回并清空 builder，复用命令分配。每次 draw 传入布局位置与设备缩放组成的 Affine，移动控件不必重建其局部图元。场景的变换/裁剪作用域只在本次 draw 内生效，不泄漏到其他记录。

`Frame::draw_clipped(scene, transform, clip)` 接受可选设备坐标矩形，非有限或负尺寸的矩形 panic，超出设备坐标范围返回错误；矩形不随节点 transform 再次变换，并与 scene 内部裁剪相交。它复用既有 mask 路径，增加一层预算；调用结束不影响后续 scene。Ui 的 `visit_scenes` 返回局部记录、窗口逻辑平移及可选祖先矩形；宿主必须同时缩放平移和裁剪，不能丢弃第三个参数。原生软件宿主已接入。多个 ScrollView 的祖先矩形先求交，所以嵌套滚动本身只增加一个外部 mask 层；控件自身的圆角裁剪另外计层。

目前支持实色矩形、统一圆角、居中边框、共享 RGBA 图像、填充/描边路径、二维仿射变换及嵌套矩形/圆角裁剪；可选 `text` 支持定位后的字形。`fill_gradient(shape, &Gradient)` 用线性或圆形渐变（2–16 个非递减色标，两端外延）填充圆角矩形，色标间在预乘线性光中插值；`shadow(shape, color, blur)` 绘制形状与标准差为 blur 的高斯卷积：x 方向用 erf 精确积分，y 方向取四行并按各区间的高斯质量加权（直角形状精确，圆角处近似，参照 Evan Wallace 的闭式解），三倍标准差之外不绘制。偏移与扩展由调用方移动或放大 shape。软件与 GPU 在像素中心用同一公式求值。builder 检查非有限几何、负尺寸、不可逆变换及作用域配对，总嵌套最多 64 层。空形状不绘制，空 clip 排除绘制；软件后端拒绝超出 ±1,048,576 的设备路径/字形坐标。Wayland 原生软件呈现与输入已接入；独立 Vulkan 几何范围见下节。

`Image` 是非预乘 sRGB RGBA8，单边 1..=16384，clone 共享像素；`Path` 由 move/line/quad/cubic/close 组成并带填充规则，`PathBuilder::finish` 一次检查顺序与有限值。两者带进程内唯一 id，renderer 以此缓存，不比较内容。图像拉伸到目标矩形，在线性预乘空间双线性过滤，采样钳制到边缘纹素，矩形边缘按解析覆盖率抗锯齿，放大时边界不向外渐隐；单位缩放且整像素对齐时直接复制纹素。没有 mipmap，大幅缩小会走样，调用方应提供接近显示尺寸的图像。描边居中、宽度为局部单位，cap/join 可选，miter 上限固定为 4；零宽描边和无线段路径不录入命令。两个后端的曲线细分和描边偏移近似不同，边缘可差零点几像素。

scene 字形 run 保存共享字体句柄、字号、变化轴、前景色及基线位置；`aegle-text/scene` 可从保留段落生成它们。软件 `text` 必须显式启用：如果 Cargo 统一开启了 scene/text 而 renderer/text 关闭，遇到字形命令返回 `UnsupportedCommand`，不能跳过文字后假称成功。

保留 `Editor` 也使用同一文字绘制函数；选区背景、预编辑下划线和 caret 从当前排版生成几何，再录入普通 scene 命令。宿主负责焦点/闪烁时机、滚动与裁剪，模块没有自己的定时器。候选窗使用 `ime_rect()` 的未裁剪局部文字几何；长单行文字溢出字段宽度时不能只截断右边缘而产生负宽度。原生平台 adapter 应统一应用呈现/滚动变换后再转换坐标。

正向、轴对齐、均匀缩放的文字在设备字号光栅化，基线取整到设备像素、水平原点按四分之一像素相位取整，排版 advance 保持原值；灰度覆盖率经与前景亮度相关的共用对比曲线，平衡线性混合下深色/浅色文字的粗细；旋转、反射、斜切和非均匀缩放通过逆变换双线性采样。彩色字形在线性预乘空间过滤，字形接口输出非预乘 sRGB，合成后仍遵守 `Surface` 的预乘 sRGB 格式。前景透明度只应用一次。灰度 glyph 与前景色分离，颜色主题更新可复用字形缓存；彩色格式范围与错误见[文字](text-input.md)。

`Surface` 借用紧密排列、从上到下的 RGBA8（`Surface::new`）或 BGRA8（`Surface::new_bgra`）字节切片，要求长度准确等于宽×高×4。BGRA8 是小端 Wayland XRGB8888 与 Windows 32 位 DIB 的内存顺序，原生软件宿主直接画进平台缓冲，不再逐像素转换；renderer 在生成颜色处（纯色、渐变色标、图像与彩色字形纹素、阴影色）交换红蓝，混合按通道独立进行，两种顺序的结果只差红蓝互换。大端 Wayland 目标在呈现前把区域内像素反转为 XRGB。begin_frame 清空整帧；`begin_region(surface, clear, rect)` 只清空并绘制向外取整到整像素的矩形，其余像素保留，供保留上一帧的宿主局部重绘，矩形外的绘制不需要 mask；损伤有多个矩形时宿主逐个调用。draw 按调用次序合成保留记录；无像素变化时宿主不调用绘制。错误可能发生在部分像素已更新之后，失败帧不得呈现。

tiny-skia 仅负责几何覆盖率。线性光合成使用约 8 KiB 的共享、按需初始化转换表，避免逐像素幂运算；每次绘制量化到 RGBA8，完全不透明覆盖直接复制。透明背景、半透明叠加与旋转已有像素级验证。裁剪 mask 的预算、复用与释放见[资源](resources.md)。

## 图层

`Frame::push_layer(&Layer)` 与 `pop_layer()` 在三个后端语义一致。`Layer` 记录形状（圆角矩形加变换）、内容范围、可选设备裁剪、不透明度和背景模糊标准差。push 后的绘制进入一张覆盖范围（与裁剪求交）的线性图像，pop 时以不透明度 source-over 合成到下层，仍受裁剪限制。可嵌套；帧结束时仍打开的层先依次合成。软件 `Frame` 在 drop 时同样合成，所以读取表面前要先结束 frame。

背景模糊为正时，push 先对已绘制内容做模糊，按层形状和裁剪、以层不透明度画回，再开始绘制层内容。模糊采用 W3C 三次盒式近似：`d = floor(σ·3√(2π)/4 + 0.5)`，上限 4095。d 为奇数时用三个居中的盒；为偶数时用 (左 d/2, 宽 d)、(左 d/2−1, 宽 d)、(左 d/2, 宽 d+1)。先横向三遍再纵向三遍，边缘夹取。σ 先乘层变换的 √|det|。软件后端在线性光预乘下计算；GPU 端按 `aegle-gpu` 的计划，把采样区复制到暂存图像后跑六遍 `blur.wgsl`，参数编码在实例索引中。

GPU 合成复用 text 的图像管线，因此 wgpu 与 Vulkan 的图层需要 `text` feature，未启用时 `push_layer` 返回 `UnsupportedCommand`。Vulkan 在每个层边界提交一次并等待。直接写 swapchain 的路径需要 `TRANSFER_SRC` 才能复制背景，因此创建 swapchain 时只要表面支持就请求该用法；表面不支持时，该路径上的背景模糊返回 `Unsupported` 错误。三个后端的离屏层测试（`tests/layers.rs`）比较同一场景：GPU 与软件结果仅在抗锯齿边缘处不同，平均通道差 RX 6800 XT 为 0.128，lavapipe 为 0.104。窗口路径由 `aegle-app/tests/native.rs` 的 `layers_draw_into_native_windows`（私有 headless Sway，`AEGLE_TEST_COMPOSITOR=private`）覆盖：条纹上一块半透明、σ=4 背景模糊的圆角面板，内含组透明度 0.5 的子树，分别以软件、Vulkan（直接写 sRGB swapchain，复制 swapchain 图像做模糊）与 wgpu 呈现；grim 截图与软件窗口相比，RX 6800 XT 上 Vulkan/wgpu 的平均通道差为 0.055/0.040，lavapipe 上为 0.071/0.034，超过 8 级的像素只有面板圆角上的 21–27 个。

每个后端的代价由 `{software,vulkan,wgpu}_layer_cost` 示例实测（`cargo run --release -p aegle-render-<后端> [--features text] --example <后端>_layer_cost`）：1280×800 帧铺满条纹，再加一个 480×320 圆角卡片的组透明度层，或在其下加 σ=8 背景模糊。i5-13600KF 与 RX 6800 XT（RADV）上的中位数：

| 后端 | 无层 | 组透明度层 | 加背景模糊 | 说明 |
|---|---|---|---|---|
| 软件 | 0.46 ms | 5.2 ms | 10.8 ms | 层内半透明填充走非不透明目标的逐像素路径，合成按 8 通道向量化；直接把同一卡片画到窗口约 2.7 ms，层约为其两倍。模糊在线性光 f32 中做六遍盒式 |
| Vulkan | 9.1 ms | +0.11 ms | +0.37 ms | 含读回；基线主要是不带 HOST_CACHED 的读回内存，窗口路径没有这一步。每个层边界提交一次并等待 |
| wgpu | 0.50 ms | +0.04 ms | +0.17 ms | 含读回 |
| Vulkan / wgpu（lavapipe） | 3.2 / 3.5 ms | 5.2 / 5.5 ms | 12.7 / 13.4 ms | CPU 实现，与软件后端同一量级 |
| Windows 11 软件 | 0.43 ms | 5.2 ms | 11.3 ms | 同一台机器，2026-10-11 |
| Windows 11 Vulkan（AMD 专有驱动 25.10.30） | 10.2 ms | +0.3 ms | +0.8 ms | 含读回，基线同样由读回内存决定 |
| Windows 11 wgpu Vulkan / DX12 | 0.69 / 0.67 ms | +0.23 / +0.14 ms | +0.85 / +0.44 ms | 含读回；`WGPU_BACKEND` 选择后端 |

因此软件后端上的大面积模糊会明显占用帧时间，默认组件不使用它；GPU 后端上层的代价可以忽略。

Ui 中 `Node::set_opacity`（0..=1，可过渡）和 `set_backdrop_blur` 让该子树以层绘制。`visit_scenes` 依次给出 `Visit::Scene`、`Visit::PushLayer(Layer)` 与 `Visit::PopLayer`，层坐标是窗口逻辑坐标，宿主像处理 scene 变换那样缩放它们（`layer.then(scale)`、裁剪对齐到整像素）。不透明度为 0 的子树不再访问。

## 当前 Vulkan 离屏接口

`aegle-render-vulkan` 已独立实现相同 Scene 的实色/圆角/居中边框、仿射变换与裁剪；外部设备矩形与内部裁剪合计最多八层，draw 之间不泄漏作用域。离屏与透明窗口在 RGBA16F attachment 线性混合，再由 GPU 编码为预乘 sRGB RGBA8；不透明窗口直接写 sRGB swapchain，由硬件解码/编码线性混合；解析抗锯齿与软件覆盖率不要求边缘位精确。

可选 `text` 已将共享 aegle-glyph 缓存接到按需 R8 灰度/RGBA8_SRGB 彩色图集；缓存身份、像素基线与水平相位、灰度对比曲线及仿射光栅策略与软件共用。彩色图集在线性预乘空间过滤，灰度图集复用不同前景色；默认纯几何构建不带字体，遇到文字命令返回不支持错误。

`begin_frame → draw/draw_clipped → finish` 完成提交，`wait` 或下一帧等待 fence 后复用资源；只有显式 `read_pixels` 才分配并使用读回缓冲。shader 由构建期 Naga 生成 Vulkan 1.1 SPIR-V。几何与文字测试/示例已在 RX 6800 XT 和 Lavapipe 上执行验证层检查；可选 window 已支持 Wayland/Win32 swapchain，App 通过 RendererBackend 显式选择；默认软件，Vulkan-only 构建默认 Vulkan。资源及失败恢复契约见 [Vulkan](vulkan.md)，设备执行证据见[实现状态](implementation.md)。

## 当前 Wayland 接口

`Wayland::connect/create_window/dispatch/next_event/present` 提供一个连接上的多个 xdg-toplevel。需要 wl_compositor ≥4、xdg-shell 与 wl_shm；text-input-v3 可查询，启用缺失能力返回 `ImeUnavailable`。窗口初始 configure 前不分配像素或呈现；SCTK 处理 configure acknowledgement，宿主使用最新 `WindowInfo` 的逻辑尺寸和 `scale` 生成物理 framebuffer。compositor 同时提供 wp-fractional-scale-v1 与 wp-viewporter 时，surface 保持 buffer scale 1，缓冲区为 `round(逻辑尺寸 × scale)`，viewport 把它映射回逻辑尺寸，scale 取 compositor 的 preferred_scale（1/120 单位）；否则退回输出的整数 scale。preferred_scale 在 surface 映射后才送达，首帧按 1 绘制。输出变换由 compositor 处理。

`request_redraw` 合并变化，仅在已配置、前一 frame callback 完成且有空闲缓冲时通知宿主。每次实际提交才请求下一次 frame callback；没有变化不会持续呈现。`dispatch(None)` 使用 calloop/WaylandSource 的 FD 等待和 prepare-read 流程；已有应用事件时只做非阻塞分发。帧失败不附着，宿主显式请求重试；同时保持活动窗口的最新状态。

`present` 借出紧密排列的预乘 BGRA8 缓冲，即小端 XRGB8888 的内存顺序（大端目标呈现前就地反转），再 attach/commit。XRGB8888 与 ARGB8888 都是 wl_shm 必备格式；用 XRGB 表明窗口不透明，合成器忽略 alpha，可省去混合。不使用额外完整颜色缓冲。每窗口至多两个独立 SlotPool，尺寸变化仅释放空闲旧缓冲，不改写 compositor 尚未 release 的映射；具体预算见[资源](resources.md)。

`configure_ime` 使用带可选周边文字的 ImeRequest；长选区不能完整容纳时可保留组合输入而不报告 surrounding。显式禁用立即结束会话并清除该窗口已排队的 IME Update，避免焦点切换后的串写；其余序号、批次和编辑事务见[文字](text-input.md)。

输入事件携带原生 seat 身份。键盘翻译与 compose 复用 SCTK/XKB；指针保留 button、axis 和 logical position，光标使用 compositor cursor-shape 或系统 cursor theme。窗口移除时结束输入焦点与 IME 会话，删除尚未消费的窗口事件；窗口 ID 不复用。layer-shell 表面复用同一窗口表与帧门控：两侧相对边同时锚定时该轴拉伸到输出，configure 为 0 的轴保留当前尺寸，层表面始终报告 active。剪贴板按 seat 以 data device 设置/读取，管道读写不阻塞事件循环。拖放复用同一 data device：拖动进入窗口或移动时发出 `Event::Drag`（seat、逻辑坐标），宿主以 `accept_drag(seat, bool)` 回答，接受时选 `text/uri-list` 优先、其次文本类型并设置复制动作；compositor 只把放下交给已接受的窗口，数据经同一非阻塞管道读完后作为 `Event::Drop` 交出并 `finish` offer，读不出或拒绝时为 `DragLeave`。`start_drag(window, seat, data)` 以该 seat 最近的按键/按钮 serial 发起复制拖动，按需非阻塞写出数据。外观偏好由会话总线上的 desktop portal 提供：连接时以最多 100 ms 的有界等待读取，之后的回复与 SettingChanged 通过同一事件循环的 socket 源转成 `Event::Preferences`；没有总线或 portal 时偏好保持未知，不影响 Wayland 连接。`wl_touch` 以 `Event::Touch`（手指 ID、逻辑坐标、毫秒时间、阶段）交给宿主，窗口移除、能力丢失或 compositor cancel 时合成 Cancel。客户端窗口装饰及完整系统无障碍仍待接入；gpu feature 的原生租约与 present_external 复用当前窗口与帧门控。没有服务端装饰的 compositor 不会因此获得完整窗口标题栏。

`wake_handle()` 按需创建一个共享的 calloop ping source，克隆句柄可从后台线程请求 `Event::Wake`；宿主先将工作入自己的队列，再发信号，不引入轮询。该连接点已用于可选 Unix 无障碍回调：原生 App 启用 `unix-accessibility` 后，由独立 aegle-access adapter 导出控件树；Wayland 库的正常依赖仍不包含它。

## 当前应用宿主

`aegle-app/wayland` 与 `windows` 将目标平台接口接到同一 Ui；每窗口独立保留树，应用共享 TextSystem。software 共享 renderer/字形缓存；vulkan 与 wgpu 的窗口共享第一个窗口创建的实例/设备/队列（wgpu 还共享管线），每窗口仍有自己的 surface/swapchain、图集（Vulkan 还有管线），通过原生 swapchain 呈现；共享设备不能向某窗口呈现时创建返回错误，不静默换设备。`App::run` 在最后窗口关闭后返回，`dispatch(timeout)` 可由已有主循环显式驱动。嵌套 dispatch 返回重入错误，回调产生的新动作在下一轮执行，有待执行动作时不会进入无限期平台等待。

启用 motion 时，App 共享一个 Instant 时钟；每次刷新先采样活动过渡，实际呈现后仍有活动动画才请求下一帧。平台的 frame callback 与缓冲门控继续生效；无活动动画或 compositor 暂停回调时不加入轮询定时器。外观动画不改变几何，绘制和语义前景共用呈现值。

窗口、输入和辅助技术动作使用同一个 Ui。每个相关事件后刷新布局并取消旧 IME 会话，再处理下一条排队输入；不存在 text-input-v3 时，请求编辑会话返回能力错误。每窗口由活动键盘 seat 管理一个逻辑焦点域；失焦取消组合/手势，返回时恢复仍可用的原控件。编辑器的剪贴板请求在刷新前交给活动键盘 seat；异步读取在该 seat 仍持有焦点时粘贴，失焦后丢弃。连击由 Ui 按时间与距离统一计数（1、2、3 循环，编辑器据此选词/选行，`Node::on_double_click` 与 Canvas 的 `Press { clicks }` 读取它），App 把系统双击间隔交给它：Windows 为 `GetDoubleClickTime`，Linux 读 portal 的 GNOME `org.gnome.desktop.peripherals.mouse double-click`，都没有时 400 ms；右键按下、Menu 键与 Shift+F10 由 Ui 转成上下文菜单请求（`Node::on_context_menu`），不读取 Win32 `WM_CONTEXTMENU`；平台拖动事件经 `Ui::drag_motion`/`drag_leave`/`drop_data` 找到点下最近的 `on_drop` 节点并回答平台，控件 `start_drag` 的数据在刷新时交给平台。Win32 连接时 `OleInitialize`，每窗口注册 `IDropTarget`：OLE 同步询问效果而宿主异步回答，因此同一次拖动在宿主回答前接受任何文字/文件（本线程自己的阻塞拖动循环期间宿主无法回答，自拖自放因此可用），回答后以最新结果为准；`start_drag` 用 `SHCreateDataObject` 承载 `CF_UNICODETEXT` 或 `CF_HDROP`，`SHDoDragDrop` 使用系统默认的拖动源；动态窗口属性尚未接入应用 API；触摸经 `Ui::touch` 接入：点击与控件拖动成为指针事件，非拖动内容上超过 10 px 的拖动取消点击并平移滚动视图，抬起时带速度惯性滚动。

## 当前 Windows 原生路径

`aegle-platform-win32` 直接管理 Win32 HWND、消息循环、每显示器 DPI、鼠标/滚轮/双击、键盘和 UTF-16 字符输入。空闲以 MsgWaitForMultipleObjectsEx 等待消息与共享 wake event；重绘期间仍泵消息，避免动画饿死关闭和输入。窗口先隐藏创建，GPU/UIA 完成安装并成功绘制后才显示。逻辑关闭先停路由和隐藏，最后一个原生租约释放时才 DestroyWindow。

软件呈现借用一个有界 RGBA8 CPU buffer，经 GDI DIB 上传，窗口不透明：GDI 忽略 alpha，半透明像素按预乘颜色（等同叠在黑色上）显示，与 Wayland 的 XRGB 呈现一致，不逐像素检查；Vulkan 用同一 HWND 直接呈现。平台与 renderer 的预算独立，GDI/DWM 与驱动分配不属于 CPU buffer_budget。输入法经每窗口的 TSF 文本存储（周边文字、重转换），触屏键盘与 InputScope 尚未接入，见[文字输入](text-input.md#当前-windows-输入边界)。UIA 通过独立 aegle-access/windows 接入。Windows 11 实机上已运行原生窗口生命周期、软件与硬件 Vulkan/wgpu（Vulkan、DX12）呈现、TSF 与 Microsoft Pinyin 组合、OLE 拖放、桌面服务与 UIA 客户端查询，证据与仍缺的 ARM64、其他 GPU 驱动及屏幕阅读器验收见[实现状态](implementation.md)；Wine 或交叉编译不能替代这些证据。
