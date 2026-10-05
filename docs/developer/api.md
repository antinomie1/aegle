# Aegle 开发者 API 指南

本文面向用 Aegle 编写应用的开发者，按任务介绍当前已实现的接口。各控件的外观、状态和专属方法见[控件参考](controls.md)；设计契约与验证记录分别见 [Rust API 契约](../rust-api.md)、[标记语言](../markup.md)和[实现状态](../implementation.md)。

Aegle 是保留模式 GUI：控件创建一次，之后通过句柄修改，没有每帧重建界面的入口。几乎所有调用都返回 `aegle::Result<T>`（`Result<T, Box<dyn Error>>`），用 `?` 传递即可。

## 1. 依赖与 feature

当前未发布到 crates.io，以 path 或 git 依赖引入 facade crate `aegle`：

```toml
[dependencies]
aegle = { path = "../aegle/crates/aegle" }
```

| feature | 默认 | 作用 |
| --- | :---: | --- |
| `native` | ✓ | 原生窗口：Linux Wayland、Windows Win32（按编译目标选择） |
| `software` | ✓ | CPU 软件绘制 |
| `system-fonts` | ✓ | 系统字体发现（Linux 链接 Fontconfig） |
| `markup` | ✓ | `ui!` 宏与 `aegle::loader` 动态标记引擎 |
| `motion` | ✓ | 外观/位移过渡与动画 |
| `vulkan` |  | Vulkan 绘制，与 `software` 可同时编译 |
| `accessibility` |  | 语义树导出（`Ui::accessibility`），不接系统 |
| `unix-accessibility` |  | Linux AT-SPI 适配，引入 zbus |
| `windows-accessibility` |  | Windows UI Automation 适配 |

Linux 构建需要 libxkbcommon 开发文件（pkg-config），运行需要 libxkbcommon；启用 `system-fonts` 时还需要 Fontconfig。最小化依赖：`default-features = false` 后按需选择，例如 `features = ["native", "software"]` 并显式注册字体。

## 2. 最小程序

命令式：

```rust
use aegle::prelude::*;

fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Hello")?;
    window.text("你好，世界")?;
    app.run()
}
```

标记（`hello.aegle` 与 `main.rs`，路径相对于包的 `Cargo.toml`）：

```text
Window {
    title: "Hello"
    Text { text: "你好，世界" }
}
```

```rust
fn main() -> aegle::Result<()> {
    aegle::App::run_ui(aegle::ui!("examples/hello.aegle"))
}
```

可运行示例在 `crates/aegle/examples/`：`hello`、`controls`、`widgets`、`components`、`scrolling`、`visuals`、`dynamic` 等，使用 `cargo run -p aegle --example 名称`。

## 3. 应用与窗口

`App` 是主线程上的原生应用，拥有平台连接、共享字体和 renderer；每个窗口有独立的控件树。

```rust
use aegle::prelude::*;

let app = App::with_options(AppOptions {
    app_id: "com.example.notes".into(),
    theme: Theme::light(),
    ..Default::default()
})?;
let window = app.window_with_options("Notes", WindowOptions { width: 640, height: 480, ..Default::default() })?;
```

| 接口 | 说明 |
| --- | --- |
| `App::new()` / `App::with_options(opts)` | 连接平台并发现系统字体（需 `system-fonts`） |
| `App::with_fonts(TextSystem, opts)` | 使用显式字体集合，不做系统发现 |
| `App::run_ui(build)` | 创建 App、调用构造闭包（如 `ui!` 返回的闭包）并运行 |
| `app.window(title)` / `window_with_options(title, opts)` | 新窗口；`Window` 解引用为根 `Container`（列） |
| `app.run()` | 阻塞运行，直到最后一个窗口关闭或回调返回错误 |
| `app.dispatch(Some(timeout))` | 单步运行，供需要自己循环的宿主；返回是否仍有窗口 |
| `app.preferences()` | 最近应用的系统偏好 `Preferences { dark, high_contrast, reduced_motion }` |
| `app.ime_available()` | 平台是否支持原生输入法组合 |
| `window.close()` | 关闭窗口并使其所有控件句柄失效 |
| `window.set_theme(theme)` / `window.set_reduced_motion(b)` | 窗口级主题与减少动态效果 |

