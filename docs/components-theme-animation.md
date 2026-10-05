# 默认组件、主题与动画

状态：v0.1。当前已有基础行为、中性皮肤、局部样式、可替换纯函数皮肤和小型 Theme；任意绘制/行为组件扩展、局部主题继承与几何动画仍是目标；外观过渡已接入。三者共用属性、状态、生命周期和失效规则，不建立第二套运行时。对应 R12、R17–R21。

## 当前行为接口

`aegle-controls::Button` 保存 enabled/focused/hovered/pressed，不含标签或绘制。指针按下请求 capture，匹配释放且仍在命中区才激活；Space 在释放时激活，Enter 只在首次按下时激活，重复按键不重复触发。失焦、取消和禁用释放 capture，不产生激活；语义 `Input::Activate` 同样检查启用状态。

可选 `text` 的 `TextField` 直接拥有 Editor，复用选择、按词/行移动、grapheme 删除、撤销、单行提交和原子 IME 事务。宿主提供当前文字局部坐标，应用 `Outcome` 的焦点/capture/重绘/IME 重置请求，并消费 Editor 的失效标记。`Outcome::semantics` 独立表达焦点/启用状态变化，普通 hover/pressed 绘制不会因此重新导出语义。只读仍可选择；失焦或禁用取消组合且恢复已提交值。剪贴板、密码和平台差异快捷键尚未全部接入。

这些行为可由不同皮肤共享；当前 `aegle-app` 已将它们与 row/column、标签及单行/多行编辑器组合，使用统一 Theme 绘制中性基础外观，并同步布局、命中、IME 和可选 Unix 系统语义。`set_skin` 可替换现有控件的配色、边框、圆角和文字装饰，不重写行为。尚无独立 widgets crate、任意绘制/新行为注册接口或完整跨平台组件集成。控件行为层不创建窗口或定时器。

## 当前切换与数值控件

`Container::check_box(text, checked)` / `switch(text, checked)` 创建带可选可见标签的二态控件；标签同时作为默认无障碍名称，`set_accessible_label` 可覆盖。Toggle 复用 Button 的按压、捕获、Space 释放/Enter 首次按下与语义激活规则；复选标记和开关位置都表达当前状态，不只用颜色区分。当前没有三态、分组单选或专属触摸处理。

`slider(min, max, value)` / `progress(min, max, value)` 共用独立 `aegle-controls::Range`；边界与数值为 f64，要求有限、min < max 且跨度有限，有限越界值按公开契约 clamp。`set_range` 原子替换边界，失败保留旧状态。Progress 只显示确定进度，不接受焦点或用户调整，没有不确定进度的循环动画。

Slider 为水平连续滑块；`set_step(step)` 可选有限非负步长，零表示连续。步长以 min 为网格起点，max 为额外可达端点；取最近值，等距时取较大值。步长低于浮点可表示精度时不声称精确网格。单步在额外最大端点向下移动至最后格点，例如0..10/step3为10→9→6。方向键/语义调整使用一个步长，连续时为跨度的1%；PageUp/Down为十步或10%，Home/End到端点。数值增量低于一个浮点间隔时按下一可表示数推进。

指针按下轨道直接设值并捕获；拖出仍 clamp 到端点，其他指针不能接管。失焦、禁用、取消或删除释放捕获并保留最后值；取消不会回滚整个拖动。输入和绘制共用轨道起点/长度计算，零长度轨道不做指针调值；键盘及语义路径仍可用。

`on_change(|control| ...)` 与按钮回调共用版本化队列，在树借用外执行。用户/语义操作真正改变值才排队；`set_checked` / `set_value` / `set_range` / `set_step` 是模型更新，不回调，方便多个控件同步且不会形成循环。处理器读取执行时的最新值，通知不保存每次中间值；替换处理器或销毁节点会丢弃旧通知。显式 `toggle`、`increment`、`decrement` 走用户行为并检查有效可见/启用状态。

