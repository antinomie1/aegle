# 不经 facade 使用控件库

`aegle` facade 把原生窗口、renderer、标记和控件打包在一起。已有窗口与绘制宿主、只想要控件，或要写自己的控件库时，直接依赖下面几个 crate；它们不依赖任何平台或 renderer。可运行示例在 `crates/aegle-widgets/examples/`。

## 选哪些 crate

| crate | 提供 | 何时需要 |
| --- | --- | --- |
| `aegle-ui` | 无窗口引擎：`Ui`、`Container`/`Node` 句柄、布局、输入路由与焦点、滚动、主题应用、过渡、IME 与语义导出，以及 `Control` trait | 总是 |
| `aegle-widgets` | 全部默认控件与皮肤，经 `Widgets` trait 在 `Container` 上创建 | 使用默认控件时 |
| `aegle-theme` | `Theme`、`Style`、`Appearance`、`Skin`、`Token` 等纯值类型，no_std、不分配 | 不经引擎使用这些值时；`aegle-ui` 已重导出全部类型 |
| `aegle-motion` | `Tween`、`Animation`、`Easing`、`Spring`，由调用者提供时间 | 不经引擎做补间时；`aegle-ui` 的 `motion` feature 已重导出 |
| `aegle-render-software` 等 | 绘制 `Ui::visit_scenes` 给出的 scene | 没有自己的 renderer 时 |

```toml
[dependencies]
aegle-ui = { path = "../aegle/crates/aegle-ui", features = ["system-fonts"] }
aegle-widgets = { path = "../aegle/crates/aegle-widgets", features = ["motion"] }
aegle-render-software = { path = "../aegle/crates/aegle-render-software", features = ["text"] }
```

| 需要 | feature | 说明 |
| --- | --- | --- |
| 过渡、动画、惯性滚动 | `aegle-widgets/motion` | 转发到 `aegle-ui/motion`；滑块与进度条的数值滑动也随之启用。只开 `aegle-ui/motion` 时引擎有动画，但默认控件不做数值滑动 |
| 语义树（无障碍） | `aegle-widgets/accessibility` | 转发到 `aegle-ui/accessibility`；控件的角色与动作在 widgets 中，只开引擎的 feature 时默认控件没有角色 |
| 系统字体发现 | `aegle-ui/system-fonts` | 否则用 `TextSystem::register_fonts` 显式注册 |
| 中日文词典分词 | `aegle-ui/text-dictionary` | 按词移动与双击选词 |
| Grid 与 Stack 容器 | `aegle-ui/grid` | |

Cargo 会统一同一 crate 的 feature：在任一依赖上开启的 feature 对整个程序生效。

## 最小程序

```rust
use aegle_ui::{Result, Size, TextSystem, Theme, Ui};
use aegle_widgets::Widgets;
use std::{cell::RefCell, rc::Rc};

fn main() -> Result {
    let fonts = TextSystem::system();         // 系统字体；或 TextSystem::new() 后 register_fonts
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    let status = ui.root().text("Not saved")?;
    ui.root().button("Save")?.on_click(move |_| status.set_text("Saved"))?;
    ui.resize(Size::new(240.0, 100.0))?;
    ui.refresh()?;
    ui.visit_scenes(|_scene, _transform, _clip| Ok(()))?; // 交给 renderer
    Ok(())
}
```

`crates/aegle-widgets/examples/standalone.rs` 是完整版本：显式注册应用字体并设为 sans-serif、点击按钮、用弹簧把标签移开，按 16 ms 推进动画直到结束，再用软件 renderer 写出 `target/aegle-standalone.ppm`：

```sh
cargo run -p aegle-widgets --features motion --example standalone
```

## 宿主的职责

原生 `App` 做的事，嵌入宿主自己做。按每轮事件的顺序：

| 时机 | 调用 |
| --- | --- |
| 窗口逻辑尺寸变化 | `ui.resize(size)` |
| 平台输入 | `pointer`/`pointer_at`、`key`/`key_at`、`wheel`、`touch`、`ime`、`window_focus`；完整列表见 [API 指南 §10](api.md#10-嵌入自有宿主) |
| 每批输入之后 | `ui.dispatch_callbacks()`：运行控件事件处理器；出错时返回第一个错误，处理器保留 |
| 每帧 | 若 `ui.wants_frames()`，`ui.run_frame(Instant)`；若 `ui.has_animations()`，`ui.advance_animations(单调时间)` |
| 绘制前 | `ui.refresh()` 返回是否有像素变化；变化时用 `visit_scenes` 绘制（可只重绘 `damage()`，呈现后 `clear_damage()`） |
| 平台协作 | `take_ime_state` 同步输入法，`take_clipboard`/`paste` 处理剪贴板，`cursor()` 设置指针形状 |
| 等待 | 没有 `has_animations`、`wants_frames`、`has_pending_callbacks` 时睡眠，最迟在 `next_wake()` 醒来调用 `ui.wake(now)` |

无窗口 `Ui` 默认不给交互控件安装过渡；要与原生 App 一致，创建控件前调用 `ui.set_default_transition(Some(Transition::default()))`。系统深浅色、高对比与减少动态效果由宿主读取，再调用 `set_theme`、`set_reduced_motion`。

## 自定义控件

控件库只依赖 `aegle-ui`。实现 `aegle_ui::Control`（`kind`、`measure`、`handle`、`hover`、`paint`、可选 `semantics`/`action_input`），用 `Container::add` 插入，再用 `handle!` 定义类型化句柄，用 `style_methods!` 生成与 `kind` 相符的样式 setter。控件状态通过 `state.control_as::<T>(id)` 读写，`state.dirty_visual_state(id)` 请求重绘，`state.on_action(id, ..)` 注册事件处理器，`Outcome::action = Some(Action::Change)` 触发它们。`kind` 决定默认皮肤与可接受的局部样式，所以自定义控件同样适用 `Style`、皮肤、主题、token 与过渡。

`crates/aegle-widgets/examples/custom_control.rs` 实现一个五级评分控件：指针悬停预览、按下选择并获得焦点、方向键与语义增减、使用共享外观绘制、语义角色为 Slider。

```sh
cargo run -p aegle-widgets --example custom_control
```

需要跨节点协作的行为（弹出层、单选组、虚拟列表）安装 `Hooks`，库自己的数据放在 `State::ext`；契约见 [Rust API · 组件库作者](../rust-api.md#组件库作者)。
