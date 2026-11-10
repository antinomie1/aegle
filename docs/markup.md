# 界面标记语言 (.aegle)

Aegle 提供类 QML 风格的花括号标记语言（扩展名 `.aegle`），作为界面的声明式编写入口。它支持**编译期零开销宏展开**与**运行时动态解释加载**两条并行路径。

---

## 一、 语法概览与最小示例

### 1. 结构与语法
```text
Window {
    title: "Aegle 示例"
    width: 480dp
    height: 320dp
    state count: int = 0

    Column {
        padding: 16dp
        gap: 8dp

        Text {
            id: label
            text: "计数: " + str(count)
        }

        Button {
            id: btn
            text: "增加"
            on clicked {
                count = count + 1
            }
        }
    }
}
```

* **属性赋值**：`key: value`；表达式自动根据引用的 `state` 变量建立**单向响应式绑定**。
* **事件块**：`on <event> { ... }` 处理点击、数值变更等交互动作。
* **具名引用**：`id: name` 赋予节点文件内唯一标识，用于跨控件访问或在 Rust 中获取有类型弱句柄。

### 2. 效果、过渡与动画

```text
Column {
    radius: 12dp; corners: [bottom_left, bottom_right]; opacity: 0.95; backdrop_blur: 8dp
    shadow: [0dp, 4dp, 12dp, 0dp, #00000040]
    background_gradient: linear(90, #89b4fa, #f5c2e7)
    transition: spring(170, 26)                          // 所有过渡用弹簧
    scale_transition: [300ms, cubic_bezier(0.2, 0.8, 0.2, 1)]
    Text { text: "…"; opacity_animation: [[0.4, 1], 1000ms, ease_in_out, alternate] }
    Arc { width: 40dp; height: 40dp; sweep: level * 360.0; stroke: #89b4fa
          stroke_width: 4dp; cap: round }
    Shape { width: 24dp; height: 24dp; path: "M 4 12 L 10 18 L 20 6"; stroke: #ffffff
            stroke_width: 2dp; cap: round }
    Shader { source: "glow.wgsl"; values: [level, 0.5]; height: 24dp; radius: 6dp }
}
```

```wgsl
// glow.wgsl：uv 为 0..1（自左上角），size 为逻辑像素；返回非预乘 sRGB。
fn shade(uv: vec2f, size: vec2f) -> vec4f {
    let pulse = 0.5 + 0.5 * sin(time * 3.0 - uv.x * 6.0);
    return vec4f(0.54, 0.71, 0.98, values[0] * pulse);
}
```

* **过渡时序**（`transition` 与 `paint_`/`offset_`/`scale_`/`rotation_`/`shadow_`/`opacity_transition`）：时长、`[时长, 缓动]` 或 `spring(刚度, 阻尼)`（时长为弹簧自身的稳定时间）。缓动为 `linear`、`ease_in`、`ease_out`、`ease_in_out`、`cubic_bezier(x1, y1, x2, y2)`（`x` 在 0..=1）或弹簧。
* **动画**（`scale_animation`、`rotation_animation`、`opacity_animation`）：`[[值...], 时长]`，可再跟一个缓动和 `forever` 或 `alternate`（永远往返）。两个以上的值均匀分布在一个周期上；节点建好即开始，默认播放一次。只接受字面量。
* **裁剪**：容器上的 `clip: true` 把子项的绘制、命中测试与指针形状限制在自身边界内，布局不变。
* **挖洞**：`cut: true` 的节点不画背景，而是把自身形状内已画的内容清成透明（透明窗口在那里露出后面的桌面）；边框、焦点环与内容照常绘制。在有自身 opacity 或 backdrop_blur 的组里只挖穿该组。
* **内阴影**：`inset_shadow: [x, y, blur, spread, color]` 画在背景之上、边框与内容之下，跟随圆角；偏移移动未被遮暗的中间，正的 spread 加宽阴影边。blur 必须为正（无模糊的内阴影就是边框），不参与过渡。
* **圆角**：`corners: [bottom_left, bottom_right]` 让 `radius` 只作用于列出的角（`top_left`、`top_right`、`bottom_right`、`bottom_left`，默认全部），例如贴在屏幕顶边的栏只圆下面两个角。
* **着色器**（`shaders` 特性）：`Shader` 以 `source` 处的 WGSL 文件（相对工作目录）作背景，跟随圆角，没有固有尺寸；程序定义 `fn shade(uv: vec2f, size: vec2f) -> vec4f`，可读全局 `time`（程序开始后的秒数）与 `values`（`array<f32, 8>`，来自 `values: [...]`，至多 8 个数，可绑定 `list<float>`）。源码（注释外）提到 `time` 时节点在可见期间每帧重绘，否则只画一次；减少动态效果时 `time` 停在 0。编译失败时视图清空并触发 `failed`，`self.error` 带文件、行、列。只有 Vulkan 与 wgpu 渲染器能画，软件渲染器让该帧失败。
* **形状**：`Shape` 的 `path` 是 SVG 路径数据（全部命令，含圆弧与相对坐标），单位为 dp、原点在左上角，解析失败时不绘制；`Arc` 画能放进边界的最大圆上从 `start` 起 `sweep` 度的弧（0 度向上、顺时针，默认整圈）。两者都有 `fill`、`stroke`、`stroke_width`、`cap`（`butt`/`round`/`square`），没有固有尺寸。缩放形状用 `scale`。

