# 无障碍与可检查的控件树

状态：设计目标与当前实现并列。已实现独立 `aegle-access` 回调桥、可选 Unix AT-SPI / Windows UIA adapter、保留编辑器文本桥及应用层接入；尚未完成完整跨平台无障碍。facade 默认不启用系统适配（用户决定），应用按需选择 `unix-accessibility`、`windows-accessibility`，或只导出语义树的 `accessibility`；独立模块同样显式选择 feature。关联 R04–R07、R12–R14、R18–R21。

`Ui::accessibility(initial, title)` 可检查当前控件树及文本 run，使用与绘制相同的父子关系、布局与滚动几何；语义检查保留宿主尚未消费的重绘请求。身份由每个 Ui 单调分配，文本 run 与动态控件共用无冲突命名空间。`access_action` 对已经删除的节点、过期选择或预编辑期间的选择返回 false，原生应用不会因此退出。App 首次/重新激活完整导出，之后使用语义失效差量；文本的主题、局部样式及自定义皮肤前景同步到系统语义。自定义皮肤可依交互状态改变文字颜色，因此相关状态变化也标记语义失效；默认普通 hover/pressed 仍仅改变绘制。当前没有独立树检查器 GUI。

## 已确认的行为目标

框架必须为自绘组件提供完整系统无障碍语义，并提供可检查的控件树。不要求每个控件均由平台原生控件实现；参见 [ADR-0001](adr/0001-accessibility-and-custom-controls.md) 及其实现补充：语义由框架提供并始终可经 `accessibility` 检查，系统适配采用 AccessKit，并由应用显式启用。

“完整”应按每种已支持控件的行为验收：按钮需要可被辅助技术识别并激活，输入框需要文本、选择、焦点及编辑行为，滑块需要数值范围和调整操作。只输出角色与名称、只暴露静态树或只支持内置组件，均不能覆盖该目标。框架声称支持某种控件前，应写明其语义与平台支持范围；不声称首版支持所有未来可能出现的控件类型。

## 三种结构的关系

| 结构 | 表达内容 | 例子 |
| --- | --- | --- |
| 控件树 | 控件身份、逻辑包含关系和状态 | 一个按钮带图标与标签 |
| 无障碍语义树 | 辅助技术需要理解和操作的元素及关系 | 上述按钮通常作为一个命名的按钮暴露，装饰图标无需独立朗读 |
| 原生视图树 | 平台窗口或视图对象的实际层级 | 多个自绘控件可以由一个宿主视图承载 |

这些关系需有明确映射，不强求逐节点对应。绘制序列和 Taffy 布局结构也不应直接替代无障碍语义。语义仅从控件/文字状态派生；首次完整提交，此后提交差量，AccessKit 缓存计入资源预算。

## Interface 必须覆盖的契约

| 范围 | 必须定义的内容 | 与其他模块的连接 |
| --- | --- | --- |
| 身份与关系 | 稳定标识、父子、标签关联、节点出现和移除 | 控件生命周期与检查能力 |
| 属性与状态 | 角色、名称、描述、值、启用/选中/展开等状态 | 组件公开状态与主题呈现不可矛盾 |
| 动作 | 激活、聚焦、调整数值、选择及编辑等控件相关动作 | 与键盘、指针使用一致的行为入口 |
| 文本 | 内容、选择、范围及所支持的编辑操作 | CJK、光标边界、IME 预编辑与提交模型 |
| 焦点 | 当前焦点、请求聚焦、焦点迁移及通知 | 输入路由、窗口激活和 IME 会话 |
| 几何 | 屏幕坐标边界、缩放、裁剪和可见状态 | Taffy 布局、动画变换、多显示器与平台坐标 |
| 更新 | 结构、属性、焦点、文本和值变化的有效通知 | 更新提交时机及平台事件协议 |
| 调用条件 | 平台查询与动作的线程、重入、对象释放规则 | 平台事件循环、状态所有权及防止失效访问 |

不同平台的文本偏移单位和范围约定需要在适配契约中说明，不得默认 Rust 字节偏移可以直接传给每个平台。语义动作和信息查询的线程条件也不能靠应用猜测。