`AppOptions` 字段：`app_id`；`theme`、`dark_theme: Option<Theme>`、`high_contrast_theme: Option<Theme>`（按系统偏好选择，`None` 忽略该偏好）；`renderer: RendererBackend`（`Software`/`Vulkan`，编译了软件绘制时默认软件）；`vulkan`（Vulkan 预算）；`mask_budget`；`transition: Option<Transition>`（交互控件默认过渡，默认 120 ms ease-out）；`reduced_motion: Option<bool>`（`None` 跟随系统）。

`WindowOptions` 字段：`width`、`height`（逻辑像素建议值）、`buffer_budget`（软件呈现字节上限），Linux 另有 `layer: Option<LayerOptions>`，用 wlr layer-shell 创建面板/背景/覆盖层：

```rust
use aegle::{Anchor, KeyboardInteractivity, Layer, LayerOptions};

let panel = app.window_with_options("Panel", WindowOptions {
    height: 32,
    layer: Some(LayerOptions {
        layer: Layer::Top,
        anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
        exclusive_zone: 32,
        keyboard: KeyboardInteractivity::None,
    }),
    ..Default::default()
})?;
```

## 4. 控件树与句柄

在 `Container`（列、行、ScrollView 和窗口根）上创建子控件，新控件追加到末尾：

| 方法 | 返回 | 控件 |
| --- | --- | --- |
| `column()` / `row()` | `Container` | 纵向 / 横向容器 |
| `scroll_view()` | `ScrollView` | 可滚动列 |
| `list_view(row_height, count, row)` | `ListView` | 等高虚拟列表 |
| `text(s)` | `Label` | 文本 |
| `button(s)` | `Button` | 按钮 |
| `text_field(s)` / `text_area(s)` | `TextField` | 单行 / 多行编辑器 |
| `check_box(s, checked)` / `switch(s, checked)` | `CheckBox` / `Switch` | 二态控件 |
| `slider(min, max, value)` / `progress(min, max, value)` | `Slider` / `Progress` | 数值控件 |
| `image(&Image)` | `ImageView` | 图像 |
| `canvas(painter)` | `Canvas` | 自定义绘制 |

所有类型化句柄都解引用为 `Node`，共享以下方法：

| `Node` 方法 | 说明 |
| --- | --- |
| `is_alive()` | 控件是否仍存在 |
| `bounds()` / `visible_bounds()` | 最近刷新后的窗口逻辑坐标 / 与祖先视口的交集 |
| `remove()` / `reparent(&container)` | 删除子树 / 移到另一容器末尾 |
| `set_visible(b)` / `set_enabled(b)` | 作用于整棵子树；隐藏不占布局 |
| `focus()` / `ensure_visible()` | 聚焦 / 滚动祖先使其可见 |
| `set_accessible_label(s)` | 无障碍名称 |
| `keep_alive(value)` | 让任意值与控件同生命周期 |

句柄是弱引用：丢弃句柄不会删除控件；删除控件或关闭窗口后，其句柄的调用返回 `UiError::DeadHandle`。句柄可以克隆后移入回调。

## 5. 布局

布局由 Taffy flexbox 计算。列/行的子控件沿主轴排列，交叉轴默认拉伸。

| 方法 | 说明 |
| --- | --- |
| `set_size(w, h)`、`set_width(Some(w))`、`set_height(None)` | 固定尺寸，`None` 恢复自动 |
| `set_min_size(Size)`、`set_min_width`、`set_min_height` | 最小尺寸 |
| `set_grow(f)` | 占用剩余空间的比例 |
| `set_padding(p)` | 内边距（文字控件的内容边距） |
| `set_gap(g)` | 容器子控件间距 |

默认值来自主题：控件高 36、间距 8、窗口根内边距 8，文字 14。显式设置的布局值在切换主题后保留。文字按父宽度自动换行。内容超出时放进 `ScrollView` 或 `ListView`。

<img src="images/layout.png" width="690" alt="列、行与 grow 布局">

## 6. 外观：主题、样式与皮肤

**主题** `Theme` 是一组颜色与尺寸：`background`、`surface`、`foreground`、`muted`、`accent`、`border`、`hover`、`pressed`、`selection`、`font_size`、`padding`、`gap`、`radius`（默认 0，直角）、`control_height`。内置 `Theme::light()`、`dark()`、`high_contrast()`，可修改字段后用 `validate()` 检查。