---

## 二、 编译期执行模式 (`ui!`)

通过过程宏在编译期将 `.aegle` 文档直接生成为普通 Rust 控件树调用代码：

```rust
use aegle::prelude::*;

fn main() -> Result<()> {
    // 1. 作为 App 启动入口直接运行
    App::run_ui(aegle::ui!("main.aegle"))?;

    // 2. 或挂载到现有窗口容器，返回类型化视图
    let view = aegle::ui!(&window, "panel.aegle")?;
    view.btn.on_click(move |_| {
        println!("Rust 闭包直接接收点击事件");
    });
    Ok(())
}
```

* **零解析开销**：发布二进制中不包含标记解析器代码。
* **类型安全**：生成的 `View` 自动携带文件中所有 `id` 的具体弱句柄（如 `Button`、`Text`），在编译期验证属性与事件匹配。
* **文件跟踪**：宏为每个导入文件生成构建依赖，文件变更自动触发 `cargo` 增量重编译。

---

## 三、 动态加载模式 (`aegle-loader`)

通过解释引擎在运行时加载、运行或热重载标记文档：

```rust
use aegle::loader::Program;

let program = Program::load("ui/dashboard.aegle")?;
let view = program.build(&window)?;

// 动态修改 state
view.set("count", 42)?;

// 支持热重载更新
view.reload()?;
```

* **安全边界**：不引入通用 JavaScript 引擎或脚本 VM。表达式仅支持基础算术、逻辑运算、字符串拼接、三元运算与安全内置函数。
* **受限执行**：事件块语句执行上限为 10,000 条，嵌套调用上限 64 层，彻底杜绝死循环导致 UI 挂起。

---

## 四、 结构控制与组件复用

### 1. 条件渲染与循环
```text
Column {
    if show_details {
        Text { text: "详细信息内容" }
    }

    for item in items key item.id {
        Row {
            Text { text: item.title }
            Button { text: "删除"; on clicked { host.remove(item.id) } }
        }
    }
}
```

### 2. 自定义组件与插槽 (`component` / `slot`)
```text
component Card(title: string = "默认标题") {
    Column {
        border_width: 1dp
        border_color: #cccccc
        Text { text: title; font_size: 16dp }
        slot
    }
}

// 使用组件
Card {
    title: "用户资料"
    Text { text: "姓名: 张三" }
}
```
* **参数**：`component 名称(参数: 类型 = 默认值, ...)`，没有默认值的参数必须在使用处写出；组件内部直接按名字读取参数，参数可绑定外部表达式并随之更新。组件可声明自己的 `state`，用 `event 名称(类型)` 与 `emit` 向外发事件。
* **单向作用域**：插槽内容继承外部作用域，避免状态冲突。

---

## 五、 窗口文档与桌面面板

根为 `Window` 的文档由 `Program::open(&app)`（或 `ui!(&app, ...)`）打开为原生窗口。`title`、`width`、`height`、`theme` 作用于窗口本身。加上 `layer` 后同一文档打开为桌面 shell 的层（Wayland wlr layer-shell），一个文件即一个面板：

```text
Window {
    layer: top                     // background | bottom | top | overlay
    anchor: [top, left, right]     // 一个或多个不重复的 top / bottom / left / right
    exclusive_zone: 32dp           // 非负整数 dp，或 ignore（与其他保留区重叠）
    keyboard: on_demand            // none | exclusive | on_demand
    margin: [4dp, 0dp]             // 整数 dp，一个、两个或四个（CSS 顺序）
    output: "DP-1"                 // 输出名，未连接时打开失败
    height: 32dp

    Row { Text { text: "面板" } }
}
```

* 这些属性只接受字面量，并且都要求同时写 `layer`；锚定两条相对边的轴随输出拉伸，否则使用 `width`/`height`。
* `program.open_on(&app, output.id)` 忽略文档里的 `output`，把同一文档放到给定输出上，配合 `app.outputs()` 与 `app.on_outputs` 可为每个显示器各开一个面板。
* **热重载**：`view.reload(&program)` 保留原生 surface、窗口句柄和同名同类型 state，原地应用新的 `layer`、`anchor`、`exclusive_zone`、`keyboard`、`margin` 与尺寸；增删 `layer` 或改指 `output` 会在构建前被拒绝，原面板不变，需重新打开。

---

## 六、 与宿主交互

* **宿主函数**：事件块中的 `host.name(args)` 调用宿主以 `aegle::loader::action(name, &[Type], f)` 注册的函数（本线程所有程序共享）；构建时校验存在性与参数类型。
* **状态变化**：`State<T>::on_change(f)` 在 state 每次变化后调用 `f(新值)`，无论变化来自事件块、绑定源还是 Rust；绑定先更新，处理器在所有 UI 借用之外运行，不会被自身对该 state 的修改重入；返回的错误中止本次更新并交给修改方。返回的 `Watch::remove()` 注销处理器。重载后处理器与 `State` 句柄跟随同名同类型的 state。
* **与 Rust `Signal` 连接**：`State<T>::bind_signal(&signal)` 让 state 取 `aegle_ui::Signal<T>` 的当前值，之后两边的修改互相同步（已相等时停止回传），连接随重载跟随 state；signal 一侧修改写入 state 时的错误交给宿主的错误处理（原生 App 为 `on_error`）。返回的 `Binding::remove()` 断开两个方向。
