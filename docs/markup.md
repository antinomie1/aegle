# Aegle 标记语言

状态：v0.1 语言契约。采用 `.aegle` 扩展名；QML 风格的结构，不依赖 Qt/QML，也不强制 JavaScript。编译路径和可选运行时路径具有相同界面语义。

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

## 结构、值和状态

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

## 事件与宿主

事件块允许赋值、局部 let、if/else 和调用已声明的宿主动作。内建纯函数只含数值、字符串格式化与 clamp 等有限集合；不允许任意函数定义、递归、while、文件访问或网络访问。

宿主注册具名动作及参数/返回类型，界面使用 `host.refresh()` 等直接调用。耗时操作由宿主异步执行，结果通过更新 state/模型返回；标记语言不内置 await 或通用 VM。动作只能使用宿主明确暴露的能力。

每个事件块最多执行 10,000 个简单操作，每轮更新传播上限为 64 轮；超限报告错误并停止本轮，保留此前合法赋值。编译型与运行时执行型均遵守这些语义，限制可由宿主显式下调，不默认放开。

## 条件、列表和组件

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

## 编译、加载与重载

编译工具和 loader 共用解析、源位置和类型检查规则；前者生成 Rust，后者创建相同组件并保留必要表达式程序。运行时 AST 在创建后释放，不永久保留整份解析树；表达式采用小型树/指令表解释，不引入 JIT、GC 或动态代码加载。

运行时组件/宿主动作注册表只有 loader 构建需要。未注册组件、未知属性、错误类型和不支持能力均在挂载前报错，不能静默忽略拼写错误。

首版只提供显式 `reload()`，不内置文件监视器。先解析、检查、构造未挂载子树，成功后原子替换根；失败保留旧界面。替换保留宿主显式模型，重置控件本地状态，结束旧 IME 会话和动画，不承诺复杂热重载状态迁移。

默认加载上限：源文件总计 1 MiB、嵌套 64 层、单窗口 10,000 个逻辑节点；超限错误，不隐式无限扩容。宿主可配置上限。错误包含文件、行列、组件路径和原因，提供数据结构及文本呈现，不依赖重型常驻语言服务器。

首版不做通用脚本插件、在线包管理器、动态 Rust 编译、反射式任意方法调用或独立可视化 IDE。
