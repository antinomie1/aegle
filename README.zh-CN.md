# Aegle

[English](README.md) · **简体中文**

Aegle 是面向 Rust 2024 的模块化保留模式 GUI 库，关注低内存占用、事件驱动更新、CJK 文字与原生输入法。控件创建一次，之后通过句柄修改；界面空闲时不重绘也不轮询。

它由一组相互独立的 crate 组成（保留树、Taffy 布局、文字与字形、绘制 scene、软件（tiny-skia 或系统 cairo）/ Vulkan / wgpu 渲染器、SDL3、Wayland、Win32 与 macOS 平台后端、控件、主题、标记语言），再加上把它们串起来的 `aegle` facade。项目仍在开发中，尚未发布到 crates.io；当前可用的能力见[实现状态](docs/implementation.md)。

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

fn main() -> Result<()> {
    App::run_ui(aegle::ui!("main.aegle"))
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

在 `aegle` crate 上设置。默认启用 `native`、`tiny-skia`、`system-fonts`、`markup`、`motion`、`effects`、`colrv1` 与 `desktop-services`。

一个构建只包含一个渲染器。四个渲染器 feature（`tiny-skia`、`cairo`、`vulkan`、`wgpu`）平级且互斥：同时启用两个会编译失败，因此二进制里不会带着用不到的后端，`aegle::app::RENDERER` 给出本构建的那一个。`desktop` 是除渲染器以外的全部默认 feature，换渲染器就用 `--no-default-features --features desktop,<渲染器>` 替掉 tiny-skia，例如 `cargo run -p aegle --release --no-default-features --features desktop,cairo --example showcase`。

| Feature | 默认 | 作用 |
| --- | :---: | --- |
| `native` | ✓ | 原生窗口宿主：Windows Win32、macOS AppKit；Linux 另需 `sdl3`（默认）或 `wayland` |
| `sdl3` | ✓ | Linux：经发行版 `libSDL3.so.0` 开窗，运行时加载 |
| `wayland` |  | Linux：改用原生 Wayland 客户端，带 layer 表面、拖动源与 text-input-v3 周边文字 |
| `tiny-skia` | ✓ | 渲染器：CPU 绘制（tiny-skia 覆盖率） |
| `cairo` |  | 渲染器：CPU 绘制（系统 cairo 覆盖率，仅 Linux），运行时加载 `libcairo.so.2` |
| `system-fonts` | ✓ | 系统字体发现 |
| `markup` | ✓ | `ui!` 宏与标记引擎，含运行时加载 |
| `motion` | ✓ | 过渡、关键帧动画与弹簧 |
| `effects` | ✓ | 图像效果（`aegle::image::effects`） |
| `colrv1` | ✓ | COLRv1 彩色字形 |
| `desktop-services` | ✓ | 文件对话框、通知、托盘与全局快捷键（`App::desktop`） |
| `vulkan` |  | 渲染器：经 ash 的 Vulkan |
| `wgpu` |  | 渲染器：全平台通用的 wgpu（Vulkan、Metal、Direct3D 12） |
| `accessibility` |  | 语义树与目标平台的系统适配（Linux AT-SPI，引入 zbus；Windows UI Automation；macOS NSAccessibility）；隐含 `native` |

`native` 只编译目标平台的窗口 crate，`accessibility` 只编译目标平台的适配器；Linux 上 `sdl3` 与 `wayland` 互斥，同时开启是编译错误，经本 facade 只给 `native` 而没有后端同样是带重建命令的编译错误；各 crate 的分工见 [Crate、示例与构建组合](docs/crates-and-examples.md)。

## 文档

- [API 指南](docs/developer/api.md)：依赖与 feature、应用与窗口、布局、样式、事件、动画、标记与嵌入宿主。
- [控件参考](docs/developer/controls.md)：每个默认控件的用法与各状态截图。
- [不经 facade 使用控件库](docs/developer/standalone.md)：只用 aegle-ui、aegle-widgets、aegle-theme、aegle-motion 接入自己的宿主，以及编写自定义控件。
- [Crate、示例与构建组合](docs/crates-and-examples.md)、[设计索引](docs/README.md)、[实现状态](docs/implementation.md)。

## 许可证

可任选 [Apache License 2.0](LICENSE-APACHE) 或 [MIT 许可证](LICENSE-MIT)。

除非你明确另行声明，你有意提交以纳入本项目的任何贡献（按 Apache-2.0 许可证的定义），均按上述双许可证授权，不附加任何其他条款或条件。
