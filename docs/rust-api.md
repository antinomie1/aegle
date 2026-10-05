# Rust 命令式 API

状态：v0.1。基础 Ui/App、弱句柄、命令式控件与静态标记编译已有源码；组件宏、动态标记和完整扩展接口仍是设计目标，下文分别标注。验证记录见[实现状态](implementation.md)。项目采用 Rust 2024，编译器基线见[依赖](dependencies.md)。

## 完整 Hello world

```rust
use aegle::prelude::*;
fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Hello")?;
    window.text("你好，世界")?;
    app.run()
}
```

源码位于 `crates/aegle/examples/hello.rs`，7 行 Rust 加 1 行文档注释，包含导入、入口、初始化和事件循环；运行入口为 `cargo run -p aegle --example hello`。当前需要 Linux Wayland 和可显示所用字符的系统字体。按 rustfmt 后非空源码行计数，不将多个语句强压在同一行。Cargo 清单不计；任何必须手写的应用初始化辅助文件计入。

当前 `App::new` 按目标连接 Wayland / Win32 并建立系统字体上下文；`run(self)` 在主线程接管循环，最后一个窗口关闭后退出。各窗口的控件树独立，字体系统和软件 renderer 共享。`App::with_fonts` 接受显式字体集合，可关闭 `system-fonts`；`AppOptions` 配置 app_id、初始主题、renderer 选择、可选 Vulkan 预算和软件 mask 预算；motion feature 另提供 transition 与 reduced_motion，`WindowOptions` 配置初始尺寸与 SHM 预算，Linux 上可选 `layer: Some(LayerOptions)` 创建 wlr layer-shell 面板/覆盖层（compositor 缺少协议时创建失败）。macOS 尚未实现；Vulkan 通过原生 swapchain 呈现，Windows 当前输入法为 IMM 兼容路径。

`aegle` 当前默认启用 Wayland、系统字体、Unix 无障碍、编译型静态标记与外观过渡。嵌入式宿主可直接使用无默认平台 feature 的 `aegle-app::Ui::with_fonts(Rc<RefCell<TextSystem>>, Theme)`，取得 root 后创建同样的控件；通过输入、`refresh`、`visit_scenes`、IME 和可选语义接口对接自己的宿主。

界面优先写在 `.aegle` 文件中；`App::run_ui(aegle::ui!("main.aegle"))` 完成默认初始化、构造和运行。`let view = aegle::ui!(&window, "panel.aegle")?` 返回带 `root` 和各 `id` 字段的有类型弱句柄集合，可直接给 `view.done.on_click(...)` 绑定下面的普通 Rust 回调。文件路径相对使用者包清单目录，编译器跟踪其变化；完整已实现属性见[标记语言](markup.md)。

## 创建、修改与事件

```rust
let label = window.text("等待操作")?;
let button = window.button("完成")?;
button.on_click(move |_| {
    label.set_text("已完成")?;
    Ok(())
})?;
```

控件创建一次。`Window` 解引用到根 `Container`；容器提供 `row`、`column`、`scroll_view`、`text`、`button`、`text_field`、`text_area`，返回相应弱句柄。多行与单行编辑器共用 `TextField` 句柄。设置是直接命令，不要求嵌套函数、消息枚举或 builder 链。`parent.add(component)` 和生成的第三方组件函数仍是后续扩展目标。

同一个 `on_click` 再次设置时替换前一处理器，`clear_on_click` 删除处理器；单行编辑器的 `on_submit` 使用相同规则。排队的动作携带注册版本，旧动作不会误调用替换后的处理器。处理器在树和原生宿主借用之外执行，可修改其他控件、删除自己或关闭窗口。回调中产生的新动作留待下一轮；控件销毁清理处理器，丢弃普通句柄不销毁控件。通用 `listen` 多订阅接口尚未实现。

公开创建和修改操作返回 `Result`；当前错误保留底层来源，包括 `DeadHandle`、跨 Ui 父节点、不合法数值、字体/呈现预算及平台能力错误。回调出错时删除失败处理器，保留此前合法修改；`App::run` 返回错误并结束。可配置 `App::on_error` 尚未实现。

当前公共操作包括：

