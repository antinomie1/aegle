# 实现状态

完整目标保持不变：模块化保留模式 GUI、GPU 与无 GPU 软件绘制、CJK/原生 IME、无障碍、组件/主题/动画、命令式与标记入口、跨平台及 API 文档。

## 已实现

- Rust 2024 workspace，MIT OR Apache-2.0；目标 MSRV 1.88，实际验证工具链为 1.96.1。
- aegle-types：no_std 几何与紧凑 RGBA 颜色，无第三方依赖。
- aegle-core：代数 ID、可复用槽位、保留树、索引子节点、结构变更与三通道失效，另有路由快照、默认动作控制及策略化焦点遍历；无第三方依赖。叶节点不分配子节点数组，删除不递归。
- aegle-layout：Taffy 0.14.0 直接适配同一棵保留树，无第二份拓扑；共享默认样式、测量缓存、Flex/Block、可选 Grid。它不依赖字体、窗口或 renderer。
- aegle-scene：no_std + alloc 的局部绘制记录；实色矩形、圆角、居中边框、共享 RGBA 图像、填充/描边路径、仿射变换、嵌套裁剪。可选 text 保存定位字形、变体坐标与共享字体句柄，不依赖排版器或栅格器。
- aegle-text：复用 Parley/Fontique 的 Unicode shaping、字体选择、回退、换行与定位；Paragraph 保留文字和排版结果，宽度变化只重排，颜色覆盖无需重新 shaping。显式字体为默认，system-fonts、text-dictionary、text-a11y、scene 独立选用；缺字与无可用字体分别报告。
- Editor：复用同一 TextSystem 的 Parley PlainEditor，提供单/多行、选择/命中、视觉移动、grapheme 删除、精确 UTF-8 替换、只读、组合输入模型及有界差量撤销/重做。预编辑只保留被替换片段，提交值不随预编辑改变；取消恢复原选区。文字、装饰和候选区域来自同一布局。密码模式只排版等量 `•`，明文另存且不记录历史、拒绝 IME 组合。apply_ime 在完整验证后应用删除/提交/预编辑事务，删除与提交合成一次撤销；surrounding 无分配地借出有界周边文字。编辑、宽度和样式改变目前仍会重新 shaping，不宣称增量编辑引擎。
- aegle-controls：共享无分配 Range、Toggle、Slider 和无皮肤 Button 的键盘/指针/语义激活、capture 和取消状态；可选 text 提供复用 Editor 的 TextField。宿主拥有树、命中和焦点；controls 默认只依赖 types，不依赖窗口或 renderer。
- aegle-access：平台回调经 Mailbox/Handlers 排队并唤醒 UI；可选 UnixAdapter/WindowsAdapter 复用 AccessKit AT-SPI/UIA。示例从同一控件树按脏标记导出语义，系统 Focus/Click/SetTextSelection 回到同一焦点、按钮和 Editor。text-a11y 提供文字 run 与有校验的选择转换；不是完整跨平台无障碍。
- aegle-glyph：复用 Swash/Skrifa，按需生成灰度字形与 COLRv0/嵌入位图，LRU 同时约束图像字节和条目数；缓存不持有字体文件。PNG 位图使用有解码预算的 png crate；库不内嵌字体。
- aegle-render-software：借用 RGBA8 缓冲，tiny-skia 负责抗锯齿覆盖率，线性光 SourceOver 合成器处理透明颜色。默认仅几何；可选 text 接同一 Scene 的字形、变换和裁剪。支持均匀缩放的四分之一像素定位及任意可逆仿射变换的双线性采样，无裁剪文字无需面大小的 mask。
- aegle-render-vulkan：独立 Vulkan 1.1 离屏绘制，复用 Scene；GPU 绘制矩形/圆角/居中边框、仿射变换及最多八层裁剪，可选 text 接有界按需灰度/彩色字形图集。RGBA16F 线性混合后由第二遍 GPU 编码预乘 sRGB RGBA8；显式读回、有界设备/记录分配和单次在途提交。已验证硬件与软件 ICD；可选 window 已提供原生 swapchain，App 可显式选择。

