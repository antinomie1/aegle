# 编写控件库

本指南按唯一推荐的顺序讲第三方控件库：登记控件类型 → 定义控件 → 句柄 → 皮肤与 token → 装饰 → 标记元素 → 动画。内置控件（`aegle-widgets`）与内置标记元素（`aegle_loader::elements`）用的就是同一组公开机制。贯穿示例是一个五级评分控件 Rating，完整可运行代码见 `crates/aegle-widgets/examples/custom_control.rs`。只用控件、不经 facade 时的依赖选择见[不经 facade 使用控件库](standalone.md)。

完整的真实例子是同级目录的 [am3](../../../am3/README.md)：一个单 crate 的 Material 3 Expressive 控件库，覆盖按钮、选择、纸片、指示器、卡片与浮层、导航、文本框与日期/时间选择器等全部组件，只用本指南的公开机制实现。它展示了几处本指南只点到的做法：皮肤从 `Scheme::of(theme)` 取色，使主题切换与子树局部主题无需库自己的状态；模态层、Tab 循环与提示条超时经 `Hooks` 实现；标签页指示器在 `place` 钩子里读取布局；文本框包住内置编辑器，只用 `text_viewport` 与 `Editor::changes` 接入；全部控件以 `Md*` 元素进入标记，子元素经 `State::ext` 中按节点登记的父组件句柄创建。性能数据（1000 控件场景与 Aegle 默认控件对照）见 am3 的 `docs/implementation.md`。

## 1. 登记控件类型

每种控件声明一个 `static ControlKind`：名称、默认皮肤、接受的样式组和是否为布局容器。类型按地址比较，节点存活期间不变。

```rust
use aegle_ui::{Accepts, Appearance, ControlKind};

pub static RATING: ControlKind = ControlKind {
    name: "Rating",
    skin: rating_skin,
    accepts: Accepts::INTERACTIVE.with(Accepts::PRESSED).with(Accepts::INDICATOR),
    container: false,
};
```

样式组决定该类控件接受哪些局部样式：`TEXT`（字号）、`INTERACTIVE`（悬停背景、焦点环）、`PRESSED`（按下背景）、`INDICATOR`（标志色）、`EDITOR`（选区与插入符，类型须提供 `editor()`）。

## 2. 定义控件

