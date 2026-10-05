# Aegle 标记语言

状态：v0.1。编译型静态结构、类型检查和具名弱句柄已实现；状态表达式、事件块、组件导入与运行时加载仍是下文明确列出的目标。采用 `.aegle` 扩展名；QML 风格的结构，不依赖 Qt/QML，也不强制 JavaScript。

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

两份手写源码合计 7 行，初始化包含在 App::run_ui 中。宏是构建期能力，生成直接创建/设置控件的 Rust 代码；编译产物不携带标记解析器。宏为读取文件生成编译器可追踪的依赖标记，修改标记文件必须触发重编译。

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

构造期间先建立子树再设置属性；任一步返回错误时删除本次新建的整棵子树，Window 根则关闭该窗口，保留调用方原有父节点。清理本身失败时返回清理错误。该规则只覆盖构造返回前的同步错误；后续刷新或原生呈现失败仍遵守 App 的错误处理。当前不是运行时原子重载 API。

支持 Window、Column、Row、Text、Button、TextField、TextArea。只有前三种可以包含子节点；Window 只可为文件根。文本默认为空字符串，窗口标题默认为 `Aegle`，其他默认值沿用命令式构造器。

| 属性 | 值与适用范围 |
| --- | --- |
| `id` | 唯一标识符，生成有类型句柄 |
| `title` | Window 字符串，最多 4000 UTF-8 字节且无 NUL |
| `text` | Text/Button/TextField/TextArea 字符串；单行编辑器拒绝硬换行 |
| `width`、`height` | 控件为非负 `dp` 或 `auto`；Window 为正整数 `dp`，对应原生建议尺寸，可被 compositor 覆盖 |
| `min_width`、`min_height`、`padding` | 非负 `dp` |
| `gap` | 容器的非负 `dp` |
| `grow` | 有限非负数值 |
| `visible`、`enabled` | bool，作用于控件子树 |
| `label` | 无障碍名称字符串 |
| `read_only` | TextField/TextArea 的 bool |
| `theme` | Window 的 `light`、`dark`、`high_contrast` |
| `background`、`foreground`、`border_color` | `#RRGGBB` 或 `#RRGGBBAA` 颜色 |
| `hover_background`、`pressed_background` | hover 限 Button/TextField/TextArea，pressed 限 Button |
| `disabled_background`、`disabled_foreground` | 对应禁用状态的颜色覆盖 |
| `border_width`、`radius` | 非负 `dp`；边框宽度为零时关闭 |
| `focus_color`、`focus_width` | Button/TextField/TextArea 的焦点颜色与非负 `dp` 宽度 |
| `selection_color`、`caret_color` | TextField/TextArea 的选择与 caret/预编辑颜色 |
| `font_size` | Text/Button/TextField/TextArea 的正 `dp` |

Window 的通用控件属性作用于其内容根；例如 `visible: false` 隐藏内容，不卸载原生窗口。单独设置宽度不会清除高度的主题默认值；显式高度在切换主题后保留。

声明必须以换行或分号分隔，最后一项可以直接跟 `}`；支持 `//` 注释和 JSON 字符串转义。数值为有限 f32，长度写为 `8dp`，颜色为非预乘 sRGB 字节，严格接受六位或八位十六进制；当前不支持百分比或时长字面量。未知类型/属性、重复属性/ID、不适用属性、错误类型及未实现语法均在编译期拒绝，错误带文件、行、Unicode scalar 列和源码片段。外观属性直接调用同一套本地 setter，状态优先级见[组件样式](components-theme-animation.md)，没有另一套标记样式引擎。

独立 `aegle-markup` 无第三方依赖，提供 AST、字节跨度、`parse`/`parse_with_limits` 和内建 schema 的 `check`。默认解析上限为 1 MiB、64 层、10,000 节点；显式解析深度最多 256，schema 检查最多 256 层/10,000 节点。`ui!` 使用默认上限。运行时不保留 AST、schema 或解析器；`syn`/`quote`/`proc-macro-crate` 仅用于构建宏及识别重命名依赖。

## 后续目标：结构、值和状态

以下状态、表达式、事件块、条件、列表、组件及 loader 均尚未实现；现在通过具名句柄绑定普通 Rust 回调。

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

事件块允许赋值、局部 let、if/else 和调用已声明的宿主动作。内建纯函数只含数值、字符串格式化与 clamp 等有限集合；不允许任意函数定义、递归、while、文件访问或网络访问。

宿主注册具名动作及参数/返回类型，界面使用 `host.refresh()` 等直接调用。耗时操作由宿主异步执行，结果通过更新 state/模型返回；标记语言不内置 await 或通用 VM。动作只能使用宿主明确暴露的能力。

每个事件块最多执行 10,000 个简单操作，每轮更新传播上限为 64 轮；超限报告错误并停止本轮，保留此前合法赋值。编译型与运行时执行型均遵守这些语义，限制可由宿主显式下调，不默认放开。

## 后续目标：条件、列表和组件

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

组件支持有类型的输入属性、具名事件和具名 slot；slot 只表示由调用方提供的子内容，不引入继承层次。首版使用组合，不做组件类继承。导入通过 `use "relative.aegle"` 或 `use md3 from rust("my_md3")`，依赖环拒绝，Rust 映射需构建时或宿主显式注册。

组件间通信采用输入属性和事件；没有隐式全局状态。样式通过主题 token 与本地属性，不实现 CSS 选择器/级联语言。动画通过类型化属性的 transition，例如 `transition opacity: 120ms ease_out`；详见[主题动画](components-theme-animation.md)。

## 后续目标：加载与重载

编译工具和 loader 共用解析、源位置和类型检查规则；前者生成 Rust，后者创建相同组件并保留必要表达式程序。运行时 AST 在创建后释放，不永久保留整份解析树；表达式采用小型树/指令表解释，不引入 JIT、GC 或动态代码加载。

运行时组件/宿主动作注册表只有 loader 构建需要。未注册组件、未知属性、错误类型和不支持能力均在挂载前报错，不能静默忽略拼写错误。

首版只提供显式 `reload()`，不内置文件监视器。先解析、检查、构造未挂载子树，成功后原子替换根；失败保留旧界面。替换保留宿主显式模型，重置控件本地状态，结束旧 IME 会话和动画，不承诺复杂热重载状态迁移。

默认加载上限：源文件总计 1 MiB、嵌套 64 层、单窗口 10,000 个逻辑节点；超限错误，不隐式无限扩容。宿主可配置上限。错误包含文件、行列、组件路径和原因，提供数据结构及文本呈现，不依赖重型常驻语言服务器。

首版不做通用脚本插件、在线包管理器、动态 Rust 编译、反射式任意方法调用或独立可视化 IDE。
