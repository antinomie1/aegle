# Aegle

[English](README.md) · **简体中文**

Aegle 是面向 Rust 2024 的模块化保留模式 GUI 库，关注低内存占用、事件驱动更新、CJK 文字与原生输入法。控件创建一次，之后通过句柄修改；界面空闲时不重绘也不轮询。

它由一组相互独立的 crate 组成（保留树、Taffy 布局、文字与字形、绘制 scene、软件 / Vulkan / wgpu 渲染器、Wayland 与 Win32 平台后端、控件、主题、标记语言），再加上把它们串起来的 `aegle` facade。项目仍在开发中，尚未发布到 crates.io；当前可用的能力见[实现状态](docs/implementation.md)。

## 最小开始

以 path 或 git 依赖引入 facade crate（Linux 需要 libxkbcommon 开发文件，使用系统字体还需要 Fontconfig）：

```toml
[dependencies]
aegle = { path = "path/to/aegle/crates/aegle" }
```

在 `Cargo.toml` 旁边创建 `main.aegle` 描述界面：

```text
Window {
    title: "Hello"
    Text { text: "你好，世界" }
}
```

然后运行：

```rust
use aegle::prelude::*;

fn main() -> aegle::Result<()> {
    aegle::App::run_ui(aegle::ui!("main.aegle"))
}
```

等价的纯 Rust 写法：

```rust
use aegle::prelude::*;

fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Hello")?;
    window.text("你好，世界");
    app.run()
}
```

也可以直接运行自带示例：`cargo run -p aegle --example controls --release`。

## Features

在 `aegle` crate 上设置。默认启用 `native`、`software`、`system-fonts`、`text-dictionary`、`markup`、`motion`、`effects`、`colrv1` 与 `desktop-services`。

| Feature | 默认 | 作用 |
| --- | :---: | --- |
| `native` | ✓ | 原生窗口：Linux Wayland、Windows Win32 |
| `software` | ✓ | CPU 软件绘制 |
| `system-fonts` | ✓ | 系统字体发现 |
| `markup` | ✓ | `ui!` 宏与标记引擎，含运行时加载 |
| `text-dictionary` | ✓ | 中日及东南亚文字的词典分词 |
| `motion` | ✓ | 过渡、关键帧动画与弹簧 |
| `effects` | ✓ | 图像效果（`aegle::image::effects`） |
| `colrv1` | ✓ | COLRv1 彩色字形 |
| `desktop-services` | ✓ | 文件对话框、通知、托盘与全局快捷键（`App::desktop`） |
| `vulkan` |  | Vulkan 绘制 |
| `wgpu` |  | 全平台通用的最小 wgpu 绘制 |
| `accessibility` |  | 导出语义树，不接系统适配器 |
| `unix-accessibility` |  | Linux AT-SPI 适配（含 `accessibility`） |
| `windows-accessibility` |  | Windows UI Automation 适配（含 `accessibility`） |

更细的 feature（`wayland`、`windows`）和各 crate 的分工见 [Crate、示例与构建组合](docs/crates-and-examples.md)。

## 文档

- [API 指南](docs/developer/api.md)：依赖与 feature、应用与窗口、布局、样式、事件、动画、标记与嵌入宿主。
- [控件参考](docs/developer/controls.md)：每个默认控件的用法与各状态截图。
- [不经 facade 使用控件库](docs/developer/standalone.md)：只用 aegle-ui、aegle-widgets、aegle-theme、aegle-motion 接入自己的宿主，以及编写自定义控件。
- [Crate、示例与构建组合](docs/crates-and-examples.md)、[设计索引](docs/README.md)、[实现状态](docs/implementation.md)。

## 许可证

可任选 [Apache License 2.0](LICENSE-APACHE) 或 [MIT 许可证](LICENSE-MIT)。

除非你明确另行声明，你有意提交以纳入本项目的任何贡献（按 Apache-2.0 许可证的定义），均按上述双许可证授权，不附加任何其他条款或条件。
