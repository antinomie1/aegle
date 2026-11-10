# 待办与优先级

按优先级排列尚未实现或不完整的能力。已实现的能力与验证证据见[实现状态](implementation.md)；完成一项后从这里移除，并在实现状态中记录事实与证据。

---

## 一、 优先事项

1. **窗口管理（剩余）**：尺寸、约束、模式、位置、置顶、关闭请求、无边框窗口、移动/改尺寸区域（平台 `set_areas`，控件 `Node::set_window_area`）与窗口图标已实现（macOS 实机验证）。还缺，按顺序：
   * **标记语言 `Window` 的对应属性**（已暂停，尚未开始）：`min_size`/`max_size`、`resizable`、`mode`、`position`、`always_on_top`、`decorations` 与 `icon`（需在 loader 里解码图像；图标在 macOS 与多数 Wayland 合成器上是 `Unsupported`，要先定下标记里遇到它是忽略还是报错），以及元素属性 `window_area: move | client | resize_*`。涉及 `aegle-markup` 的 schema 与校验、`aegle-loader/src/window.rs`（打开与重载时原地应用）、`aegle-macros/src/generate.rs`（`ui!` 生成 `WindowOptions`）与 `docs/markup.md`。
   * Win32、SDL3、Wayland 的全部窗口管理实现（含无边框、窗口区域与图标）只经交叉 clippy，需实机验证；重点是 Win32 的 `WM_NCCALCSIZE`（最大化取工作区、自动隐藏任务栏）与切换装饰后的客户区、SDL3 命中测试回调、Wayland 的 `xdg-toplevel-icon-v1`。
   * 移动区域的标题栏行为：macOS（按系统设置双击缩放或最小化）、Wayland（双击最大化、右键 `show_window_menu`）与 SDL3 上只有拖动；Wayland 的触摸按下不触发窗口区域。
   * 窗口内浮层（`Popup`、`Menu`）盖在移动区域上时，只有其中的交互控件被排除，浮层的空白处仍会拖动窗口。
   * macOS：带标题栏窗口的原生改尺寸是 AppKit 的模态循环，期间宿主停在 `dispatch` 内、不重新布局；应用图标（Dock）没有 API。Wayland：合成器没有 `xdg-decoration` 时无法提供装饰，`set_decorations(true)` 不报错。
2. **原生菜单与弹出层**：macOS 应用菜单只有退出项，没有应用菜单栏、Dock 菜单与原生上下文菜单 API；控件库的 `Popup`/`Menu`/`Dropdown` 仍是窗口内浮层，不能越过窗口边缘；Win32 原生弹出层返回 `Unsupported`；标记语言没有弹出层元素。
3. **UI 线程定时器**：只有逐帧回调（存在期间持续出帧）或“后台线程 + `UiProxy`”。需要一次性与周期定时器，空闲时按最近截止时间休眠，不轮询。
4. **应用级测试工具**：`Ui` 可无窗口运行，但没有按文字、无障碍名称或角色查找控件并点击、键入、滚动的辅助；各测试自写 `click`/`key`。
5. **剪贴板与拖放多格式**：`DragData` 与剪贴板只有纯文本和文件列表；缺图像、HTML/富文本、自定义格式与延迟提供。
6. **开发工具**：控件树、盒模型、样式与帧耗时检查器；Rust 侧无热重载（仅标记可热重载）。
7. **打包与分发**：应用包（macOS `.app`、签名与公证）、安装包、自动更新与项目脚手架；macOS 通知只在应用包内可用，目前需手工搭建。

## 二、 控件与媒体

* 富文本编辑器、代码编辑器、Markdown 视图。
* ComboBox/自动完成、日期/时间/颜色选择器、Toast、可折叠面板/手风琴、工具栏溢出、标记输入框、图表。
* 动图播放（GIF 目前只解码首帧，无 APNG）、视频、音频、内嵌 Web 视图。
* 虚拟列表的行内状态在滚出后丢失；组合控件（ListView、Table、Tabs）不能直接放子控件。

## 三、 应用级桌面能力

* 单实例、深度链接、文件关联、最近文档、开机自启。
* 系统强调色、电源与网络状态、密钥存储、文件监视。
* 国际化：消息目录、复数规则与伪本地化（RTL 布局已实现）。

## 四、 易用性

* 渲染器只能在构建期选择（一个 feature），没有运行时按 GPU 可用性回退到软件渲染；默认仍为 CPU 渲染，是否改为默认 GPU 需按包体、常驻内存与动画 CPU 的实测再定。
* Linux 默认后端依赖 `libSDL3.so.0`，部分较旧的发行版仓库不提供。

## 五、 已知不完整（详见实现状态第三节）

* 文本编辑器不处理辅助技术的 `SetValue` 与 `ReplaceSelectedText`。
* SDL3：不能发起拖动、输入法无周边文字、无触控板惯性结束事件、弹出层不按锚点翻转。
* macOS 弹出层不随锚点或屏幕变化重新定位。
* Windows 彩色 emoji 未在实机核对；全局快捷键按 ANSI 键位匹配。
* VoiceOver 语音、日韩输入法、跨应用拖放未实测。
* Pango 布局尺寸取整导致段落间最多约 1 px 误差。
* Windows 软件呈现每帧以 `SetDIBitsToDevice` 上传整幅 DIB，未按损伤区域上传（未在实机测量）。
* GPU 渲染器（wgpu、Vulkan）每帧整幅重绘：macOS 上 wgpu 的 `showcase` 动画时应用 CPU 用户 11.4% + 内核 7.5%；按损伤局部重绘需要保留离屏目标。
* macOS 软件呈现的 `showcase` 动画时应用 CPU 约 15%，主要是 Core Animation 每帧提交与后备存储更新，Aegle 自身的布局、场景遍历与光栅化约占 2.5%。
