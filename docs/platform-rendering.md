# 平台与绘制

状态：v0.1 设计决定。平台窗口与绘制后端分别选择，通过显式窗口资源与生命周期契约连接。

## 支持目标

| 平台 | 首版目标 | 绘制与输入 |
| --- | --- | --- |
| Linux | x86_64/aarch64，glibc 2.36+、内核 6.1+；Wayland | ash/Vulkan；SCTK + wayland-client；text-input-v3 |
| Windows | Windows 11，x86_64/aarch64，存在兼容 Vulkan 驱动 | ash/Vulkan；原生窗口与 TSF，必要时 IMM 兼容 |
| macOS | macOS 13+，x86_64/aarch64，存在可用 Metal 设备 | 原生 Metal；AppKit、NSTextInputClient |

这些是验收目标，不是已经验证所有 OS/GPU 组合。Linux 不实现 X11；基础普通窗口要求 xdg-shell，shell 表面另要求 layer-shell。没有对应协议时报告明确能力缺失，不假造成功。无 GPU 不属于首版支持范围；scene 接口允许后续增加 CPU renderer，不先实现它。

Linux 使用一个连接和事件队列承载普通窗口及可选 layer-shell，补齐预编辑、提交、周边文字/删除、焦点和提交时序。SCTK 的 input_method 面向输入法程序，不代替普通客户端 text-input-v3。默认不会引入 winit 再维护另一套窗口循环。

各平台按事件等待；Linux 使用 FD readiness 和协议要求的读取流程，Windows/AppKit 保留原生消息循环。文本输入、系统偏好、剪贴板和无障碍动作归一化后交给 UI 主线程，不跨 FFI 展开 panic。

## 绘制契约

Vulkan 最低 API 1.1，采用传统 render pass、普通 descriptor、fence 与 binary semaphore；不要求 descriptor indexing、timeline semaphore 或 dynamic rendering。还必须检查 graphics/present queue、swapchain、surface format 和所用纹理格式能力。

Metal 使用系统设备、CAMetalLayer 和常规 command buffer。shader 在构建期编译：Vulkan 使用匹配 Vulkan 1.1 的 SPIR-V，macOS 生成 metallib；开发工具不进入运行依赖。维护两套小型二维实现，不建立通用三维 RHI。

基础 scene 提供预乘颜色、文字图集、RGBA 图像、矩形/圆角/边框、透明度、二维仿射变换和裁剪。颜色在线性空间混合，输出转换遵循目标色彩格式。默认字形用灰度抗锯齿，不依赖 LCD 子像素排列。

轴对齐裁剪用 scissor，圆角通过专用绘制路径；任意路径由可选 Lyon 细分，路径裁剪使用受限 stencil 层级，最多 8 层，超过时返回明确错误。先实现按绘制顺序的相邻批次合并，不做复杂全局重排。

高级效果为显式 feature：渐变、阴影和区域模糊。模糊只作用于框架拥有的离屏区域，不承诺跨平台桌面背景模糊；超预算时移除装饰模糊、使用实色/边框替代阴影，并发出可查询诊断。Strict 模式则返回错误。默认组件不依赖这些特效保持可用。

默认 FIFO 呈现、最多两帧在途；有像素变化才请求新帧，首版每次呈现完整窗口。后台上传和临时资源遵守[资源预算](resources.md)。DeviceLost 停止相关窗口提交并报告，首版不隐藏地切换后端。

## 坐标与平台差异

内部使用逻辑 dp、统一二维变换；平台层转换为物理像素和平台坐标，整数边界向外取整。命中、IME 光标矩形和语义位置共享几何来源。

Wayland 通常不向普通客户端公开全局窗口位置。平台能力必须区分窗口局部坐标与可取得的屏幕原点，不能编造全局坐标；对应辅助技术的屏幕定位能力受 compositor 和 adapter 限制，但角色、文本、焦点和动作仍需正确。具体支持矩阵通过真实 compositor 验收。

首版 shell 只含面板/背景层、输出绑定、锚点、exclusive zone 与键盘交互策略；托盘、通知、全局快捷键等业务协议不进入核心。
