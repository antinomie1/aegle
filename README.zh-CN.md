# Aegle

[English](https://github.com/antinomie1/aegle/blob/main/README.md) · **简体中文**

Aegle 是面向 Rust 2024 的模块化保留模式 GUI 库，关注低内存占用、事件驱动更新、CJK 文字与原生输入法。控件创建一次，之后通过句柄修改；界面空闲时不重绘也不轮询。

它由一组相互独立的 crate 组成（保留树、Taffy 布局、文字与字形、绘制 scene、软件（tiny-skia 或系统 cairo）/ Vulkan / wgpu 渲染器、SDL3、Wayland、Win32 与 macOS 平台后端、控件、主题、标记语言），再加上把它们串起来的 `aegle` facade。0.1 是预览版：各平台已实现与已验证的能力及已知限制见[实现状态](https://github.com/antinomie1/aegle/blob/main/docs/implementation.md)。

**API 稳定性。** 1.0 之前，任何 0.x 次版本（0.1 → 0.2）都可能有破坏性变更，这符合 Cargo 的 semver 规则；补丁版本（0.1.x）不会。对外支持的入口是 `aegle` facade，编写控件库时另有 `aegle-ui` 与 `aegle-widgets`；其余 crate 是为了让 facade 能依赖它们而发布的组成部分，变动更自由。部分 API 重导出了 `wgpu`、`ash` 与 `accesskit`，这些依赖的破坏性升级也是 Aegle 的破坏性版本。

## 最小开始

引入 facade crate（Linux 使用系统字体需要 Fontconfig，`wayland` 后端还需要 libxkbcommon 开发文件）：

```toml
[dependencies]
aegle = "0.1"
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

在 `aegle` crate 上设置。默认启用 `desktop`、`sdl3` 与 `tiny-skia`；`desktop` 即 `native`、`system-fonts`、`markup`、`motion`、`effects` 与 `desktop-services`。

一个构建只包含一个渲染器。四个渲染器 feature（`tiny-skia`、`cairo`、`vulkan`、`wgpu`）平级且互斥：同时启用两个会编译失败，因此二进制里不会带着用不到的后端，`aegle::app::RENDERER` 给出本构建的那一个。`desktop` 是除渲染器与 Linux 窗口后端以外的全部默认 feature，换渲染器时 Windows 与 macOS 用 `--no-default-features --features desktop,<渲染器>`，Linux 用 `desktop,sdl3,<渲染器>`（或 `desktop,wayland,<渲染器>`），例如 `cargo run -p aegle --release --no-default-features --features desktop,sdl3,cairo --example showcase`。

| Feature | 默认 | 作用 |
| --- | :---: | --- |
| `desktop` | ✓ | 除渲染器与 Linux 窗口后端以外的默认 feature |
| `native` | ✓ | 原生窗口宿主：Windows Win32、macOS AppKit；Linux 另需 `sdl3`（默认）或 `wayland` |
| `sdl3` | ✓ | Linux：经发行版 `libSDL3.so.0` 开窗，运行时加载 |
| `wayland` |  | Linux：改用原生 Wayland 客户端，带 layer 表面、拖动源与 text-input-v3 周边文字 |
| `tiny-skia` | ✓ | 渲染器：CPU 绘制（tiny-skia 覆盖率） |
| `cairo` |  | 渲染器：CPU 绘制（系统 cairo 覆盖率，仅 Linux），运行时加载 `libcairo.so.2` |
| `system-fonts` | ✓ | 系统字体发现 |
| `markup` | ✓ | `ui!` 宏与标记引擎，含运行时加载 |
| `motion` | ✓ | 过渡、关键帧动画与弹簧 |
| `effects` | ✓ | 图像效果（`aegle::image::effects`） |
| `colrv1` |  | COLRv1 彩色字形 |
| `desktop-services` | ✓ | 文件对话框、通知、托盘与全局快捷键（`App::desktop`） |
| `vulkan` |  | 渲染器：经 ash 的 Vulkan |
| `wgpu` |  | 渲染器：全平台通用的 wgpu（Vulkan、Metal、Direct3D 12） |
| `jpeg` / `webp` / `gif` |  | `aegle::image::decode` 解码 JPEG、静态 WebP 与 GIF 首帧 |
| `svg` |  | 静态 SVG 图像（`aegle::image::svg`）与 OpenType-SVG 字形 |
| `grid` |  | 网格与叠放容器，及标记的 `Grid`/`Stack` 组件 |
| `shaders` |  | 自定义片元程序（`ShaderView`、标记 `Shader`），由 `vulkan` 与 `wgpu` 渲染器绘制 |
| `accessibility` |  | 语义树与目标平台的系统适配（Linux AT-SPI，引入 zbus；Windows UI Automation；macOS NSAccessibility）；隐含 `native` |

`native` 只编译目标平台的窗口 crate，`accessibility` 只编译目标平台的适配器；Linux 上 `sdl3` 与 `wayland` 互斥，同时开启是编译错误，经本 facade 只给 `native` 而没有后端同样是带重建命令的编译错误；各 crate 的分工见 [模块、依赖与构建组合](https://github.com/antinomie1/aegle/blob/main/docs/modules.md)。

## 文档

- [API 指南](https://github.com/antinomie1/aegle/blob/main/docs/developer/api.md)：依赖与 feature、应用与窗口、布局、样式、事件、动画、标记与嵌入宿主。
- [控件参考](https://github.com/antinomie1/aegle/blob/main/docs/developer/controls.md)：每个默认控件的用法与各状态截图。
- [不经 facade 使用控件库](https://github.com/antinomie1/aegle/blob/main/docs/developer/standalone.md)：只用 aegle-ui、aegle-widgets、aegle-theme、aegle-motion 接入自己的宿主，以及编写自定义控件。
- [模块、依赖与构建组合](https://github.com/antinomie1/aegle/blob/main/docs/modules.md)、[设计索引](https://github.com/antinomie1/aegle/blob/main/docs/README.md)、[实现状态](https://github.com/antinomie1/aegle/blob/main/docs/implementation.md)。

## 许可证

可任选 [Apache License 2.0](https://github.com/antinomie1/aegle/blob/main/LICENSE-APACHE) 或 [MIT 许可证](https://github.com/antinomie1/aegle/blob/main/LICENSE-MIT)。

除非你明确另行声明，你有意提交以纳入本项目的任何贡献（按 Apache-2.0 许可证的定义），均按上述双许可证授权，不附加任何其他条款或条件。
