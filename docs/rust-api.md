# Rust 命令式 API

状态：v0.1。基础 Ui/App、弱句柄、命令式控件、静态与动态标记编译及运行时加载已有源码；第三方控件经 `Control` trait、`handle!` 句柄宏与 `element!` 标记元素宏接入；超出现有接口的部分在“目标接口族”中单列。验证记录见[实现状态](implementation.md)。项目采用 Rust 2024，编译器基线见[依赖](dependencies.md)。

## 完整 Hello world

```rust
use aegle::prelude::*;
fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Hello")?;
    window.text("你好，世界");
    app.run()
}
```

源码位于 `crates/aegle/examples/hello.rs`，7 行 Rust 加 1 行文档注释，包含导入、入口、初始化和事件循环；运行入口为 `cargo run -p aegle --example hello`。当前需要 Linux Wayland 和可显示所用字符的系统字体。按 rustfmt 后非空源码行计数，不将多个语句强压在同一行。Cargo 清单不计；任何必须手写的应用初始化辅助文件计入。

当前 `App::new` 按目标连接 Wayland / Win32 并建立系统字体上下文；`run(self)` 在主线程接管循环，最后一个窗口关闭后退出。各窗口的控件树独立，字体系统和软件 renderer 共享。`App::with_fonts` 接受显式字体集合，可关闭 `system-fonts`；`AppOptions` 配置 app_id、浅色/深色/高对比主题（后两者可为 None）、renderer 选择、可选 Vulkan 预算和软件 mask 预算；motion feature 另提供 transition 与 `reduced_motion: Option<bool>`（None 跟随系统）；`App::preferences` 返回最近应用的系统偏好，`WindowOptions` 配置初始尺寸与 SHM 预算，Linux 上可选 `layer: Some(LayerOptions)` 创建 wlr layer-shell 面板/覆盖层（compositor 缺少协议时创建失败）。macOS 尚未实现；Vulkan 通过原生 swapchain 呈现，Windows 输入法经 TSF 文本存储。

`aegle` 当前默认启用原生窗口、软件绘制、系统字体、编译型标记（含动态标记引擎）与外观过渡；系统无障碍适配需显式启用 `unix-accessibility`（AT-SPI）或 `windows-accessibility`（UI Automation），`accessibility` 单独提供语义树导出。嵌入式宿主可直接使用无平台依赖的 `aegle-ui::Ui::with_fonts(Rc<RefCell<TextSystem>>, Theme)`，取得 root 后用 `aegle-widgets` 的 `Widgets` trait（需引入）创建同样的控件，或实现 `aegle_ui::Control` 提供自己的控件；通过输入、`refresh`、`visit_scenes`、IME 和可选语义接口对接自己的宿主。

界面优先写在 `.aegle` 文件中；`App::run_ui(aegle::ui!("main.aegle"))` 完成默认初始化、构造和运行。`let view = aegle::ui!(&window, "panel.aegle")?` 返回带 `root`、各 `id` 和入口 state（`loader::State<T>`）字段的有类型句柄集合，可直接给 `view.done.on_click(...)` 绑定下面的普通 Rust 回调。文件路径相对使用者包清单目录，编译器跟踪其变化；完整已实现属性见[标记语言](markup.md)。

## 创建、修改与事件

```rust
let label = window.text("等待操作");
let button = window.button("完成");
button.on_click(move |_| {
    label.set_text("已完成");
});
```

