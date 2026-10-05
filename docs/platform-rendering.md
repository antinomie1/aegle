# 平台与绘制

状态：v0.1 设计决定。平台窗口与绘制后端分别选择，通过显式窗口资源与生命周期契约连接。

## 支持目标

| 平台 | 首版目标 | 绘制与输入 |
| --- | --- | --- |
| Linux | x86_64/aarch64，glibc 2.36+、内核 6.1+；Wayland | ash/Vulkan；SCTK + wayland-client；text-input-v3 |
| Windows | Windows 11，x86_64/aarch64，存在兼容 Vulkan 驱动 | ash/Vulkan；原生窗口与 TSF，必要时 IMM 兼容 |
| macOS | macOS 13+，x86_64/aarch64，存在可用 Metal 设备 | 原生 Metal；AppKit、NSTextInputClient |

这些是验收目标，不是已经验证所有 OS/GPU 组合。Linux 不实现 X11；基础普通窗口要求 xdg-shell，shell 表面另要求 layer-shell。没有对应协议时报告明确能力缺失，不假造成功。无 GPU 设备通过独立软件 renderer 支持；与 GPU 共用 scene、布局、文字和输入，不建立第二套 UI。软件像素缓冲由各平台呈现，按可用后端显式选择。

Linux 使用一个连接和事件队列承载普通窗口及可选 layer-shell，补齐预编辑、提交、周边文字/删除、焦点和提交时序。SCTK 的 input_method 面向输入法程序，不代替普通客户端 text-input-v3。默认不会引入 winit 再维护另一套窗口循环。

各平台按事件等待；Linux 使用 FD readiness 和协议要求的读取流程，Windows/AppKit 保留原生消息循环。文本输入、系统偏好、剪贴板和无障碍动作归一化后交给 UI 主线程，不跨 FFI 展开 panic。

## 绘制契约

Vulkan 最低 API 1.1，采用传统 render pass、普通 descriptor、fence 与 binary semaphore；不要求 descriptor indexing、timeline semaphore 或 dynamic rendering。还必须检查 graphics/present queue、swapchain、surface format 和所用纹理格式能力。

Metal 使用系统设备、CAMetalLayer 和常规 command buffer。shader 在构建期编译：Vulkan 使用匹配 Vulkan 1.1 的 SPIR-V，macOS 生成 metallib；开发工具不进入运行依赖。维护两套小型二维实现，不建立通用三维 RHI。

基础绘制目标包括文字图集、RGBA 图像、矩形/圆角/边框、透明度、二维仿射变换和裁剪。颜色在线性空间混合，输出转换遵循目标色彩格式。默认字形用灰度抗锯齿，不依赖 LCD 子像素排列。

具体颜色边界：调用方和 scene 的 `Color` 为非预乘 sRGB RGBA8；renderer 在合成时解码并预乘，输出遵循目标格式。软件缓冲为预乘 sRGB RGBA8，不能将其直接当作线性颜色或透明 PNG 的非预乘颜色使用。

GPU 轴对齐裁剪用 scissor，圆角通过专用绘制路径；任意路径由可选 Lyon 细分，路径裁剪使用受限 stencil 层级，最多 8 层，超过时返回明确错误。先实现按绘制顺序的相邻批次合并，不做复杂全局重排。软件裁剪使用下文的覆盖率 mask，不依赖 stencil。

高级效果为显式 feature：渐变、阴影和区域模糊。模糊只作用于框架拥有的离屏区域，不承诺跨平台桌面背景模糊；超预算时移除装饰模糊、使用实色/边框替代阴影，并发出可查询诊断。Strict 模式则返回错误。默认组件不依赖这些特效保持可用。

默认 FIFO 呈现、最多两帧在途；有像素变化才请求新帧，首版每次呈现完整窗口。后台上传和临时资源遵守[资源预算](resources.md)。DeviceLost 停止相关窗口提交并报告，首版不隐藏地切换后端。

## 坐标与平台差异

内部使用逻辑 dp、统一二维变换；平台层转换为物理像素和平台坐标，整数边界向外取整。命中、IME 光标矩形和语义位置共享几何来源。

Wayland 通常不向普通客户端公开全局窗口位置。平台能力必须区分窗口局部坐标与可取得的屏幕原点，不能编造全局坐标；对应辅助技术的屏幕定位能力受 compositor 和 adapter 限制，但角色、文本、焦点和动作仍需正确。具体支持矩阵通过真实 compositor 验收。

首版 shell 只含面板/背景层、输出绑定、锚点、exclusive zone 与键盘交互策略；托盘、通知、全局快捷键等业务协议不进入核心。

## 当前软件绘制接口

已实现部分以 `SceneBuilder → Scene → Renderer::begin_frame → Frame::draw` 连接。`Scene` 为不可变局部绘制记录；重新构建时可以取回并清空 builder，复用命令分配。每次 draw 传入布局位置与设备缩放组成的 Affine，移动控件不必重建其局部图元。场景的变换/裁剪作用域只在本次 draw 内生效，不泄漏到其他记录。

目前支持实色矩形、统一圆角、居中边框、二维仿射变换及嵌套矩形/圆角裁剪；可选 `text` 支持定位后的字形。builder 检查非有限几何、负尺寸、不可逆变换及作用域配对，总嵌套最多 64 层。空形状不绘制，空 clip 排除绘制；软件后端拒绝超出 ±1,048,576 的设备路径/字形坐标。通用图片、任意路径、组透明度和高级特效尚未实现。Wayland 原生软件呈现与输入已接入；GPU 后端仍未实现。

