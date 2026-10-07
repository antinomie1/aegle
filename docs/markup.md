# Aegle 标记语言

状态：v0.1。编译型静态结构、类型检查和具名弱句柄已实现；类型化 state、属性表达式绑定、`on` 事件块、`if`/`for` 结构块、组件与 `use` 导入、运行时加载和显式 `reload` 已实现，见[当前动态标记](#当前动态标记)。record 与带 key 的列表、`let`、宿主动作、slot、组件事件和可配置限额也已实现，见[当前动态标记](#当前动态标记)末段；呈现几何（offset_x/offset_y/scale/rotation）与逐属性过渡时长也已实现。采用 `.aegle` 扩展名；QML 风格的结构，不依赖 Qt/QML，也不强制 JavaScript。

## 最小程序

`main.aegle`：

```text
Window {
    title: "Hello"
    Text { text: "你好，世界" }
}
```

`main.rs`：

```rust
fn main() -> aegle::Result<()> {
    aegle::App::run_ui(aegle::ui!("main.aegle"))
}
```

两份手写源码合计 7 行，初始化包含在 App::run_ui 中。宏是构建期能力：静态文档生成直接创建/设置控件的 Rust 代码；含动态特性的文档生成构造已检查程序的 Rust 代码，由 `aegle::loader` 执行。两者的编译产物都不携带标记解析器。宏为入口及每个导入文件生成编译器可追踪的依赖标记，修改任一文件都会触发重编译。

对应可执行示例为 `cargo run -p aegle --example hello_markup`；加上示例的文档注释，两份文件共 8 行。当前 App 需要 Linux Wayland 与系统字体；`markup` 本身不启用原生平台，片段可挂到无窗口 Ui。

## 当前编译接口

`ui!("path.aegle")` 返回构造闭包：Window 根接收 `&App`，其他根接收 `&Container`，返回 `Result<View>`。`ui!(&parent, "path.aegle")` 立即构造并且只求值一次 parent。路径相对使用宏的包的 `CARGO_MANIFEST_DIR`，允许显式 `../`，不接受绝对路径；没有隐式查找目录。文件必须随使用者的源包一起发布。

```text
Column {
    Text { id: status; text: "等待" }
    Button { id: done; text: "完成" }
}
```

```rust
let view = aegle::ui!(&window, "panel.aegle")?;
view.done.on_click(move |_| view.status.set_text("已完成"))?;
```

生成的局部 View 有 `root` 字段和每个 `id` 对应的有类型公开字段；没有运行时字符串查询表。ID 在一个文件中唯一，使用 ASCII Rust 标识符，不能是 Rust 关键字、`_` 或保留名 `root`。丢弃 View 不删除控件；它与命令式 API 使用相同弱句柄、回调、布局、主题、IME 和语义树。

一次构建（视图本身，或一个 `if` 分支、一行 `for`）分三步：先按文档顺序创建全部控件，再按文档顺序（父先于子）设置属性、建立绑定与 `if`/`for` 块，最后安装过渡与事件。编译型构造与运行时引擎顺序相同，因此容器设置属性时子控件已存在，祖先的初始值（如 `enabled: false`、窗口主题）不会让后代的过渡在首次显示时启动。任一步返回错误时删除本次新建的整棵子树，Window 根则关闭该窗口，保留调用方原有父节点。清理本身失败时返回清理错误。该规则只覆盖构造返回前的同步错误；后续刷新或原生呈现失败仍遵守 App 的错误处理。编译型 View 不提供重载；运行时加载的 View 用 `reload` 原子替换。

支持 Window、Column、Row、ScrollView、Grid、Stack、Tabs、Tab、Splitter、Text、Button、TextField、TextArea、CheckBox、Switch、RadioButton、Slider、Progress、NumberField、Separator。只有前九种可以包含子节点；Window 只可为文件根。Tabs 只接受 Tab 子节点，Tab 只能在 Tabs 中且必须有字面量 `title`；Splitter 必须恰好有两个内建控件子节点（不能是块、slot 或组件实例），依次放入两个窗格；编译型 View 中 Tab 的句柄类型为 Container。Grid 与 Stack 需要 facade 的 `grid` feature：编译型标记未启用时生成代码报找不到方法，运行时加载返回错误。文本默认为空字符串，窗口标题默认为 `Aegle`，其他默认值沿用命令式构造器。

| 属性 | 值与适用范围 |
| --- | --- |
| `id` | 唯一标识符，生成有类型句柄 |
| `title` | Window 或 Tab 的字符串，最多 4000 UTF-8 字节且无 NUL |
| `text` | Text/Button/TextField/TextArea/CheckBox/Switch 字符串；单行编辑器拒绝硬换行 |
| `width`、`height` | 控件为非负 `dp`、百分比、`calc(...)` 或 `auto`；Window 为正整数 `dp`，对应原生建议尺寸，可被 compositor 覆盖 |
| `min_width`、`min_height`、`max_width`、`max_height`、`basis` | 非负 `dp`、百分比或 `auto`（最大尺寸的 `auto` 为不限） |
| `aspect_ratio` | 正数，宽/高 |
| `padding` | 容器为非负 `dp`/百分比，或按 CSS 顺序的 `[上下, 左右]`、`[上, 右, 下, 左]`；其他控件为一个非负 `dp` |
| `margin` | 同 padding 的写法，可为负或 `auto`（左右 `auto` 水平居中） |
| `inset` | 同 margin 的写法；设置即绝对定位，相对父容器内边距框，绘制在兄弟之上 |
| `gap` | 容器的非负 `dp`/百分比，或 `[行间距, 列间距]` |
| `grow`、`shrink` | 有限非负数值 |
| `direction` | Window/Column/Row/ScrollView 的 `row`、`column`、`row_reverse`、`column_reverse` |
| `layout_direction` | 任意控件的 `ltr`、`rtl`，子树继承；镜像行、对齐、文本、滚动条与方向性控件 |
| `wrap` | 同上容器的 `no_wrap`、`wrap`、`wrap_reverse` |
| `align`、`align_self` | 容器子项 / 本控件的交叉轴对齐：`start`、`end`、`center`、`stretch`、`baseline` |
| `justify`、`align_content` | 容器的主轴剩余空间 / 行间剩余空间：`start`、`end`、`center`、`stretch`、`space_between`、`space_around`、`space_evenly` |
| `columns`、`rows` | Grid 的显式轨道列表：轨道（`dp`、百分比、`fr`、`auto`、`min_content`、`max_content`、`minmax(dp, fr)`、`fit_content(dp)`）、字符串线名，以及 `repeat(次数或 auto_fill/auto_fit, 轨道与线名…)`；repeat 不嵌套且至少含一条轨道，自动 repeat 至多一个，此时全部轨道须为固定尺寸（`dp`、百分比、`minmax(dp, fr)`） |
| `auto_columns`、`auto_rows` | Grid 的隐式轨道：单个或列表，不含线名与 repeat |
| `areas`、`grid_area` | Grid 的命名区域：每行一个字符串，空白分隔的格名，各行格数相同，`.` 为未命名格，同名格须构成矩形；子项以区域名字符串放入区域（区域生成 `名-start`/`名-end` 线） |
| `flow`、`justify_items` | Grid 的 `row`、`column`、`row_dense`、`column_dense`；子项水平对齐 |
| `grid_column`、`grid_row`、`justify_self` | Grid 子项：非零整数线号（负数从末尾数）或线名/区域名字符串（同 CSS `grid-column: name`），或 `[线号、auto 或线名, 跨度或结束线名]`；格内水平对齐 |
| `visible`、`enabled` | bool，作用于控件子树 |
| `label` | 无障碍名称字符串 |

长度属性（尺寸、内外边距、inset、gap）还接受 `calc(...)`：百分比与 `dp` 的线性组合，可用 `+`、`-`、一元负号与数字乘除，如 `calc((100% - 8dp) / 2)`；检查时折叠为“百分比 + dp”，因此不支持 `min`/`max`/`clamp` 或两个长度相乘。结果的正负取决于父尺寸，不按非负约束拒绝；Taffy 把负尺寸与内边距截为零。需要 64 位目标。
| `read_only` | TextField/TextArea 的 bool |
| `password` | TextField 的 bool；以 `•` 遮盖值，禁用复制/剪切、IME 组合与撤销历史 |
| `theme` | Window 的 `light`、`dark`、`high_contrast` |
| `background`、`foreground`、`border_color` | `#RRGGBB` 或 `#RRGGBBAA` 颜色 |
| `hover_background`、`pressed_background` | hover 限 Button/TextField/TextArea/CheckBox/Switch/Slider，pressed 限 Button/CheckBox/Switch/Slider |
| `disabled_background`、`disabled_foreground` | 对应禁用状态的颜色覆盖 |
| `border_width`、`radius` | 非负 `dp`；边框宽度为零时关闭 |
| `focus_color`、`focus_width` | Button/TextField/TextArea/CheckBox/Switch/Slider 的焦点颜色与非负 `dp` 宽度 |
| `selection_color`、`caret_color` | TextField/TextArea 的选择与 caret/预编辑颜色 |
| `font_size` | Text/Button/TextField/TextArea/CheckBox/Switch 的正 `dp` |
| `token("包.名称")` | 上述颜色属性及 `border_width`、`radius`、`focus_width`、`font_size`、`padding`、`gap` 也可绑定已登记的 token（如 `token("theme.accent")`；padding 与 gap 取统一值）；构建时按名查找，此后随主题和 token 覆盖更新。字体与过渡时长 token 只能在 Rust 中绑定 |
| `checked` | CheckBox/Switch/RadioButton 的 bool，默认 false；同一父容器中的 RadioButton 互斥 |
| `mixed` | CheckBox 的 bool，部分选中状态 |
| `min`、`max`、`value` | Slider/Progress/NumberField 的有限数，默认0/1/0；min须小于max，value按共享Range契约clamp |
| `step` | Slider/NumberField 的有限非负数，默认0连续，正值启用步进 |
| `orientation` | Slider/Progress/Splitter 的 `horizontal`、`vertical`；Splitter 仅在构造时使用 |
| `indeterminate` | Progress 的 bool，不确定进度动画 |
| `decimals` | NumberField 显示的小数位，0 到 9 的整数 |
| `ratio` | Splitter 首窗格占比，0 到 1 的数或 0% 到 100% |
| `tooltip` | 除 Window 外任意控件的提示字符串，同时作为无障碍描述 |
| `indicator_color` | CheckBox/Switch/Slider/Progress 的标志或完成部分颜色 |
| `transition` | 全节点外观过渡，非负整数毫秒，如 `120ms`；零表示立即到目标 |
| `easing` | 同节点须有 transition；linear/ease_in/ease_out/ease_in_out，默认 ease_out |
| `offset_x`、`offset_y` | 除 Window 外任意控件的呈现位移，有限 `dp`；可绑定 float（按 dp）。另一轴保持当前目标 |
| `scale` | 除 Window 外任意控件以中心缩放子树，(0, 1000] 的数，默认 1；可绑定 |
| `rotation` | 除 Window 外任意控件以中心旋转子树，有限的度数（顺时针为正）；可绑定 |
| `paint_transition`、`offset_transition`、`scale_transition`、`rotation_transition` | 单独设置外观、位移、缩放、旋转的过渡：`200ms` 或 `[200ms, linear]`，未写 easing 时为 ease_out；覆盖同节点 `transition` 中的对应部分 |

数值控件的 min/max/value 在全部属性收集完成后一起交给构造器，不依赖源码顺序；step 随后设置。当前标记数字保持有限 f32 解析再转 f64；需要完整 f64 精度可用 Rust API。四种新控件均为叶，Progress 拒绝交互状态、text、font_size 和 step 等不适用属性。

Window 的通用控件属性作用于其内容根；例如 `visible: false` 隐藏内容，不卸载原生窗口。单独设置宽度不会清除高度的主题默认值；显式高度在切换主题后保留。

ScrollView 可作为片段根或嵌套容器，内部按列布局；用 `height`、`width` 或 flex 分配约束视口即可产生滚动溢出。它接受普通容器的布局和外观属性，不接受 font_size、hover/pressed/focus 等交互状态属性。当前没有初始滚动偏移属性；通过具名 ScrollView 句柄调用 `scroll_to`，或对子控件调用 `ensure_visible`。布局刷新、裁剪、嵌套滚轮和保留状态遵守同一套 [Rust 滚动契约](rust-api.md#当前滚动契约)；完整示例为 `crates/aegle/examples/scrolling.aegle`。

声明必须以换行或分号分隔，最后一项可以直接跟 `}`；支持 `//` 注释和 JSON 字符串转义。数值为有限 f32，长度写为 `8dp`，百分比写为紧跟数字的 `50%`（`a % b` 取余在数字后需留空格），网格份数写为 `1fr`；`[8dp, auto]` 这类只含字面量与标识符的列表只用于上表注明的布局属性。颜色为非预乘 sRGB 字节，严格接受六位或八位十六进制；时长严格采用 ASCII 整数加 `ms`，覆盖完整 u64，拒绝负数、小数、指数与溢出。布局属性只接受字面量，不能绑定表达式。未知类型/属性、重复属性/ID、不适用属性、错误类型及未实现语法均在编译期拒绝，错误带文件、行、Unicode scalar 列和源码片段。外观属性直接调用同一套本地 setter，状态优先级见[组件样式](components-theme-animation.md)，没有另一套标记样式引擎。

`transition: 120ms` 与可选 `easing: ease_out` 需要 facade 的 `motion` feature（默认 desktop 已启用）；关闭该 feature 却使用过渡会在生成代码的 API 检查时报错。过渡在本次构建的全部属性设置后才安装（见上文构建顺序），首次显示没有初始样式动画。`transition` 为外观、位移、缩放和旋转统一设置时长，四个 `*_transition` 随后逐项覆盖；只写某一项时其余属性没有过渡，直接到目标。几何属性调用同一套 `set_offset`/`set_transform`，只改变呈现层，不影响布局、字号或文本行为；绑定的几何值变化时按对应时长补间。

独立 `aegle-markup` 无第三方依赖，提供 AST、字节跨度、`parse`/`parse_with_limits`、静态文档的 `check`、多文件 `compile`（经调用方提供的读取函数解析 `use`）与 `check_program`。默认解析上限为 1 MiB、64 层、10,000 节点，表达式嵌套也受层数上限约束；显式解析深度最多 `Limits::MAX_DEPTH`（256）。尺寸预算只由解析器执行；`check` 与 `check_program` 对手工构造的 AST 只施加同一 256 层上限，使构建不会递归更深。`ui!` 与 `Program::load` 使用默认上限。`choices(name)` 列出枚举属性接受的标识符：`ui!` 按 `snake_case` → `CamelCase` 生成变体，缺少变体即编译错误；运行时引擎逐项显式映射，不把未知值落到默认值。静态文档的运行时不保留 AST、schema 或解析器；`syn`/`quote`/`proc-macro-crate` 仅用于构建宏及识别重命名依赖。

## 当前动态标记

```text
use "tasks.aegle"
Window {
    state added: int = 0
    state tasks: list<string> = ["Write markup"]
    Button { text: "Add"; on clicked { added += 1; tasks += ["Task " + str(added)] } }
    if len(tasks) > 0 { Text { text: str(len(tasks)) + " tasks" } } else { Text { text: "Empty" } }
    for task in tasks { Task { title: task } }
}
```

`state name: type = value` 只能写在文档根节点或组件体顶层；类型为 bool、int（i64）、float（有限 f32）、string、`list<int>`、`list<string>`。初始值可读参数和此前声明的 state，每个实例求值一次。表达式包含字面量、名称（内层 for 项 → state → 参数）、事件块中的 `self.checked`/`self.text`/`self.value`/`self.selected`、`!`、一元 `-`、`|| && == != < <= > >= + - * / %`、列表字面量，以及 `str`、`len`、`int`、`float`。除整数字面量可按上下文转为 float 外没有隐式转换；`+` 也连接字符串和同类列表，比较只用于数值和字符串。未知名称、类型不符、不适用属性等在编译或加载时报告文件、行、Unicode 列和源码片段。

属性值写表达式即为单向绑定，可绑定 text/label/tooltip（string）、visible/enabled/checked/read_only/indeterminate（bool）与 value（float），其余属性只接受字面量。绑定在求值时记录读取的 state，只在这些 state 变化时重新求值，结果相等不调用 setter，不按帧轮询。用户编辑字段或切换控件不会回写 state，需要时用事件。Button 支持 `on clicked`，CheckBox/Switch/Slider/NumberField/Tabs 支持 `on changed`（Tabs 中 `self.selected` 为 int 页序号），单行 TextField 支持 `on submitted`；语句为 `x = e`、`x += e`（数值、字符串、列表）、`x -= e`（数值）及 `if/else if/else`，只能赋值本文档或组件的 state。事件块与普通回调一样在 UI 借用外执行，每次赋值立即更新相关绑定；整数溢出、除零、非有限浮点和越界 `int()` 返回 `RuntimeError` 并停止本次处理，保留此前赋值。没有循环语句，单次执行量受源码大小约束。标记事件块占用控件的回调槽，Rust 再设置同一回调会替换它。

`if c { } else if d { } else { }` 在条件变化时销毁旧分支并重建新分支，分支内本地状态随之重置。`for item in list { }` 以列表项值（int 或 string）作 key，重复 key 返回错误且块保留原有行；保留 key 的行保持控件身份和本地状态，删除的行被销毁，顺序变化时一次性重新挂接各行。块内子节点放在一个透明的 contents 分组中，直接参与父容器的布局：在 Row/Column 中与兄弟共享对齐、换行、gap 和 grow，在 Grid 中各自占一格，在 Stack 中叠放。release 测量（本机 CJK 测试字体，7 次中位数）：1000 行 `for` 首次构建加刷新 3.6 ms，追加一行 0.8 ms，整体反序 5.8 ms；大数据仍应使用 ListView。

`component Name(p: type = literal, q: type) { state ...; Root { ... } }` 声明组件，组件体只有一个根节点；`Name { p: expr }` 实例化，参数随调用方表达式读取的 state 更新，组件 state 每个实例独立。实例不接受子节点、事件或 id；递归实例化、与内建同名或重复组件名均为错误。`use "relative.aegle"` 导入另一文件声明的所有组件：路径相对于导入方文件、以 `/` 分隔且不能为绝对路径；导入环为错误，重复导入只加载一次，被导入文件只能声明组件，一个程序最多 256 个文件。组件名在已加载文件间全局可见。

**record、key、let、宿主动作、slot、组件事件与限额**：

```text
record Task { id: int; title: string; done: bool }
component Card(title: string) {
    event closed(int)
    state taps: int = 0
    Column {
        Text { text: title + "#" + str(taps) }
        slot
        Button { text: "close"; on clicked { taps += 1; let n = taps * 10; host.note(title, n); emit closed(n) } }
    }
}
Column {
    state tasks: list<Task> = [Task(1, "a", false)]
    state total: int = 0
    for task in tasks key task.id {
        Card { title: task.title; on closed(n) { total += n }; Text { text: "extra " + task.title } }
    }
}
```

- `record Name { field: type }` 全局声明（跨文件，与组件名不得重名），字段为 bool/int/float/string；值用位置式调用 `Task(1, "a", false)` 构造，读取用 `task.title`；record 可作为 state、参数和 `list<Task>` 的元素类型，`==`/`!=` 比较各字段。
- `for item in list key expression { }`：key 表达式可读本次循环项，类型为 int 或 string；record 列表必须写 key，标量列表默认以项值为 key。同一 key 的行在项值不变时保留控件与本地状态，项值变化则就地重建；重复 key 仍为错误并保留原有行。
- `let name = expression` 在事件块内声明局部，作用到所在块结束；`on name(value)` 把组件事件携带的值绑定为局部。
- `host.name(args)`：调用宿主注册的动作。动作用 `Program::action(name, &[Type], f)` 注册，或用 `aegle::loader::action` 注册到线程共享表（`ui!` 生成的视图使用它）；构建视图时校验每个调用有同名动作且参数类型一致，缺失或不符在挂载前报错。动作在 UI 借用外运行，返回的错误停止本次处理；动作不返回值，结果通过更新 state 带回。
- `slot` 写在组件体中，放置实例的子节点：`Card { title: "x"; Text {} }`；子节点在调用方的环境里求值，不可带 `id`，每个组件至多一个 slot，没有 slot 的组件不接受子节点。
- 组件事件：组件体内 `event name` 或 `event name(type)` 声明，事件块里 `emit name` / `emit name(expr)` 触发，实例上 `on name { }` / `on name(v) { }` 在调用方的环境里处理。
- 限额：`Program::set_limits(Limits { steps, rows, emit_depth })`，默认每次处理最多 10,000 条语句、单个 `for` 最多 10,000 行、嵌套 emit 最多 64 层；超限是运行时错误，保留此前的赋值与旧的行。运行时错误带文件路径与字节跨度。
- Rust 一侧：record 与 record 列表的 state 以 `State<Data>` 访问（`Data::Record`），设置时检查形状；`StateValue::accepts` 取代旧的 `ty()`。

`id` 只能用于入口文档中不在块、slot 内容或组件体内的控件。`ui!` 的 View 在 `root` 和各 `id` 外，为入口根的每个 state 生成 `loader::State<T>` 字段（bool、i64、f32、String、Vec<i64>、Vec<String>），`get`/`set` 读写并触发绑定。`id` 与入口 state 同名由共享检查器拒绝（“names both a control and a state”），`ui!` 与运行时加载给出同一诊断。绑定和块由控件通过 `Node::keep_alive` 持有，丢弃 View 不影响更新。

运行时加载使用 `aegle::loader::Program::load(path)` 或 `from_sources(entry, read)`，再 `build(&container)` 片段或 `open(&app)` Window 文档；`View` 提供 `root`、`handle(id)`、`get`/`set`、`state::<T>(name)` 和 `reload`。加载复用同一解析、检查和诊断，因此携带解析器；示例文档解析并检查约 0.1 ms。`reload` 与首次构建一样先校验宿主动作，再完整构建新界面、移除旧界面，失败保留旧界面；同名同类型的入口 state 保留取值，其余控件本地状态重置。Window 文档保留原生窗口、标题和尺寸并重建内容，新版本省略的窗口属性保留原值。没有文件监视器。可执行示例：`cargo run -p aegle --example dynamic`，其中面板运行时从磁盘加载并可重载。

## 后续目标：结构、值和状态

当前动态标记之外，更多值类型（color、length、duration、enum）、嵌套 record 仍是目标；示例中的事件块和 state 已按上节实现。

```text
Window {
    title: "计数器"
    state count: int = 0
    Column {
        gap: 8dp
        Text { text: str(count) }
        Button {
            text: "增加"
            on clicked { count += 1 }
        }
    }
}
```

类型名区分大小写。`name: value` 设置属性，子元素写在花括号内；换行或分号分隔声明，行尾没有强制分号。`id: counter` 定义当前组件内的稳定名称，跨组件不能直接写其私有节点。

内建值包括 bool、int（有符号 64 位）、float（有限 32 位）、string、color、length、duration、enum、具名 record 和 list。`8dp` 是逻辑长度，`50%` 仅用于允许比例的布局属性，`120ms` 是时长，`#RRGGBB`/`#RRGGBBAA` 是颜色。类型不正确直接报错；不隐式把字符串变数值，不接受 NaN/Infinity。

整数溢出和除零为带源位置的错误，两条执行路径一致。float 计算结果非有限时也报错。字符串是 UTF-8，用户可见字符操作遵守文本模块的 grapheme 规则，不按字节切割。

属性表达式可读取 state、输入属性、已命名节点的公开属性和宿主导出的只读数据；编译/加载时建立依赖。`text: str(count)` 在 count 改变时更新，不每帧求值。绑定环被拒绝；赋值和动画优先级见[架构](architecture.md)。

## 后续目标：事件与宿主

当前事件块支持赋值、`let`、if/else、宿主动作与 `emit`，并受可配置的语句与嵌套限额约束。当前没有循环或派生 state，绑定不会互相触发。

事件块允许赋值、局部 let、if/else 和调用已声明的宿主动作。内建纯函数只含数值、字符串格式化与 clamp 等有限集合；不允许任意函数定义、递归、while、文件访问或网络访问。

宿主注册具名动作及参数/返回类型，界面使用 `host.refresh()` 等直接调用。耗时操作由宿主异步执行，结果通过更新 state/模型返回；标记语言不内置 await 或通用 VM。动作只能使用宿主明确暴露的能力。

每个事件块最多执行 10,000 个简单操作，每轮更新传播上限为 64 轮；超限报告错误并停止本轮，保留此前合法赋值。编译型与运行时执行型均遵守这些语义，限制可由宿主显式下调，不默认放开。

## 后续目标：条件、列表和组件

条件、带 key 的列表（含 record）、组件参数、slot、组件事件与 `use` 文件导入已按[当前动态标记](#当前动态标记)实现；Rust 组件映射仍是目标。

```text
if online {
    Text { text: "已连接" }
} else {
    Text { text: "离线" }
}
for item in items key item.id {
    Text { text: item.title }
}
```

结构条件为假时销毁对应子树，重新出现时重新初始化本地状态。列表必须给稳定唯一 key；重复 key 为错误，既有列表保持不变。更新保留 key 未变的控件身份和本地状态，移动仅修改顺序，不执行全树 diff。大数据使用虚拟列表控件，普通 for 不自动虚拟化。

```text
component Counter(start: int = 0) {
    state count: int = start
    Column {
        Text { text: str(count) }
        Button { text: "+1"; on clicked { count += 1 } }
    }
}
```

组件支持有类型的输入属性、具名事件和一个默认 slot（具名 slot 仍是目标）；slot 只表示由调用方提供的子内容，不引入继承层次。首版使用组合，不做组件类继承。导入通过 `use "relative.aegle"` 或 `use md3 from rust("my_md3")`，依赖环拒绝，Rust 映射需构建时或宿主显式注册。

组件间通信采用输入属性和事件；没有隐式全局状态。样式通过主题 token 与本地属性，不实现 CSS 选择器/级联语言。动画通过按属性组的 transition，例如 `scale_transition: [120ms, ease_out]`；详见[主题动画](components-theme-animation.md)。

## 后续目标：加载与重载

编译工具和 loader 共用解析、源位置和类型检查规则；前者生成构造已检查程序的 Rust 代码，后者在运行时解析，两者交给同一引擎执行。当前引擎保留已检查程序（含表达式树）供绑定和块重建使用，不保留源码或解析器状态；表达式以小型树解释，不引入 JIT、GC 或动态代码加载。

运行时组件/宿主动作注册表只有 loader 构建需要。未注册组件、未知属性、错误类型和不支持能力均在挂载前报错，不能静默忽略拼写错误。

首版只提供显式 `reload()`，不内置文件监视器。先解析、检查、构造未挂载子树，成功后原子替换根；失败保留旧界面。替换保留宿主显式模型，重置控件本地状态，结束旧 IME 会话和动画，不承诺复杂热重载状态迁移。

默认加载上限：源文件总计 1 MiB、嵌套 64 层、单窗口 10,000 个逻辑节点；超限错误，不隐式无限扩容。宿主可配置上限。错误包含文件、行列、组件路径和原因，提供数据结构及文本呈现，不依赖重型常驻语言服务器。

首版不做通用脚本插件、在线包管理器、动态 Rust 编译、反射式任意方法调用或独立可视化 IDE。
