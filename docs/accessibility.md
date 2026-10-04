# 无障碍与可检查的控件树

状态：v0.1 设计基线。使用兼容版本组中的 AccessKit 平台 adapters，默认 desktop 启用；逻辑节点使用代数 ID，语义按需差量同步。具体实现仍需真实平台验收。关联 R04–R07、R12–R14、R18–R21。

## 已确认的行为目标

框架必须为自绘组件提供完整系统无障碍语义，并提供可检查的控件树。不要求每个控件均由平台原生控件实现；参见 [ADR-0001](adr/0001-accessibility-and-custom-controls.md)。

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

以下事实于 2026-10-05 核实，属于文档调查，未进行三平台实机测试。

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

最终选择按目标平台接入 AccessKit 独立 adapter，版本见 dependencies.md；adapter 的树、线程和异步任务都计入资源。Parley 发布版的 text-a11y 提供文本节点与选区转换。

## 检查能力的范围

用户已要求可检查的控件树。建议区分系统辅助技术/检查工具所见的语义树，与框架开发工具所见的逻辑控件、状态及关联信息。首版以 debug-only 的本地树快照接口输出逻辑关系、语义映射、几何和状态；不建立常驻服务器、远程调试协议或运行时反射系统。密码内容始终隐藏。

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
- [Apple：减少动态效果](https://developer.apple.com/documentation/appkit/nsworkspace/accessibilitydisplayshouldreducemotion)
- [Apple：提高对比度](https://developer.apple.com/documentation/appkit/nsworkspace/accessibilitydisplayshouldincreasecontrast)
- [Apple：辅助显示偏好变化通知](https://developer.apple.com/documentation/appkit/nsworkspace/accessibilitydisplayoptionsdidchangenotification)
- [Apple：有效外观](https://developer.apple.com/documentation/appkit/nsappearancecustomization/effectiveappearance)