```rust
window.set_theme(Theme { radius: 6.0, ..Theme::dark() })?;   // 整个窗口
panel.set_theme(Some(Theme::dark()))?;                      // 只作用于 panel 子树
panel.set_theme(None)?;                                     // 恢复继承
```

**局部样式** 覆盖单个控件，优先于主题和皮肤：`set_background`、`set_foreground`、`set_border_color`、`set_border_width`、`set_radius`、`set_focus_color`、`set_focus_width`、`set_hover_background`、`set_pressed_background`、`set_disabled_background`、`set_disabled_foreground`、`set_selection_color`、`set_caret_color`、`set_indicator_color`，以及字号 `set_font_size` / `clear_font_size`。也可以用 `set_style(Style { .. })` 一次设置，`style()` 读取，`appearance()` 返回当前解析结果。不适用的属性返回 `UiError::WrongKind`。

**皮肤** 是纯函数 `fn(&Theme, VisualState) -> Appearance`，可替换默认外观而不改行为，适合组件库：

```rust
fn primary(theme: &Theme, state: VisualState) -> Appearance {
    let mut look = Appearance::new(theme, state);
    look.radius = 18.0;
    look.border_width = 0.0;
    look.background = if state.pressed { Color::rgb(66, 45, 103) } else { Color::rgb(103, 80, 164) };
    look.foreground = Color::WHITE;
    look
}
button.set_skin(primary)?;
```

`VisualState` 含 `kind`、`enabled`、`hovered`、`pressed`、`focused`、`read_only`、`checked`。

<img src="images/styles.png" width="600" alt="默认、圆角、自定义颜色与局部深色主题">

不同主题下的同一组控件：

<img src="images/theme-light.png" width="280" alt="浅色主题"> <img src="images/theme-dark.png" width="280" alt="深色主题"> <img src="images/theme-high-contrast.png" width="280" alt="高对比主题">

原生 App 默认跟随系统深浅色、高对比与减少动态效果（见第 3 节 `AppOptions`）。注意：当前高对比主题的 `muted` 为白色，禁用控件与启用控件外观相同，只能依靠语义区分。

## 7. 事件与回调

```rust
let status = window.text("Ready")?;
let save = window.button("Save")?;
save.on_click(move |_button| status.set_text("Saved"))?;
```

| 控件 | 事件 | 清除 |
| --- | --- | --- |
| `Button` | `on_click(FnMut(Button) -> Result)` | `clear_on_click()` |
| `TextField`（单行） | `on_submit(FnMut(TextField) -> Result)`，Enter 触发 | `clear_on_submit()` |
| `CheckBox` / `Switch` / `Slider` | `on_change(FnMut(Self) -> Result)` | `clear_on_change()` |
| 任意控件（`motion`） | `on_transition_end(FnMut(Node) -> Result)` | `clear_on_transition_end()` |

- 回调在本批输入处理后、所有 UI 借用之外执行，可以自由创建、修改或删除控件，包括关闭窗口。
- 回调返回错误时，该处理器被移除，`App::run` 返回此错误并结束。
- 程序 setter（`set_checked`、`set_value`、`set_text` 等）不触发回调，可安全地相互同步；`activate()`、`toggle()`、`increment()`/`decrement()` 模拟用户操作并触发回调。
- 每个控件每种事件只有一个处理器，再次设置会替换。

## 8. 过渡与动画（`motion`）

```rust
use std::time::Duration;

card.set_transition(Transition::new(Duration::from_millis(180), Easing::EaseOut))?;
card.set_background(Color::rgb(235, 240, 255))?;      // 外观变化按过渡补间
card.set_offset(Point::new(0.0, 480.0))?;               // 布局后平移，命中与 IME 跟随
card.on_transition_end(move |_| window.close())?;       // 全部过渡完成后执行
```

- 外观过渡覆盖背景、文字、边框、圆角、焦点环、选择、caret 和标志颜色；`presented_appearance()` 返回当前呈现值。
- `finish_transition()` 立即到终点并完成；`cancel_transition()` 停在当前呈现值；`clear_transition()` 移除策略并回到目标。
- `set_offset` 不改变布局；没有过渡策略时立即生效。
- 原生 App 为新建的交互控件默认安装 120 ms 过渡；`AppOptions.transition = None` 关闭。减少动态效果时所有过渡直接到终点（仍会触发完成回调）。
- 没有活动动画时不请求帧、不唤醒 CPU。