默认标志为18dp复选框或36×20dp开关；滑块手柄16dp，轨道2dp、完成部分4dp，以粗细差异辅助表达数值。小尺寸时缩小标志并裁剪至自身范围，布局宽度优先容纳可见标签；当前不做标签自动换行。字号/主题切换复用同一段落，值变化不重排文字。外观可用 Skin/Style 和 `indicator_color` 修改；四种控件的值、checked 与手柄位置即时更新，配色/焦点轮廓沿用现有外观过渡，尚无数值或开关位移动画。

`widgets` 示例用 `.aegle` 构造四种控件和 CJK 编辑器，通过简短 Rust 回调实现互相同步、启禁编辑、进度更新、主题切换与窗口关闭。

## 当前局部样式与组件复用

`aegle-theme` 提供无分配的 `VisualState`、`Appearance`、`Style` 和 `Skin`，可独立于 app 使用。VisualState 包含控件种类、有效 enabled、hovered、pressed、focused 和编辑器 read_only；祖先禁用反映在有效 enabled 中。当前 app 的按钮/编辑器提供 hover 和 focus，pressed 由按钮、切换控件和滑块行为提供；其他种类的相应状态为 false。

`Node::set_skin(fn(&Theme, VisualState) -> Appearance)` 以纯函数替换默认外观。皮肤只能根据传入数据计算，不能重入 UI 或执行应用回调；不捕获环境、不创建注册表或虚函数对象。当前结果在安装前验证，以后状态在刷新时验证；非有限或负几何返回错误，不静默回退。皮肤不决定布局或字号。

`Node::set_style(Style)` 设置稀疏本地覆盖；`set_background`、`set_foreground`、`set_radius` 等是简短命令式入口。`style()` 读取覆盖，`appearance()` 读取当前解析结果。`None` 恢复皮肤值；`set_style(Style::default())` 清除覆盖，`clear_skin()` 单独恢复默认皮肤。局部字号通过 `set_font_size` / `clear_font_size` 控制，仅适用于文字控件，不向子节点继承。

解析顺序为默认/自定义皮肤 → 本地基础覆盖 → 本地 disabled、pressed 或 hover 覆盖。高优先状态没有指定覆盖时保留基础值，不回落到其他状态；focus 环最后独立绘制；有效启用且聚焦时才有非零目标宽度，失焦过渡可短暂保留渐隐的呈现轮廓。边框与 focus 宽度为零可关闭，相对于自身矩形向内绘制，不侵入相邻控件；容器圆角不隐含对子树的裁剪。hover/focus 覆盖限交互控件，pressed 覆盖限按钮/切换控件/滑块，selection/caret 限编辑器，indicator 限复选框/开关/滑块/进度条，不适用时 setter 返回 WrongKind，标记属性在编译期拒绝。

局部视觉数据按 NodeId 放在 Ui 的稀疏表中，无样式节点不保存一份完整 Style。纯配色/边框变化只失效绘制，前景色同时失效语义；自定义皮肤可随交互状态改变前景，相关状态变化会同时刷新语义。字号改变才重排文字及布局，保持编辑器、组合输入、选择和控件身份。

普通 Rust 函数组合现有控件即可形成组件库。`crates/aegle/examples/components.rs` 使用按钮工厂、主题皮肤和 `.aegle` 结构展示复用；它不是完整 MD3 套件。新控件行为、任意 painter、组件标记导入、系统主题观察、几何动画和动画完成回调尚未实现。

## 默认组件范围

默认皮肤采用跨平台一致的中性极简外观。首版包含 Box/Row/Column、Text、Button、CheckBox、Switch、Slider、Progress、TextField、TextArea、ScrollView、等高虚拟 ListView，以及窗口内 Popup/Menu/Tooltip。Grid、图像格式、路径图标和高级特效按 feature 提供。

默认 Popup/Menu/Tooltip 在当前窗口的 overlay 层内显示，不承诺越过宿主窗口边缘；需要独立原生 popup 的 shell/应用通过平台扩展显式创建，走相同焦点及语义契约。首版没有表格引擎、富文档编辑器、可变高度虚拟列表或完整 MD3 套件；第三方可用公开接口实现这些组件。

行为与皮肤分离：controls 负责激活、切换、调整、编辑、滚动等行为及语义，widgets 负责默认外观。第三方 MD3 库应复用 controls，并增加自己的 token、图标与绘制；不重写平台输入、CJK 或无障碍。

