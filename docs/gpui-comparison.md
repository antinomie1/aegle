# Aegle 与 GPUI 架构对比

本篇作为横向架构参考，对比 Aegle 与 Zed 的图形框架 GPUI（基于 `gpui 0.2.2`）在定位、数据模型、渲染与资源策略上的差异。

---

## 一、 对比总表

| 维度 | Aegle | GPUI (0.2.2) | 差异本质 |
|---|---|---|---|
| **核心定位** | 低资源、面向嵌入式/轻量桌面的保留模式 GUI | 现代桌面/IDE 驱动的高性能 GPU 渲染框架 | 场景与硬件目标不同 |
| **界面模型** | **真保留模式**：创建一次，弱句柄显式修改，局部失效打脏 | **混合即时/保留模式**：`Entity` 持有状态，每帧执行 `Render` 重新返回元素树 | 状态与树拓扑机制不同 |
| **编写入口** | 花括号标记语言（`.aegle`）+ 命令式 Rust 句柄 API | 纯 Rust 链式 DSL（Tailwind 风格） | 入口与可读性取舍不同 |
| **渲染基线** | **默认 CPU 软件光栅**（极低内存、无 GPU 可用）；可选 Vulkan / wgpu | **强制硬件 GPU 渲染**（基于 blade-graphics，Metal/Vulkan/D3D11） | 硬件依赖门槛不同 |
| **文字排版** | 三平台全走**系统文字栈**（Linux Pango/FreeType, Win DirectWrite, macOS CoreText） | Linux 基于 cosmic-text；macOS CoreText；Windows DirectWrite | Linux 下系统继承 vs 独立排版栈 |
| **默认控件** | **开箱即用完整控件库**（Button, TextField, Table, Menu, Dialog, Drawer 等） | 核心仅提供 `Div`、`List`、`Canvas` 等基元，业务控件需在应用层自建 | 框架边界定义不同 |
| **桌面主题** | 原生内建类型化 Token、深浅色/高对比自适应、局部覆盖 | 核心仅提供基础色板，完整主题在 Zed 应用层实现 | 开箱即用程度不同 |
| **无障碍 (a11y)** | 原生 AccessKit 语义树，支持 Linux AT-SPI、Windows UIA、macOS NSAccessibility | 0.2.2 公开 API 未集成完整系统语义适配 | 语义可用性差异 |
| **Linux 窗口** | 默认 SDL3（Wayland/X11 自适应），可选原生 Wayland（支持 layer-shell 面板） | 支持 X11 + Wayland | Aegle 支持桌面 layer 覆盖层 |
| **异步模型** | 核心无异步运行时；单 UI 线程 + `UiProxy` 线程安全消息通道 | 内置 smol 执行器、`Task`、异步上下文管理 | 异步侵入性不同 |
| **发布包体积** | 默认 release 示例（`hello`）约 **1.47 MiB** | 依赖较重（smol, resvg, lyon 等），发布二进制通常数十 MiB | 资源敏感度不同 |

---

## 二、 核心架构取舍

### 1. 界面生命周期：保留模式 vs 响应式函数树
* **Aegle**：控件节点保存在集中的槽位表（`NodeId`）中，只有被修改的节点及其依赖的布局/绘制会打上脏标记（`Dirty`）。在界面无交互、无动画时，主循环完全阻塞在平台事件等待上，**静止时 CPU 与唤醒次数为零**。
* **GPUI**：应用数据由 `Entity<T>` 管理，状态变动时触发 `cx.notify()`，调度执行组件的 `Render` 函数重新构造元素树并与上一帧比对提交 GPU。

### 2. 渲染与资源哲学：系统栈优先 vs 自建跨平台栈
* **Aegle 走系统精简路线**：
  * 文字排版在 Linux 上直接加载发行版的 Pango/FreeType，不打包庞大的 Unicode/ICU 词典或自绘文字引擎。
  * 窗口优先通过动态加载的 SDL3 打开，运行时仅消耗必要资源，适合 Linux 嵌入式屏、工控终端、桌面覆盖层面板（Quickshell 类）。
* **GPUI 走 GPU 深度优化路线**：
  * 为高性能代码编辑器设计，追求 120 FPS 满帧滚动与高并发多窗格渲染，必须依赖现代 GPU 驱动。

---

## 三、 选型指南

* **适合选择 Aegle 的场景**：
  * 面向 Linux ARM64、低内存（512 MB～1 GB）、无 GPU 或老旧核显设备。
  * 需要开发 Wayland 桌面面板、锁屏、状态栏、覆盖层（利用 `layer-shell` 协议）。
  * 强调极小安装包、轻量分发、原生中日韩文字与输入法（CJK/IME）即插即用。
  * 偏好标记语言快速声明界面，或需要嵌入现有非异步宿主进程。

* **适合选择 GPUI 的场景**：
  * 开发面向高配置 PC/Mac 的重度桌面生产力工具（如 IDE、复杂设计软件）。
  * 强依赖异步任务系统，重度使用 async/await 驱动数据流。
  * 愿意自行构建整套设计系统和控件层，且只运行在拥有现代 GPU 的设备上。
