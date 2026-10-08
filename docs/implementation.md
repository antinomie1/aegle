# 实现状态

完整目标保持不变：模块化保留模式 GUI、GPU 与无 GPU 软件绘制、CJK/原生 IME、无障碍、组件/主题/动画、命令式与标记入口、跨平台及 API 文档。

## 已实现

- Rust 2024 workspace，MIT OR Apache-2.0；目标 MSRV 1.88，Linux 验证工具链为 1.96.1，Windows 为 1.99.0；MSRV 1.88 已在 Windows 上 `cargo check --workspace --all-features`（不含仅限 Linux 的 Wayland crate）通过。
- aegle-types：no_std 几何与紧凑 RGBA 颜色，无第三方依赖。
- aegle-core：代数 ID、可复用槽位、保留树、索引子节点、结构变更与三通道失效，另有路由快照、默认动作控制及策略化焦点遍历；无第三方依赖。叶节点不分配子节点数组，删除不递归。
- aegle-layout：Taffy 0.14.0 直接适配同一棵保留树，无第二份拓扑；共享默认样式、测量缓存、Flex/Block、可选 Grid。它不依赖字体、窗口或 renderer。
- aegle-scene：no_std + alloc 的局部绘制记录；实色矩形、圆角、居中边框、共享 RGBA 图像、填充/描边路径、仿射变换、嵌套裁剪，以及供 renderer 使用的图层描述 `Layer` 与模糊盒计划。可选 text 保存定位字形、变体坐标与共享字体句柄，不依赖排版器或栅格器。
- aegle-text：复用 Parley/Fontique 的 Unicode shaping、字体选择、回退、换行与定位；Paragraph 保留文字和排版结果，宽度变化只重排，颜色覆盖无需重新 shaping。显式字体为默认，system-fonts、text-dictionary、text-a11y、scene 独立选用；缺字与无可用字体分别报告。
- Editor：复用同一 TextSystem 的 Parley PlainEditor，提供单/多行、选择/命中、视觉移动、grapheme 删除、精确 UTF-8 替换、只读、组合输入模型及有界差量撤销/重做。预编辑只保留被替换片段，提交值不随预编辑改变；取消恢复原选区。文字、装饰和候选区域来自同一布局。密码模式只排版等量 `•`，明文另存且不记录历史、拒绝 IME 组合。apply_ime 在完整验证后应用删除/提交/预编辑事务，删除与提交合成一次撤销；surrounding 无分配地借出有界周边文字。编辑、宽度和样式改变目前仍会重新 shaping，不宣称增量编辑引擎。
- aegle-controls：共享无分配 Range、Toggle、Slider 和无皮肤 Button 的键盘/指针/语义激活、capture 和取消状态；可选 text 提供复用 Editor 的 TextField。宿主拥有树、命中和焦点；controls 默认只依赖 types，不依赖窗口或 renderer。
- aegle-access：平台回调经 Mailbox/Handlers 排队并唤醒 UI；可选 UnixAdapter/WindowsAdapter 复用 AccessKit AT-SPI/UIA。示例从同一控件树按脏标记导出语义，系统 Focus/Click/SetTextSelection 回到同一焦点、按钮和 Editor。text-a11y 提供文字 run 与有校验的选择转换；不是完整跨平台无障碍。
- aegle-glyph：复用 Swash/Skrifa，按需生成灰度字形与 COLRv0/嵌入位图，LRU 同时约束图像字节和条目数；缓存不持有字体文件。PNG 位图使用有解码预算的 png crate；库不内嵌字体。
- aegle-render-software：借用 RGBA8 缓冲，tiny-skia 负责抗锯齿覆盖率，线性光 SourceOver 合成器处理透明颜色。默认仅几何；可选 text 接同一 Scene 的字形、变换和裁剪。支持均匀缩放的四分之一像素定位及任意可逆仿射变换的双线性采样，无裁剪文字无需面大小的 mask。
- aegle-render-vulkan：独立 Vulkan 1.1 离屏绘制，复用 Scene；GPU 绘制矩形/圆角/居中边框、仿射变换及最多八层裁剪，可选 text 接有界按需灰度/彩色字形图集。RGBA16F 线性混合后由第二遍 GPU 编码预乘 sRGB RGBA8；显式读回、有界设备/记录分配和单次在途提交。已验证硬件与软件 ICD；可选 window 已提供原生 swapchain，App 可显式选择。
- 图层：`Node::set_opacity`（可过渡）与 `set_backdrop_blur` 让子树经 `Visit::PushLayer`/`PopLayer` 在离屏层中绘制，软件、Vulkan 与 wgpu 的 `Frame::push_layer`/`pop_layer` 实现同一合成与三次盒式模糊（见[平台与绘制](platform-rendering.md#图层)）；GPU 后端需要 `text`。组效果损伤整个子树，背景模糊在损伤触及采样区时整体重绘。原生 App 与 gallery 宿主已转发图层。
- 阴影与渐变：`TransitionProperty::Shadow` 补间阴影的偏移、模糊、扩展与颜色（出现/消失按颜色淡入淡出），接入 `with_transition`/`snap`、减少动态效果与完成回调；`Shadow` 移到 `aegle-types` 并成为 token 类型（`bind_shadow`），渐变色标经 `ColorSlot::GradientStop(i)` 跟随颜色 token。渐变本身不补间（每帧会分配新的色标数组，见[组件契约](components-theme-animation.md#呈现变换惯性与图像特效)）。标记节点属性 `shadow`、`background_gradient`、`opacity`、`backdrop_blur`、`shadow_transition`、`opacity_transition` 在 `ui!` 与运行时引擎中都经 loader 的同一组 setter 应用，`aegle/tests/effects.rs` 以同一文档比较两条路径并检查主题切换后的 token 跟随；`aegle-ui/tests/effects.rs` 检查阴影淡入、中途改目标、结束与淡出移除。加入后 1000 控件场景不变：首帧 3.05–3.10 ms/12,253 次分配，空闲 22 µs/4 次，改值 22 µs/4 次，悬停 31 µs/7 次。

- aegle-platform-wayland：一个连接上的多个 xdg-shell 窗口与可选 wlr layer-shell 表面、整数缩放及 wp-fractional-scale（经 wp-viewporter 映射）、事件等待、键盘/指针输入、`wl_touch` 触摸（`Event::Touch`，App 交给 `Ui::touch`）、光标、text-input-v3 与按 seat 的非阻塞剪贴板；软件绘制直接借用最多两块有界 SHM 映射。平台不依赖文字/scene/renderer，原生示例把这些模块接到同一控件树和 Editor。外观偏好经内置最小 D-Bus 客户端读取 desktop portal 并监听变化；只请求服务端装饰，compositor 不提供时没有客户端装饰；gpu feature 提供原生句柄租约与共享帧门控。
- aegle-platform-win32：原生多窗口、消息等待、Unicode/指针输入、DPI、IMM 兼容组合、`CF_UNICODETEXT` 剪贴板、注册表/SPI 外观偏好与 `WM_SETTINGCHANGE` 更新、GDI 软件与 GPU HWND 租约；已在 Windows 11 上运行，执行证据见本页末尾，TSF/重转换/触屏键盘及硬件 GPU、ARM64 验收未完成。
- aegle-motion：独立 Tween/Transition 与关键帧 Animation（延迟、`Cycles`、往返；两帧时不分配），标量、Point 与预乘线性 Color 插值，二次、CSS 式三次 Bézier 与阻尼弹簧曲线（`Transition::spring` 取其稳定时间）；共用 types 的可选 std 颜色转换表。app 的可选 motion 已连接外观与平移过渡、完成回调、生命周期、语义颜色和 Wayland 帧驱动。缩放/旋转经 `Node::set_transform` 动画（见末节）；`Node::with_transition`/`snap` 为单次变化覆盖策略，`Node::animate` 播放位移/缩放/旋转关键帧，共用过渡的完成与取消（`aegle-ui/tests/transitions.rs`）；原生 App 跟随系统减少动态效果。
- aegle-theme：无分配的有类型配色/尺寸、控件类型 `ControlKind`（控件库声明的 static：默认皮肤、可接受样式组、是否为容器）、VisualState、Appearance/Style 和纯函数 Skin；皮肤按节点、子树内某类型（`set_kind_skin`，随 reparent 与新建控件解析）和类型默认三个作用范围取用，每节点只缓存解析结果；浅色、深色与显式高对比主题。app 支持整份主题快照的子树继承；原生 App 按系统深浅色/高对比选择主题；类型化 token（颜色、长度、时长、字体）的注册表、全局与子树覆盖及属性绑定见“主题 token”一节。
- aegle-ui、aegle-widgets、aegle-app 与 aegle：无窗口引擎（aegle-ui）、全部默认控件（aegle-widgets，经 `Widgets` trait 创建）、可选 Wayland/Win32 软件/Vulkan/wgpu 应用宿主（aegle-app）；命令式 row/column/scroll_view/text/button/text_field/text_area/check_box/switch/radio/slider/progress/list_view/table/popup/dropdown/menu/menu_bar、弱句柄、布局 setter、按事件累加的回调（出错保留处理器，原生 App 交给 `App::on_error`）、Ui 统一的连击计数（系统双击间隔来自 Windows 与 GNOME 设置）与 `on_double_click`、上下文菜单请求（右键按下、Menu 键、Shift+F10、ShowContextMenu 动作，经 `on_context_menu` 交给应用）及主题切换；菜单含命令项、勾选项、单选组项（相邻为一组）、快捷键提示文字（随字号重排，导出为无障碍快捷键；只是提示，不绑定按键）、子菜单、分隔线、点位放置与菜单栏，指针、键盘导航、单选互斥与提示由 `tests/menus.rs` 覆盖；`Decorator` 经 `Node::decorate` 在任意控件（含内置控件）的背景之下与内容之上追加图元，并在控件处理后以局部坐标观察其输入，动画只在活动期间请求帧（`aegle-widgets/tests/decorators.rs` 给内置 Button 加涟漪）；`handle!` 声明句柄的控件类型与样式组，生成类型化 `read`/`update` 与只适用于部分控件的样式与字体 setter，误用在编译期报错。每窗口独立树，应用共享字体和 renderer；可选语义能力已接到原生循环。
- aegle-markup 与 aegle-macros：有界语法解析、按元素规格（`ElementSpec`）的单一检查器 `check_program` 和 `ui!` 编译；检查器没有内置元素表，只认识文档根 `Window`。`element!` 声明元素规格与胶水并实现 `aegle_loader::Element`，19 个内置元素（Column/Row/ScrollView/Grid/Stack/Tabs/Tab/Splitter/Text/Button/TextField/TextArea/CheckBox/Switch/RadioButton/Slider/Progress/NumberField/Separator）与第三方元素用同一形式声明。`ui!` 在调用处按名称取得每个元素宏转交的规格，用与运行时相同的检查器在编译期报错；静态文档直接调用元素胶水与 loader 的节点 setter，动态文档生成已检查程序的构造代码并登记用到的元素。具名弱句柄绑定 Rust 回调；默认 facade 包含编译宏。
- aegle-loader：动态标记执行引擎（state 单元与效果、单向绑定、事件块、按值 key 的 for、if 分支重建、响应式组件参数）、运行时 `Program::load`/`load_with(path, &Elements)` 与原子 `reload`；绑定随控件经 `Node::keep_alive` 释放。宿主动作、`let`、record、slot、组件事件与可配置限额见末节。

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
cargo run -p aegle-widgets --features motion --example standalone --release
cargo run -p aegle-widgets --example custom_control --release
cargo run -p aegle-layout --example retained --release
cargo run -p aegle-render-software --example software_scene --release
cargo run -p aegle-render-software --features text --example text_scene --release
cargo run -p aegle-render-software --features text --example editor_scene --release
cargo run -p aegle-platform-wayland --example editor --release
```

hello 是 7 行 Rust 加 1 行文档注释的完整应用；hello_markup 为 3 行 Rust、4 行标记加 1 行文档注释。controls 与 markup_controls 用相同界面演示跨控件回调、CJK 编辑、主题和关闭窗口，均使用系统字体。layout 与三个 renderer 示例没有窗口。形状示例将保留树的 Taffy 结果接到局部 Scene；首次建立 12 个记录，仅改按钮背景时重建 1 个记录，输出 `target/aegle-software.png`。

文字示例将 Paragraph 保留在同一棵树的节点中，以 Taffy 测量回调换行，并按最终布局宽度录制字形。真实显示拉丁文字、中文、日文、韩文及裁剪，输出 `target/aegle-text.png`；不是可交互控件或 GUI Hello world。测试字体共约 21 KiB，仅供测试/示例，附 OFL 原始声明和重建脚本。

`aegle-widgets` 的 standalone 示例不经 facade 和原生窗口，只用 aegle-ui、aegle-widgets、aegle-theme、aegle-motion 与软件 renderer：模拟点击、按 16 ms 推进弹簧动画到结束（约 1 s），写出 `target/aegle-standalone.ppm`，已目视检查焦点环、禁用按钮与位移后的 CJK 标签。custom_control 示例只依赖 aegle-ui 实现评分控件，指针与方向键的变化依次报告为 `[5, 4, 3, 1]`。

编辑示例输出 `target/aegle-editor.png`：同一 Editor 先录制选区、预编辑和光标，再提交中文、录制结果，最后撤销/重做并检查已提交值。它以程序调用模拟输入，不代表已经连接真实输入法。

## 验证

- Linux：`cargo test --workspace` 在默认与 `--all-features` 下通过且无警告；`cargo clippy --workspace --all-targets` 在两种配置下无告警；`cargo doc --workspace --all-features` 无告警；facade 的九组 feature 组合（无 feature、`markup`、`markup,wayland`、`markup,wayland,software`、`markup,motion`、`motion,wayland,software`、`grid,markup`、`wayland,vulkan`、`wayland,wgpu`）`cargo check --all-targets` 无警告。
- Wayland：在隔离的 Sway/wlroots headless + Pixman compositor 上做了窗口生命周期、软件 SHM 绘制与截图检查，并用 fake input-method-v2 / virtual-keyboard 对 text-input-v3 做了 CJK 预编辑、提交、取消与跨窗口焦点的协议验证。需要专用 compositor 的测试标记 ignored；运行方式：

```sh
AEGLE_TEST_COMPOSITOR=private cargo test -p aegle-platform-wayland --tests -- --ignored --test-threads=1
```

  需自行设定隔离的 `XDG_RUNTIME_DIR` 与 `WAYLAND_DISPLAY`，compositor 须提供 input-method-v2、virtual-keyboard-v1；不得使用用户正在使用的输入法会话。
- Vulkan：离屏几何与文字场景在 RX 硬件上运行并与软件像素比较（允许 3 级通道量化误差），窗口渲染器与共享设备由 native 集成测试覆盖。
- 图层：`aegle-render-software/tests/layers.rs` 以像素检查组透明度、嵌套与背景模糊；Vulkan 与 wgpu 的 `tests/layers.rs` 在 RX 6800 XT（RADV）上与软件结果比较，平均通道差 0.128，Vulkan 另在 lavapipe 上为 0.104，并在 Khronos 验证层（含同步验证）下无报告；`aegle-ui/tests/layers.rs` 覆盖访问顺序、不透明度过渡与损伤。窗口路径由 `aegle-app/tests/native.rs` 的 `layers_draw_into_native_windows` 在私有 headless Sway 中以软件、Vulkan（直接写 swapchain，复制其图像做背景模糊）与 wgpu 运行，grim 截图与软件窗口的平均通道差 RX 6800 XT 上为 0.055/0.040、lavapipe 上为 0.071/0.034，差异只在圆角抗锯齿；Windows 上的图层未在窗口中运行过。三个后端的 `layer_cost` 示例实测了层与模糊的帧时间（数字见[平台与绘制](platform-rendering.md#图层)）；软件合成改用按 8 通道向量化的同一混合后，480×320 组透明度层从 6.2 ms 降到 5.2 ms，像素结果不变（`tests/layers.rs`）。
- 标记：编译型与运行时加载由 facade 的 `tests/paths.rs` 用同一文档对照绘制记录与过渡时序；`tests/elements.rs` 在测试 crate 中用 `element!` 声明第三方 Chip 元素，静态文档经 `ui!` 与 `Program::load_with` 构建出相同绘制记录（字面量与 token 属性），动态文档覆盖绑定、事件与 `self` 字段、有类型 id、`for`/`if` 与组件 slot，未登记时运行时加载报 unknown component；内置元素规格的拒绝用例在 `aegle-loader/tests/checking.rs`。解析器对超深表达式、字段链与 `list<` 嵌套返回错误而不溢出栈。
- 1000 控件场景（`cargo run -p aegle-widgets --release --example thousand`，无窗口、CJK 测试字体，计数全局分配器）：125 行，每行 Label、Button、CheckBox、Slider 与 4 个自定义类型 Meter，Meter 的外观由根上一次 `set_kind_skin` 继承。控件类型与继承皮肤改造前（f4d5bde，Meter 逐节点 `set_skin`）首帧 3.4–4.0 ms、12,260 次分配，空闲 refresh 约 23 µs/4 次，改一个值约 23 µs/4 次，悬停约 33 µs/7 次；改造与装饰器、元素契约之后首帧 3.0–4.3 ms（首次运行偶有 6–9 ms 冷启动）、12,253 次分配，空闲 22–24 µs/4 次，改值 22–24 µs/4 次，悬停 31–33 µs/7 次。稳态分配次数不变，绘制与输入热路径没有新增每帧分配；没有动画、过渡或活动装饰器时 `wants_frames` 为 false（`aegle-widgets/tests/decorators.rs` 检查涟漪结束后不再请求帧）。
- Windows（2026-10-08，Windows 11 Pro for Workstations 26100，rustc 1.99.0 `x86_64-pc-windows-gnu`，无 GPU 的 Microsoft Basic Display Adapter 虚拟机）：工作区（不含 Wayland crate）`cargo fmt --check`、默认与 `--all-features` 的 `cargo clippy --all-targets -D warnings`、`cargo test` 及 `cargo doc -D warnings` 均通过；facade 的 Windows 组合（含 vulkan、wgpu、windows-accessibility 与全部可选格式）clippy 无告警。下列 opt-in 测试在真实桌面会话中执行通过：
  - `aegle-platform-win32` `native`：窗口创建/隐藏到显示、GDI 呈现、UTF-16 surrogate 文字、IMM 上下文、已发布输入不被动画饿死、WM_SIZE 几何、逻辑关闭到最后租约释放时 DestroyWindow、`CF_UNICODETEXT` 往返与跨线程唤醒。
  - Microsoft Pinyin：`aegle-platform-win32` `ime` 与 `aegle-app` `ime_windows` 发送真实按键，覆盖预编辑、单次提交、取消、焦点转移与重新启用，详见[文字输入](text-input.md)。
  - `aegle-app` `native` 三个场景分别以软件、Vulkan（Mesa lavapipe 26.2.4 ICD，`AEGLE_TEST_VULKAN=1`）与 wgpu（lavapipe Vulkan 与 DX12 WARP，`AEGLE_TEST_WGPU=1`）呈现；`aegle` `window` 标记窗口文档通过。
  - 离屏 GPU：`aegle-render-vulkan` 全部 ignored 测试在 lavapipe 上通过；`aegle-render-wgpu` 全部 ignored 测试在 `WGPU_BACKEND=vulkan`（lavapipe）与 `WGPU_BACKEND=dx12`（Microsoft Basic Render Driver）上通过。
  - UIA：系统 UI Automation 客户端读取 `controls` 示例的完整控件树并执行 Invoke/TextPattern/SetFocus，见[无障碍](accessibility.md)。

  运行方式（在专用测试桌面；IME 测试会向前台测试窗口输入，需安装简体中文 Microsoft Pinyin）：

```sh
cargo test -p aegle-platform-win32 -- --ignored --test-threads=1
AEGLE_TEST_COMPOSITOR=private cargo test -p aegle-app --features windows,software,vulkan,wgpu --tests -- --ignored --test-threads=1
AEGLE_TEST_COMPOSITOR=private AEGLE_TEST_VULKAN=1 cargo test -p aegle-app --features windows,software,vulkan,wgpu --test native -- --ignored windows_callbacks
AEGLE_TEST_COMPOSITOR=private AEGLE_TEST_WGPU=1 cargo test -p aegle-app --features windows,software,vulkan,wgpu --test native -- --ignored windows_callbacks
cargo test -p aegle-render-vulkan --all-features -- --ignored --test-threads=1
WGPU_BACKEND=dx12 cargo test -p aegle-render-wgpu --all-features -- --ignored --test-threads=1
```

  这次运行发现并修复两处问题：wgpu `Options::default()` 的图集页尺寸被派生为 0，Win32 + wgpu 窗口首帧即以 `TooLarge` 失败（现恢复默认 1024 并在构造时校验）；facade 的系统无障碍 feature 未启用控件语义（见[无障碍](accessibility.md)）。硬件 GPU 驱动、ARM64 与真实屏幕阅读器仍无执行证据。
- 许可证为 `MIT OR Apache-2.0`（仓库仅一位作者，已重新授权）。CI（`.github/workflows/ci.yml`）含 fmt、clippy、测试、feature 矩阵、MSRV 1.88、doc、Windows 检查与 cargo-deny。本机复现 CI 步骤时有两处失败：stable clippy 新增的 `chunks_exact_to_as_chunks` 告警（`-D warnings`）及 cargo-deny 把无版本的 workspace 路径依赖判为通配；现以 `as_chunks` 替换常量分块、为路径依赖标注 `version = "0.1.0"`，本机 clippy 与 `cargo deny check`（advisories、bans、licenses、sources）通过。Windows 作业另运行 win32 crate 与 facade（含 `windows-accessibility`）的测试。Linux 专属的 Wayland crate 依赖 libxkbcommon，未能在本机交叉检查，其两处改动等待 CI 的 Linux 作业确认。

## 已知取舍

- 公开错误为 `Box<dyn Error>` 包裹各模块的类型化错误，用 `downcast_ref` 区分，不携带节点身份；模块错误枚举标 `#[non_exhaustive]`，`aegle-gpu::Error` 除外（后端须逐项映射）。
- `Element`/`State` 字段公开，是控件库的创作面（`aegle-widgets` 依赖）；未收窄。文字读写（`State::text`/`set_text`）与事件处理器注册（`State::on_action`）也在这一层，由类型化句柄包装，不在通用 `Node` 上。
- 句柄操作保留 `Result`（含 `is_alive`）：弱句柄可能失效、绘制与钩子期间 Ui 被借用，返回错误比 panic 或静默忽略更可预测；回调错误默认不结束原生 App，减轻 `?` 的代价。
- `aegle-loader` 对 `aegle-app` 的依赖是可选 feature，app 不依赖 loader。
- GPU 多窗口只共享实例/设备/管线，图集按窗口独立。

## 剩余工作

- 平台验收：Windows 硬件 Vulkan/DX12 驱动与 ARM64、日文/韩文输入法与候选窗位置、讲述人/NVDA、TSF text store/重转换/触屏键盘；macOS AppKit/Metal；Wayland 客户端窗口装饰与真实触摸设备；Windows 触摸与惯性；真实桌面 portal 与 Windows 设置变更；fcitx/IBus 真人候选窗与真实屏幕阅读器验收。
- 桌面集成：拖放（Wayland `wl_data_device`、Win32 OLE）、文件选择对话框（portal FileChooser、Win32 `IFileDialog`）、托盘、通知与全局快捷键。
- 文字/无障碍：Unix adapter 的上游 EditableText 等限制。
- 工程：多 compositor/GPU 与嵌入式完整资源测量；在 CI 上确认 Linux MSRV 与 Wayland 作业。现有桌面样本不能替代这些证据。
