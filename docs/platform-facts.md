# 影响选型的事实

核实日期：2026-10-04。以下引用只用于说明技术边界，不意味着选用 MoltenVK、GTK、HarfBuzz 或 FreeType。

## macOS 与 Vulkan

macOS 不原生提供 Vulkan。MoltenVK 将 Vulkan 映射到 Metal，可以采用静态或动态集成；静态链接减少独立文件数量，并不消除对应实现的代码体积。完整 Vulkan SDK 不是必需的发布依赖。

因此需要比较“通过 MoltenVK 复用 Vulkan 绘制路径”与“额外实现 Metal 绘制后端”的包体、维护成本和能力限制，不能同时无条件许诺原生 Vulkan 与 macOS 零移植成本。

来源：[Khronos 平台说明](https://github.com/KhronosGroup/Vulkan-Guide/blob/main/chapters/platforms.adoc)、[MoltenVK 运行时集成指南](https://github.com/KhronosGroup/MoltenVK/blob/main/Docs/MoltenVK_Runtime_UserGuide.md)。

## Wayland 窗口与 shell 表面

layer-shell 协议提供面板、背景等桌面层表面的层级、定位和输入相关语义，但不是所有 Wayland compositor 都实现。gtk-layer-shell 上游列出 Sway、KDE Plasma、Mir 等支持，并指出 GNOME-on-Wayland 不支持。此来源用于说明协议覆盖范围，不是 GTK 依赖建议。

跨平台普通窗口和 Quickshell 类 shell 能力应分别定义兼容性；Wayland 支持本身不等于 layer-shell 支持。Quickshell 文档站本次访问返回 403，未据此推断该项目的具体兼容范围。

来源：[layer-shell 原始协议](https://github.com/swaywm/wlr-protocols/blob/master/unstable/wlr-layer-shell-unstable-v1.xml)、[上游桌面支持说明](https://github.com/wmww/gtk-layer-shell#supported-desktops)。

## GPU 加速与资源预算

Vulkan 的独显路径通常涉及 staging buffer 与数据传输；统一内存架构减少某些传输，却使 CPU 与 GPU 共享系统内存压力。驱动分配也有成本。因此，仅凭采用 Vulkan 无法证明小型静态 GUI 的总 RAM、CPU 或能耗一定更低。

这些成本需要结合设备、窗口尺寸、窗口数量和更新频率验证。当前没有本项目实测数据，也没有决定是否提供 CPU 绘制后端。

来源：[Khronos 内存分配说明](https://github.com/KhronosGroup/Vulkan-Guide/blob/main/chapters/memory_allocation.adoc)。

## CJK 显示与 IME

文字成形（shaping）需要结合文字、字体、书写系统和语言；缺字回退、文字布局与字形光栅化仍是需要处理的职责。FreeType 的缓存接口表明字形图像缓存可以设置内存上限；这不等于整个文本系统或驱动内存已被同一上限约束，也不代表此处已经选择 FreeType。

Wayland `text-input-v3` 包含预编辑、文字提交、删除周边文字、光标矩形、焦点与状态提交等行为。IME 不能被简化为普通键盘字符事件，正确显示 CJK 也不能替代输入法支持。具体平台协议覆盖和库选型仍待决定。

来源：[HarfBuzz 职责说明](https://harfbuzz.github.io/what-is-harfbuzz.html)、[FreeType 缓存接口](https://github.com/freetype/freetype/blob/master/include/freetype/ftcache.h)、[Wayland text-input-v3 原始协议](https://github.com/wayland-mirror/wayland-protocols/blob/main/unstable/text-input/text-input-unstable-v3.xml)。