密码或其他受保护文本应遵守平台保护语义；不能因无障碍导出或开发者检查能力而默认泄露内容。密码控件默认禁用明文快照和普通复制，采用平台受保护文本语义。

## 平台事实与选型候选

以下系统接口事实于 2026-10-05 核实。Linux 的当前协议验证范围见下文；未进行三平台辅助技术实机验收。

| 平台 | 系统接口与已核实能力 | 仍需验证 |
| --- | --- | --- |
| macOS | NSAccessibility 支持自绘控件；Apple 明确支持无独立视图的控件使用 NSAccessibilityElement | 所选最低 OS、控件类型、通知、线程和实际辅助技术行为 |
| Windows | UI Automation provider 暴露属性、控件模式和导航关系；单一 HWND 可承载自绘 UI 的语义子树 | 文本模式、焦点、事件及各控件的实际支持 |
| Linux | AT-SPI 提供元素关系、角色、状态，以及动作、文本和几何等接口 | 目标发行环境、辅助技术、可用总线服务与 shell 配置 |

Wayland 不自动为自绘控件提供 AT-SPI 语义。窗口接入与无障碍适配分别具有责任，但需要共享控件焦点、身份和坐标的一致定义。

取舍记录：已选择第一项，第二项保留为未采用方案。

- **AccessKit 平台适配**：官方提供 macOS NSAccessibility、Windows UI Automation、Unix AT-SPI 适配。首次完整树、后续增量更新，并回传动作；adapter 保留无障碍树，需计入 RAM。直接接入不强制使用 winit；Unix 适配使用 zbus，须核查所选版本的依赖与体积。
- **直接适配三个系统接口**：有机会针对既有控件状态设计存储和查询方式，但需自行承担各平台语义、线程、通知和文本支持；不能在未实现和测量前宣称一定更小或更快。

AccessKit 当前上游 README 表示已发布适配支持单行和多行输入，但仍有未覆盖的元素/属性，rich text / hypertext 尚有缺口。最终选择必须以锁定版本和本项目控件清单验证，不能把某个库的存在等同于需求已满足。

最终选择按目标平台接入 AccessKit 独立 adapter，版本见 [依赖基线](dependencies.md)；adapter 的树、线程和异步任务都计入资源。已接入的文本桥复用 Parley 的节点和选区转换，补充 Aegle 编辑状态边界。

## 当前接入与行为边界

`aegle-access` 默认仅依赖 AccessKit schema，提供与窗口循环无关的 `Mailbox/Handlers`。`unix` feature 才编译 `UnixAdapter` 与 Unix AT-SPI 依赖；Wayland 模块不因此永久绑定无障碍实现。原生 App 在启用 `unix-accessibility` 时组合它们，从控件树派生窗口、控件、文本框及文字 run，并用 `Dirty::SEMANTICS` 提交变化。首次激活或重新激活提交完整树，其后提交发生变化的控件及其文字 run；当前没有每个文字 run 的独立差量比较缓存。 facade 默认 desktop 不启用任何系统适配：Linux 应用显式选择 `unix-accessibility` 才会链接 zbus/AT-SPI 依赖并启动上游 worker，Windows 应用显式选择 `windows-accessibility` 才接入 UIA；只需检查语义树时选择 `accessibility`。

AccessKit Unix 的激活、动作与停用回调均在后台线程执行。Handlers 只排队并唤醒主线程；`Wayland::wake_handle()` 复用 calloop 的事件唤醒，不增加轮询定时器。激活回调返回 `None`，宿主收到 `InitialTree` 后立即在 UI 线程构建完整树，即使没有像素需要重画。`UnixAdapter::update_if_active` 的闭包只在原生适配处于活动或待初始化状态时调用；逻辑控件焦点和原生窗口激活分别同步；当前辅助 Focus 不会请求 xdg-activation 来激活后台窗口。按钮 Click 经过与物理输入共用的 `Input::Activate`，Focus 经过现有焦点策略，过期或不适用的目标不会直接访问已失效控件。