## 9. 标记语言

`ui!("path.aegle")` 在编译期解析并检查文件，返回构造闭包；`ui!(&parent, "path.aegle")` 立即构造。返回的 View 有 `root` 和每个 `id` 的类型化字段：

```text
Column {
    gap: 8dp
    Text { id: status; text: "Waiting" }
    Button { id: done; text: "Done" }
}
```

```rust
let view = aegle::ui!(&window, "panel.aegle")?;
let status = view.status.clone();
view.done.on_click(move |_| status.set_text("Finished"))?;
```

动态文档可声明 state、表达式绑定、事件块、`if`/`for` 和组件：

```text
use "parts.aegle"
Column {
    state count: int = 0
    Text { text: "count " + str(count) }
    Button { text: "Add"; on clicked { count += 1 } }
    if count > 3 { Text { text: "Many" } }
}
```

View 为根节点的每个 state 提供 `aegle::loader::State<T>` 字段（`view.count.get()`、`view.count.set(5)?`），设置后相关绑定立即更新。运行时加载与重载：

```rust
use aegle::loader::Program;

let mut view = Program::load("ui/panel.aegle")?.build(&window)?;
view.set("count", aegle::loader::Data::Int(2))?;
view.reload(&Program::load("ui/panel.aegle")?)?;   // 失败时保留旧界面
```

完整语法、可绑定属性、类型规则和限制见[标记语言](../markup.md)。

## 10. 嵌入自有宿主

不使用 `App` 时，可以把无窗口 `Ui` 接到自己的窗口系统和 renderer（`default-features = false` 即可）：

```rust
use aegle::{Theme, TextSystem, Ui, Size};
use std::{cell::RefCell, rc::Rc};

let fonts = Rc::new(RefCell::new(TextSystem::new()));   // 注册应用字体
let ui = Ui::with_fonts(fonts, Theme::light())?;
ui.root().button("OK")?;
ui.resize(Size::new(320.0, 200.0))?;
if ui.refresh()? {
    ui.visit_scenes(|scene, transform, clip| {
        // 交给 renderer：transform 为窗口逻辑平移，clip 为祖先裁剪（必须应用）
        Ok(())
    })?;
}
```

| 输入与同步 | 说明 |
| --- | --- |
| `pointer(id, kind, point, modifiers)`、`pointer_leave()` | 指针移动/按下/释放/离开 |
| `key(KeyInput { key, text, modifiers, pressed, repeat })` | 键盘；`text` 为已翻译文字 |
| `scroll(point, dy)` / `scroll_by(point, delta)` | 滚轮，按嵌套视口路由 |
| `window_focus(b)` | 窗口获得/失去键盘焦点 |
| `ime(ImeEdit { .. })`、`ime_left()`、`take_ime_state(max)` | 输入法事务与需要同步给平台的状态 |
| `take_clipboard()` / `paste(text)` | 编辑器发出的复制/粘贴请求 |
| `dispatch_callbacks()` | 执行排队的回调 |
| `advance_animations(now)`、`has_animations()` | 宿主时钟驱动动画 |
| `accessibility(initial, title)`、`access_action(req)` | 语义树导出与动作（需 `accessibility`） |

`crates/aegle/examples/gallery.rs` 是完整的无窗口示例：用软件 renderer 把 `Ui` 渲染成 PNG，本文档的截图即由它生成。

## 11. 错误

`UiError` 变体：`DeadHandle`（控件已删除）、`WrongKind`（操作不适用于该控件）、`ForeignUi`（父子属于不同 Ui）、`InvalidValue`（非有限或越界数值）、`RootMutation`（删除或移动根）、`ReentrantAccess`（在 scene 访问等借用期间修改 Ui）、`IdentityExhausted`。其他错误保留来源类型，例如字体缺失、平台能力缺失、`aegle::loader::RuntimeError`（标记运行时溢出等）和 `aegle::loader::markup::ProgramError`（带文件/行/列的标记诊断）。可用 `error.downcast_ref::<UiError>()` 区分。
