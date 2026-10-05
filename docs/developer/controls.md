# 默认控件参考

每个默认控件的创建方式、常用方法、事件、标记写法和各状态截图。通用的布局、样式、主题、回调和句柄规则见 [API 指南](api.md)。

截图由 `cargo run -p aegle --example gallery` 生成：每个状态是一个独立的无窗口 `Ui`，经与原生窗口相同的软件 renderer 以 2 倍设备缩放渲染，使用仓库自带的测试字体（Noto Sans CJK 子集）和浅色主题。修改控件外观后重新运行该命令即可更新本页图片。

默认外观是中性直角风格：边框 1 dp，焦点环 2 dp 使用主题 `accent`；悬停和按下使用主题 `hover`/`pressed` 底色；禁用时文字使用 `muted`。焦点环只在控件获得键盘焦点时出现。所有控件都可以用局部样式、皮肤或主题修改外观，见 [API 指南第 6 节](api.md#6-外观主题样式与皮肤)。

| 控件 | Rust 创建 | 标记 |
| --- | --- | --- |
| [Text](#text) | `text(s) -> Label` | `Text` |
| [Button](#button) | `button(s) -> Button` | `Button` |
| [TextField / TextArea](#textfield--textarea) | `text_field(s)` / `text_area(s) -> TextField` | `TextField` / `TextArea` |
| [CheckBox](#checkbox) | `check_box(s, checked) -> CheckBox` | `CheckBox` |
| [Switch](#switch) | `switch(s, checked) -> Switch` | `Switch` |
| [Slider](#slider) | `slider(min, max, value) -> Slider` | `Slider` |
| [Progress](#progress) | `progress(min, max, value) -> Progress` | `Progress` |
| [Column / Row](#column--row) | `column()` / `row() -> Container` | `Column` / `Row` |
| [ScrollView](#scrollview) | `scroll_view() -> ScrollView` | `ScrollView` |
| [ListView](#listview) | `list_view(h, n, row) -> ListView` | — |
| [ImageView](#imageview) | `image(&Image) -> ImageView` | — |
| [Canvas](#canvas) | `canvas(painter) -> Canvas` | — |

## Text

<img src="images/text.png" width="660" alt="Text：默认、font_size 20、自动换行">

```rust
let title = window.text("Hello, 世界")?;
title.set_font_size(20.0)?;
title.set_text("新的文字")?;
```

```text
Text { text: "Hello, 世界"; font_size: 20dp }
```

- 方法：`set_text`、`text`、`set_font_size` / `clear_font_size`、`set_foreground`。
- 按父容器宽度自动换行；默认无内边距，不可聚焦，不压缩高度。
- 无障碍角色 Label，值为文字内容。

## Button

<img src="images/button.png" width="700" alt="Button：普通、悬停、按下、聚焦、禁用">

```rust
let save = window.button("Save")?;
save.on_click(|button| button.set_text("Saved"))?;
save.activate()?;            // 按用户激活的规则排队回调
```

```text
Button { id: save; text: "Save"; on clicked { saved = true } }
```

- 方法：`set_text`、`on_click` / `clear_on_click`、`activate`。
- 指针在按钮上按下并在按钮上释放才激活；Space 在释放时激活，Enter 在首次按下时激活，按键重复不重复触发。失焦、禁用或指针取消时不激活。
- 默认高度为主题 `control_height`（36），宽度为文字加两侧内边距；`set_width`、`set_grow` 可改变。
- 无障碍角色 Button，名称默认为按钮文字，支持 Focus/Click 动作。

## TextField / TextArea

<img src="images/text-field.png" width="1140" alt="TextField：空、有文字、聚焦并选择、只读、密码、禁用">

<img src="images/text-area.png" width="720" alt="TextArea：多行、溢出与滚动条、聚焦">

```rust
let name = window.text_field("")?;
name.on_submit(|field| {
    println!("submitted {}", field.text()?);
    Ok(())
})?;
let notes = window.text_area("First line\nSecond line")?;
notes.select(aegle::Selection { anchor: 0, focus: 5 })?;
let secret = window.text_field("")?;
secret.set_password(true)?;
```

```text
TextField { id: name; text: ""; on submitted { submitted = self.text } }
TextArea { text: "First line\nSecond line"; read_only: true }
```

- 方法：`text`、`set_text`（清空撤销历史并结束输入法预编辑）、`select(Selection)`（UTF-8 字节偏移）、`set_read_only`、`set_password`、`on_submit` / `clear_on_submit`（仅单行，Enter 触发）。
- 编辑：选择、按词/行移动、按字素删除、撤销/重做（Ctrl+Z / Ctrl+Y）、全选（Ctrl+A）、复制/剪切/粘贴（Ctrl+C / X / V，macOS 用 Cmd）；原生宿主处理剪贴板，自有宿主用 `take_clipboard` / `paste`。
- 输入法：Wayland text-input-v3、Windows IMM；预编辑不改变已提交的值。
- 只读可选择和复制；密码模式显示 `•`，拒绝复制、输入法组合并不保留撤销历史。
- 默认高度：单行为 `control_height`，多行为其 4 倍；多行内容溢出时显示覆盖式纵向滚动条，caret 移动会自动滚动到可见。
- 无障碍角色 TextInput / MultilineTextInput / PasswordInput，导出文字与选择。

## CheckBox

<img src="images/check-box.png" width="750" alt="CheckBox：未选中、选中、悬停、聚焦、禁用">

```rust
let agree = window.check_box("I agree", false)?;
agree.on_change(|control| {
    println!("checked: {}", control.is_checked()?);
    Ok(())
})?;
agree.set_checked(true)?;   // 程序设置，不触发 on_change
```

```text
CheckBox { text: "I agree"; checked: agree; on changed { agree = self.checked } }
```

- 方法：`is_checked`、`set_checked`、`toggle`（按用户操作规则切换并触发回调）、`text`、`set_text`、`on_change` / `clear_on_change`。
- 标志 18 dp，选中时绘制对勾，不只依靠颜色区分；激活规则与 Button 相同。
- 无障碍角色 CheckBox，带 Toggled 状态。

## Switch

<img src="images/switch.png" width="750" alt="Switch：关、开、悬停、聚焦、禁用">

```rust
let wifi = window.switch("Wi-Fi", true)?;
wifi.on_change(|control| {
    println!("on: {}", control.is_checked()?);
    Ok(())
})?;
```

```text
Switch { text: "Wi-Fi"; checked: true }
```

- 方法与 CheckBox 相同。开关 36×20 dp，滑块位置表示状态：开在右侧，关在左侧。
- 无障碍角色 Switch。

## Slider

<img src="images/slider.png" width="950" alt="Slider：数值 30、步长 25、悬停、聚焦、禁用">

```rust
let volume = window.slider(0.0, 100.0, 30.0)?;
volume.set_step(5.0)?;
volume.on_change(|slider| {
    println!("value: {}", slider.value()?);
    Ok(())
})?;
```

```text
Slider { min: 0; max: 100; value: 30; step: 5; on changed { volume = self.value } }
```

- 方法：`value`、`range`、`set_value`、`set_range`、`step`、`set_step`（0 为连续）、`increment` / `decrement`、`on_change` / `clear_on_change`。越界的有限值会被限制到范围内。
- 键盘：方向键移动一步（连续时为跨度的 1%），PageUp/PageDown 十步（或 10%），Home/End 到端点。指针按下轨道直接设值并可拖动。
- 手柄 16 dp，轨道 2 dp，已完成部分 4 dp；宽度不足时缩小。
- 无障碍角色 Slider，带数值、范围与步长。

## Progress

<img src="images/progress.png" width="570" alt="Progress：0%、40%、100%">

```rust
let download = window.progress(0.0, 100.0, 0.0)?;
download.set_value(40.0)?;
```

```text
Progress { min: 0; max: 100; value: 40 }
```

- 方法：`value`、`range`、`set_value`、`set_range`。只显示确定进度，不可聚焦、不接受用户调整，没有不确定进度动画。
- 默认高度为 `control_height` 的一半。无障碍角色 ProgressIndicator，带数值。

## Column / Row

<img src="images/layout.png" width="690" alt="Column、Row 与 grow">

```rust
let form = window.column()?;
form.set_gap(12.0)?;
form.set_padding(16.0)?;
let actions = form.row()?;
actions.button("Cancel")?;
actions.button("OK")?.set_grow(1.0)?;
```

```text
Column { gap: 12dp; padding: 16dp
    Row { Button { text: "Cancel" }; Button { text: "OK"; grow: 1 } }
}
```

- 纵向 / 横向排列子控件，默认间距为主题 `gap`（8），交叉轴拉伸。窗口根就是一个列。
- 容器默认透明、无边框；可以设置背景、边框、圆角和局部主题，但圆角不裁剪子控件。

## ScrollView

<img src="images/scroll-view.png" width="660" alt="ScrollView：纵向溢出、已滚动、双轴溢出">

```rust
let list = window.scroll_view()?;
list.set_height(Some(200.0))?;
for i in 1..=50 {
    list.text(&format!("Item {i}"))?;
}
list.scroll_to(Point::new(0.0, 120.0))?;
```

```text
ScrollView { height: 200dp
    Text { text: "Item 1" }
    Text { text: "Item 2" }
}
```

- 内部按列排列；限制高度/宽度或 flex 分配后，超出部分可滚动，两轴均支持。
- 方法：`offset`、`max_offset`、`content_size`、`scroll_to`、`scroll_by`；子控件用 `ensure_visible` 滚动到可见，`visible_bounds` 读取可见区域。
- 滚轮、Tab 焦点和 caret 移动都会滚动；嵌套视口会把未消费的滚动传给外层。
- 溢出时在边缘绘制覆盖式滚动条：不占布局，12 dp 指针带内 6 dp 直角滑块，可拖动或点击定位。
- 无障碍角色 ScrollView，带滚动偏移与范围。

## ListView

<img src="images/list-view.png" width="440" alt="ListView：一万行、滚动到第五千行">

```rust
let rows = window.list_view(28.0, 10_000, |row, index| {
    row.text(&format!("Row {index}"))?;
    Ok(())
})?;
rows.set_height(Some(300.0))?;
rows.set_count(20_000)?;   // 行数变化
rows.reload()?;            // 行数据变化，重建已显示的行
```

- 等高虚拟列表：只有与可见区域相交的行真正存在，滚出的行被删除，进入的行调用回调重建。一万行首次刷新约 0.24 ms，内存增长约 1 MB。
- `ListView` 解引用为 `ScrollView`，滚动接口和滚动条相同；另有 `count`、`set_count`、`row_height`、`reload`。
- 回调在刷新期间、UI 借用之外执行，可以使用任意句柄；行内状态在行滚出后丢失，应保存在应用数据中。
- `行高 × 行数` 不超过 16,777,216。可变高度列表尚未实现。

## ImageView

<img src="images/image-view.png" width="320" alt="ImageView：原始像素尺寸、拉伸为 96×48">

```rust
use aegle::scene::Image;

let pixels = vec![255u8; 48 * 48 * 4];             // 非预乘 sRGB RGBA8，首行在前
let image = Image::new(48, 48, pixels)?;
let view = window.image(&image)?;
view.set_size(Some(96.0), Some(48.0))?;            // 按边界拉伸，不保持宽高比
```

- 默认尺寸为图像像素尺寸（逻辑像素），交叉轴不拉伸。`image()` 读取、`set_image()` 替换。
- `Image` 克隆共享像素；renderer 按图像 id 缓存上传结果。应用自行解码 PNG/JPEG。
- 无障碍角色 Image，名称用 `set_accessible_label` 设置。

## Canvas

<img src="images/canvas.png" width="160" alt="Canvas：自定义绘制的星形">

```rust
use aegle::scene::{Color, FillRule, PathBuilder, Point};

let canvas = window.canvas(|builder, size| {
    let mut path = PathBuilder::new();
    path.move_to(Point::new(0.0, 0.0));
    path.line_to(Point::new(size.width, size.height));
    path.line_to(Point::new(0.0, size.height));
    path.close();
    builder.fill_path(&path.finish(FillRule::NonZero)?, Color::rgb(53, 92, 218))?;
    Ok(())
})?;
canvas.set_size(Some(64.0), Some(64.0))?;
canvas.invalidate()?;   // 数据变化后重新绘制
```

- painter 用局部坐标和当前尺寸录制 scene 命令（矩形、圆角、边框、路径、图像、变换、裁剪），只在创建、尺寸变化、`invalidate` 或 `set_painter` 后重新执行，不按帧调用。
- painter 运行时持有 UI 借用，不能使用控件句柄；默认尺寸为零，需设置尺寸或 grow；绘制不裁剪到边界。
- Canvas 只是绘制扩展，不接收输入；无障碍角色 Canvas。

## 已知限制

- 高对比主题中禁用控件与启用控件外观相同（`muted` 为白色），只能依靠语义区分。
- 没有三态复选框、单选组、下拉菜单、弹出层、表格和可变高度列表；可以用现有控件与 Canvas 组合。
- 悬停与按下截图通过指针事件生成；原生环境中的实际颜色还会受过渡（默认 120 ms）影响。