控件创建一次。`Window` 解引用到根 `Container`；容器提供 `row`、`column`、`scroll_view`、`text`、`button`、`text_field`、`text_area`，返回相应弱句柄。多行与单行编辑器共用 `TextField` 句柄。设置是直接命令，不要求嵌套函数、消息枚举或 builder 链。第三方控件用 `Container::add` 加入，见[组件库作者](#组件库作者)。

命名规则只有两条：`on_*` 追加处理器，`set_*` 替换值。`on_click`、`on_change`、`on_submit`、`on_transition_end`、`on_double_click`、`on_context_menu`、`on_drop`、`on_frame` 与 `Ui::on_key` 都追加，同一事件按注册顺序执行；处理器与控件同生命周期，没有单独的移除方法，需要停止时在闭包内判断（`on_frame` 返回 `false` 即停止）。`Canvas::set_input` 与 `set_painter` 一样是画布自身的行为，再次设置会替换。所有动作处理器存放在引擎的同一张表里：控件只报告激活或值变化，组合控件（Tabs、Dropdown、MenuItem）把自身行为放在按钮的延后工作中，再用 `State::queue_action` 排队自己的处理器，不另存处理器列表。处理器在树和原生宿主借用之外执行，可修改其他控件、删除自己或关闭窗口；执行中新增的处理器从下一次事件起生效。回调中产生的新动作留待下一轮；控件销毁清理处理器，丢弃普通句柄不销毁控件。

句柄的创建、修改与读取方法直接返回值。误用句柄是程序错误，因此以 `UiError` 的消息 panic，而不是让每个调用都返回 `Result`：控件已删除或窗口已关闭（`DeadHandle`，先用 `is_alive()` 判断）、在 painter、钩子或 scene 访问中使用（`ReentrantAccess`）、文档排除的参数（`InvalidValue`、`WrongKind`、`ForeignUi`、`RootMutation`、`Token`）。`Result` 只留给会因环境或应用代码失败的操作：创建 App 与窗口、`Window::close`、加载与构建标记、标记状态更新（重新求值绑定可能出错）、按名查找 token，以及宿主驱动的 `refresh`、输入投递和 `dispatch_callbacks`。处理器返回 `()` 或 `Result`（`HandlerResult`），逐帧与按键处理器返回 `bool`；处理器出错时保留，同一事件的其他处理器照常执行，此前合法修改保留；无窗口 `Ui::dispatch_callbacks` 返回第一个错误。原生 App 把回调、代理与输入处理的错误交给 `App::on_error`，返回错误才结束 `run`，未设置时打印到 stderr 并继续；绘制与平台失败仍结束 `run`。

可清除的属性只有一个 setter：参数为 `impl Into<Option<T>>`，传值设置、传 `None` 清除，如 `set_font_size(18.0)` / `set_font_size(None)`、`set_theme(Theme::dark())` / `set_theme(None)`、`set_background(None)`；不再有 `clear_*` 方法。皮肤是函数指针，`set_skin`/`set_kind_skin` 仍写 `Some(skin)`。

当前公共操作包括：

| 类型 | 已有接口 |
| --- | --- |
| Node / 所有控件句柄 | `decorate`、`on_double_click`、`on_context_menu`、`on_drop`、`on_frame`、`start_drag`、`is_alive`、`bounds`、`visible_bounds`、`ensure_visible`、`remove`、`reparent`、`set_visible`、`set_enabled`、`focus`、`set_accessible_label` |
| 布局（Node） | 逐轴的 `set_width`、`set_height`、`set_min_width`、`set_min_height`、`set_max_width`、`set_max_height`，`set_aspect_ratio`、`set_grow`、`set_shrink`、`set_basis`、`set_align_self`、`set_margin`、`set_absolute`、`set_padding`、`set_gap(行, 列)`、`set_layout_direction`、`layout_direction`、`baseline`；`grid` 另有 `set_grid_column`、`set_grid_row`（`Placement` 或可用线名/区域名的 `GridLines`）、`set_grid_area`、`set_justify_self` |
| 布局（Container） | `row`、`column`、`contents`、`set_direction`、`set_wrap`、`set_align_items`、`set_justify_content`、`set_align_content`；`grid` 另有 `grid`、`stack`、`set_columns`、`set_rows`（`Track` 列表，或含线名与 `Repeat` 的 `TemplateItem` 列表）、`set_areas`、`set_auto_columns`、`set_auto_rows`、`set_flow`、`set_justify_items` |
| 外观（Node） | `set_style`、`style`、`set_skin`（本节点）、`set_kind_skin`（子树中某类型的全部控件）、`appearance`、`visual_state`；所有控件都接受的 `set_background`、`set_foreground`、`set_border_color`、`set_border_width`、`set_radius`、`set_disabled_background`、`set_disabled_foreground` |
| 外观（按控件） | 由 `aegle_ui::handle!` 的样式组生成在相应句柄上：文字控件的 `set_font_size`/`set_font`/`font`，交互控件的 `set_hover_background`/`set_focus_color`/`set_focus_width`，按钮/切换/滑块的 `set_pressed_background`，切换/滑块/进度条的 `set_indicator_color`，编辑器的 `set_selection_color`/`set_caret_color`；分布见[API 指南](developer/api.md#6-外观主题样式与皮肤) |
| 局部主题与位移 | `set_theme`、`set_theme_override`、`theme`；`set_offset(Point)`、`offset` |
| 过渡与动画（motion） | `set_transition`、`set_property_transition`、`property_transition`、`with_transition`、`snap`、`animate(Animate)`、`presented_appearance`、`is_animating`、`finish_transition`、`cancel_transition`、`on_transition_end`；曲线与关键帧见 `aegle-motion` 的 `Easing`、`Spring`、`Animation`、`Keyframe`、`Cycles` |
| Label / TextField | `text`、`set_text`；TextField 另有 `select`、`set_read_only`、`set_password`、`on_submit` |
| Button | `set_text`、`activate`、`on_click` |
| Container（值控件） | `check_box(text, checked)`、`switch(text, checked)`、`slider(min, max, value)`、`progress(min, max, value)` |
| CheckBox / Switch / Radio | `is_checked`、`set_checked`、`toggle`、`text`、`set_text`、`on_change`；CheckBox 另有 `is_mixed`、`set_mixed` |
| Container（选择/表格） | `radio(text, checked)`、`dropdown(items, selected)`、`table(columns, row_height, rows, cell)` |
| Dropdown / Popup / Table | Dropdown 有 `selected`、`set_selected`、`items`、`set_items`、`on_change`；`Node::popup()` 返回 Popup（`show`、`show_at`、`hide`、`is_shown`、`anchor`）；`NodeWidgets` 的 `menu()`/`context_menu()` 与 `menu_bar().menu(text)` 返回菜单 Popup（另有 `item`、`check_item`、`radio_item`、`submenu`、`separator`），MenuItem 有 `on_click`、`set_shortcut`、`set_checked`、`is_checked`；Table 有 `rows()` |
| Slider / Progress | `value`、`range`、`set_value`、`set_range`；Slider 另有 `step`、`set_step`、`increment`、`decrement`、`on_change` |
| Node（滚动） | `scroll_offset`、`max_scroll_offset`、`content_size`、`scroll_to`、`scroll_by`，用于 `scroll_view()` 返回的 Container、ListView 与编辑器；其他控件范围为零 |
| Container（绘制/列表） | `image(&Image)`、`canvas(painter)`、`list_view(height, count, row)`（`height` 为等高行的 `f32` 或 `RowHeight::Estimate`） |
| ImageView / Canvas | ImageView 有 `image`、`set_image`；Canvas 有 `invalidate`、`set_painter`、`set_input` |
| ListView | `count`、`set_count`、`row_height`、`reload`；解引用到 Node |
| loader::Program / View | `load`、`load_with(path, &Elements)`、`from_sources(entry, &Elements, read)`、`build(&Container)`、`open(&App)`（`from_checked` 仅供 `ui!` 生成的代码，文档隐藏）；View 有 `root`、`handle`、`id`、`get`、`set`、`state`、`state_at`、`reload`，`Handle::typed::<T>()` 取得有类型句柄；`State<T>` 有 `get`、`set` |
| loader::Element / Elements / element! | 标记元素契约：`element!` 声明规格与胶水并实现 `Element`；`Elements::new()` 为内置元素，`with::<E>()` 登记第三方元素 |
| Node（生命周期） | `keep_alive(value)`：值随控件删除或窗口关闭释放 |
| Ui / Window | Ui 有 `set_theme`（无局部主题节点的基础主题）、`on_key`、`set_reduced_motion`、`set_token`、`set_default_transition`；Window 有 `close` 与 `ui()`，后者给出窗口的 Ui，窗口级设置都经它完成，`window.set_theme(..)` 即根节点的局部主题；无窗口 Ui 宿主用 `take_clipboard` 取 `ClipboardRequest`、`paste` 送回读取结果；拖放用 `drag_motion`/`drag_leave`/`drop_data` 报告原生拖动、`take_drag` 取控件发起的拖动 |

`bounds` 返回最近刷新后的窗口逻辑坐标，包含呈现位移。显式设置的 size、padding、gap、字号和外观在切换主题后仍生效；`appearance` 是当前状态的逻辑外观目标。`Node::set_theme` 给子树一份局部主题，`theme` 读取解析结果；`set_offset` 在布局后平移子树，`set_transform(Transform { scale, rotation })` 以节点中心缩放/旋转子树（呈现层，可补间）；`set_theme_override(override)` 只替换指定 token 并随父主题更新；`Ui::fling`/`touch` 提供惯性滚动与手指输入。启用 motion 后用 `set_transition(Transition::default())` 安装外观与几何过渡，`set_property_transition(TransitionProperty::Scale, ..)` 逐项设置，`presented_appearance` 查询最近呈现值，`finish_transition`、`cancel_transition`、`set_transition(None)` 控制生命周期，`on_transition_end` 接收完成；详见[主题契约](components-theme-animation.md#主题契约)与[过渡契约](components-theme-animation.md#当前外观过渡)。`register_token("pkg.name", |theme| ..)` 登记类型化组件 token，`Ui::set_token`/`Node::set_token` 设全局或子树覆盖，`bind_color`/`bind_length`/`bind_shadow` 让 Style 颜色、渐变色标、宽度、圆角、字号或阴影跟随 token，直接 setter 结束绑定；详见[主题契约](components-theme-animation.md#主题契约)。当前没有通用属性表。

数值与切换控件的程序 setter 不触发用户修改回调；范围、步长、键盘及无障碍规则见[值控件契约](components-theme-animation.md#当前切换与数值控件)。

## 当前滚动契约

`scroll_view()` 返回保留状态的列容器（`Container`），默认透明并带 1dp 主题边框和半个主题 padding 的内边距；限制尺寸或 flex 分配后，两轴溢出均可滚动。`scroll_to(Point)` 和 `scroll_by(Point)` 接受有限逻辑坐标，先刷新布局再分别限制到各轴范围；负偏移归零。`scroll_offset` 是当前状态，`max_scroll_offset` 与 `content_size` 来自最近刷新布局，范围包含末尾 padding。隐藏保留偏移，但隐藏布局的范围需重新显示并刷新后才恢复。

`ensure_visible` 先刷新布局，再逐层滚动祖先，使控件或编辑器 caret 可见，不改变焦点；隐藏节点无操作。Tab 焦点和 caret 更新也使用这条显露路径。`visible_bounds` 返回最近刷新几何与祖先滚动视口的交集；隐藏或完全裁剪时为 None，不额外裁剪到窗口边缘。移出视口不销毁控件。嵌套视口和编辑器通过 `Ui::scroll_by(position, delta)` 将未消费的双轴滚轮位移向外传递。

滚动裁剪为直角矩形，不随外观圆角改变。自有宿主的 `Ui::visit_scenes` 回调按顺序接收 `Visit`：`Scene { scene, transform, clip }` 是保留绘制记录、平移和窗口逻辑坐标裁剪，宿主必须应用裁剪；`PushLayer(Layer)` 与 `PopLayer` 包住有组透明度或背景模糊的子树，宿主缩放后交给 renderer 的 `push_layer`/`pop_layer`。某轴溢出时 ScrollView 在该轴末端绘制滚动条：12dp 指针带内贴外缘是轨道与胶囊形滑块（最短 24dp），静止时 4dp 粗，视图悬停或拖动时 8dp，两端让开视图圆角，两轴同时出现时纵轴让出角落；多行编辑器只绘制纵向滑块。溢出的一侧在内边距之外为滚动条留出 14dp（轨道、边距与 4dp 间隙），内容不会滚到滚动条下面。滑块在子树之后绘制，外层视口优先命中。按下滑块拖动，按下滑块外的指针带先把滑块中心移到该处再拖动；拖动期间其他控件不接收该指针事件，取消、隐藏或删除结束拖动且保留最后偏移。轨道与滑块颜色取自外观的 `scrollbar` 字段，默认为主题的 pressed、border 与 muted；没有淡出计时器或动画。当前没有 ScrollView 独立键盘焦点或滚动动画。可执行嵌套表单示例：`cargo run -p aegle --example scrolling`。

## 当前图像、画布与虚拟列表

`aegle::ui::scene` 重新导出绘制命令。`image(&Image)` 以像素尺寸为固有逻辑尺寸，交叉轴不拉伸，`set_width`/`set_height` 后按边界拉伸，不保持宽高比；导出 Image 角色。像素须由调用方解码，App 不内置 PNG/JPEG 解码器。`canvas(painter)` 的 painter 以局部坐标和当前尺寸录制 scene 命令，只在创建、尺寸变化、`invalidate` 或 `set_painter` 后重新执行；它在刷新期间持有 UI 借用，不能使用控件句柄，绘制不裁剪到边界；`set_input(|canvas, event| ...)` 使其可聚焦并在借用之外按序接收 `CanvasEvent`（按下/移动/释放/离开/取消/滚轮/按键/焦点，以及右键、中键与侧键的 `ButtonPress`/`ButtonRelease`），作为自定义输入行为；实现 `Control` 的控件从 `PointerKind::ButtonDown`/`ButtonUp` 收到同样的按键；导出 Canvas 角色。需要逐帧动画的自定义控件在 `paint` 中调用 `PaintCx::request_frame`，以 `cx.time` 计算进度；需要悬停或延时的扩展安装 `Hooks::hover` / `Hooks::wake` 并设置 `State::wake`。

`list_view(row_height, count, row)` 为等高虚拟列表：间隔节点高 `count × row_height`（不超过 16,777,216），只有与视口、祖先裁剪和窗口相交的行作为真实控件存在。`Ui::refresh` 在借用外先为新进入的行建立空列并调用 `row(&Container, index)`，再布局；离开的行连同焦点和局部状态删除，因此只能 Tab 到已存在的行。行按索引插入以保持焦点顺序，行内容溢出行高时不裁剪。`set_count` 删除超出的行，`reload` 在下次刷新重建全部已存在行。列表默认按 flex 收缩到父容器剩余空间，因此嵌套在 ScrollView 中时自身滚动。辅助技术只看到已建立的行，不报告总行数。

## 当前可用的组件皮肤

普通函数组合已有控件即可复用输入与语义；不要求组件宏或注册器。例如：

```rust
fn primary(parent: &Container, text: &str) -> Button {
    let button = parent.button(text);
    button.set_background(Color::rgb(103, 80, 164));
    button.set_foreground(Color::WHITE);
    button.set_hover_background(Color::rgb(91, 68, 130));
    button.set_radius(18.0);
    button
}
```

需要随主题、禁用、按压和焦点改变外观时使用皮肤纯函数：`set_skin` 给一个控件，`set_kind_skin` 给一个子树里某类型的全部控件；可执行示例为 `cargo run -p aegle --example components`，规则见[组件样式](components-theme-animation.md)。需要新的行为或绘制时实现 `Control`，见[组件库作者](#组件库作者)。

## 目标接口族

以下覆盖完整版本的设计范围，超出上表的接口不应当作当前 API 使用。

| 接口族 | 必须覆盖的操作 |
| --- | --- |
| 结构 | 创建、添加/移除、移动父节点、按显式 ID 查询、子节点遍历、显示/隐藏 |
| 内容 | 文本、图标路径、解码图像/图像资源、tooltip/标签关联 |
| 布局 | 尺寸约束、内外边距、gap、Flex、对齐、滚动；可选 Grid |
| 视觉 | 颜色、边框、圆角、透明度、二维变换、裁剪；可选路径/阴影/模糊 |
| 状态 | enabled、checked、selected、expanded、value、编辑只读/密码模式 |
| 事件 | 指针、键盘、焦点、控件动作、值变化；停止传播与阻止默认动作 |
| 语义 | role、label、description、关系、范围和动作；自定义语义合并 |
| 主题与动画 | 选择主题、局部覆盖、类型化 token、过渡、显式动画和取消 |
| 平台 | 窗口属性、可用能力、显示输出、剪贴板、拖放、关闭；可选 layer-shell |

`get_*` 返回逻辑目标值；动画呈现值通过 `presented_*` 查询。几何默认是最近提交的布局结果；需要立即读取新布局时使用显式 `flush_layout()`，该调用可有较高成本且禁止在布局/绘制回调内重入。

`app.batch(|| ...)` 合并失效提交，不提供事务回滚；回调内修改本来就会合并。运行模型详见[架构](architecture.md)。

## 组件库作者

自定义控件实现 `aegle_ui::Control`，经 `Container::add(|state, theme| Ok((Box::new(control), style)))` 加入树；输入、测量、绘制与语义分别经 `InputCx`、`MeasureCx`、`PaintCx`、`SemanticsCx` 访问，段落、编辑器与视口以能力方法（`paragraph`、`editor`、`viewport` 等）暴露；字号或字体变化时引擎调用 `restyle`，默认重排段落或编辑器，显示更多文字的控件（如菜单项的快捷键提示）覆盖它一并重排。文字有自己排版角色的控件（如 Material 的 label/title 角色）实现 `text_role`，把节点文字样式换算为角色的字号、行高、字重与字距；引擎在设置文字、字体变化与主题变化时都先经它换算，控件无需覆盖这些路径。在编辑器周围绘制标签或图标的控件用 `content_offset` 给出文字原点、用 `text_viewport` 给出可见文字区域，滚动据此保持插入点可见。需要跨节点协作的行为安装 `Hooks`，库数据放在 `State::ext`。

- `kind()` 返回控件类型 `&'static ControlKind`：控件库为自己的每种控件声明一个 `static`（名称、默认皮肤、接受的样式组 `Accepts`、是否为布局容器），与 `aegle_widgets::kinds` 中的内置类型完全同一形式；类型在节点存活期间不变。默认皮肤即该类型的第一方皮肤，`set_kind_skin` 与 `set_skin` 在子树或单个节点上替换它（优先级见[API 指南](developer/api.md#6-外观主题样式与皮肤)）。类型化句柄用 `aegle_ui::handle! { pub Name(NameControl): text, interactive, pressed, indicator, editor }` 定义：声明控件类型后句柄得到 `read(|c| ..)` 与 `update(|c| ..)`（修改后自动请求重绘并更新语义；尺寸变化走布局），列出的样式组（与类型的 `accepts` 相符）生成对应 setter；Rust 1.86 起 trait 对象可直接向上转型为 `dyn Any`，`Control` 不需要 `as_any`；文字读写用 `State::text`/`State::set_text`；句柄的 `on_*` 方法用 `Node::on_action` 登记处理器，控件用 `State::queue_action` 报告用户操作。句柄方法经 `Node::change` 访问状态，内部的 `Result` 在那里变成 panic；参数检查用 `aegle_ui::require`，其余内部结果用 `OrFail::or_fail`。接受 `Accepts::EDITOR` 的类型必须提供 `editor()`，其他类型不能提供；不一致时 `Container::add` 以 `WrongKind` panic。
- 不改行为、只给已有控件（含内置控件）加绘制时实现 `aegle_ui::Decorator` 并用 `node.decorate(..)` 挂上：`input` 在控件处理完输入后以节点局部坐标观察它（不能吞掉或改变结果，返回是否重绘），`under` 在背景与边框之前、`over` 在内容之后焦点环之前，以与控件相同的 `PaintCx` 录制；动画中的装饰器每次绘制调用 `request_frame`，静止时不产生帧。例如按下涟漪：`input` 记下按下点，`over` 按 `cx.time` 画随时间扩大并裁剪到 `cx.shape` 的圆（`aegle-widgets/tests/decorators.rs`）。装饰器随节点删除。
- `retheme(theme, local, root, style)` 在主题变化时更新跟随主题的布局；`local: LocalLayout` 标出应用设置过、需要保留的高度、内边距、间距和最小高度。
- `paint` 与 `Hooks` 在 Ui 借用期间运行，只能使用传入的 `State`/上下文；此时调用 Ui 或句柄的方法以 `ReentrantAccess` panic；钩子返回的错误原样传给宿主。
- `Control` 与 `Decorator` 的方法（`handle`、`hover`、`measure`、`finalize`、`restyle`、`paint`、`paint_overlay`、`under`、`over`）直接返回值，不返回 `Result`：其中的失败只能来自控件代码的错误，录制 scene 的非法几何直接 panic，其余内部结果用 `OrFail::or_fail` 展开。外部数据在 `Ui` 入口校验：指针坐标在 `Ui::pointer`，输入法批次在 `Ui::ime`（`Editor::check_ime`），出错时返回给宿主、不进入控件。

- 控件出现在标记中用 `aegle::element!` 声明元素：构造参数、可绑定属性、事件与 `self` 字段，与内置元素走同一条检查与构建路径（[ADR 0004](adr/0004-element-contract.md)，写法见[编写控件库 §6](developer/library.md#6-标记元素)）。

可执行示例 `cargo run -p aegle-widgets --example custom_control` 只依赖 `aegle-ui` 实现一个带指针、键盘、语义动作与共享外观的评分控件；`aegle_ui::control` 重导出 `Input`、`Outcome`、`Action` 等类型，控件库无需直接依赖 `aegle-controls`。完整顺序见[编写控件库](developer/library.md)。

## 所有权与异步

Ui 拥有控件树；App 持有各窗口 Ui，控件句柄为弱引用和代数 ID。处理器可以捕获其他控件或窗口句柄而不形成强拥有环。UI 句柄不能发送到后台线程；`App::proxy(handler)` 返回可克隆、可发送的 `UiProxy<T>`，`send` 的消息由 UI 线程上的 handler 处理，handler 里持有的弱句柄在目标已销毁后会 panic，handler 应先检查 `is_alive()`。`App::desktop(app_id, handler)`（`desktop-services` feature）在此之上接入文件对话框、通知、托盘与全局快捷键的事件。

第三方任务系统的 post/取消句柄也是后续目标。框架不要求应用把所有函数变成 async，不让文本输入处理等待任意网络任务。

## 当前可用的底层接口

`Tree` 保存实际状态，`Focus::set/advance` 按宿主策略处理焦点；这不是第二套函数式 UI 入口。输入只交给命中、捕获或聚焦的目标控件，没有捕获或冒泡阶段。`Button::handle(Input)` 返回激活、capture、焦点和绘制效果，`TextField::handle(&mut TextSystem, Input)` 在同一个 Editor 上实现编辑行为。控件不隐式获取平台服务。

IME 数据通过 `EditorDriver::apply_ime` 一次验证并应用，`Editor::surrounding` 借出有界周边文字。调用方消费 `Outcome` 与 `Editor::take_changes` 后同步布局、平台和绘制。`aegle --example controls` 展示应用层创建、编辑、按钮回调、主题切换与关闭窗口，使用上面的统一接口。
