# 界面标记语言 (.aegle)

Aegle 提供类 QML 风格的花括号标记语言（扩展名 `.aegle`），作为界面的声明式编写入口。它支持**编译期零开销宏展开**与**运行时动态解释加载**两条并行路径。

---

## 一、 语法概览与最小示例

### 1. 结构与语法
```text
Window {
    title: "Aegle 示例"
    width: 480
    height: 320

    Column {
        padding: 16
        gap: 8

        Text {
            id: label
            text: "计数: " + str(count)
        }

        Button {
            id: btn
            text: "增加"
            on click {
                count = count + 1
            }
        }
    }

    state {
        count: int = 0
    }
}
```

* **属性赋值**：`key: value`；表达式自动根据引用的 `state` 变量建立**单向响应式绑定**。
* **事件块**：`on <event> { ... }` 处理点击、数值变更等交互动作。
* **具名引用**：`id: name` 赋予节点文件内唯一标识，用于跨控件访问或在 Rust 中获取有类型弱句柄。

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
            Button { text: "删除"; on click { remove(item.id) } }
        }
    }
}
```

### 2. 自定义组件与插槽 (`component` / `slot`)
```text
component Card {
    property title: string = "默认标题"
    slot content

    Column {
        border_width: 1
        border_color: #ccc
        Text { text: self.title; font_size: 16 }
        slot content
    }
}

// 使用组件
Card {
    title: "用户资料"
    Text { text: "姓名: 张三" }
}
```
* **单向作用域**：组件内部通过 `self.property` 访问自身属性，插槽内容继承外部作用域，避免状态冲突。

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