| 类型 | 已有接口 |
| --- | --- |
| Node / 所有控件句柄 | `is_alive`、`bounds`、`visible_bounds`、`ensure_visible`、`remove`、`reparent`、`set_visible`、`set_enabled`、`focus`、`set_accessible_label` |
| 布局 | `set_size`、`set_width`、`set_height`、`set_min_size`、`set_min_width`、`set_min_height`、`set_grow`、`set_padding`、`set_gap` |
| 外观 | `set_style`、`style`、`set_skin`、`clear_skin`、`appearance`、`visual_state`；背景/前景、状态背景、边框、圆角、焦点环和编辑器颜色 setter |
| 局部主题与位移 | `set_theme(Option<Theme>)`、`theme`；`set_offset(Point)`、`offset` |
| 过渡（motion） | `set_transition`、`clear_transition`、`presented_appearance`、`is_animating`、`finish_transition`、`cancel_transition`、`on_transition_end`、`clear_on_transition_end` |
| 字号 | `set_font_size`、`clear_font_size`，限文字控件，保留输入/组合状态 |
| Label / TextField | `text`、`set_text`；TextField 另有 `select`、`set_read_only`、`set_password`、`on_submit`、`clear_on_submit` |
| Button | `set_text`、`activate`、`on_click`、`clear_on_click` |
| Container（值控件） | `check_box(text, checked)`、`switch(text, checked)`、`slider(min, max, value)`、`progress(min, max, value)` |
| CheckBox / Switch | `is_checked`、`set_checked`、`toggle`、`text`、`set_text`、`on_change`、`clear_on_change` |
| Slider / Progress | `value`、`range`、`set_value`、`set_range`；Slider 另有 `step`、`set_step`、`increment`、`decrement`、`on_change`、`clear_on_change` |
| ScrollView | `offset`、`max_offset`、`content_size`、`scroll_to`、`scroll_by`；解引用到 Container |
| Container（绘制/列表） | `image(&Image)`、`canvas(painter)`、`list_view(row_height, count, row)` |
| ImageView / Canvas | ImageView 有 `image`、`set_image`；Canvas 有 `invalidate`、`set_painter` |
| ListView | `count`、`set_count`、`row_height`、`reload`；解引用到 ScrollView |
| Ui / Window | `set_theme`；Window 另有 `close`；无窗口 Ui 宿主用 `take_clipboard` 取 `ClipboardRequest`、`paste` 送回读取结果 |