## 视觉规范

| token | 浅色 | 深色 |
| --- | --- | --- |
| canvas | #F6F7F9 | #15181D |
| surface | #FFFFFF | #1E232B |
| on_surface | #20242B | #F0F2F5 |
| secondary_text | #5B6472 | #ADB6C4 |
| accent / focus | #355CDA | #9EB5FF |
| on_accent | #FFFFFF | #15213F |
| border | #737D8C | #788596 |

尺寸采用 dp；基础正文 14 dp、辅助文字 12 dp、标题 20 dp，字体族使用带 locale 的系统 sans-serif 回退。间距为 4/8/12/16/24 dp，标准控件高 36 dp、紧凑模式 28 dp、触摸模式命中区域至少 44×44 dp。默认控件、面板、标志、开关、滑块和进度条均为直角（主题圆角 0）；应用设置正的主题 radius 时这些默认控件统一圆化。边框 1 dp，焦点环 2 dp。

默认不使用背景模糊、大面积阴影或持续装饰动画。hover/pressed 用轻度叠色，拖动响应直接；disabled 不只靠变淡区分，语义同步不可用状态。选中、错误和焦点不能仅靠颜色，应有形状/标记或文字反馈。

普通文字对比度目标至少 4.5:1，交互轮廓/焦点目标至少 3:1。此处是设计与验收要求，不是认证声明。应用自定义颜色也要接受对比度检查。高对比模式把默认 token 调整为系统或黑白高对比配色；显式本地覆盖仍由应用负责。

## 主题契约

当前可用的 `aegle-theme::Theme` 是公开字段的无分配快照：颜色为 `background/surface/foreground/muted/accent/border/hover/pressed/selection`，尺寸为 `font_size/padding/gap/radius/control_height`。`light()`、`dark()`、`high_contrast()` 提供显式配色；`Default` 为浅色。三种配色共用正文 14、padding 8、gap 8、圆角 0、控件高 36 的逻辑像素尺寸；窗口可用 `set_padding` 独立增加外侧留白。`validate()` 拒绝非有限或负尺寸，并要求正文大小和控件高度大于零。accent 用于焦点/标记，selection 与普通 foreground 配对，不隐含另一套文本颜色。

`Ui::set_theme` 和 `Window::set_theme` 更新现有控件，不重新创建编辑器。颜色切换更新外观；字体或尺寸变化使相应布局失效。焦点、文本、选择及预编辑保留。本地布局、字号和视觉覆盖优先于主题，自定义皮肤按新 Theme 解析；没有局部主题树、token 注册表或系统偏好监听。以下是进一步扩展时的目标契约。

主题使用类型化 token：Color、Length、Font、Radius、Duration 等。Rust 常量提供类型检查，标记使用具名 token。名称只在主题/注册阶段解析为紧凑索引，不给每个控件复制一份字符串样式字典。

全局主题是共享不可变快照，局部主题只保存稀疏覆盖。查找顺序为最近局部覆盖到全局默认；作用范围沿逻辑控件树继承。组件自己的 token 使用包命名空间，重复定义且类型不同为错误。

组件状态样式采用固定状态集合及明确优先顺序：disabled、pressed、selected/checked、hover、normal；focus 环作为独立覆盖，不被 hover 隐藏。复合组件需要不同优先级时在自己的有类型样式函数中显式定义，不引入 CSS specificity。

控件默认值 → 主题/状态值 → 本地常量或绑定，构成逻辑目标。动画覆盖呈现值；直接 setter 替换该属性的绑定。颜色变化只刷新绘制，字体/尺寸变化才使布局失效，语义无关的 token 变化不广播语义值更新。

默认跟随系统深浅色、对比度、文本缩放与减少动态效果；应用可显式选 Light/Dark/System。系统没有提供某项偏好时用默认值并允许应用配置。系统字体缩放与设备像素缩放各应用一次，不能重复放大。

## 当前外观过渡