实现 `aegle_ui::Control`：`kind` 返回上面的静态类型，`measure` 给出尺寸，`handle`/`hover` 处理输入并返回 `Outcome`，`paint` 用 `cx.appearance` 录制图元，`semantics` 与 `action_input` 提供无障碍语义和动作。用 `Container::add` 插入树中。用户改变取值时返回 `Outcome { action: Some(Action::Change), .. }`，已注册的处理器随后在 UI 借用外运行。跨节点协作（弹出层、单选组）用 `Hooks`，库数据放在 `State::ext`。契约细节见 [Rust API · 组件库作者](../rust-api.md#组件库作者)。

## 3. 句柄

```rust
aegle_ui::handle! {
    /// A five-step rating.
    pub Rating(RatingControl): interactive, pressed, indicator
}

impl Rating {
    pub fn value(&self) -> aegle_ui::Result<u8> {
        self.read(|rating| rating.value)
    }
    pub fn set_value(&self, value: u8) -> aegle_ui::Result {
        self.update(|rating| rating.value = value.clamp(1, 5))
    }
    pub fn on_change(&self, mut f: impl FnMut(Rating) -> aegle_ui::Result + 'static) -> aegle_ui::Result {
        self.change(|state, id| state.on_action(id, move |node| f(Rating(node))))
    }
}
```

`read`/`update` 取得控件状态；`update` 之后自动重绘并更新语义。列出的样式组生成相应 setter（如 `set_indicator_color`），必须与类型的 `accepts` 一致；未列出的 setter 不存在，误用在编译期报错。

## 4. 皮肤与 token

皮肤是 `fn(&Theme, VisualState) -> Appearance` 纯函数。类型的 `skin` 是它的默认外观；应用或另一个库可以在任意子树按类型替换它，也可以只替换一个节点：

```rust
fn rating_skin(theme: &Theme, state: VisualState) -> Appearance {
    Appearance { radius: theme.radius, ..Appearance::base(theme, state) }
}
window.root().set_kind_skin(&RATING, Some(gold_rating))?; // 这个子树里的全部 Rating
rating.set_skin(Some(gold_rating))?;                      // 只有这一个
```

优先级从高到低：局部 `Style`（含 token 绑定）> 节点皮肤 > 最近祖先的类型皮肤 > 类型默认皮肤，完整表见 [API 指南 §6](api.md#6-外观主题样式与皮肤)。库的设计 token 用 `register_token("库名.名称", 默认值函数)` 登记，应用经 `bind_color`/`bind_length` 或标记 `token("库名.名称")` 绑定，随主题与覆盖更新。

## 5. 装饰

只想在已有控件（包括内置控件）上追加绘制，例如按下涟漪，实现 `Decorator` 并 `node.decorate(..)`：`input` 观察控件已处理的输入，`under`/`over` 在背景之下或内容之上录制图元，动画期间调用 `cx.request_frame()`。不需要复制控件行为；示例见 `crates/aegle-widgets/tests/decorators.rs`。

## 6. 标记元素

`element!` 让控件出现在标记里，内置元素与第三方元素没有区别（[ADR 0004](../adr/0004-element-contract.md)）：

```rust
aegle::element! {
    /// A five-step rating.
    pub Rating {
        style interactive pressed indicator;
        create |parent, value: int(1, 5) = 3| Rating::new(parent, value as u8);
        set value: int(1, 5) => |rating, value| rating.set_value(value as u8);
        event changed => |rating, run| rating.on_change(move |_| run());
        get value: int => |rating| rating.value().map(i64::from);
    }
}
```

`create` 的参数是构造属性（有默认值可省略，没有默认值的必填且只能写字面量），`set` 声明可设置、可绑定表达式的属性，`event` 注册处理块，`get` 声明处理块里的 `self.value`。容器元素加 `layout box|flex|grid`，可用 `children only Name|exactly N`、`parent Name` 和 `children => |handle, index| container` 约束和放置子节点。完整语法见 `aegle::element!` 的文档，值类型与 Rust 类型的对应也在那里。

`element!` 为句柄类型实现 `aegle::loader::Element`，并在 crate 根定义同名的隐藏宏；在 crate 根重导出句柄类型，使用者一次 `use rating_lib::Rating;` 就同时得到两者：

```text
Row {
    Rating { id: stars; value: 4; indicator_color: token("rating.gold"); on changed { score = self.value } }
}
```

```rust
use rating_lib::Rating;
let view = aegle::ui!(&parent, "review.aegle")?;          // 编译期按 Rating 的规格检查
let elements = aegle::loader::Elements::new().with::<Rating>();
let view = aegle::loader::Program::load_with("review.aegle", &elements)?.build(&parent)?; // 挂载前检查
```

未知属性、类型不符、不存在的事件在 `ui!` 的编译期、运行时加载的挂载前给出带文件、行、列的诊断。`id` 得到 `Rating` 类型的字段；元素可以出现在 `if`/`for` 块、组件体和 slot 内容中。

## 7. 动画

外观过渡对库控件自动生效：应用写 `paint_transition`/`transition`，或在 Rust 中 `set_transition`，皮肤算出的新外观从当前呈现值补间。库自己的取值动画（如滑块平滑移动）在控件中保存起止值，`paint` 时按 `cx.time` 采样，未完成时 `cx.request_frame()`；静止时不请求帧，空闲保持零唤醒。宿主经 `set_reduced_motion` 打开减少动态效果后，引擎的过渡直接到达目标；库自己的动画读取 `cx.reduced_motion`，为真时直接画终点。
