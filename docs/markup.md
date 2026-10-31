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
