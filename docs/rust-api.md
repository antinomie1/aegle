# Rust 命令式 API

状态：v0.1 公共接口设计。代码表达约定的 API 形态，当前仓库没有实现，不能宣称这些示例已编译。项目采用 Rust 2024，编译器基线见[依赖](dependencies.md)。

## 完整 Hello world

```rust
use aegle::prelude::*;
fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Hello")?;
    window.text("你好，世界")?;
    app.run()
}
```

完整源码 7 行，包含导入、入口、初始化和事件循环。按 rustfmt 后非空源码行计数，不将多个语句强压在同一行。Cargo 清单不计；任何必须手写的应用初始化辅助文件计入。

`App::new` 初始化目标平台和 UI 上下文，文字/GPU 大资源按首次使用创建；`run(self)` 在主线程接管循环。创建窗口失败返回错误；应用最后一个普通窗口关闭后退出。shell 应用通过 `AppOptions { lifetime: Explicit, .. }` 保持宿主，调用 `app_proxy.quit()` 退出，不用隐藏窗口维持生命周期。

## 创建、修改与事件

```rust
let label = window.text("等待操作")?;
let button = window.button("完成")?;
button.on_click(move |_| {
    label.set_text("已完成")?;
    Ok(())
})?;
```

控件创建一次。父对象提供 `row`、`column`、`text`、`button` 等常用简写，第三方组件使用 `parent.add(component)` 或生成的组件函数。设置是直接命令，不要求嵌套函数、消息枚举或 builder 链。

同一个 `on_click` 再次设置时替换前一处理器；多订阅使用显式 `listen` 返回的订阅句柄，并由所属控件持有或调用者保留。控件销毁清理全部订阅，丢弃普通控件句柄不销毁控件。

公开创建和修改操作返回 `Result`，常见失败包括失效句柄、错误属性类型、能力缺失和资源预算不足；失败不引起 FFI 展开。`App::on_error` 设置运行中错误处理器，默认向诊断输出写一条带来源的错误，不无限重复记录同一故障。

## 常用接口族

| 接口族 | 必须覆盖的操作 |
| --- | --- |
| 结构 | 创建、添加/移除、移动父节点、按显式 ID 查询、子节点遍历、显示/隐藏 |
| 内容 | 文本、图标路径、解码图像/图像资源、tooltip/标签关联 |
| 布局 | 尺寸约束、内外边距、gap、Flex、对齐、滚动；可选 Grid |
| 视觉 | 颜色、边框、圆角、透明度、二维变换、裁剪；可选路径/阴影/模糊 |
| 状态 | enabled、checked、selected、expanded、value、编辑只读/密码模式 |
| 事件 | 指针、键盘、焦点、控件动作、值变化；停止传播与阻止默认动作 |
| 语义 | role、label、description、关系、范围和动作；自定义语义合并 |
| 主题与动画 | 选择主题、局部覆盖、类型化 token、过渡、显式动画和取消 |
| 平台 | 窗口属性、可用能力、显示输出、剪贴板、关闭；可选 layer-shell |

`get_*` 返回逻辑目标值；动画呈现值通过 `presented_*` 查询。几何默认是最近提交的布局结果；需要立即读取新布局时使用显式 `flush_layout()`，该调用可有较高成本且禁止在布局/绘制回调内重入。

`app.batch(|| ...)` 合并失效提交，不提供事务回滚；回调内修改本来就会合并。运行模型详见[架构](architecture.md)。

## 组件库作者

简单皮肤通过现有行为创建，不重写输入和无障碍：

```rust
#[aegle::component]
pub fn QuietButton(parent: &Container, text: &str) -> Result<Button> {
    let button = parent.control_button(text)?;
    button.set_background(theme::SURFACE)?;
    button.set_foreground(theme::ON_SURFACE)?;
    button.set_radius(8.dp())?;
    button.set_hover_background(theme::HOVER)?;
    button.transition(Background, 120.ms(), EaseOut)?;
    Ok(button)
}
```

`control_button` 自带按钮语义、标签关联、焦点、键盘与指针激活及禁用规则，不带默认皮肤。组件宏生成可选的标记属性/构造描述，不将运行时反射加入不使用 loader 的构建。事件继续通过返回的 Button 句柄绑定。

复杂控件通过 `Control` 接口定义状态与事件行为，通过 `Painter` 输出 scene 命令，通过 `Semantics` 定义辅助技术行为。布局使用共享的 layout 属性及文字测量服务。只有绘制自定义形状时才需要 Painter，不强制每个组件实现一组空 trait 方法。

控件可通过 `register_component::<T>("package.Type")` 加入运行时注册表；编译型标记的 `use` 映射到 Rust 路径，直接调用构造器。模块内使用有类型的属性，运行时加载边界才进行 Value 类型检查。

## 所有权与异步

App 拥有控件树；控件句柄为弱引用和代数 ID。处理器可以捕获其他控件句柄而不形成强拥有环。后台线程只能使用 UiProxy，不发送 UI 句柄；投递后在主线程重新定位 ID，目标已销毁时返回/报告 DeadHandle。

第三方任务系统接入通过显式 post/取消句柄。框架不要求应用把所有函数变成 async，不让文本输入处理等待任意网络任务。