可选 `motion` 已提供 `Node::set_transition(Transition::new(Duration::from_millis(120), Easing::EaseOut))`，同时过渡背景、文字、边框、圆角、焦点环、选择和 caret 的外观值。`appearance()` 返回逻辑目标，`presented_appearance()` 返回最近采样值；失焦/禁用的焦点宽度目标为零，绘制使用相同呈现值。前景动画同步语义颜色，不重新成形文字，也不延迟焦点、文本、预编辑或命中状态。布局、字号、transform 和窗口清屏背景当前不做过渡。

`clear_transition()` 移除策略并在刷新时回到目标；`finish_transition()` 立即到达当前目标；`cancel_transition()` 将最近呈现写为本地外观覆盖，替换状态颜色覆盖，保留后续设置的过渡策略。焦点轮廓的可见性仍受真实行为约束。尚无完成回调、关键帧、弹簧或循环 API。

无窗口 Ui 默认不安装过渡。`set_default_transition` 只影响随后创建的交互控件；首次刷新直接建立呈现值，不做入场动画。原生 App 在启用 motion 时为交互控件默认安装120ms EaseOut，`AppOptions.transition=None` 可关闭自动安装。各 App 共用一个单调时钟，通过 Wayland frame callback 推进；没有活动动画时不请求动画帧，无轮询定时器。隐藏子树刷新时直接到目标；compositor 暂停窗口帧回调时不主动唤醒，恢复时采样当前时刻。

`Ui::advance_animations(Duration)` 供独立宿主显式采样，拒绝时钟倒退；随后按常规 refresh/呈现。新目标从最近采样的呈现值开始。`is_animating()`/`has_animations()` 反映最近刷新后是否仍活动，节点删除和窗口关闭立即清理对应动画。`Ui/Window::set_reduced_motion(true)` 立即到目标，保留最后一帧重绘；期间不启动新过渡。AppOptions 可设置初始偏好，尚未自动读取 OS 偏好。

## 后续动画契约

默认 motion 只提供标量、二维向量和颜色的补间及属性过渡：linear、ease_in、ease_out、ease_in_out。颜色在预乘线性空间插值。关键帧和阻尼弹簧为可选模块能力；无通用时间线编辑器或动画脚本。

默认 hover/焦点过渡 120 ms，开关/选择 160 ms，面板出现 180 ms。几何动画可以修改 transform，命中与候选窗跟随呈现变换；width/height 动画明确触发布局，不伪装成免费合成动画。

新目标从当前呈现值继续过渡。cancel 将当前呈现值固化为本地目标并解除该属性绑定，随后移除动画；finish 立即到达逻辑目标。完成回调只在正常完成/显式 finish 且节点仍存活时执行，取消或销毁不执行完成回调。

同一 UI 共用时钟，只管理活动动画。隐藏窗口暂停装饰动画，恢复时按当前逻辑目标重建呈现，不积压追赶大量历史帧。没有活动动画就没有动画定时唤醒。重复循环必须由调用者显式请求，不在默认皮肤启动永久循环。

减少动态效果开启时，非必要位移、缩放、弹簧和循环直接跳到目标；保留不超过 80 ms 的轻微透明度反馈，应用也可完全关闭。进度仍通过语义值表达，不能把关闭动画变成丢失进度信息。

## 组件作者的完整接口

| 任务 | 提供的能力 |
| --- | --- |
| 组合 | 创建子控件、输入属性、事件、内容 slot、生命周期清理 |
| 布局 | Taffy 属性、内容测量、基线、滚动和裁剪 |
| 交互 | 指针捕获、键盘、焦点、状态、语义动作及默认行为复用 |
| 绘制 | 文字、图像、矩形、圆角、边框、变换；可选路径/特效 |
| 文本编辑 | PlainEditor 接入、选择、剪贴板、IME、密码/只读与撤销 |
| 无障碍 | 角色、标签、值、关系、文本范围、动作和合并规则 |
| 风格 | 自定义 token、局部主题、状态颜色和系统偏好 |
| 动效 | 过渡、显式动画、取消/完成与可选弹簧 |

组件宏和小型示例见 [Rust API](rust-api.md)。一次 MD3 风格 Button/TextField/Switch 示例必须共同覆盖主题、动效、键盘、CJK、IME 和语义，不能只验证静态外观。