`aegle-text/text-a11y` 的 `EditorDriver::accessibility` 导出当前显示文本、布局几何、只读状态及选区；显示文本包含 IME 预编辑及下划线，不能视为已提交值。`select_accessibility` 使用最近一次导出的 run 身份验证范围，布局重建后须重新导出。AccessKit 的 run 内位置以可选择的 shaping cluster 计数，AT-SPI 外部偏移以 Unicode scalar 计数；平台 adapter 与 Parley 依据 run 的字符长度完成转换，不能直接传递 UTF-8 字节偏移。

只读编辑器仍接受合法选择。活动预编辑期间 `select_accessibility` 返回 `CompositionActive`，引擎拒绝该次辅助技术选择，不隐式取消组合或重解释旧范围；应用代码的 `TextField::select` 则先结束组合再选择。未知 run、越界位置或重排后尚未重新发布的选择返回 `InvalidRange`。密码编辑器导出 `PasswordInput` 角色，值、文字 run 与选择只含 `•` 遮盖字符，选择按遮盖偏移转换。

当前 Unix 平台能力如下；API 已存在与系统能力已验证必须分别表述。

| 能力 | 当前结果 |
| --- | --- |
| 控件树、名称、角色、父子关系 | 示例通过 AT-SPI 查询验证 |
| 按钮动作、文本框与按钮焦点 | AT-SPI GrabFocus/DoAction 经过保留控件状态验证 |
| CJK 文本、字符数、选择和 caret | AT-SPI Text 查询及选择动作通过；本地桥覆盖只读、run 边界、失效范围与 IME 拒绝 |
| 通过辅助技术替换文本 | 上游 `accesskit_unix` 0.22.1 未实现 `org.a11y.atspi.EditableText`；当前不能宣称支持 |
| 几何 | 从绘制使用的逻辑窗口几何与滚动偏移派生；Wayland 无全局窗口位置，不调用 set_root_window_bounds；adapter 的默认原点不能作为真实屏幕位置，也未完成屏幕定位验收 |
| Windows | UIA adapter 已接入；兼容环境与实机证据见实现状态 |
| 密码控件 | 导出 `PasswordInput` 与遮盖值；无窗口 ui 场景验证语义树不含明文，AT-SPI/UIA 客户端未验收 |
| macOS、真实屏幕阅读器 | 尚未接入或验收 |

协议验证使用独立 headless Wayland compositor、私有 `dbus-run-session`、最小假 Status/Registry 服务，以及真实 D-Bus AT-SPI 查询/动作：遍历树、读取 CJK、改变选择/caret、切换焦点、激活按钮并观察文字清空，以及停用/重新启用后的完整树与焦点恢复。这证明 adapter 与当前示例的协议和状态连接，不等于真实 AT-SPI registry、屏幕阅读器或全部桌面环境验收。测试总线未修改用户桌面设置。

上游 Unix adapter 首次构造启动进程级 worker，窗口销毁后该 worker 仍保留；它没有公开总线错误或就绪状态接口，构造成功不能证明辅助技术已连接。初始总线/服务失败可能结束 worker，当前没有可承诺的自动恢复路径。停用和线程、队列、语义缓存成本见 [资源边界](resources.md)。

## 当前切换与数值控件

本地语义树将 CheckBox/Switch 暴露为相应角色、标签与 Toggled 状态，Slider 暴露为水平 Slider、数值/边界/步长以及 Focus/SetValue/Increment/Decrement 动作，Progress 为不可聚焦的 ProgressIndicator 与数值边界。动作回到共享 Toggle/Slider 行为，拒绝隐藏、禁用、错误类型和非有限数值；数值按同一 Range 契约 clamp/步进，回调仍在树借用外执行。程序 setter 只更新语义与绘制，不产生用户修改回调。

私有 Sway/Pixman + D-Bus 已验证：CheckBox 对应 AT-SPI CheckBox/Checked，Switch 对应 ToggleButton/Pressed；两者 DoAction 更新保留状态。Slider/Progress 实际提供 org.a11y.atspi.Value，可读最小值、最大值、当前值，Slider 的 MinimumIncrement 对应步长。写 CurrentValue 经应用回调同步进度，越界和步进结果与 Rust API 一致；祖先禁用后交互写入不能改变值。