scene 字形 run 保存共享字体句柄、字号、变化轴、前景色及基线位置；`aegle-text/scene` 可从保留段落生成它们。软件 `text` 必须显式启用：如果 Cargo 统一开启了 scene/text 而 renderer/text 关闭，遇到字形命令返回 `UnsupportedCommand`，不能跳过文字后假称成功。

保留 `Editor` 也使用同一文字绘制函数；选区背景、预编辑下划线和 caret 从当前排版生成几何，再录入普通 scene 命令。宿主负责焦点/闪烁时机、滚动与裁剪，模块没有自己的定时器。候选窗使用 `ime_rect()` 的未裁剪局部文字几何；长单行文字溢出字段宽度时不能只截断右边缘而产生负宽度。原生平台 adapter 应统一应用呈现/滚动变换后再转换坐标。

正向、轴对齐、均匀缩放的文字在设备字号光栅化，基线按每轴四分之一像素相位取整，排版 advance 保持原值；旋转、反射、斜切和非均匀缩放通过逆变换双线性采样。彩色字形在线性预乘空间过滤，字形接口输出非预乘 sRGB，合成后仍遵守 `Surface` 的预乘 sRGB 格式。前景透明度只应用一次。灰度 glyph 与前景色分离，颜色主题更新可复用字形缓存；彩色格式范围与错误见[文字](text-input.md)。

`Surface` 借用紧密排列、从上到下的 RGBA8 字节切片，要求长度准确等于宽×高×4。平台呈现方负责 stride 或 BGRA 等格式转换。begin_frame 清空整帧，draw 按调用次序合成保留记录；无像素变化时宿主不调用绘制。错误可能发生在部分像素已更新之后，失败帧不得呈现。

tiny-skia 仅负责几何覆盖率。线性光合成使用约 8 KiB 的共享、按需初始化转换表，避免逐像素幂运算；每次绘制量化到 RGBA8，完全不透明覆盖直接复制。透明背景、半透明叠加与旋转已有像素级验证。裁剪 mask 的预算、复用与释放见[资源](resources.md)。

## 当前 Wayland 接口

`Wayland::connect/create_window/dispatch/next_event/present` 提供一个连接上的多个 xdg-toplevel。需要 wl_compositor ≥4、xdg-shell 与 wl_shm；text-input-v3 可查询，启用缺失能力返回 `ImeUnavailable`。窗口初始 configure 前不分配像素或呈现；SCTK 处理 configure acknowledgement，宿主使用最新 `WindowInfo` 的逻辑尺寸和整数 scale 生成物理 framebuffer。输出变换由 compositor 处理，当前不提供 fractional-scale/viewport 协议。

`request_redraw` 合并变化，仅在已配置、前一 frame callback 完成且有空闲缓冲时通知宿主。每次实际提交才请求下一次 frame callback；没有变化不会持续呈现。`dispatch(None)` 使用 calloop/WaylandSource 的 FD 等待和 prepare-read 流程；已有应用事件时只做非阻塞分发。帧失败不附着，宿主显式请求重试；同时保持活动窗口的最新状态。

`present` 借出紧密排列的 RGBA8 预乘缓冲，成功绘制后就地转换为 Wayland 必备 ARGB8888 的本机字节序，再 attach/commit。不使用额外完整颜色缓冲。每窗口至多两个独立 SlotPool，尺寸变化仅释放空闲旧缓冲，不改写 compositor 尚未 release 的映射；具体预算见[资源](resources.md)。

`configure_ime` 使用带可选周边文字的 ImeRequest；长选区不能完整容纳时可保留组合输入而不报告 surrounding。显式禁用立即结束会话并清除该窗口已排队的 IME Update，避免焦点切换后的串写；其余序号、批次和编辑事务见[文字](text-input.md)。

输入事件携带原生 seat 身份。键盘翻译与 compose 复用 SCTK/XKB；指针保留 button、axis 和 logical position，光标使用 compositor cursor-shape 或系统 cursor theme。窗口移除时结束输入焦点与 IME 会话，删除尚未消费的窗口事件；窗口 ID 不复用。触摸、剪贴板、客户端窗口装饰、平台偏好、layer-shell、原生 GPU 句柄及完整系统无障碍仍待接入。没有服务端装饰的 compositor 不会因此获得完整窗口标题栏。

`wake_handle()` 按需创建一个共享的 calloop ping source，克隆句柄可从后台线程请求 `Event::Wake`；宿主先将工作入自己的队列，再发信号，不引入轮询。该连接点已用于可选 Unix 无障碍回调。示例启用 `example-accessibility` 后，由独立 aegle-access adapter 导出同一控件树；Wayland 库的正常依赖仍不包含它。

## 当前应用宿主

`aegle-app/wayland` 将这些平台接口接到可独立使用的 Ui；每窗口独立保留树，应用共享 TextSystem、软件 renderer 和字形缓存。`App::run` 在最后窗口关闭后返回，`dispatch(timeout)` 可由已有主循环显式驱动。嵌套 dispatch 返回重入错误，回调产生的新动作在下一轮执行，有待执行动作时不会进入无限期平台等待。

启用 motion 时，App 共享一个 Instant 时钟；每次刷新先采样活动过渡，实际呈现后仍有活动动画才请求下一帧。平台的 frame callback 与缓冲门控继续生效；无活动动画或 compositor 暂停回调时不加入轮询定时器。外观动画不改变几何，绘制和语义前景共用呈现值。

窗口、输入和辅助技术动作使用同一个 Ui。每个相关事件后刷新布局并取消旧 IME 会话，再处理下一条排队输入；不存在 text-input-v3 时，请求编辑会话返回能力错误。每窗口由活动键盘 seat 管理一个逻辑焦点域；失焦取消组合/手势，返回时恢复仍可用的原控件。原生双击计数、触摸、系统剪贴板与动态窗口属性尚未接入应用 API。
