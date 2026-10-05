# Rust 命令式 API

状态：v0.1。基础 Ui/App、弱句柄、命令式控件与静态标记编译已有源码；组件宏、动态标记和完整扩展接口仍是设计目标，下文分别标注。验证记录见[实现状态](implementation.md)。项目采用 Rust 2024，编译器基线见[依赖](dependencies.md)。

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

源码位于 `crates/aegle/examples/hello.rs`，7 行 Rust 加 1 行文档注释，包含导入、入口、初始化和事件循环；运行入口为 `cargo run -p aegle --example hello`。当前需要 Linux Wayland 和可显示所用字符的系统字体。按 rustfmt 后非空源码行计数，不将多个语句强压在同一行。Cargo 清单不计；任何必须手写的应用初始化辅助文件计入。

当前 `App::new` 连接 Wayland 并建立系统字体上下文；`run(self)` 在主线程接管循环，最后一个窗口关闭后退出。各窗口的控件树独立，字体系统和软件 renderer 共享。`App::with_fonts` 接受显式字体集合，可关闭 `system-fonts`；`AppOptions` 配置 app_id、初始主题和软件 mask 预算，`WindowOptions` 配置初始尺寸与 SHM 预算。shell 显式生命周期、GPU 和其他原生平台尚未实现。

`aegle` 当前默认启用 Wayland、系统字体、Unix 无障碍与编译型静态标记。嵌入式宿主可直接使用无默认平台 feature 的 `aegle-app::Ui::with_fonts(Rc<RefCell<TextSystem>>, Theme)`，取得 root 后创建同样的控件；通过输入、`refresh`、`visit_scenes`、IME 和可选语义接口对接自己的宿主。

界面优先写在 `.aegle` 文件中；`App::run_ui(aegle::ui!("main.aegle"))` 完成默认初始化、构造和运行。`let view = aegle::ui!(&window, "panel.aegle")?` 返回带 `root` 和各 `id` 字段的有类型弱句柄集合，可直接给 `view.done.on_click(...)` 绑定下面的普通 Rust 回调。文件路径相对使用者包清单目录，编译器跟踪其变化；完整已实现属性见[标记语言](markup.md)。

## 创建、修改与事件

```rust
let label = window.text("等待操作")?;
let button = window.button("完成")?;
button.on_click(move |_| {
    label.set_text("已完成")?;
    Ok(())
})?;
```

控件创建一次。`Window` 解引用到根 `Container`；容器提供 `row`、`column`、`text`、`button`、`text_field`、`text_area`，返回相应弱句柄。多行与单行编辑器共用 `TextField` 句柄。设置是直接命令，不要求嵌套函数、消息枚举或 builder 链。`parent.add(component)` 和生成的第三方组件函数仍是后续扩展目标。

同一个 `on_click` 再次设置时替换前一处理器，`clear_on_click` 删除处理器；单行编辑器的 `on_submit` 使用相同规则。排队的动作携带注册版本，旧动作不会误调用替换后的处理器。处理器在树和原生宿主借用之外执行，可修改其他控件、删除自己或关闭窗口。回调中产生的新动作留待下一轮；控件销毁清理处理器，丢弃普通句柄不销毁控件。通用 `listen` 多订阅接口尚未实现。

公开创建和修改操作返回 `Result`；当前错误保留底层来源，包括 `DeadHandle`、跨 Ui 父节点、不合法数值、字体/呈现预算及平台能力错误。回调出错时删除失败处理器，保留此前合法修改；`App::run` 返回错误并结束。可配置 `App::on_error` 尚未实现。

当前公共操作包括：

| 类型 | 已有接口 |
| --- | --- |
| Node / 所有控件句柄 | `is_alive`、`bounds`、`remove`、`reparent`、`set_visible`、`set_enabled`、`focus`、`set_accessible_label` |
| 布局 | `set_size`、`set_width`、`set_height`、`set_min_size`、`set_min_width`、`set_min_height`、`set_grow`、`set_padding`、`set_gap` |
| Label / TextField | `text`、`set_text`；TextField 另有 `select`、`set_read_only`、`on_submit`、`clear_on_submit` |
| Button | `set_text`、`activate`、`on_click`、`clear_on_click` |
| Ui / Window | `set_theme`；Window 另有 `close` |

`bounds` 返回最近刷新后的窗口逻辑坐标。显式设置的 size、padding 和 gap 在切换主题后仍生效；当前没有通用属性表、局部主题树或动画呈现值查询。

## 目标接口族

以下覆盖完整版本的设计范围，超出上表的接口不应当作当前 API 使用。

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

## 组件库作者：后续扩展目标

本节的宏、Control/Painter/Semantics 和注册接口尚未实现。当前可直接组合 Container 的现有控件，或用独立 controls/text/scene 模块编写自己的宿主；尚不能用下面的代码定义可插入 App 的任意皮肤。

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

Ui 拥有控件树；App 持有各窗口 Ui，控件句柄为弱引用和代数 ID。处理器可以捕获其他控件或窗口句柄而不形成强拥有环。UI 句柄不能发送到后台线程；`UiProxy` 尚未实现，后续由其投递后在主线程重新定位 ID，目标已销毁时返回/报告 DeadHandle。

第三方任务系统的 post/取消句柄也是后续目标。框架不要求应用把所有函数变成 async，不让文本输入处理等待任意网络任务。

## 当前可用的底层接口

`Tree` 保存实际状态，`Route::rebuild/iter` 构造捕获/目标/冒泡路径，`Focus::set/advance` 按宿主策略处理焦点；这不是第二套函数式 UI 入口。`Button::handle(Input)` 返回激活、capture、焦点和绘制效果，`TextField::handle(&mut TextSystem, Input)` 在同一个 Editor 上实现编辑行为。控件不隐式获取平台服务。

IME 数据通过 `EditorDriver::apply_ime` 一次验证并应用，`Editor::surrounding` 借出有界周边文字。调用方消费 `Outcome` 与 `Editor::take_changes` 后同步布局、平台和绘制。`aegle-platform-wayland --example editor` 保留底层显式组装示例；`aegle --example controls` 展示应用层创建、编辑、按钮回调、主题切换与关闭窗口，使用上面的统一接口。