当前上游 Unix adapter 仅将 click 映射为 Action，因此滑块的系统调整使用 Value + MinimumIncrement，没有独立 Increment/Decrement Action 接口。Progress 的 Value 写入也会得到 D-Bus 属性层的应答，但应用拒绝该非交互目标，值保持不变。应答不是已生效的证明，协议验证在处理队列后重新读取值。

已知上游缺口：accesskit_unix 0.22.1 / accesskit_atspi_common 对不支持只读属性的 ProgressIndicator 未传播 disabled，禁用进度条的 AT-SPI GetState 仍包含 Enabled/Sensitive。本地 TreeUpdate 正确标记 disabled，CheckBox/Switch/Slider 的系统禁用状态及行为均已验证；不能把这一组合称为完整系统语义一致性。此缺口待上游修复或适配层补齐，未通过改变控件角色绕过。

## 检查能力的范围

用户已要求可检查的控件树。当前示例的系统语义树可经 AT-SPI 查询；框架开发工具所见的完整逻辑控件、状态与语义映射快照仍待实现。计划使用 debug-only 的本地树快照接口输出逻辑关系、语义映射、几何和状态，不建立常驻服务器、远程调试协议或运行时反射系统；密码内容应始终隐藏。

检查输出应能解释一个逻辑控件为何被合并、隐藏或映射为多个语义元素，方便第三方组件作者排查问题。发布物不能默认携带完整开发工具而不计入成本。

## 与主题、动画和输入共同验收

用户已确认默认视觉适配深浅色、对比度和减少动态效果等偏好。macOS 提供相关偏好查询与变更通知；Windows 通过系统主题/辅助设置通知接入，Linux 通过桌面设置 portal（可用时）及应用显式配置接入；缺失偏好使用默认值。平台的有效外观可能被应用或祖先视图覆盖，不应直接等同于系统全局设置。

主题切换不能重置焦点或编辑内容；纯装饰动画不应产生无意义的朗读事件。位置变化时，屏幕边界、命中、焦点提示和 IME 候选窗必须按商定规则保持一致。减少动态效果的处理方式应由框架和组件协作，不让每个应用重新判断系统偏好。

验收应覆盖辅助技术发起动作、动态增删节点、CJK 编辑与选择、动画中的位置、销毁后停止通知等场景，参见[联合验收 S3–S6](quality.md)。

## 官方来源