- aegle-platform-wayland：一个连接上的多个 xdg-shell 窗口与可选 wlr layer-shell 表面、整数缩放、事件等待、键盘/指针输入、光标、text-input-v3 与按 seat 的非阻塞剪贴板；软件绘制直接借用最多两块有界 SHM 映射。平台不依赖文字/scene/renderer，原生示例把这些模块接到同一控件树和 Editor。外观偏好经内置最小 D-Bus 客户端读取 desktop portal 并监听变化；尚无触摸、客户端装饰；gpu feature 提供原生句柄租约与共享帧门控。
- aegle-platform-win32：原生多窗口、消息等待、Unicode/指针输入、DPI、IMM 兼容组合、`CF_UNICODETEXT` 剪贴板、注册表/SPI 外观偏好与 `WM_SETTINGCHANGE` 更新、GDI 软件与 GPU HWND 租约；已交叉编译，执行证据见本页末尾，TSF/重转换/触屏键盘及真实 Windows 验收未完成。
- aegle-motion：独立无分配 Tween/Transition，标量、Point 与预乘线性 Color 插值、四种 easing；共用 types 的可选 std 颜色转换表。app 的可选 motion 已连接外观与平移过渡、完成回调、生命周期、语义颜色和 Wayland 帧驱动。缩放/旋转经 `Node::set_transform` 动画（见末节）；原生 App 跟随系统减少动态效果。
- aegle-theme：无分配的有类型配色/尺寸、VisualState、Appearance/Style 和纯函数 Skin；浅色、深色与显式高对比主题。app 支持整份主题快照的子树继承；原生 App 按系统深浅色/高对比选择主题；类型化 token（颜色、长度、时长、字体）的注册表、全局与子树覆盖及属性绑定见“主题 token”一节。
- aegle-ui、aegle-widgets、aegle-app 与 aegle：无窗口引擎（aegle-ui）、全部默认控件（aegle-widgets，经 `Widgets` trait 创建）、可选 Wayland/Win32 软件/Vulkan/wgpu 应用宿主（aegle-app）；命令式 row/column/scroll_view/text/button/text_field/text_area/check_box/switch/radio/slider/progress/list_view/table/popup/dropdown、弱句柄、布局 setter、可替换回调及主题切换。每窗口独立树，应用共享字体和 renderer；可选语义能力已接到原生循环。
- aegle-markup 与 aegle-macros：有界静态语法解析/校验和 `ui!` 编译，Window/Column/Row/ScrollView/Grid/Stack/Tabs/Tab/Splitter/Text/Button/TextField/TextArea/CheckBox/Switch/RadioButton/Slider/Progress/NumberField/Separator 直接创建同一套保留控件，具名弱句柄绑定 Rust 回调；默认 facade 包含编译宏。markup 另有 state/表达式/事件/块/组件的类型检查与多文件 `use` 导入；动态文档由宏生成已检查程序的构造代码。
- aegle-loader：动态标记执行引擎（state 单元与效果、单向绑定、事件块、按值 key 的 for、if 分支重建、响应式组件参数）、运行时 `Program::load` 与原子 `reload`；绑定随控件经 `Node::keep_alive` 释放。宿主动作、`let`、record、slot、组件事件与可配置限额见末节。

图像缓存预算不包括字体映射、排版、缓存索引及上游栅格 scratch；具体边界见 [资源](resources.md)。合成/过滤使用线性预乘颜色，公共字形彩色图像为非预乘 sRGB RGBA8。

## 可运行的组合

```sh
cargo run -p aegle --example hello --release
cargo run -p aegle --example controls --release
cargo run -p aegle --example hello_markup --release
cargo run -p aegle --example markup_controls --release
cargo run -p aegle --example components --release
cargo run -p aegle-render-vulkan --example geometry --release -- target/aegle-vulkan.ppm
cargo run -p aegle-render-vulkan --features text --example vulkan_text_scene --release -- target/aegle-vulkan-text.ppm
cargo run -p aegle --example widgets --release
cargo run -p aegle --example scrolling --release
cargo run -p aegle-layout --example retained --release
cargo run -p aegle-render-software --example software_scene --release
cargo run -p aegle-render-software --features text --example text_scene --release
cargo run -p aegle-render-software --features text --example editor_scene --release
cargo run -p aegle-platform-wayland --example editor --release
```

hello 是 7 行 Rust 加 1 行文档注释的完整应用；hello_markup 为 3 行 Rust、4 行标记加 1 行文档注释。controls 与 markup_controls 用相同界面演示跨控件回调、CJK 编辑、主题和关闭窗口，均使用系统字体。layout 与三个 renderer 示例没有窗口。形状示例将保留树的 Taffy 结果接到局部 Scene；首次建立 12 个记录，仅改按钮背景时重建 1 个记录，输出 `target/aegle-software.png`。