`bounds` 返回最近刷新后的窗口逻辑坐标，包含呈现位移。显式设置的 size、padding、gap、字号和外观在切换主题后仍生效；`appearance` 是当前状态的逻辑外观目标。`Node::set_theme` 给子树一份局部主题，`theme` 读取解析结果；`set_offset` 在布局后平移子树。启用 motion 后用 `set_transition(Transition::default())` 安装外观与位移过渡，`presented_appearance` 查询最近呈现值，`finish_transition`、`cancel_transition`、`clear_transition` 控制生命周期，`on_transition_end` 接收完成；详见[主题契约](components-theme-animation.md#主题契约)与[过渡契约](components-theme-animation.md#当前外观过渡)。当前没有通用属性表或 token 注册表。

数值与切换控件的程序 setter 不触发用户修改回调；范围、步长、键盘及无障碍规则见[值控件契约](components-theme-animation.md#当前切换与数值控件)。

## 当前滚动契约

`ScrollView` 是保留状态的列容器；限制尺寸或 flex 分配后，两轴溢出均可滚动。`scroll_to(Point)` 和 `scroll_by(Point)` 接受有限逻辑坐标，先刷新布局再分别限制到各轴范围；负偏移归零。`offset` 是当前状态，`max_offset` 与 `content_size` 来自最近刷新布局，范围包含末尾 padding。隐藏保留偏移，但隐藏布局的范围需重新显示并刷新后才恢复。

`ensure_visible` 先刷新布局，再逐层滚动祖先，使控件或编辑器 caret 可见，不改变焦点；隐藏节点无操作。Tab 焦点和 caret 更新也使用这条显露路径。`visible_bounds` 返回最近刷新几何与祖先滚动视口的交集；隐藏或完全裁剪时为 None，不额外裁剪到窗口边缘。移出视口不销毁控件。嵌套视口和编辑器通过 `Ui::scroll_by(position, delta)` 将未消费的双轴滚轮位移向外传递。

滚动裁剪为直角矩形，不随外观圆角改变。自有宿主的 `Ui::visit_scenes` 回调接收 `(&Scene, Affine, Option<Rect>)`：分别为保留绘制记录、平移和窗口逻辑坐标裁剪；宿主必须应用裁剪。某轴溢出时 ScrollView 在该轴末端绘制覆盖式滚动条：不占布局空间，12dp 指针带内贴边 6dp 直角滑块（最短 24dp），两轴同时出现时纵轴让出角落；多行编辑器只绘制纵向滑块。滑块在子树之后绘制，外层视口优先命中。按下滑块拖动，按下滑块外的指针带先把滑块中心移到该处再拖动；拖动期间其他控件不接收该指针事件，取消、隐藏或删除结束拖动且保留最后偏移。滑块静止用 theme `border`，悬停或拖动用 `muted`，没有淡出计时器或动画。当前没有 ScrollView 独立键盘焦点或滚动动画。可执行嵌套表单示例：`cargo run -p aegle --example scrolling`。

## 当前图像、画布与虚拟列表

`aegle::scene` 重新导出绘制命令。`image(&Image)` 以像素尺寸为固有逻辑尺寸，交叉轴不拉伸，`set_size` 后按边界拉伸，不保持宽高比；导出 Image 角色。像素须由调用方解码，App 不内置 PNG/JPEG 解码器。`canvas(painter)` 的 painter 以局部坐标和当前尺寸录制 scene 命令，只在创建、尺寸变化、`invalidate` 或 `set_painter` 后重新执行；它在刷新期间持有 UI 借用，不能使用控件句柄，绘制不裁剪到边界，只是绘制扩展，不提供自定义输入行为；导出 Canvas 角色。

`list_view(row_height, count, row)` 为等高虚拟列表：间隔节点高 `count × row_height`（不超过 16,777,216），只有与视口、祖先裁剪和窗口相交的行作为真实控件存在。`Ui::refresh` 在借用外先为新进入的行建立空列并调用 `row(&Container, index)`，再布局；离开的行连同焦点和局部状态删除，因此只能 Tab 到已存在的行。行按索引插入以保持焦点顺序，行内容溢出行高时不裁剪。`set_count` 删除超出的行，`reload` 在下次刷新重建全部已存在行。列表默认按 flex 收缩到父容器剩余空间，因此嵌套在 ScrollView 中时自身滚动。辅助技术只看到已建立的行，不报告总行数。

## 当前可用的组件皮肤

普通函数组合已有控件即可复用输入与语义；不要求组件宏或注册器。例如：

```rust
fn primary(parent: &Container, text: &str) -> Result<Button> {
    let button = parent.button(text)?;
    button.set_background(Color::rgb(103, 80, 164))?;
    button.set_foreground(Color::WHITE)?;
    button.set_hover_background(Color::rgb(91, 68, 130))?;
    button.set_radius(18.0)?;
    Ok(button)
}
```

需要随主题、禁用、按压和焦点改变外观时使用 `set_skin` 纯函数入口；可执行示例为 `cargo run -p aegle --example components`，规则见[组件样式](components-theme-animation.md)。当前仅修改已有控件外观，下文任意绘制/行为扩展仍为目标。

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
| 平台 | 窗口属性、可用能力、显示输出、剪贴板、关闭；可选 layer-shell |

`get_*` 返回逻辑目标值；动画呈现值通过 `presented_*` 查询。几何默认是最近提交的布局结果；需要立即读取新布局时使用显式 `flush_layout()`，该调用可有较高成本且禁止在布局/绘制回调内重入。

`app.batch(|| ...)` 合并失效提交，不提供事务回滚；回调内修改本来就会合并。运行模型详见[架构](architecture.md)。

## 组件库作者：后续扩展目标

本节的宏、Control/Painter/Semantics 和注册接口尚未实现。当前可直接组合 Container 的现有控件，或用独立 controls/text/scene 模块编写自己的宿主；尚不能用下面的代码定义可插入 App 的任意皮肤。

简单皮肤通过现有行为创建，不重写输入和无障碍：

```rust
#[aegle::component]
pub fn QuietButton(parent: &Container, text: &str) -> Result<Button> {
    let button = parent.control_button(text)?;
    button.set_background(theme::SURFACE)?;
    button.set_foreground(theme::ON_SURFACE)?;
    button.set_radius(8.dp())?;
    button.set_hover_background(theme::HOVER)?;
    button.transition(Background, 120.ms(), EaseOut)?;
    Ok(button)
}
```

`control_button` 自带按钮语义、标签关联、焦点、键盘与指针激活及禁用规则，不带默认皮肤。组件宏生成可选的标记属性/构造描述，不将运行时反射加入不使用 loader 的构建。事件继续通过返回的 Button 句柄绑定。

复杂控件通过 `Control` 接口定义状态与事件行为，通过 `Painter` 输出 scene 命令，通过 `Semantics` 定义辅助技术行为。布局使用共享的 layout 属性及文字测量服务。只有绘制自定义形状时才需要 Painter，不强制每个组件实现一组空 trait 方法。

控件可通过 `register_component::<T>("package.Type")` 加入运行时注册表；编译型标记的 `use` 映射到 Rust 路径，直接调用构造器。模块内使用有类型的属性，运行时加载边界才进行 Value 类型检查。

## 所有权与异步

Ui 拥有控件树；App 持有各窗口 Ui，控件句柄为弱引用和代数 ID。处理器可以捕获其他控件或窗口句柄而不形成强拥有环。UI 句柄不能发送到后台线程；`UiProxy` 尚未实现，后续由其投递后在主线程重新定位 ID，目标已销毁时返回/报告 DeadHandle。

第三方任务系统的 post/取消句柄也是后续目标。框架不要求应用把所有函数变成 async，不让文本输入处理等待任意网络任务。

## 当前可用的底层接口

`Tree` 保存实际状态，`Route::rebuild/iter` 构造捕获/目标/冒泡路径，`Focus::set/advance` 按宿主策略处理焦点；这不是第二套函数式 UI 入口。`Button::handle(Input)` 返回激活、capture、焦点和绘制效果，`TextField::handle(&mut TextSystem, Input)` 在同一个 Editor 上实现编辑行为。控件不隐式获取平台服务。

IME 数据通过 `EditorDriver::apply_ime` 一次验证并应用，`Editor::surrounding` 借出有界周边文字。调用方消费 `Outcome` 与 `Editor::take_changes` 后同步布局、平台和绘制。`aegle-platform-wayland --example editor` 保留底层显式组装示例；`aegle --example controls` 展示应用层创建、编辑、按钮回调、主题切换与关闭窗口，使用上面的统一接口。