- [Apple：自定义控件与 Controls Without Views](https://developer.apple.com/library/archive/documentation/Accessibility/Conceptual/AccessibilityMacOSX/ImplementingAccessibilityforCustomControls.html)
- [Apple：无障碍模型](https://developer.apple.com/library/archive/documentation/Accessibility/Conceptual/AccessibilityMacOSX/OSXAXmodel.html)
- [Apple：NSAccessibilityProtocol](https://developer.apple.com/documentation/appkit/nsaccessibilityprotocol)
- [Microsoft：UI Automation provider](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-providersoverview)
- [GNOME：AT-SPI Accessible](https://gnome.pages.gitlab.gnome.org/at-spi2-core/libatspi/class.Accessible.html)
- [AccessKit 上游说明](https://github.com/AccessKit/accesskit#readme)
- [AccessKit Unix 0.22.1：adapter API](https://docs.rs/crate/accesskit_unix/0.22.1/source/src/adapter.rs)
- [AccessKit Unix 0.22.1：worker 与激活生命周期](https://docs.rs/crate/accesskit_unix/0.22.1/source/src/context.rs)
- [AccessKit Unix 0.22.1：实际注册的 AT-SPI interfaces](https://docs.rs/crate/accesskit_unix/0.22.1/source/src/atspi/bus.rs)
- [Apple：减少动态效果](https://developer.apple.com/documentation/appkit/nsworkspace/accessibilitydisplayshouldreducemotion)
- [Apple：提高对比度](https://developer.apple.com/documentation/appkit/nsworkspace/accessibilitydisplayshouldincreasecontrast)
- [Apple：辅助显示偏好变化通知](https://developer.apple.com/documentation/appkit/nsworkspace/accessibilitydisplayoptionsdidchangenotification)
- [Apple：有效外观](https://developer.apple.com/documentation/appkit/nsappearancecustomization/effectiveappearance)

## 当前滚动语义

RadioButton 导出同名角色；复选框部分选中时 Toggled 为 Mixed。Dropdown 导出 ComboBox（带展开状态），其列表为 ListBox，选项为 ListBoxOption（带选中状态）；Table 导出 Table/Row/Cell/ColumnHeader；一般 Popup 为通用容器；Menu 导出 Menu，MenuBar 导出 MenuBar，菜单项为 MenuItem 或 MenuItemCheckBox（带勾选状态），打开子菜单的项带 `has_popup: Menu` 与展开状态。有 `on_context_menu` 处理器的可用控件导出 ShowContextMenu，执行时与 Menu 键相同，在该控件左上角请求上下文菜单。ImageView 导出 Image 角色，Canvas 导出 Canvas 角色，名称来自 `set_accessible_label`。Separator 导出带方向的 Splitter 角色（AccessKit 中即 ARIA separator），Splitter 的把手是可聚焦的 Canvas；NumberField 导出 SpinButton 与数值/边界；Tabs 导出 TabList、Tab（selected）与 TabPanel，未选页隐藏；竖直 Slider/Progress 报告竖直方向。`Node::set_accessible_description` 设置描述，Tooltip 文字即写入描述。这些新增角色只在本地语义树测试中验证，没有经 AT-SPI 实测。ListView 作为 ScrollView 导出，只包含已建立的行，不报告总行数或行位置。ScrollView 导出同名角色、`clips_children`、横纵 offset/min/max；子节点的局部 transform 减去直接父 ScrollView 偏移，嵌套后的窗口边界与绘制/命中共用同一结果。被裁出的节点保留逻辑身份，不因离屏设置 hidden；真正隐藏的树仍使用 hidden，恢复时重新导出保留偏移和有效范围。

共享动作入口支持 `SetScrollOffset`、四方向 `ScrollUnit::Item/Page`（一项为主题 control_height，一页为视口尺寸），以及无 Hint 的 `ScrollIntoView`。滚动动作先于可聚焦过滤，因此普通标签也可请求滚入；禁用祖先仍拒绝交互。`ScrollHint` 和 `ScrollToPoint` 暂不支持并明确返回 false，后者的目标坐标不能误作 offset。程序滚动和辅助滚动都不取消 IME 组合。

这已验证 AccessKit schema/consumer 的嵌套边界和直接动作，不等于完整平台滚动协议验收。当前 AccessKit Unix 的过滤器会省略部分连续离屏兄弟节点，不能声称 AT-SPI 一次遍历可取得所有离屏控件。原生语义的 HiDPI/平台坐标转换与真实屏幕阅读器滚动交互仍需补齐验证。

## 当前 Windows UIA 接入

`aegle-access/windows` 复用 accesskit_windows 0.34 的 SubclassingAdapter，在 HWND 首次显示之前安装，并持有原生租约到 subclass 卸载之后。WM_GETOBJECT 的激活回调仅排队并唤醒；上游临时 placeholder 由 UI 线程首次完整语义树替换。动作仍经 Mailbox 进入同一 Ui，原生查询不重入借用控件树。发布后先释放 AccessKit 借用再 raise 系统事件；上游直接处理窗口焦点消息。

Ui 检查 API 继续使用逻辑坐标，Windows App 在语义根应用 DPI scale，UIA adapter 再负责 client-to-screen；DPI 改变重新发布。没有维护第二棵可修改的 UI 树；AccessKit 派生缓存、标准库消息队列及 COM/UIA 成本仍需计量。

控件自身的角色与名称来自 `aegle-widgets/accessibility`；facade 的 `windows-accessibility`、`unix-accessibility` 因此同时启用 `accessibility`（此前只接系统 adapter，按钮和标签被导出为无名容器，`aegle/tests/accessibility.rs` 覆盖）。直接组合 `aegle-app` 时须自行启用该 feature。Windows 11 上用系统 UI Automation 客户端检查 `controls` 示例：窗口下依次为两个 Text、名为 "Text editor" 的 Edit（TextPattern 返回含 CJK 的全文）和三个支持 Invoke 的 Button；Invoke "Clear text" 清空编辑器，SetFocus 把系统焦点移到编辑器。Windows 真实屏幕阅读器（讲述人/NVDA）、文本模式的选择与编辑及通知的完整验收尚未完成。