文字示例将 Paragraph 保留在同一棵树的节点中，以 Taffy 测量回调换行，并按最终布局宽度录制字形。真实显示拉丁文字、中文、日文、韩文及裁剪，输出 `target/aegle-text.png`；不是可交互控件或 GUI Hello world。测试字体共约 21 KiB，仅供测试/示例，附 OFL 原始声明和重建脚本。

编辑示例输出 `target/aegle-editor.png`：同一 Editor 先录制选区、预编辑和光标，再提交中文、录制结果，最后撤销/重做并检查已提交值。它以程序调用模拟输入，不代表已经连接真实输入法。

## 验证

- Linux：`cargo test --workspace` 在默认与 `--all-features` 下通过且无警告；`cargo clippy --workspace --all-targets` 在两种配置下无告警；`cargo doc --workspace --all-features` 无告警；facade 的九组 feature 组合（无 feature、`markup`、`markup,wayland`、`markup,wayland,software`、`markup,motion`、`motion,wayland,software`、`grid,markup`、`wayland,vulkan`、`wayland,wgpu`）`cargo check --all-targets` 无警告。
- Wayland：在隔离的 Sway/wlroots headless + Pixman compositor 上做了窗口生命周期、软件 SHM 绘制与截图检查，并用 fake input-method-v2 / virtual-keyboard 对 text-input-v3 做了 CJK 预编辑、提交、取消与跨窗口焦点的协议验证。需要专用 compositor 的测试标记 ignored；运行方式：

```sh
AEGLE_TEST_COMPOSITOR=private cargo test -p aegle-platform-wayland --tests -- --ignored --test-threads=1
```

  需自行设定隔离的 `XDG_RUNTIME_DIR` 与 `WAYLAND_DISPLAY`，compositor 须提供 input-method-v2、virtual-keyboard-v1；不得使用用户正在使用的输入法会话。
- Vulkan：离屏几何与文字场景在 RX 硬件上运行并与软件像素比较（允许 3 级通道量化误差），窗口渲染器与共享设备由 native 集成测试覆盖。
- 标记：编译型与运行时加载由 facade 的 `tests/paths.rs` 用同一文档对照绘制记录与过渡时序；解析器对超深表达式、字段链与 `list<` 嵌套返回错误而不溢出栈。
- Windows：`aegle-platform-win32` 已交叉检查，**未在 Windows 上运行**；窗口销毁、IME、UIA、硬件 Vulkan 均无执行证据。
- 许可证为 `MIT OR Apache-2.0`（仓库仅一位作者，已重新授权）。CI（`.github/workflows/ci.yml`）含 fmt、clippy、测试、feature 矩阵、MSRV 1.88、doc、Windows 检查与 cargo-deny；其中 MSRV、Windows 与 cargo-deny 作业尚未在本机或 CI 上实际运行。

## 已知取舍

- 公开错误为 `Box<dyn Error>` 包裹各模块的类型化错误，用 `downcast_ref` 区分，不携带节点身份；模块错误枚举标 `#[non_exhaustive]`，`aegle-gpu::Error` 除外（后端须逐项映射）。
- `Element`/`State` 字段公开，是控件库的创作面（`aegle-widgets` 依赖）；未收窄。
- `aegle-loader` 对 `aegle-app` 的依赖是可选 feature，app 不依赖 loader。
- GPU 多窗口只共享实例/设备/管线，图集按窗口独立。

## 剩余工作

- 平台验收：Windows 真实 IME/UIA/硬件 Vulkan 与 ARM64、TSF text store/重转换/触屏键盘；macOS AppKit/Metal；Wayland 客户端窗口装饰与真实触摸设备；Windows 触摸与惯性；真实桌面 portal 与 Windows 设置变更；fcitx/IBus 真人候选窗与真实屏幕阅读器验收。
- 组件/绘制：组透明度与区域模糊（需要离屏层）；阴影与渐变的标记写法。
- 文字/无障碍：Unix adapter 的上游 EditableText 等限制。
- 工程：多 compositor/GPU 与嵌入式完整资源测量；首次运行并清理 MSRV、Windows 与 cargo-deny 作业。现有桌面样本不能替代这些证据。
