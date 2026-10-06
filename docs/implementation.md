# 实现状态

完整目标保持不变：模块化保留模式 GUI、GPU 与无 GPU 软件绘制、CJK/原生 IME、无障碍、组件/主题/动画、命令式与标记入口、跨平台及 API 文档。

## 已实现

- Rust 2024 workspace，LGPL-3.0-only；目标 MSRV 1.88，实际验证工具链为 1.96.1。
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
cargo run -p aegle-render-vulkan --features text --example text_scene --release -- target/aegle-vulkan-text.ppm
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

## 原生 Wayland 集成

`aegle-platform-wayland --example editor` 是可交互的软件绘制窗口：一棵 Tree/Taffy 树保存文字标签、TextField、Button 和局部 scene，Route/Focus 管理路由及遍历，按钮回调修改文本框。CJK、键盘编辑、拖选、滚动、撤销及 IME 使用共享行为与同一 Editor，绘制直接写 SHM。示例使用有限覆盖的 OFL 测试字体，无系统字体发现成本；它不是未来的10行 facade API，可选启用 Unix 系统无障碍，未接主题组件。

在隔离的 Sway 1.12/wlroots 0.20 headless + Pixman compositor 上验证，不连接用户桌面。原生窗口截图已目视检查，CJK 字形、选择字段与裁剪正常；两次启动的首帧均成功。真实协议探针通过 fake input-method-v2 和持有的 virtual-keyboard 对 text-input-v3 发送 CJK 预编辑、隐藏光标、删除/提交同批次、空预编辑重置、stale/current serial、跨窗口焦点、立即取消与旧会话延迟结果，暴露并修复了焦点重启和取消会话串写问题。这是实际客户端/服务端协议验证，尚未进行 fcitx/IBus 的完整真人候选窗验收。

native lifecycle 集成场景验证 configure 前不能绘制、失败帧不提交、frame callback 门控、缓冲复用、映射预算与零预算拒绝、空闲等待、销毁与失效 ID。需要专用 compositor 的测试标记 ignored，常规测试不会自行连接桌面。跨 compositor、嵌入式 PSS/CPU、真实硬件/GPU 和其他操作系统尚未验收。

运行专用 compositor 测试时需自行设定隔离的 `XDG_RUNTIME_DIR` 与 `WAYLAND_DISPLAY`，再执行：

```sh
AEGLE_TEST_COMPOSITOR=private cargo test -p aegle-platform-wayland --tests -- --ignored --test-threads=1
```

IME 场景还需要 compositor 提供 input-method-v2、virtual-keyboard-v1；不能把用户正在使用的输入法会话当作测试环境。软件生命周期测试需要可见表面的 frame callbacks。

## 共享控件与 IME 事务验证

新增的4个集成场景覆盖路由/焦点生命周期、按钮跨输入源的 capture/取消、TextField 编辑/失焦/禁用/只读，以及原子 IME 的验证、选区、历史和周边文字。workspace all-features 共18个普通集成场景通过；两个需要隔离 compositor 的原生场景也显式运行通过，新增验证包含有/无 surrounding 的会话切换与已排队旧更新取消。没有 src 内测试。

私有 Sway/Pixman 上的组合示例验证了 Tab/Shift+Tab、Space 激活、修改另一控件的应用回调、键盘输入、按钮拖出释放取消及重新点击激活；18帧的有输入探针观察到预期两次激活。截图已目视检查。另一个外部临时探针检查12996组 IME 编辑/取消/撤销/重做组合和1486个周边文字摘取；不把探针保留为膨胀的测试套件，也不代替真实输入法验收。

- all-features/all-targets 检查、严格 Rustdoc、格式和 diff 检查通过。
- release 组合示例3秒无输入运行呈现2帧，字形图像11629 B / 121项；这是短时无持续重绘检查，未测量完整空闲 CPU/PSS。

## Unix 无障碍与文字选择验证

通过 `--features example-accessibility` 在原生 editor 示例中启用 AT-SPI。`aegle-access` 不保存第二份控件树；初次/重新激活全量导出，其后提交语义脏控件，布局与滚动使用绘制的同一逻辑几何。平台线程只排队并使用懒创建的 Wayland wake handle 唤醒 UI；按钮纯 hover/pressed 不再连带语义失效。

私有 Sway/Pixman 与 `dbus-run-session` 上的真实 AT-SPI 方法调用验证：7个系统可查询元素（应用根加6个逻辑节点）的角色/名称/父子关系，CJK 文本及 Unicode scalar 数量，文本框和按钮焦点，选择一个中文字、设置 caret、按钮激活清空文字，以及停用后重新启用时的完整树和焦点恢复。Registry/Status 为最小测试服务，不是真实屏幕阅读器验收；未连接用户桌面。探针还发现并修正了示例 Label 应使用 value 导出名称的映射问题。

- workspace all-features 共20个常规集成场景通过；两个原生 Wayland 场景显式运行通过，含新增后台线程唤醒。all-targets/all-features、默认 access 构建、严格 Rustdoc、fmt/diff 检查通过。
- 新文本场景覆盖 CJK/组合字符、run 端点、只读、无效/过期选择、预编辑与空文本。临时 RTL/bidi 探针验证12种混排/换行场景中的799个位置，不将其扩张成仓库测试套件。
- 同一800×480私有输出、有限测试字体、release配置下，不启用/启用 Unix 无障碍的发布文件分别为4520968 / 8410296 B。两者 `ldd` 仍为 libxkbcommon、libgcc_s、libm、libc 与 loader；AT-SPI 增加的 Rust 协议代码静态进入文件，并另需运行中的 D-Bus/AT-SPI 服务。
- 在初帧后约200 ms、无输入时的一次 `/proc` 快照：不启用时 RSS/PSS 为9424/6325 KiB、1线程；已激活并读取语义树时为12520/9430 KiB、4线程。计量只含示例进程，未计 compositor、测试服务或真实辅助技术，也不是30秒稳态、峰值或嵌入式验收。未启用样本3秒共呈现2帧。

仍缺 Unix EditableText（上游0.22.1未实现）、密码保护、预编辑期间辅助选择的完整协调、总线故障恢复/状态报告及真实屏幕阅读器验收。Wayland 不伪造全局屏幕原点。线程、无界队列和语义映射的实际寿命见[资源](resources.md)。

## 命令式应用层验证

Ui 的逻辑树同时用于布局、命中、焦点、编辑与语义；保留局部 scene，嵌套容器累计同一几何。控件句柄只持弱引用，删除、重挂父节点、隐藏/禁用及窗口销毁会协调焦点和捕获。回调暂时移出存储，在树与原生 runtime 借用外执行，注册版本阻止旧排队动作调用替换后的处理器；回调再次排队留待下一轮，并阻止空闲循环在仍有动作时睡眠。单行提交共用这套机制。

基础主题直接用于默认控件。只改配色不重新 shaping，修改字号/尺寸更新布局；显式尺寸、最小尺寸、padding/gap 不被主题覆盖。主题变化保留同一 Editor、已提交值和预编辑。主题文本颜色也同步到语义 run。独立 Ui 的语义导出不会消耗尚未呈现的像素失效。

验证结果：

- workspace all-features 共 21 个普通集成场景通过；新增一个应用场景覆盖弱句柄、回调替换/延后/删除、父节点移动、主题/IME 状态、光标滚动、提交、禁用和语义父子关系。所有测试位于 crates/*/tests。
- 新增的一个 ignored 原生场景在私有 Sway/Pixman 中显式通过：两个窗口共享事件循环，回调关闭自身、立即失效句柄、跨窗口后续动作、禁止嵌套 dispatch 和最后窗口退出。
- 实际窗口验证 CJK 显示、Tab/Shift+Tab、清空按钮回调、键盘编辑、深色主题及关闭按钮；浅色/深色截图已目视检查。虚拟键盘探针需要等待 compositor 焦点建立后发送首键，否则按协议该键只出现在 enter 的已按下列表。
- 私有 D-Bus/AT-SPI 探针验证新 App 的系统控件树、CJK/Unicode scalar、焦点、选择/caret、按钮清空和重新激活。它发现 AccessKit 0.24.1 的 clear_children 后追加不生效问题，文字桥改用显式空 children 列表，避免生成没有父节点的 run；集成场景已覆盖。
- 独立临时原生 App 探针通过真实 input-method-v2/text-input-v3 验证：CJK 预编辑保留提交值、组合中切换深色主题、删除/提交同批次、焦点切换后旧结果隔离及新字段继续提交。它发现并修复非焦点文本框 setter 错误重启当前输入法会话的问题，覆盖加入既有应用场景；没有再增加一套仓库测试文件。
- 同一探针移除全部键盘按键后也通过：首次 text-input Entered 可建立 seat/focus，不要求先收到 wl_keyboard 事件。
- all-targets/all-features、无默认 feature 的 Ui/facade、严格 Rustdoc 均通过。MSRV、其他 OS、真实输入法候选窗和屏幕阅读器仍未验收，Clippy 未安装。

默认应用启用系统字体，因此 Linux 动态依赖新增 Fontconfig；本机还间接带入 FreeType、Expat、zlib、bzip2、libpng 和 Brotli。它们来自本机 Fontconfig 的发行版依赖，不能只凭 Rust Cargo 树声称运行依赖没有增加。显式字体集合并关闭 system-fonts 可避开这组依赖；不启用 Unix adapter 也不会启动其后台 worker。

发布 profile 改为 `strip = "symbols"`，保留优化/LTO 设置，减少发布文件的符号表占用；需要带符号排障时应使用覆盖配置的构建。当前 release 文件如下，均启用系统字体，不内嵌字体；不包括系统动态库和系统字体文件：

| 示例 | 无 Unix adapter | 默认启用 Unix adapter |
| --- | ---: | ---: |
| hello | 3,925,184 B | 7,270,472 B |
| controls | 3,962,048 B | 7,307,336 B |

私有800×480 Sway/Pixman上的单次 controls 进程快照：无 adapter 时 RSS/PSS 为28,892/11,536 KiB、1线程；启用并激活 AT-SPI 时为31,976/14,593 KiB、4线程。前者启动后约1秒采样，后者初次树查询后约200 ms采样，未计 compositor/测试服务。无 adapter 样本随后3秒 CPU tick 增量为0（CLK_TCK=100），不等于30秒稳态、完整峰值或嵌入式验收。默认 hello 也已在隔离 compositor 启动并保持事件等待。

## 编译型静态标记验证

`ui!` 在编译期读取 manifest 相对路径，生成直接创建/设置现有控件的 Rust，返回带 `root` 与各 `id` 字段的有类型 View。片段可用于无窗口 Ui，Window 根用于 App。生成代码在创建失败时清理新子树或关闭新窗口，保留原有父节点；它不是运行时重载。静态语法和当前属性范围见[标记语言](markup.md)。

- workspace all-features 的 24 个常规集成场景通过；新增 3 个小场景分别覆盖 UTF-8/转义/解析预算、schema 类型与错误边界、真实 Ui 的类型化句柄/回调/布局覆盖和销毁。没有 src 内测试。原生 ignored 场景本次未重复运行，既有协议验证记录仍见上文。
- all-features/all-targets 检查、无默认 feature 的 app/facade 检查、仅 markup 的无窗口集成测试、严格 Rustdoc 与格式/diff 检查通过。
- 外部临时消费者验证 facade 重命名（含 Rust 关键字别名）、父表达式单次求值、大小写 ID、builder 与直接构造形式。只修改 `.aegle` 文件会触发消费者重编译并改变实际控件文字；未知属性产生文件/行列诊断，不发生宏 panic。
- 外部最小宿主注入构造器与 setter 失败，验证清理整个新子树、保留旧父节点/兄弟节点、Window 失败时关闭窗口，以及清理错误传播；未向正式库增加测试钩子。
- 私有 Sway/Pixman 上运行 release markup_controls，验证 CJK 显示、Tab/Shift+Tab、清空按钮、键盘编辑、深色主题和关闭回调，浅色/深色截图已目视检查。它复用既有输入、IME、绘制与语义实现；未重复真实 IME/AT-SPI 协议探针，不声称新增协议支持。
- 同一工具链/profile、Wayland + 系统字体 + markup、关闭 Unix adapter 的发布文件：hello 与 hello_markup 均为 3,925,184 B，controls 与 markup_controls 均为 3,962,048 B。此样本标记入口没有增加最终文件大小，不代表所有界面均有相同大小，也不是 RAM/CPU 测量。解析器、AST 与 syn/quote 等仅在编译主机运行。

## 局部外观与可复用皮肤验证

默认控件和自定义皮肤使用同一套行为、文本、IME 与语义。`set_skin` 接受无捕获的纯主题/状态函数；Style 是有类型的局部覆盖，按节点放在稀疏表中。已有控件可通过普通 Rust 工厂函数复用外观，不需要注册器或组件宏。局部字号在主题变化及文字替换后保留；字号改变重排同一编辑器，配色改变只覆盖绘制。边框和独立焦点环在控件范围内绘制；容器圆角不隐式裁剪子树。

`.aegle` 支持六/八位颜色字面量及背景、前景、状态背景、边框、圆角、焦点环、选择/caret 和字号属性。复核发现状态属性适用范围与实际行为不一致，已同步收紧 schema 和运行时：hover/focus 限按钮/编辑器，pressed 限按钮，selection/caret 限编辑器。不适用的设置明确拒绝，不接受后悄悄忽略。

- workspace all-features 共 25 个常规集成场景通过；新增一个场景覆盖局部样式、无效值不修改状态、主题/字号/预编辑共存、语义前景、按压/禁用优先级、回调文字替换与字体恢复。原有标记测试加入颜色与适用范围检查，生成代码在真实 Ui 测试中执行；没有逐 setter 测试套件。
- all-targets/all-features、无默认 feature 的应用场景与仅 markup 的 facade 场景、严格 Rustdoc、格式和 diff 检查通过；
- `components` 示例采用 `.aegle` 结构、一个可复用按钮工厂和主题皮肤；私有 Sway/Pixman 上验证 CJK 显示、Tab/Shift+Tab、清空、编辑、深色主题及关闭回调，浅色/深色截图已检查。它只是圆角填色组件示例，不是完整 MD3。当前未新增 OS 协议，也未重复完整原生 IME/AT-SPI 探针。
- 外部小型状态探针验证自定义皮肤悬停/离开时的前景与语义差量一致、离开确实触发重绘，以及后续状态返回无效半径时报错、清除皮肤后可恢复。未扩张正式测试套件。
- Wayland + 系统字体 + markup、无 Unix adapter 的本机 release 文件：hello 为 3,929,280 B（较前阶段增加 4096 B），controls 为 3,962,048 B（不变），components 为 3,982,528 B。没有新发布依赖。大小受链接/对齐影响，不能据此宣称零运行成本。
- 同一800×480私有输出上的 controls 单次快照：RSS/PSS 为28,796/11,526 KiB、1线程；随后3秒 CPU tick 增量为0（CLK_TCK=100）。这只是一份短时进程样本，不是峰值、稳定基准或嵌入式验收。当前目标上的 `size_of`：Appearance 36 B、Style 104 B、VisualState 6 B；不含稀疏表桶、字号/函数指针和 allocator 开销。

## 外观过渡与帧驱动验证

新增独立 `aegle-motion`，没有第三方依赖、定时器或堆分配；Tween 支持标量、Point 与预乘线性 Color，以及四种 easing。软件 renderer 和 motion 共用 types/color-math 的8200 B转换表数据，types 默认仍是 no_std；现有软件合成像素场景保持通过。

app 的可选 motion 维护稀疏过渡策略及活动表，Node 支持目标/呈现查询、中途改目标、取消、完成和清除策略。原生按钮/编辑器默认120ms EaseOut；无窗口 Ui 默认关闭，可手动推进单调时钟。`.aegle` 支持整数毫秒 transition 与 easing，静态设置完成后才安装策略。外观变化不重建 Editor，前景采样同步语义。此次复核修复了减少动态效果丢失最后重绘、失焦轮廓直接消失，以及取消失焦过渡后重新启动的问题。

- workspace all-features 共27个常规集成场景通过。新增两组综合场景覆盖补间端点/有限值/线性透明颜色、改目标连续性、实际焦点轮廓、取消/完成、时钟倒退、隐藏/删除、减少动态效果、CJK 预编辑保留与语义前景；已有标记场景扩展时长和首帧无动画检查。测试全部在 crates/*/tests。
- all-targets/all-features、app/facade 无默认 feature、独立 motion 与 types 默认 no_std 检查、严格 Rustdoc、fmt/diff 检查通过。最后的焦点取消修复后，相关 motion 场景在有/无 accessibility 两种组合重新通过。
- 外部小探针在私有 Sway/Pixman 上运行400ms过渡，实测406ms、27次 Wayland frame 请求；全程 app.dispatch(None)，没有后续输入或主动 wake。结束并排空已有平台事件后，600ms等待完整阻塞，新增 frame 请求和皮肤采样均为0。还验证了动画中删除控件、关闭窗口、晚建窗口共享时钟及减少动态效果；不把此桌面协议样本当作硬件帧时保证。
- Wayland + 系统字体 + markup + motion、关闭 Unix adapter 的本机 release：hello 3,949,760 B、controls 3,982,528 B、components 4,003,008 B，分别比前阶段增加20,480 B。没有新增第三方运行依赖，发布配置保持相同。
- 800×480私有输出的 release controls 单次快照：RSS/PSS 28,776/11,450 KiB，1线程，随后3秒 CPU tick 增量0（CLK_TCK=100）。波动范围内的单次样本不能解释为内存改善，也不是峰值、持续动画成本或嵌入式验收。

## 切换、滑块与进度组件验证

新增复选框、开关、水平滑块与确定进度条，Rust 与 `.aegle` 入口共用相同构造器、行为、主题、过渡和语义。Toggle 复用 Button 生命周期，Slider 与 Progress 共用有限 Range/步长/clamp；段落访问由字号、文字与主题更新共用。没有新增第三方依赖。程序 setter 不触发用户修改回调，互相同步不会形成反馈循环。

- workspace all-features 共29个常规场景通过；新增两组综合场景覆盖范围失败保持旧值、浮点步进/最大端点、多指针与捕获取消、键盘/语义同状态、禁用祖先、回调销毁、CJK 预编辑保持，以及局部 gap 下主题变化的布局失效和极小尺寸焦点绘制。旧标记场景扩展四种 typed handle、乱序范围属性、clamp/step。没有 src 内测试。
- all-features/all-targets、无默认 features 的 app/controls、仅 markup+motion 的 facade 场景、严格 Rustdoc、格式与 diff 检查通过。
- 私有 Sway/Pixman 上的 release widgets 示例验证 checkbox/switch 同步启禁 CJK 字段、键盘输入、滑块同步进度、浅深主题与关闭，截图已检查。根据视觉复核将默认未填充轨道改用主题 border，完成部分4dp、轨道2dp；进度不只靠色相区分。外部4536组小尺寸/空标签/边框/圆角场景检查 scene 构造与裁剪，未扩展正式测试套件。
- 私有 D-Bus 的实际 AT-SPI 查询/写入验证 CheckBox/Checked、Switch→ToggleButton/Pressed、Slider/Progress 的 Value。10..20/step3 的滑块初值10，写15得到16并经回调同步 Progress，写100得到20，写12得到13；祖先禁用后交互不能改变值。Slider 在 Unix 通过 Value/MinimumIncrement 调整，没有独立增减 Action。上游禁用 ProgressIndicator 的 Enabled/Sensitive 状态传播仍有缺口，详见[无障碍](accessibility.md#当前切换与数值控件)，未声称完整系统一致性。
- 相同 release 配置，Wayland + 系统字体 + markup + motion、关闭 Unix adapter：hello 3,962,048 B、controls 3,994,816 B、components 4,015,296 B，较上一阶段各增加12,288 B；新 widgets 为4,019,392 B。
- widgets 在800×480私有输出的单次快照：RSS/PSS 28,532/11,315 KiB、1线程，随后3秒 CPU tick 增量0（CLK_TCK=100）。它不是100控件基准，且与之前示例内容不同，不能当作内存下降证明；不包含 Unix worker，未进行嵌入式或跨平台性能验收。

## 保留滚动容器与跨节点裁剪验证

新增 ScrollView 和同名静态标记节点，复用 Taffy 的滚动范围、Element 既有偏移和软件 renderer 的裁剪 mask，没有新增依赖或 crate。纯滚动更新窗口几何及祖先 clip，复用局部 scene 与文字布局；横纵滚轮剩余量从编辑器/内层视口传到外层。控件命中、捕获释放、静止指针悬停、焦点揭示、IME 和语义使用同一几何。`visit_scenes` 的第三参数是必须由宿主应用的窗口逻辑裁剪。

- workspace all-features 共30个常规集成场景通过。仅新增一组滚动生命周期场景，覆盖嵌套余量、保留记录、裁掉的捕获按钮、显式 refocus、CJK 组合/候选范围、语义动作、隐藏恢复、删除后范围缩小、横向及有限值边界。现有 renderer 场景追加外部/内部 clip 相交、文字、预算、空/非法 clip 和调用间隔离；控件场景补悬停更新不改滑块值。
- all-targets/all-features、无默认功能的滚动场景、markup+motion 无窗口 facade 场景及严格 Rustdoc/格式/diff 检查通过。
- 外部 Taffy 探针确认 padding、嵌套滚动溢出隔离、隐藏恢复；AccessKit schema/consumer 探针验证嵌套语义边界等于 Ui 几何、四方向 Item/Page、SetScrollOffset、普通标签滚入、禁用/无效数据拒绝及 IME 组合保持。此阶段未重复真实 AT-SPI 滚动或真人输入法验收。
- 测试发现并修复两处边界：内部滚动的 TextArea/自裁剪控件使用 Taffy Hidden overflow，避免其文字扩大外层内容范围；完全离屏的候选锚点依次夹到控件、祖先和窗口，保持组合而不返回视口外坐标。可完整容纳的编辑器整体滚入，过大的轴以 caret 为准。
- 私有800×640 Sway/Pixman 中，release scrolling 通过真实 virtual-pointer 轴事件、Tab 连续滚入内外视口、顶部/末尾回调及关闭；滚动前后截图已目视检查。初次仅使用 swaymsg 的测试未产生 pointer capability，改用独立虚拟指针后轴输入生效，未连接用户桌面。
- 同一 release 配置、Wayland + 系统字体 + markup + motion、关闭 Unix adapter：hello 3,970,240 B、controls 4,007,104 B、components 4,023,488 B、widgets 4,027,584 B，较上一阶段分别增加8192/12288/8192/8192 B；新 scrolling 为4,019,392 B。
- scrolling 在800×480私有输出的一次进程快照：RSS/PSS 29,364/10,292 KiB，1线程，随后3秒 CPU tick 增量0（CLK_TCK=100）。示例内容、输出和系统映射均影响该样本；这不是对比改善、峰值、100控件、默认无障碍或嵌入式验收。

系统无障碍离屏过滤、ScrollHint/ScrollToPoint 和 HiDPI 验收限制见[无障碍](accessibility.md#当前滚动语义)，其他平台/GPU仍未实现。

## Vulkan 离屏几何验证

新增独立 `aegle-render-vulkan`，消费已有 Scene，尚未接入 App。矩形/圆角/居中描边、仿射与八层裁剪由 GPU 光栅化，CPU 仅生成有界记录；没有上传软件栅格化的整帧。采用 ash 0.38、bytemuck 1.25 和构建期 Naga 30，不引入 wgpu。两遍 GPU 绘制保留线性混合与预乘 sRGB 输出语义，接口与预算见 [Vulkan](vulkan.md)。

- workspace all-features 的30个常规集成场景、all-targets/all-features、独立无文字 Vulkan library 检查、严格 Rustdoc 和格式检查通过。新增一个默认 ignored 的综合 Vulkan 场景，覆盖混合/裁剪/仿射/作用域、预算、失败帧拒绝提交和恢复，以及 resize/释放/重建，没有 src 内测试。
- 同一场景分别在 AMD Radeon RX 6800 XT（RADV NAVI21）和 llvmpipe（LLVM 22.1.8）上显式通过。Khronos validation layer 1.4.363 仅解压至临时目录；两次均确认 loader 实际插入该层并启用同步验证，没有 VUID、Validation Error/Warning 或 SYNC-HAZARD。Lavapipe 属于软件 ICD，单列记录，不当作硬件加速证据。
- Lavapipe 上的临时像素探针验证0.2px细矩形、小数外部 clip、八层重复 clip、2730个避开抗锯齿边缘的旋转/反射/斜切采样，以及局部单位与设备缩放为 `1e±19` 的场景。探针发现并修复局部距离平方溢出与固定 AA 阈值失真：上传前将局部原点/长度等比归一化。记录预算320 B下的一条 clip + 一条 draw 也通过，空余容量可在另一个向量需要空间时回收；未增加另一套正式测试文件。
- 硬件上运行800×480 release geometry 示例并检查生成图片。发布可执行文件512,872 B，包含示例 PPM 写出及 SPIR-V；不含 Vulkan loader、驱动或其依赖，也不包含文本/窗口/App。normal 依赖没有字体栈或运行时 shader 编译器，bytemuck 的 derive 过程宏同样只在编译期运行。Vulkan loader 通过动态加载打开，不能仅凭 ldd 没列出它就声称无系统依赖。
- 临时 release 成本探针在 RX 6800 XT 上一次构造100个不透明圆角矩形，800×480；初始化15.115 ms、首帧1.330 ms，预热20帧后300帧共29.829 ms，平均0.099 ms、P95 0.102 ms。帧样本计量 begin/draw/finish/wait 的主机 wall time，包含提交及 fence 等待，没有逐帧读回；不是 GPU timestamp、窗口呈现或嵌入式帧时保证。
- 该硬件探针计时前后显式设备分配均5,529,664 B，CPU记录capacity均16,384 B；最后一次读回耗时3.692 ms（含首次 staging 分配/copy/wait/map），分配随后为7,065,664 B。相同探针的 Lavapipe 单次样本为平均1.363 ms、P95 1.686 ms，读回前/后分配4,608,064/6,144,064 B。差异来自驱动的实际分配要求；这些数字不包括调用方像素Vec、驱动内部对象及映射，不是进程 RAM/PSS，也不能证明达到资源目标。

## Vulkan 按需文字与共享光栅策略验证

新增可选 `aegle-render-vulkan/text`，与几何按相同记录顺序绘制，同用变换和八层裁剪。R8灰度与RGBA8_SRGB彩色页按需分配；每字形透明边、页LRU、当前帧固定、取消脏页回滚和独立上传上限保证不会用尚未上传或已被覆盖的条目。GPU缓存使用完整GlyphKey；软件和GPU共用RasterTransform，减少两个后端之间的字号/相位/仿射差异。没有新增第三方包；复用已有glyph/hashbrown依赖，默认几何构建无字体栈，text的normal依赖无Parley/Naga。

- 全workspace all-features仍为30个常规场景；all-targets/all-features、Vulkan无默认feature、glyph/software有/无feature场景、严格Rustdoc和格式检查通过。新增一个224行ignored文字综合场景，仅扩展既有glyph场景验证共享key/变换。
- RX 6800 XT与Lavapipe分别通过几何和文字综合场景，并开启Khronos层和同步验证，无Vulkan验证错误/警告。文字场景验证CJK/quarterphase、透明颜色、COLRv0/PNG字形、过滤/反射/旋转/clip、几何文字穿插顺序，以及CPU缓存仅一个条目时的GPU命中、取消帧、整页淘汰、预算/超大字形错误、resize和释放重建；与软件逐通道对照容差3。debug下未启用中日词典的既有ICU诊断仍会出现，不代表字形或Vulkan失败。
- 独立复核的临时小探针发现完全裁掉的字形仍因解析几何AA外扩而占用图集，单条目配置错误返回AtlasFull。修复后轴向clip使用真实像素覆盖边界，字形只保留自身过滤支持范围；同一探针及正式文字场景都验证一条目可绘制唯一可见字形。页数上限同时为目标/clip/upload/readback预留五个Vk内存分配；上传缓冲在fence完成即释放，避免只等下一帧。
- 800×480 release text_scene已在RX硬件运行并检查图片，包含CJK三语、裁剪、四相位和仿射文字；库不内嵌字体，示例使用有OFL许可的测试子集。未重复真人输入法、原生桌面或屏幕阅读器验收。
- 相同示例场景的临时release成本探针：scene/font准备0.220 ms、renderer初始化14.317 ms、首帧提交及wait为1.911 ms；预热20帧后300帧的begin/draw/finish/wait平均0.166 ms、P95 0.218 ms，无逐帧读回。首次末帧读回另为4.287 ms。它是单次桌面GPU主机wall-time样本，不是GPU timestamp、窗口延迟或嵌入式性能保证。
- 该探针计时前后显式设备分配均5,791,872 B（含1张262,144 B字形页），绘制/clip容量33,792 B；127个图集条目，CPU上传capacity40,960 B、128个region槽、CPU字形像素29,004 B。等待后staging为0，首次读回后设备分配7,327,872 B。空字形及被裁掉未入图集字形仍会查CPU缓存，raster_requests在300帧中由547增至6547；未发生图集上传，不将此计数误称为重新光栅化。所有计量均不含完整driver/font/shaping/allocator成本，不能当作PSS或资源目标已达标。

## Vulkan 窗口、App 与 Windows 接入

App 的同一 retained Ui 复用两种 renderer，文字、滚动裁剪、输入、IME、主题、动画与语义不另建状态。默认保留软件组合；native,vulkan 可单独构建，不带 tiny-skia。Windows adapter 在隐藏 HWND 上安装，DPI 通过语义根变换与 IMM 光标矩形共同同步。

- Linux workspace 全 features 的31个常规场景通过；all-targets 检查、严格 Rustdoc、格式/diff 检查通过。首轮同时链接多个大型 debug 示例耗尽可用内存，限制为2个构建任务后通过；这不是运行路径的内存样本。
- 新增一项默认 ignored 的 Vulkan 窗口生命周期场景，分别在 Lavapipe 与 RX 6800 XT（RADV NAVI21）上通过。私有 Sway/Pixman 或 GLES2 compositor，不连接用户桌面；开启 Khronos validation 1.4.363 与同步检查，无验证错误。覆盖呈现、resize、弃帧/失败帧拒绝、zero extent、release/recreate、SHM占用为0及原生租约销毁。
- App 既有双窗口 native 场景在上述软/硬 Vulkan ICD 通过：CJK、回调关闭、递归 dispatch 拒绝、旧句柄失效及最后窗口退出。Vulkan-only release scrolling 在 RX 硬件通过真实滚轮、Tab 焦点显露、滚动按钮与关闭，截图目视确认中文/日文/韩文和嵌套裁剪；没有验证层诊断。当前二进制4,028,032 B（native,vulkan,system-fonts,markup，不含系统库、驱动、adapter或motion），不是默认desktop体积。
- 原生800×480硬件探针显示显式设备分配3,686,464 B，swapchain估计6,144,000 B（驱动返回4图）；Lavapipe显式3,072,064 B、相同WSI估计。此为简单几何探针，无整帧readback；WSI实际驱动内存、字体、UIA及进程PSS不包括其中，不代表嵌入式预算达标。
- Windows x86_64-pc-windows-gnu：Rust 1.96.1 + 同版本 Debian rust-src、临时提取的 MinGW交叉构建，facade全features/all-targets检查与App/平台native测试可执行文件链接通过，Win32严格Rustdoc通过；源文件与工具链未安装进系统。Rust标准库通过 -Z build-std 验证当前工具链，未据此宣称MSRV1.88通过。真实Windows11/ARM64仍待验收。


- Wine10 + 私有 Xvfb 兼容环境执行 Win32 native 综合场景通过：隐藏创建、软件像素、Unicode代理对、IMM上下文启停、resize、持续redraw期间消息公平性、两窗关闭/租约寿命及wake。App同一个双窗口场景分别以软件、Vulkan（Lavapipe经Wine Win32 WSI）运行通过；UIA adapter安装/销毁已实际执行，但没有系统客户端文本/动作查询验收。这是兼容层及软件ICD证据。该Xvfb不支持硬件RADV所需DRI3，硬件Windows/Wine呈现未验证；库本身没有新增X11后端。

## 放弃 Metal，新增最小 wgpu 后端

- 放弃原生 Metal 方案，各设计文档与依赖表中的 `aegle-render-metal`、objc2-metal、MoltenVK 路线改为"Metal 仅经 wgpu 使用"；ash/Vulkan 仍是默认 GPU 路径。macOS 窗口、输入与无障碍仍未实现，状态不变。
- 新增 `aegle-render-wgpu`（wgpu 30.0.1，pollster）：几何、八层裁剪、mask/color 字形图集、离屏读回，以及 `window` feature 的 raw-window-handle surface。`aegle-app` 增加 `wgpu` feature、`RendererBackend::Wgpu` 与 `AppOptions::wgpu`。契约与取舍见 [wgpu](wgpu.md)。
- 验证：综合场景 `tests/render.rs`（默认 ignored）在 RX 6800 XT（RADV NAVI21）与 llvmpipe 上通过，对照软件渲染器，并覆盖图集中途清空和失败帧恢复；Windows、Metal、DX12 没有运行，也没有为 Windows 目标交叉编译。离屏示例在 RADV 上写出图片，目视确认 CJK 文字与圆角卡片。
- App 集成：私有 Sway（GLES2 compositor）上以 RADV 运行 `native,wgpu,system-fonts,markup` 的 release `scrolling`，经真实指针滚轮、Tab 焦点显露、滚动按钮与关闭回调通过，截图目视确认 CJK、嵌套裁剪与滚动条；未连接用户桌面，没有开启 Khronos validation。`wayland,software`、`wayland,vulkan`、`wayland,wgpu,vulkan,software` 与无窗口 `wgpu` 的 App 检查，以及 workspace all-targets 检查、严格 Rustdoc 与格式检查通过；本机没有 Clippy。
- 体积：native,wgpu,system-fonts,markup 的 release `controls` 为 8,578,728 B，同组合换成 vulkan 为 4,449,928 B（均 strip，Linux），wgpu 版多 4,128,800 B。没有测量帧时间、空闲 CPU 或 PSS，也没有与 Vulkan 后端比较性能。

## wgpu 后端补齐图像与路径

- `aegle-render-wgpu` 的 `text` feature 现在绘制 RGBA 图像与填充/描边路径，补完与 Vulkan 后端相同的 Scene 命令集。图集拆为 `atlas.rs`，图像与路径在 `vector.rs`；新增依赖只有已在 aegle-glyph/swash 闭包中的 zeno。着色器恢复图像分支（边缘钳制与解析覆盖率）。放不进页的图像和路径 mask 使用按尺寸创建的专用纹理，最后一次使用后的下一帧释放；字形超大、图像或 mask 超过设备纹理上限返回 `TooLarge`（取代 `GlyphTooLarge`）。
- 验证：新增 ignored 场景 `tests/vector.rs`，在 RX 6800 XT（RADV NAVI21）与 llvmpipe 上通过：缩放/平移/旋转的图像、偶奇填充星形与圆头二次/三次描边对照软件渲染器，平均通道差 0.29–0.31（界限 0.5）；未缩放图像的像素精确；整像素平移不增加条目，新的线性变换增加；超出页的 80×10 图像得到专用纹理，其后两帧不再绘制时释放。原有综合场景仍通过。
- 私有 Sway（GLES2 compositor）上以 RADV 运行 `native,wgpu,system-fonts,markup` 的 release `visuals` 示例，截图目视确认渐变图像、填充加描边的星形、按钮、一万行虚拟列表及滚动条均正确显示；没有操作 Rotate 按钮，也没有开启 validation。
- `cargo fmt --check`、workspace all-targets 检查与严格 Rustdoc 通过，本机无 Clippy。仍未做：Windows/macOS 实机、设备丢失恢复、与 Vulkan 后端共享记录构建，没有帧时间或内存测量。

## 抽出 aegle-gpu：两个 GPU 后端共用记录与着色器

- 新增 `aegle-gpu`（依赖 scene、types、bytemuck；`vector` feature 增加 zeno）：`Primitive`/`Clip` 存储行、`Recording`（字节上限）、逐命令的 `Walker`、货架装箱 `Shelf`、图像放置、路径 mask 键与光栅化，以及 `GEOMETRY_WGSL`/`RESOLVE_WGSL`。`aegle-render-vulkan` 删除 `geometry.rs`、`vector.rs` 的放置逻辑与 `shaders/`，build.rs 改为从 aegle-gpu 取 WGSL 编译 SPIR-V；`aegle-render-wgpu` 删除自己的 `records.rs`、路径光栅化和两份 WGSL。两者都不再依赖 zeno。
- 着色器改为由视口高度符号选择 Y 方向（Vulkan 为正，wgpu 为负），因此 wgpu 不再有 WGSL 副本。wgpu 的批次改为在提交时按图元的 kind/page 字段扫描生成，与 Vulkan 的做法一致。
- 验证：重构前后的 Vulkan 三个 ignored 场景（几何/失败帧恢复、CJK 与图集恢复、图像与路径）在 RADV 与 llvmpipe 上都通过；wgpu 两个 ignored 场景在两个适配器上通过；私有 Sway 上带 Khronos validation 与同步检查的 Vulkan `scrolling` 通过且无验证错误，wgpu 的 `scrolling` 与 `visuals` 通过，截图目视确认没有上下翻转。新增 `aegle-gpu/tests/records.rs`：遍历记录、裁剪边界、透明填充不入行、字节上限、九层裁剪拒绝、视口符号、货架换行。workspace all-targets 检查通过。
- 不改变公开 API：两个后端的 `Error` 增加从 `aegle_gpu::Error` 的转换，wgpu 增加 `Allocation`。没有测量帧时间或体积变化。

## 鼠标指针形状

- `aegle-types` 新增 `Cursor`（默认、文本、手形、十字、移动、抓取、抓取中、不可用、横向/纵向调整）。`aegle-app` 新增 `Ui::cursor()`、`Node::set_cursor/cursor`：捕获指针的控件优先，其次鼠标下最上层可见控件（含标签、图像、画布）的显式形状，可用的文本字段为 I-beam（只读也是，禁用不是），然后是最近祖先的显式形状，否则箭头；滚动条条带、滚动条拖动和被弹出层遮住的区域为箭头。
- 平台：`Wayland::set_cursor` 保存每个窗口的形状，立即应用到窗口内所有指针，并在每次 Enter 与跨输出缩放时重新应用，取代原先每次都重置为默认；`Win32::set_cursor` 与 `WM_SETCURSOR`（仅客户区）使用系统共享光标，Windows 没有抓手形状，抓取用手形、抓取中用四向箭头。`Runtime::refresh` 在每次刷新后同步，布局或可见性变化导致鼠标下控件改变时也会更新，不只在指针事件之后。
- 验证：`aegle-app/tests/pointer.rs`（无指针、标签/按钮、可用/只读/禁用字段、显式形状及其继承与优先级、按下后捕获、多行字段的滚动条条带、隐藏与删除鼠标下控件、指针离开）。私有 Sway 上以 `WAYLAND_DEBUG` 观察真实协议：`wp_cursor_shape_device_v1.set_shape` 在 Enter 后依次随标签→文本区→按钮→文本区→标签得到 default→text→default→text→default（枚举 1 与 9），使用虚拟指针，没有连接用户桌面。Win32 只通过 x86_64-pc-windows-gnu 交叉编译检查，没有在 Windows 或 Wine 上运行，因此 `WM_SETCURSOR` 与各 `IDC_*` 的实际显示未验证。没有验证不支持 cursor-shape 的合成器上的主题光标回退。

## 滚动条不再遮挡内容与边框

- 问题：`scrolling` 示例中嵌套区域的内边距（8 dp）小于滚动条占用的宽度，滚动条盖住控件右缘；滚动视图的边框与背景一起先画，滚到边缘的文本框白底会盖住它。
- 修改：溢出的视口在对应的尾边为滚动条留出 `FOOTPRINT`（轨道 8 dp、边距 2 dp、间隙 4 dp，共 14 dp）：`refresh` 在布局后检查每个视口的溢出，需要时把右/下内边距提高到该值再布局一次，溢出消失就还原；只有一次额外布局，因为变窄只会保留而不会消除溢出。子内容的裁剪区在溢出轴上同样缩进，所以横向滚动或纵向滚动中的内容也不会滑到滚动条下面。滚动视图的边框改画在覆盖记录里，即所有子内容之后、滚动条之前。
- 验证：`aegle-app/tests/scrolling.rs` 新增场景（内容右缘随溢出在 186 与 196 之间切换、边框是覆盖记录的第一条命令、横纵溢出时可见区被限制在滚动条之前），并更新了因底部留白从 8 dp 变为 14 dp 而改变的最大偏移（60 到 66）。`aegle-app --all-features` 与 workspace 测试通过。私有 Sway 里的软件渲染截图放大对比：改前文本框盖住嵌套区域的上、下、右边框，改后边框完整，滚动条与控件之间有间隙；重新生成的 `scroll-view.png` 与 `variable-list.png` 也随之更新，其余 15 张不变。没有在 GPU 后端重新截图。

## 直角默认控件与覆盖式滚动条

默认主题圆角改为 0，复选框、开关、滑块、进度条与按钮/编辑器统一使用主题圆角，因此默认全部直角；正的主题 radius 仍统一圆化。ScrollView 与多行编辑器新增覆盖式滚动条，不占布局、不新增依赖或计时器。视口的滑块保存在自身单独记录中，`visit_scenes` 按后序子树终点在子树之后输出，滚动只重录该小记录，不重录子控件或重排文字。拖动在窗口坐标中处理，经同一 `scroll_to`/几何更新路径同步裁剪、命中、IME 与语义。

- workspace all-features 测试通过；新增一个滚动条场景覆盖最上层记录、拖到两端、拖动不激活下方按钮，以及不溢出时指针归还给控件。Vulkan 与软件后端复用同一 Scene，未新增 renderer 路径；本阶段未做真人窗口拖动验收。

## 文字清晰度与 Vulkan 低功耗路径

- 字形：hinting 的轴向文字基线取整到设备像素，水平保留四分之一相位；横笔不再因小数基线跨两行发虚，每字形相位变体由 16 个降为 4 个。灰度覆盖率经共用 `aegle_glyph::mask_contrast` 曲线 `c + c(1-c)k`，k 由前景亮度给出：深色字加重、浅色字减轻，软件整数实现与 Vulkan shader 同式。浅/深底 12–20px 中英文样张在两个 renderer 上目视检查；GPU/软件 ±3 级对比测试通过。整像素基线下空格等无像素字形不驻留图集，每帧回到 CPU 缓存命中，测试已按此更新。
- Vulkan 批处理：图元记录改为 storage buffer，相邻且 pipeline/图集页相同的图元合并为一次 instanced draw，移除逐图元 push constant 与 scissor。release 离屏 800×480 探针（2000 矩形 + 30 行中英文，约 3800 图元，预热 40 帧后 300 帧，主机 wall time 含 fence 等待）：RX 6800 XT 平均 1.412 → 0.448 ms，Lavapipe 23.894 → 4.198 ms；两种驱动的读回像素与改动前逐字节相同。图元缓冲按每图元 112 B 计入设备预算。
- 直接 sRGB 窗口：不透明窗口默认直接在 BGRA8/RGBA8_SRGB swapchain 上绘制，没有 RGBA16F 目标与编码 pass；`Options::transparent` 保留原透明路径。私有 headless Sway（RADV 用 GLES2，Lavapipe 用 Pixman）同一场景 300 帧：设备分配 RADV 4,350,736 → 664,336 B、Lavapipe 3,582,736 → 664,336 B；每帧工作时间 RADV 0.579 → 0.555 ms、Lavapipe 4.441 → 3.918 ms；Lavapipe 进程 CPU 每帧 30.5 → 26.2 ms（含其光栅线程），RADV 均为 0.433 ms。两条路径截图最大通道差 1 级，占 2.1% 通道。
- 未指定设备时改为优先集成 GPU；本机没有集成 GPU，未实测该选择。Vulkan 窗口生命周期场景在 RADV 与 Lavapipe 上通过；Vulkan 版 scrolling 示例目视确认直角控件、覆盖式滚动条与 CJK 文字。当时未安装 Khronos validation layer，没有验证层诊断。以上为桌面样本，不是嵌入式功耗或帧时保证。

## 剪贴板、密码编辑与 layer-shell

- 剪贴板：TextField 的 Ctrl+C/X/V 只产生请求，App 在刷新前交给平台，不新增依赖或线程。Wayland 每 seat 一个 data device，选区数据以一份 `Rc<[u8]>` 保留到被其他客户端取代，写出/读取都是 4 KiB 一次的 calloop 非阻塞管道，读取上限 4 MiB；Win32 同步读写 `CF_UNICODETEXT`。
- 密码：`set_password`/标记 `password` 让 PlainEditor 只排版 `•`，明文仅存一份；无撤销历史、拒绝 IME 组合与复制/剪切，语义导出 `PasswordInput` 与遮盖值。测试字体按重建脚本补入 U+2022。
- layer-shell：`WindowOptions::layer` 在同一窗口表中创建 wlr layer 表面，复用输入、IME、SHM/Vulkan 呈现和帧门控；compositor 缺少协议时创建报错。不另设 shell crate。
- 验证：workspace 全 features 测试通过；ui 场景覆盖剪切/粘贴（换行剥离、独立撤销）、密码输入拒绝复制与 IME 且语义树不含明文。私有 Pixman Sway 上 Wayland native 场景验证 TOP|LEFT|RIGHT 锚定面板拉伸到输出宽度、高度 96，ime 场景以真实键盘 serial 设置 180,000 B CJK 选区并经非阻塞管道读回一致。wtype 驱动的应用级探针在软件与 Vulkan（Lavapipe）下完成跨编辑器复制/粘贴、密码框拒绝复制后粘贴仍为原值，截图确认 layer 面板与遮盖显示。Wine10 + 私有 Xvfb 执行 Win32 native 场景的 UTF-16 代理对剪贴板往返；Wine 下所有者窗口的剪贴板消息会先于已置位的 wake 被派发，wake 在下一次 dispatch 送达，测试按此接受。真实 Windows 与 GNOME 等无 layer-shell 的 compositor 未验收。
- 既有 Wayland native 场景在平铺 Sway 下会因窗口被放大超过 2 MiB SHM 预算失败（未改动的 HEAD 同样复现）；私有 Sway 以浮动规则运行，未改测试预算。

## 图像与矢量路径

- scene：`Image`（非预乘 sRGB RGBA8，单边 ≤16384）与 `Path`/`PathBuilder`（move/line/quad/cubic/close、NonZero/EvenOdd）为带唯一 id 的 `Arc` 共享句柄；`image`/`fill_path`/`stroke_path` 在 builder 边界检查尺寸、顺序、有限值与坐标范围，空矩形、无线段路径和零宽描边不录入命令。
- 软件：路径与描边由 tiny-skia 生成覆盖率，复用既有 mask/裁剪合成；图像在线性预乘空间双线性采样，钳制边缘纹素并按解析覆盖率处理矩形边缘，单位缩放且整像素对齐时直接复制纹素。
- Vulkan（需 text）：图像为 RGBA8_SRGB 图集条目，路径由 zeno（swash 已依赖，不新增 crate）生成 R8 mask，二者复用字形 pipeline 与 shader；路径键含线性矩阵、四分之一像素相位与描边样式。超页尺寸条目使用同一预算下的专用页。没有 Lyon/stencil，没有 mipmap。
- 验证：软件测试确认矩形路径与同尺寸矩形填充逐字节相同、1:1 图像纹素精确、放大 4 倍的边缘纹素不渐隐、描边中心与相邻行。Vulkan `tests/vector.rs` 与软件输出对照，平均通道差 Lavapipe 0.288/0.288/0.310、RADV（RX 6800 XT）0.291/0.291/0.312，大图专用页场景两者均为 0.000；整像素平移后图集条目仍为 3，旋转后为 5。最大单像素差来自两种曲线/描边近似，因此测试使用平均差而不逐像素比较。既有 render/text 套件在两种 ICD 上仍通过，native 呈现在私有 Sway 上通过；Windows 交叉检查通过。尚无路径光栅性能测量。

## 图像控件、Canvas 与等高虚拟列表

- App 新增 `ImageView`（共享 `Image`，固有尺寸为像素尺寸）、`Canvas`（painter 闭包录制局部 scene，仅在创建、尺寸变化或 `invalidate` 后重录）和 `ListView`；`aegle::scene` 重新导出绘制类型。核心 `Tree::insert_at` 让行按索引插入，Tab 顺序与行序一致；没有新增依赖。
- ListView 复用 ScrollView 的滚动、滚动条、裁剪与语义；`Ui::refresh` 在借用外建立/删除与可见区域相交的行，若首轮布局暴露新行则再刷新一次。行回调可使用任何句柄，删除列表、嵌套列表和回调错误均有处理。
- 验证：`tests/visual.rs` 覆盖首屏只建 5 行、向下滚动新增 5/6 行、回滚在原有行前重建第 0 行且 Tab→Enter 激活第 1 行、`set_count`/`reload`/删除、非法行高与范围；图像固有尺寸与替换、Canvas 只在 invalidate 后重录。release 临时测量（本机，10,000 行 28 px 文字行，800×480）：ListView 首次刷新 0.24 ms、每滚一行加刷新 14 µs、RSS 增长约 1 MB；同内容普通 ScrollView 构建加首次刷新 49 ms、每步 98 µs、RSS 增长约 40 MB。
- `visuals` 示例在私有 Pixman Sway（软件）与 GLES Sway（RADV Vulkan）下截图一致，wtype 键盘激活 Rotate 后 Canvas 重绘；两种后端静止 3 秒 CPU 时钟增量为 0，PSS 分别约 9.6 MB 与 21.8 MB（后者含驱动映射）。无头 Sway 没有指针设备，原生滚轮滚动列表未实测，滚动路径由集成测试覆盖。

## 局部主题继承、平移动画与完成回调

- `Node::set_theme(Option<Theme>)` 给子树一份主题快照，嵌套优先、`None` 恢复父级；`Ui::set_theme` 只作用于无局部主题的节点。每个元素缓存共享 `Rc<Theme>`，创建、reparent 与主题变化时向下传播；绘制、布局、命中、滚动条、IME、语义与创建默认值都改读节点解析主题。窗口清屏色取根节点主题。
- `Node::set_offset(Point)` 在布局后平移子树：`update_geometry` 把呈现位移加入 bounds，命中、裁剪、IME 锚点、语义 transform 和场景平移随之一致，不改变布局或滚动范围。motion 下有过渡策略时补间，请求在下一次刷新时以宿主当前时钟开始；采样只置几何失效，不重录绘制。
- `on_transition_end` 与点击回调共用版本化队列（版本跨类型唯一）；外观与位移都结束、`finish_transition`、或有策略时因减少动态效果/隐藏/零时长直接到目标会完成；取消、清除策略、删除与关闭不完成。没有新增依赖。
- 验证：`tests/appearance.rs` 覆盖局部主题对控件高度/配色、后建控件、嵌套、全局主题不越过局部主题、reparent 继承、清除恢复和非法主题；`tests/motion.rs` 覆盖无策略立即平移、补间中途 bounds 与悬停命中、完成一次、取消冻结、finish 与减少动态效果完成、删除不完成。all-features 与无 feature 的 app 测试、私有 Pixman Sway 上的 native 测试通过。
- release 临时探针（1000 个按钮，800×480，CPU 频率呈双峰）：首次刷新 0.65–2.1 ms、全局主题切换加刷新 0.14–0.27 ms，与改动前 0.71–2.05 ms、0.16–0.30 ms 在同一噪声范围；局部主题切换 0.14–0.27 ms；整组平移一步 6–14 µs，与无变化滚动同量级。
- `components` 示例（debug）在私有 Pixman Sway 上截图验证：Dark card 只把卡片子树切为深色，窗口其余部分保持浅色；Close 截到卡片 EaseIn 滑动中途，完成回调随后关闭窗口、进程退出。过渡结束后 2×3 秒 CPU tick 增量为 0。

## 系统外观偏好

- 两个平台 crate 新增相同形状的 `Preferences { dark, high_contrast, reduced_motion }` 与 `Event::Preferences`。Linux 不引入 D-Bus 库：`portal.rs` 以 EXTERNAL 认证连接会话总线，发送 Hello、SettingChanged 匹配规则和三个 `ReadOne`，连接时最多等待 100 ms，其后回复与信号由 calloop socket 源处理；只编码/解码所需的消息形状，单条超过 64 KiB 断开。Win32 读取注册表与 SPI，并在 `WM_SETTINGCHANGE` 时重读、去重后发出事件。
- `AppOptions` 新增 `dark_theme`/`high_contrast_theme`（默认 `Theme::dark()`/`Theme::high_contrast()`，None 表示忽略该偏好），`reduced_motion` 改为 `Option<bool>`（None 跟随系统）。变化只更新仍等于原解析值的窗口；`App::preferences` 供自定义配色读取。
- 验证（私有 dbus-daemon，严格校验消息，配合临时目录中按原始协议实现的假 portal；未接触用户会话总线）：连接（含 Wayland 连接）1.3 ms 内取得 dark/high-contrast，`NotFound` 的 reduced-motion 保持 None，SettingChanged 产生事件；无 portal 服务 1.1 ms、无总线 0.7 ms 后偏好均为 None；portal 每次回复延迟 150 ms 时连接在 102 ms 返回，迟到回复依次以事件到达。`controls` 示例在私有 Pixman Sway 上以深色启动，信号后无重建地切到浅色（截图已检查），静止 3 秒 CPU tick 增量 0。Win32 路径在 Wine10 + 私有 Xvfb 的独立测试前缀中由临时探针执行：初始读取得到 dark None（键不存在）、high_contrast/reduced_motion Some(false)；写入 `AppsUseLightTheme` 并广播 `WM_SETTINGCHANGE` 后依次得到深色、浅色、未知事件，重复相同广播不重复发出，探针最后删除该值。Wine 未对 `SPI_SETCLIENTAREAANIMATION` 产生变化，真实 Windows 设置变更仍待验收。
- Wayland + 系统字体 + markup + motion、无 Unix adapter 的 release `controls` 由 4,220,096 B 增至 4,248,768 B（+28,672 B）。没有新增第三方依赖；win32 只多启用 windows crate 的 Registry 与 Accessibility 绑定。没有新增自动化测试：协议、主题解析与窗口更新只在上述手工环境中验证。

## 动态标记、组件导入与运行时加载

- aegle-markup 扩展词法（整数、运算符）、解析（`use`、`component`、`state`、`on`、`if/else if/else`、`for ... in`、表达式与类型）和检查：名称解析为 state/参数/循环项引用，类型检查与整数字面量到 float 的唯一隐式转换，绑定属性白名单，事件按控件种类，id 仅入口非块区域，组件递归、导入环、重名与被导入文件含根节点均报错。`compile` 经调用方读取函数加载导入并渲染带文件/行/列的诊断，自身不做 I/O。静态文档保持原 `check` 与直接构造路径。
- 新增 aegle-loader（仅依赖 app 与 markup）：state 单元记录读取它的效果，按订阅顺序同步重跑；效果去重订阅、增长前清理已释放的订阅。if 在条件变化时重建分支；for 以项值为 key 保留行身份和本地状态，重复 key 报错且保留旧行；组件参数在调用方环境求值，跟随其 state。块内容位于跟随父方向与字面量 gap 的内部行/列。`View` 提供命名句柄、动态或 `State<T>` 读写与原子 `reload`（同名同类型 state 保留）。app 新增 `Node::keep_alive`，绑定随控件删除或窗口关闭释放。
- `ui!` 对动态文档在构建期完成全部检查，生成构造已检查程序的代码与 View 的 id/state 字段，并为每个导入生成依赖标记。release 符号统计：仅用 `ui!` 的动态示例不含 aegle-markup 的解析/词法符号（剩余约 10 KB 为程序类型），引擎约 50 KB；`dynamic` 示例因运行时加载额外带解析器（约 100 KB 符号）。Wayland + 软件 + 系统字体 + markup + motion 的 release：静态 `markup_controls` 4,248,768 B，仅 `ui!` 动态示例 4,416,704 B（+168 KB），带运行时加载的 `dynamic` 4,547,776 B。
- release 临时探针（本机，CJK 测试字体）：示例文档与其导入的解析检查约 0.09–0.1 ms；1000 行 `for` 首次构建加刷新 6.5–12 ms，命令式创建同样 1000 个文本 2.9 ms；追加一行 1.1 ms，整体反序 5.1 ms，单个绑定更新加刷新 0.24 ms（主要为 1000 行重排）。
- 验证：`aegle-markup/tests/program.rs` 覆盖导入、组件、作用域、类型及 17 类诊断；`aegle-loader/tests/engine.rs` 在无窗口 Ui 上经语义树与语义 Click 覆盖绑定、事件、if 切换、组件本地状态与响应式参数、for 追加/重排/删除的身份保留、重复 key、类型错误赋值、溢出停止、构建失败与成功的 reload 及 state 保留；facade 的 `markup.rs` 覆盖动态 `ui!` 的 id/state 字段、导入与丢弃 View 后绑定仍更新。
- `dynamic` 示例在私有 Pixman Sway 上用键盘验证：Clear 清空列表并经 `enabled: len(tasks) > 0` 禁用自身、摘要随之更新，Add 追加任务行，运行时加载面板中的 Switch 切换 if 块；修改 `panel.aegle` 后 Reload 重建面板且保留 `on` 状态；写入未知名称后 Reload 打印 `panel.aegle:6:51: unknown name` 并保留旧面板，应用继续运行。release 静止 3 秒 CPU tick 增量 0，PSS 约 7.6 MB。首个 Tab 落在第二个控件：虚拟键盘出现时窗口获得键盘焦点，既有 `window_focus` 已把焦点交给第一个控件。

## 默认关闭系统无障碍适配

- facade 默认 `desktop` 不再包含 `unix-accessibility`；Windows 仍默认启用 UIA，`Ui::accessibility` 语义树导出不受影响。Linux 需要系统无障碍时显式启用该 feature。仅在适配器存在时编译的发布函数相应改为按适配器 feature 编译，消除新默认组合下的未使用警告。
- Linux 默认组合运行时链接的第三方 crate 由 141 个降为 86 个（不再包含 zbus/atspi/async-io 等）；release `hello` 由 7,581,768 B 降为 4,224,192 B，`controls` 由 7,610,440 B 降为 4,256,960 B。私有 Pixman Sway 上默认 release `controls` 正常显示，PSS 约 9.6 MB、1 个线程。默认、显式启用 `unix-accessibility`、all-features 与 Windows 目标的检查和测试通过。
- 随后同样关闭 `windows-accessibility` 默认启用；默认 desktop 不再含 `accessibility` 语义导出，需要时显式选择 `accessibility`、`unix-accessibility` 或 `windows-accessibility`。Windows 默认组合运行时第三方 crate 由 71 个降为 66 个，交叉编译 release `hello.exe` 由 3,864,064 B 降为 3,247,616 B，`controls.exe` 由 3,889,664 B 降为 3,273,216 B；Linux 默认组合降为 84 个 crate，release 文件大小不变（未用的导出代码此前已被链接器去除）。默认、单独 `accessibility`、两个适配器 feature 与 Windows 目标的检查和测试通过。

## 开发者文档与控件截图

- 新增 `docs/developer/api.md`（依赖与 feature、应用与窗口、控件树、布局、样式/主题/皮肤、事件、动画、标记、嵌入宿主、错误）与 `docs/developer/controls.md`（每个默认控件的创建、方法、事件、标记写法与状态截图）。两份文档中的 Rust 片段已放入临时示例编译通过后移除；标记片段按[标记语言](markup.md)规则书写。
- `cargo run -p aegle --example gallery` 用无窗口 `Ui`、软件 renderer 与仓库测试字体以 2 倍缩放生成 17 张 PNG（约 368 KB），每个状态是独立 Ui，悬停/按下经指针事件、聚焦经 `focus()` 产生；无需合成器，facade 只增加 png 与 aegle-render-software 两个开发依赖。截图暴露的已知问题：高对比主题中禁用控件与启用控件外观相同（`muted` 为白色）。
- app 重新导出 `Selection`，使 `TextField::select` 无需直接依赖 aegle-text。

## 高对比禁用态、选择控件、弹出层、表格与可变高度列表

- 高对比主题的 `muted` 由白色改为对黑底 8:1 的灰色，禁用控件与次要文字可与启用控件区分。
- 新增 Radio（同父容器互斥、方向键在组内移动选择、圆形标志）与 CheckBox 部分选中（横线标志、语义 Mixed）；标记语言增加 `RadioButton` 与 `mixed`。新增 `Node::popup()`：绝对定位于窗口根的末尾子节点，按锚点放置于下方或上方，绘制在最上层并阻挡下方命中，Escape/外部按下关闭并归还焦点，锚点删除时一并删除。Dropdown 由按钮与弹出层组成，打开时聚焦当前选项并以对勾标记。Table 为表头行加 ListView 行。`variable_list_view` 在布局后测量已显示行并维护行顶前缀和，`Ui::refresh` 最多追加四次测量刷新。语义导出相应角色。均复用现有控件、回调与布局路径，没有新增依赖。
- 截图示例改为与原生 App 相同的 120 ms 默认过渡并在结束后截取，新增 radio、dropdown、popup、table、variable-list 与部分选中截图；修正文档中的绝对路径、无障碍文档的过时默认值与 ADR-0001 实现补充。
- 验证：`tests/composite.rs` 覆盖单选互斥/重复激活/方向键、部分选中、弹出层位置/焦点/Escape/命中阻挡/外部关闭、Dropdown 键盘选择与静默设置、删除、表格按需建格与语义角色、可变高度行的范围与 `set_count`；标记 schema 与编译型视图覆盖 RadioButton/mixed。workspace all-features 测试、检查与严格 Rustdoc 通过。


## 补齐功能缺口

每项都做最小实现，可选的额外格式放在 feature 后面；下列只记录已有证据，没有测量体积、内存或帧时间。

- 合成粗体/斜体：`GlyphRun::synthesized(embolden, skew)` 记录 Parley 的合成建议，`RasterOptions` 与 `GlyphKey` 带这两个值，aegle-glyph 对轮廓做 em/32 的外扩与错切后再光栅；软件、Vulkan、wgpu 三个后端经同一字形键使用，原先的 `PaintError::SyntheticStyle` 已删除。位图字体（sbix/CBDT）不合成。
- COLRv1 与 OpenType-SVG 字形：`aegle-glyph/colrv1`（tiny-skia，已在 workspace）把渐变、变换、裁剪、混合层与线/径/扫掠渐变画进一张位图，扫掠渐变逐像素计算；`aegle-glyph/svg`（resvg，无文字）渲染 `glyph<N>` 子树或单字形文档。两者结果都是普通彩色字形，GPU 图集不用改。无 ClipList 的 COLRv1 字体按 2 em 方框估算范围。没有这些 feature 时仍返回 `UnsupportedGlyph`。
- 图像格式：新增 `aegle-image`，`decode` 按签名识别 PNG（复用 aegle-glyph）与可选 JPEG（zune-jpeg）、WebP（image-webp，不含动画）、GIF（首帧，合成到透明逻辑屏），都先按字节预算检查再分配；`svg::rasterize`/`size`（resvg）把静态 SVG 栅格到指定尺寸。facade 的 `jpeg`/`webp`/`gif`/`svg` 开启它们，重导出为 `aegle::image`。
- 渐变与阴影：`aegle-widgets/effects`（facade 默认启用）生成线性/径向渐变与柔和阴影 `Image`，在预乘线性光中插值，复用各后端已有的图像绘制，不增加渲染命令。
- 缩放/旋转动画：`Node::set_transform(Transform { scale, rotation })` 以节点中心为原点作用于子树，是呈现层变换：场景（`visit_scenes` 的矩阵含它）、命中、滚动视口裁剪（取变换后的外包框，旋转时是包围盒近似）、IME 锚点与 AccessKit 节点变换都跟随；布局和滚动范围不变。有过渡策略时与位移同样补间并触发完成回调。
- 惯性滚动：`Ui::fling(position, velocity)` 以 τ=325 ms 的指数衰减推进，经 `advance_animations` 驱动，到 10 px/s、边缘、任何新滚动/按下或减少动态效果时结束。Wayland 触摸板的 axis_stop 用最近 100 ms 的样本估计速度；Windows 驱动自带惯性滚轮，不另做。
- 主题：`ThemeOverride`（逐字段 `Option`）与 `Node::set_theme_override`，叠加在父级解析主题上并随父级或 UI 主题更新，嵌套叠加；快照 `set_theme` 会替换它。系统文本缩放：`Preferences::text_scale`（百分比）来自 Windows 的 `TextScaleFactor` 与 GNOME 的 `text-scaling-factor`（经同一个 portal 客户端），`AppOptions::text_scale` 可显式覆盖，原生 App 把字号和控件高度按它缩放；没有该设置的桌面（如 KDE portal）保持未知。
- 后台投递：`App::proxy(handler)` 返回可克隆、可跨线程的 `UiProxy<T>`；`send` 入队（上限 1024 条，满或应用退出时把消息退回）并唤醒事件循环，handler 在 UI 线程于所有借用之外按序运行。
- 标记语言：`record` 声明与位置式构造 `Task(1, "a")`、`item.title` 字段读取、`for item in items key item.id`（record 列表必须给 key，键相同但值变化的行重建）；`let`；`host.name(args)` 宿主动作（`Program::action` 或线程共享的 `loader::action`，构建时校验名称与参数类型，缺失/不符在挂载前报错）；`slot` 与 `component` 内 `event name(type)`、`emit`、实例上的 `on name(value) { }`；`Program::set_limits` 配置每次处理的语句数（默认 10,000）、单个 `for` 的行数（10,000）与嵌套 emit 深度（64）；运行时错误带文件路径。`ui!` 的 View 把 record 状态暴露为 `State<Data>`，`StateValue::ty` 改为 `accepts`。
- Wayland：wp-fractional-scale-v1 + wp-viewporter（缺一则回退整数 buffer scale），`WindowInfo::scale` 改为 `f32`，缓冲区按 `round(逻辑尺寸 × scale)` 计算，视口把它映射回逻辑尺寸；`wl_touch` 经 `Event::Touch` 到达，App 的 `Ui::touch` 把点击与控件拖动映射为指针事件，在非拖动内容上超过 10 px 的拖动取消点击并平移滚动视图，抬起时带速度进入惯性。
- GPU 多窗口：Vulkan 的 `SharedDevice`/`WindowRenderer::with_device` 与 wgpu 的 `SharedGpu`/`with_gpu` 共享实例、设备、队列（wgpu 还有管线）；每窗口仍有自己的 surface/swapchain、管线（Vulkan）与字形图集。App 的第一个窗口创建设备，之后的窗口复用；不能向共享设备呈现的窗口创建失败，不静默改用新设备。

验证（本机；私有 headless Sway/Pixman，不连接用户桌面）：

- 新增测试：`aegle-glyph` 合成粗体/斜体、COLRv1（自建 984 字节字体，覆盖线/径向渐变、平移缩放、前景色、Multiply 合成、扫掠及预算）、SVG 字形（自建字体）；`aegle-image` 的 PNG/JPEG/WebP/GIF 往返、预算、截断与 SVG；`aegle-widgets` 渐变与阴影；`aegle-app` 的转换/命中、惯性、主题覆盖、触摸手势；`aegle-markup` 的 10 类新诊断；`aegle-loader/tests/language.rs` 的完整界面场景（records、slot、事件、let、宿主动作、限额、文件名）；facade 的 `ui!` record 与共享宿主动作。测试字体与生成脚本在 `tests/assets/colrv1-font.py`（FontTools 4.62.1）。
- 隔离 Sway 上：Wayland 平台新增 ignored 测试在输出缩放 1.5 下确认首帧后收到 preferred_scale 并以 `round(尺寸×1.5)` 呈现；App 的两窗口 native 场景在缩放 1.5 下以软件、Vulkan 与 wgpu 通过（Lavapipe；GLES2 compositor 上另在 RX 6800 XT/RADV 上跑了 Vulkan 与 wgpu 两窗口场景）；Vulkan 平台的两窗口共享设备场景在 Lavapipe 与 RADV 上通过；线程投递场景通过。controls 示例在 1.5 缩放下截图检查。
- Windows：x86_64-pc-windows-gnu 以同版本 Debian rust-src 加临时提取的 MinGW 做 `-Zbuild-std` 交叉检查，Win32 平台 crate 与 facade（windows、software、vulkan、wgpu、windows-accessibility、markup、motion 及新 feature，all-targets）通过；没有链接或运行，也没有执行 Wine。
- 未验证：触摸只有无头手势测试，没有真实触摸设备，Wayland 触摸协议路径只通过编译；合成粗体/斜体、COLRv1、SVG 字形只在 CPU 光栅路径做了像素测试，没有逐后端截图；共享 Vulkan 设备没有开启 validation 层；文本缩放的 portal/注册表读取只做了编译与审查，没有对应的探针；没有 Clippy、MSRV 与体积/性能测量。

## 拆分 aegle-image（PNG、其他格式与渐变阴影）

- PNG 解码（`png::decode`/`decode_with_limit`、`DecodedImage`、字体位图用的 `decode_into`）用 `git mv` 从 `aegle-glyph` 移入 `aegle-image::png`，错误统一为 `image::Error`（`NotPng` 变为 `Unsupported`）；`aegle-glyph` 反过来依赖 `aegle-image` 解码字体内嵌 PNG，不再带 png 依赖。`aegle-image` 只依赖 `aegle-scene` 与 `aegle-types`，不带字体栈。facade 删除 `aegle::decode_png/decode_image/DecodeError/DecodedImage`，一律经 `aegle::image`。
- PNG 有界解码：先检查签名，再按文件头检查宽高不超过 16,384、输出字节不超过上限（默认 64 MiB），通过后才分配像素缓冲，并以同一上限约束解码器；灰度、RGB、RGBA、调色板（含 `tRNS`）与 1–16 位都转为非预乘 sRGB RGBA8，忽略伽马与 ICC，APNG 取默认图像，损坏或截断返回错误。`aegle-image/tests/png.rs` 覆盖各编码、截断、上限边界与超宽拒绝；交错 PNG 未测试。
- 渐变与阴影从 `aegle-widgets/effects` 移到 `aegle-image/effects`（facade 的 `effects` feature 不变，路径为 `aegle::image::effects`），测试随文件移动。验证：`aegle-image`、`aegle-glyph` 全 feature 测试通过。

## 引擎、控件库与原生宿主分离

- 原 `aegle-app` 拆为三层：`aegle-ui`（无窗口引擎）、`aegle-widgets`（全部默认控件）、`aegle-app`（原生宿主）。文件用 `git mv` 移动：滚动条/滚动几何 `aegle-widgets → aegle-ui`（`bar.rs`、`scroll_geometry.rs`），list/popup/table/文本/数值/视觉控件 `aegle-ui → aegle-widgets`，测试随控件迁到 `aegle-widgets/tests`，原生测试迁到 `aegle-app/tests`。
- 引擎中封闭的 `Content` 枚举替换为 `aegle_ui::Control` trait（每节点一个对象）：输入、布局测量、绘制、语义经 `InputCx`/`MeasureCx`/`PaintCx`/`SemanticsCx` 访问；段落、编辑器、视口以能力方法暴露。需要跨节点协作的行为（弹出层的覆盖层放置/外部点击/Esc、单选组方向键、虚拟列表的行实现与测量）经 `Hooks` 函数指针安装，控件库数据存于 `State::ext`；`Container::add` 与 `aegle_ui::handle!` 供第三方控件库使用。`aegle-ui` 不再依赖 widgets 或任何平台/renderer（`cargo tree -e normal -p aegle-ui` 无这些 crate）。
- 同轮清理：`Preferences` 与 `TouchPhase` 在 Wayland/Win32/ui 中各有一份，现统一在 `aegle-types`，平台与 ui 重导出；删除宿主里的一一映射。facade 重导出 app、ui、widgets、image，prelude 增加 `Widgets`/`NodePopup`；标记宏生成的代码自行引入 `Widgets`。
- 验证：`cargo check --workspace --all-targets --all-features` 无警告；`cargo test --workspace --all-features` 全部通过（含 aegle-ui 的几何测试与 aegle-widgets 的组合/指针/触摸/滚动/动画/值控件场景）；私有 Sway 上 `aegle-app` 的 ignored 原生测试在软件、Vulkan（lavapipe）与 wgpu 下通过。过程中修复了虚拟列表在借出状态后写回顺序变化导致的越界（`take`/`put_back` 保持位置）。

## wgpu 适配器能力检查与全控件示例

- `aegle-render-wgpu` 的适配器检查原先读的是 `Limits::downlevel_defaults().using_resolution(..)` 的结果；`using_resolution` 只取适配器的纹理尺寸，存储缓冲数恒为默认值 4，检查从不触发，能力不足的适配器只会在 `request_device` 失败。现改为要求 `DownlevelFlags::VERTEX_STORAGE` 且适配器 `max_storage_buffers_per_shader_stage ≥ 2`，否则返回 `Unsupported`（GL 在顶点阶段为 0 时会报片段阶段的数量，单看限额不够）；设备请求的存储缓冲数随之降为实际需要的 2。GLES 后端仍不编译，拒绝路径没有实测。
- 新增 `showcase` 示例：一个窗口包含全部默认控件（标签、单行/密码/多行编辑、按钮与禁用按钮、弹出层、复选框含 mixed、开关、单选组、滑块联动进度条、切换主题的下拉框、图像、可旋转 Canvas、滚动视图、等高与可变高度虚拟列表、表格）和状态行。
- 验证：`aegle-render-wgpu` 的 ignored GPU 测试在 RADV（RX 6800 XT）与 Lavapipe 上通过。私有 headless Sway（GLES2 compositor）上以 RADV 运行 wgpu 构建的 release `showcase`，wtype 键盘 Tab 遍历显示焦点框、空格打开弹出层、下拉框切换到深色主题并截图。默认（软件）构建也在同一 compositor 上启动并显示同一布局，未做主题切换。

## 表格与弹出层随主题更新

- 表格、表头与弹出层（含下拉列表）原先在创建时把主题颜色写成固定的本地 `Style`，`set_theme` 后保持旧颜色。现在改用纯函数皮肤 `group::panel`（surface + border + 1 dp 边框）与 `group::header`（background），每次按当前解析主题求值；用户自己设置的本地 `Style` 仍优先。
- 弹出层的半 padding 与零 gap、表格单元格的半 padding 原先同样在创建时写死，而且弹出层没有记录本地布局位，主题变化后 `Plain::retheme` 会把 gap 重置为 `theme.gap`。现在由 `Group::retheme` 按角色给出这些默认值，创建与换主题走同一函数。
- 验证：新增 `table_and_popup_surfaces_follow_theme_changes`，在深色与高对比主题下检查表格与弹出层的背景、边框、零 gap 和内边距；修改前该测试在背景断言处失败。`cargo test --workspace --all-features` 通过。

## 完整布局：对齐、尺寸约束、网格、叠放与透明分组

- `aegle-layout` 新增经过校验的布局值：`Length`（px、百分比、auto）、`Insets`、`Align`、`Justify`、`Direction`、`Wrap`，`grid` feature 下另有 `Track`、`Placement`、`Flow`；各值有 `is_valid` 与到 Taffy 字段的转换，原始 Taffy 类型仍然导出。新增 contents 节点（`set_contents`，类似 CSS `display: contents`）：适配器在一次计算中为有可见 contents 子节点的父节点建立展平子列表（没有此类子节点的父节点直接用树子节点，不分配），并在父节点得到最终布局时给 contents 节点该尺寸与零偏移，`aegle-ui` 的坐标累加因此不变；隐藏的 contents 节点隐藏其子树。
- `aegle-ui`：布局 setter 移到 `layout_handles.rs` 并补齐 `set_max_*`、`set_aspect_ratio`、`set_shrink`、`set_basis`、`set_align_self`、`set_margin`、`set_absolute`、`set_gaps` 与容器的 `set_direction`、`set_wrap`、`set_align_items`、`set_justify_content`、`set_align_content`、`contents()`；长度参数改为 `impl Into<Length>`（原有 `f32` 与 `Option<f32>` 调用不变），`set_min_size` 改为两个长度参数，`set_padding` 接受 `Insets`（文字控件仍只接受统一像素值）。`grid` feature 增加 `grid(&[Track])`、`stack()`（1×1 网格，子控件在插入或 reparent 时放入同一格，穿过 contents 分组查找）、轨道、自动放置与子项位置 setter。
- 表格外层改为裁剪盒（overflow hidden），自动最小高度不再等于全部虚拟行高度。容器的最小尺寸仍按 CSS flex 由内容决定；把它默认设为零会压缩滚动视图里的内容，所以中间容器需要显式 `set_min_height(0.0)`，文档已说明。
- 标记：词法增加 `50%` 与 `1fr`，检查时把只含字面量和标识符的 `[...]` 规范为常量列表（组件参数仍按表达式处理）；schema 增加 Grid、Stack 与 25 个布局属性（edges 按 CSS 顺序，gap 为 `[行, 列]`，grid 位置为线号或 `[线号或 auto, 跨度]`），枚举值用 snake_case 标识符；`ui!` 在 `aegle-macros/src/layout.rs`、loader 在 `aegle-loader/src/layout.rs` 生成或执行对应 setter。loader 的 if/for 块容器从“复制父容器方向与 gap 的内部行/列”改为 contents 分组，块内子控件直接参与父容器的 flex、grid 或 stack。facade 增加 `grid` feature（默认 desktop 不含）与 prelude 的布局类型；新增 `layout` 示例，`showcase` 改用 `set_basis` 与居中对齐。
- 验证：`cargo test --workspace --all-features` 通过；默认 feature（无 grid）的 workspace `check --all-targets` 通过。新增测试：`aegle-ui/tests/layout.rs`（对齐、SpaceBetween、百分比、auto 外边距居中、basis 与 grow 分配、max 截断、换行、宽高比、绝对定位、反向、contents 分组及隐藏、主题切换保留本地布局、非法输入；grid：显式/自动轨道、跨列、自动放置、justify/align self、contents 子项入格、stack 叠放与 reparent 入格）；`aegle-widgets` 表格在中间列中收缩（修改前失败）；`aegle/tests/layout.rs` 同一布局文件经 `ui!` 与运行时加载得到完全相同的边界，`for` 生成的子控件各占网格一格；markup schema 的接受与拒绝用例。
- 测量：Taffy grid 使 `aegle-layout` 的 release `retained` 示例从 475,992 B 增至 725,856 B（+249,864 B）。临时 release 探针（CJK 测试字体，1000 行 `for`，7 次中位数），修改前/后：首次构建加刷新 3.83/3.59 ms，追加一行 1.04/0.80 ms，整体反序 5.61/5.80 ms。
- 私有 headless Sway（GLES2 compositor）上运行默认软件绘制加 `grid` 的 release `layout` 与 `showcase`，截图确认换行标签、auto 外边距居中、16:9 宽高比、跨两列的叠放卡片与角标、跨两行卡片、`for` 卡片入格及 wtype 新增的卡片、右下角绝对定位按钮；`showcase` 深色主题下表格主体、表头与下拉列表背景正确，"Theme" 标签与下拉框垂直居中。

## 逐帧回调、输入时间与窗口级按键

- `aegle-ui/src/events.rs`：`Node::on_frame(FnMut(Node, Instant))` / `clear_on_frame`，按注册顺序存放，控件删除时移除；`Ui::run_frame(now)` 在借用之外逐个取出回调调用，回调可替换或清除自己，出错的回调被移除并返回错误；`Ui::wants_frames()` 供宿主决定是否继续请求帧，注册时置 repaint 以启动空闲窗口的帧循环。`Ui::on_key` / `clear_on_key` 与 `Window::on_key`：处理器在焦点控件与 Tab 遍历之前、借用之外运行，返回 true 消费按键，可在调用中替换自己，出错时被移除；`KeyEvent` 带 `time: Instant` 与 `editing`。`Ui::key` / `pointer` 改为以当前时间调用新的 `key_at` / `pointer_at`，`InputCx::time` 把时间交给自定义控件。
- `aegle-app`：原生循环在窗口的下一帧到期（收到 Redraw）且有逐帧回调时，于运行时借用之外调用 `run_frame`，再处理回调与刷新；呈现后只要 `has_animations()` 或 `wants_frames()` 为真就请求下一帧。`event_clock.rs` 把 Wayland 键盘/指针时间戳与 Win32 新增的 `Event::Key/Pointer::time`（`GetMessageTime`）映射到 `Instant`。
- 验证：新增 `aegle-ui/tests/events.rs`（顺序、借用外修改控件、替换、删除控件、出错移除）与 `aegle-widgets/tests/events.rs`（处理器先于焦点控件、消费空格后复选框不切换、未消费的 Tab 仍遍历、editing 标志、自我替换、出错移除后按键恢复到控件）；`cargo test --workspace --all-features` 通过。私有 headless Sway（GLES2 compositor，60 Hz 输出）上临时探针用 `on_frame` 计数，1 秒内 66 次回调，随后清除并退出。
- Windows 交叉检查：`windows,software,system-fonts,markup,motion,windows-accessibility,vulkan,wgpu,grid` 的 `--all-targets` 与最小 `windows,software,system-fonts` 组合均通过 `cargo check`；没有链接或运行。
- 未验证：Win32 时间映射与真实按键时序没有实机测量；没有测量按键时间相对真实按下时刻的误差。

## 交互 Canvas 与滚轮路由

- `Canvas` 移到 `aegle-widgets/src/canvas.rs`。`on_input(FnMut(Canvas, CanvasEvent))` 使其可聚焦、带焦点框（专用皮肤）、按下时取得焦点并捕获指针、消费其上的滚轮并在聚焦时接收按键；事件在控件内排队，每批第一个事件以 `Action::Change` 调度一次回调，回调在借用之外按序取出。`CanvasEvent` 为 Press/Move（含 pressed）/Release/Leave/Cancel/Wheel/Key/Focus，坐标为局部逻辑坐标并带 `Instant`。`clear_on_input` 恢复为纯绘制并移出焦点。
- 引擎：`aegle-controls::Input::Wheel { delta, position, modifiers }`；`Control::takes_wheel()`；`Ui::wheel(point, delta, modifiers, Instant)` 从命中控件向上，先把滚轮交给取用滚轮的控件，处理即停止，否则照常滚动视口；`scroll_by` 以无修饰键调用它，惯性滚动跳过这类控件。原生宿主把 Wayland axis 与 Win32 滚轮连同修饰键和平台时间交给 `Ui::wheel`。
- 验证：`aegle-widgets/tests/canvas.rs`（未启用输入时滚轮滚动外层视图；启用后悬停移动、带时间的按下、焦点、捕获下窗外移动与释放、Ctrl+滚轮被消费且视图不动、按键；清除后失焦且滚轮恢复滚动视图）；`cargo test --workspace --all-features` 通过。

## 应用 GPU 纹理

- `aegle-scene`：`TextureId(u64)` 与 `Command::Texture { texture, rect }`，`SceneBuilder::texture` 校验矩形。`aegle-gpu::stretch` 从 `image_placement` 抽出，图像与纹理共用放置。
- wgpu：共享 `Gpu` 上的登记表（`register_texture(&wgpu::Texture)` 校验二维、单采样、`TEXTURE_BINDING` 与可过滤浮点格式并建 bind group；`unregister_texture`；`device()`/`queue()`），`Renderer` 与 `SharedGpu` 都提供。录制时 bind group 进入本次提交的列表，图元页字段用最高位标记应用纹理；`UnknownTexture` 使帧失败。重导出 `wgpu`。
- Vulkan：共享 `Device` 上的登记表（unsafe `register_texture(view, extent)`、`unregister_texture`、`raw_device()` 返回 `RawDevice`），描述符池在图集页之外预留 16 个集合，每帧 fence 之后清空映射并在首次使用时写描述符；`TooManyTextures`、`UnknownTexture`。此前 `Text::record` 对未处理命令 `unreachable!`，现在返回 `UnsupportedCommand`。重导出 `ash`。软件后端对该命令返回 `UnsupportedCommand`。
- `aegle-app`：`App::wgpu()`、`App::vulkan()` 返回窗口共用的设备句柄，并重导出 `SharedGpu`、`SharedDevice`、`RawDevice`、`wgpu`、`ash`；新增 `gpu_texture` 示例（wgpu 渲染通道每帧在 `on_frame` 中绘制旋转三角形到应用纹理，Canvas 显示）。
- 验证：`aegle-render-wgpu/tests/texture.rs`（2×2 纹理拉伸后角落纯色、中心混合，非法用途拒绝，注销后帧失败）在 RADV 与 Lavapipe 上通过；`aegle-render-vulkan/tests/texture.rs`（ash 创建并清除的 sRGB 图像读回颜色、17 个纹理超出预留、注销后帧失败）在 Lavapipe 与 RADV 上开启 Khronos validation 1.4.363 与同步检查通过，无验证消息；Vulkan 其余离屏测试同样在验证层下通过（验证层为提取到 scratchpad 的 Debian 包）。私有 headless Sway 上运行 release `gpu_texture`，截图确认三角形逐帧旋转、文字计数增长（约 64 帧/秒）。
- 未验证：Vulkan 后端没有端到端示例（只有离屏测试）；Metal/D3D12 上的应用纹理未运行。

## 更多基础组件、范围控件变体与提示

- 引擎（`aegle-ui`）：`PaintCx` 增加 `time`、`reduced_motion` 与 `request_frame()`，绘制中请求的节点在下一帧标记重绘，`wants_frames` 随之为真；`Hooks::hover`（悬停目标变化）与 `Hooks::wake`（`State::wake` 到期），悬停逻辑移入 `hover.rs`；`Node::set_accessible_description`；`Container::set_clip(bool)` 让普通容器把子树的绘制、命中和指针形状裁剪到自身边界（复用视口的 clip 传播）。原生循环的等待超时受最近的 `next_wake` 限制，在逐帧回调之前处理到期唤醒。
- 控件（`aegle-widgets`）：Slider/Progress 的 `Orientation::Vertical`、不确定进度（1.6 s 往复）与 `motion` 下 120 ms 的数值滑动（遵守减少动态效果）；聚焦 Slider 消费滚轮。新增 `NumberField`、`Separator`、`Tabs`（`Variant::Tab` 按钮、Left/Right 切换）、`Splitter`（Canvas 把手、拖动与按键、窗格裁剪）以及任意控件的 `NodeTooltip::set_tooltip`（500 ms 延时、窗口内放置、Escape/按下/离开隐藏、写入无障碍描述）。新增 SpinButton、Tab/TabList/TabPanel、带方向的分隔语义。
- 标记：上述组件与 `orientation`、`indeterminate`、`tooltip`、`decimals`、`ratio` 属性在 schema、`ui!` 代码生成与运行时加载器中一致实现；`Tabs` 只收 `Tab`、`Tab` 需字面量 title、`Splitter` 恰好两个内建子节点；tooltip/indeterminate 可绑定；NumberField 与 Tabs 有 `on changed`（`self.value`、`self.selected`）。
- 呈现：节点裁剪矩形在三个后端共用的 `scenes` 中对齐到整设备像素；软件后端对整像素外部裁剪只收窄光栅范围，不再占用整面 mask。此前 960×720 窗口中被裁剪的三层裁剪记录需要 2,764,800 B，超过默认 2 MiB 预算而无法呈现；1920×1080 窗口中任一 ScrollView 内的形状也需两层整面 mask（约 4 MB）。现在这两种情况都不再增加 mask。
- 验证：`aegle-widgets/tests/components.rs`（竖直滑块指针与键盘、聚焦滚轮、不确定进度的帧请求与停止、`motion` 下滑动结束后停止请求帧、NumberField 步进/提交/clamp/非法文本恢复/步进区点击/回调次数、分隔线方向与厚度；Tabs 单击、Left/Right、`select` 不回调、越界拒绝；Splitter 拖动、End、越界拒绝、比例为零时首窗格内容被裁出；Tooltip 延时唤醒插入标签、Escape 移除、无障碍描述）；`aegle/tests/components.rs`（同一 `.aegle` 经 `ui!` 与加载器得到相同边界与属性，加载器中 NumberField/Tabs 的 `on changed` 更新 state）；schema 接受与拒绝用例；软件渲染测试改为验证整像素裁剪不占 mask、分数裁剪仍需一层。`cargo test --workspace --all-features` 通过，默认 feature 的 workspace `check --all-targets` 通过，Windows 交叉 `cargo check`（与上一节相同组合）通过。
- 私有 headless Sway（GLES2 compositor）上运行 release `showcase`（软件后端）：截图确认分割窗格、标签页下划线与键盘切换到表格页、NumberField 步进区、竖直滑块、分隔线、不确定进度扫动；两窗口平铺时左窗格内容在把手处被裁剪，修改前该布局因 mask 预算报错退出。
- 未验证：headless Sway 没有指针设备，Tooltip 与 Splitter 拖动只经单元测试，未在原生窗口中实测；新增语义角色未经 AT-SPI 查询；Vulkan/wgpu 后端未截图验证裁剪对齐。

## 逐属性过渡与标记几何

- 引擎（`aegle-ui/src/motion.rs`）：`TransitionProperty { Paint, Offset, Scale, Rotation }`；过渡策略改为四项各自可选的 `Transition`，`set_transition` 设四项相同，`Node::set_property_transition`/`property_transition` 单独设置或读取。位移、缩放、旋转分别补间（各有运行表），缩放和旋转仍共用 `Transform` 目标；完成回调在最后一项结束时排队一次，减少动态效果、`finish`/`cancel` 与删除节点覆盖所有项。去掉某项时间时该项跳到目标而不单独完成。原生 App 的默认策略在首次绘制前不补间几何。
- 标记：`offset_x`、`offset_y`（dp）、`scale`、`rotation`（度）可写字面量或绑定 float，`ui!` 与加载器都调用 `set_offset`/`set_transform` 并保留另一轴或另一分量；`paint_transition`、`offset_transition`、`scale_transition`、`rotation_transition` 接受 `200ms` 或 `[200ms, easing]`，在 `transition` 之后逐项覆盖。schema 拒绝 Window 上的几何、非 dp 位移、非正或大于 1000 的缩放与不完整的时长列表。facade 预导出 `TransitionProperty`。
- 验证：`aegle-ui/tests/transitions.rs`（位移 200 ms、缩放 100 ms 线性分别取样，旋转无时长直接到位；只在最后一项结束时完成一次；去掉位移时长时跳到目标且不完成）；`aegle/tests/motion.rs`（同一 `.aegle` 经 `ui!` 与加载器得到相同的四项时长、位移与 90° 旋转，绑定的 scale 改变后补间 100 ms 结束）；schema 接受与拒绝用例。

## 原生渐变与阴影

- `aegle-scene`：`Gradient`（线性/圆形，2–16 个色标，`with_geometry` 共享色标换几何）、`Command::FillGradient` 与 `Command::Shadow`（标准差 blur，零 blur 记录为普通填充）、`Scene::gradients`、`SceneError::InvalidGradient`。`aegle-image::effects::Stop` 改为同一 `GradientStop`。
- GPU（`aegle-gpu`，Vulkan 与 wgpu 共用）：`Recording::gradient`/`shadow` 写几何图元，`header[3]` 选择效果，色标两个一行附在裁剪缓冲中；WGSL 的 `fs_main` 求渐变与阴影，Vulkan 构建期 Naga 校验同一 WGSL。没有新增绑定、管线、离屏纹理或预算项。
- 软件：`effects.rs` 在像素中心用与 shader 相同的公式求值，渐变复用覆盖率 mask，阴影只读取裁剪 mask，不需要新的 mask。
- UI：`Node::set_shadow(Option<Shadow>)` 与 `set_background_gradient(Option<Gradient>)` 存在稀疏的 Decoration 中，阴影在背景之前录制，渐变按节点尺寸比例换算后替代背景色。`showcase` 增加渐变卡片，并可用 `showcase vulkan|wgpu` 选择 GPU 后端。
- 验证：`aegle-scene/tests/recording.rs`（非法色标、零长度线、非正半径、负 blur、零 blur 变填充）；`aegle-render-software/tests/effects.rs`（两端外延与线性光中点值、阴影中心满强度、边缘约半、三倍标准差外不绘制、裁剪生效）；`aegle-render-{vulkan,wgpu}/tests/effects.rs`（旋转、圆角裁剪、半透明与硬色标下与软件结果比较）：Vulkan 在 RADV 与 Lavapipe 上开启 Khronos validation 与同步检查通过，平均通道差 0.136/0.128，无验证消息；wgpu 在 RADV 与 llvmpipe（Vulkan 后端）上通过。`aegle-ui/tests/effects.rs`（录制顺序、尺寸换算、清除）。私有 headless Sway 上 release `showcase` 分别以软件、Vulkan、wgpu 截图，渐变卡片区域相对软件的平均通道差为 0.15/0.18，最大差在文字边缘。
- 未实现：组透明度与区域模糊（需要离屏层和临时纹理预算）；阴影/渐变的过渡与标记属性。wgpu 的 GL 后端未编译，未验证。

## 从右到左布局

- `aegle-layout` 导出 `LayoutDirection`（Taffy `Direction`）。复核发现 Taffy 0.14 的 flex、grid、block 与叶子布局都按 CSS 语义使用 `Style::direction`，因此适配器不做映射；Taffy 不继承方向，`aegle-ui` 把解析后的方向写入每个节点的样式。
- `Node::set_layout_direction(Option<LayoutDirection>)`/`layout_direction`：`None` 继承父节点，根为左到右；插入与 reparent 时子树重新解析，只有方向改变的节点重新布局。行的首个子项在右侧，`Start` 对齐指右侧，网格列向左排；内边距、外边距、inset 与绝对位置保持物理方向。
- 文本：段落与编辑器对齐由 `Start` 改为按布局方向的 `Left`/`Right`（`MeasureCx::alignment`）；混排文本的顺序仍由 Parley 按内容检测的基础方向决定。编辑器文字原点统一为 `Element::text_origin`，供绘制、IME 光标、滚入与语义共用。
- 滚动：偏移从起始边计量，右到左的视口中内容向左溢出，`scroll_shift` 在绘制、命中、语义父偏移中把横向偏移取反；滚轮、滚入与 ScrollLeft/Right 动作按视觉方向。竖直滚动条和槽位移到左侧（切换方向时交换视口的左右内边距，槽位一侧由另一侧推出），横向滑块从右端开始，`Bar::fraction` 返回逻辑比例。
- 控件：`InputCx`/`MeasureCx`/`PaintCx` 携带 `rtl`，`InputCx::logical` 交换左右方向键。Slider/Progress 横向镜像（竖直不变），开关、复选框、单选标记与文字互换位置，NumberField 步进条在左，Dropdown 箭头与选项勾在左，Tabs 与单选组的方向键、Splitter 的拖动比例与方向键镜像；弹出层继承锚点方向并右对齐；表格的列头与行是 flex 行，随之镜像。`Control::finalize` 改为接收带最终宽度的 `MeasureCx`。
- 标记：`layout_direction: ltr|rtl`，`ui!` 与加载器都调用 `set_layout_direction`；facade 预导出 `LayoutDirection`。`showcase ... rtl` 以右到左运行。
- 验证：`aegle-widgets/tests/rtl.rs`（行镜像、左侧槽位等于 `FOOTPRINT`、向左滚动 50 后偏移 50 且内容右移、滑块右端为最小值与方向键反转、切回后左内边距与宽度恢复）；`aegle-ui/tests/geometry.rs`（右到左竖直条在 x=0、横向滑块起于右端、比例为逻辑值）；`aegle/tests/layout.rs` 的 `.aegle` 夹具经 `ui!` 与加载器得到相同的镜像位置；schema 拒绝非法方向。私有 headless Sway 上 release `showcase` 以软件后端右到左截图，分隔条、标签页、文字、开关、滑块、进度条、步进条、下拉箭头与滚动条均镜像；Vulkan 左到右截图不变。
- 未实现：没有按段落内容自动选择布局方向；Windows 原生输入法候选窗位置未在右到左下验证。Windows 目标交叉检查通过。

## 命名网格、repeat、calc 与文字基线

- `calc`：`Length::Calc { percent, px }`（CSS `calc(percent% + px)`）用于尺寸、最小/最大尺寸、basis、内外边距、inset 与 gap。开启 Taffy 的 `calc` feature；Taffy 只把不透明的 calc 句柄回传给适配器而不解引用，因此在 64 位目标上把像素与比例的位直接打包进句柄（比例丢弃低 3 位尾数，相对误差低于 10⁻⁶），无需分配、表或 unsafe，样式克隆与比较不受影响；有一项为零时退化为普通长度。32 位目标上 `is_valid` 拒绝 `Calc`。
- 网格（`aegle-layout/src/template.rs`）：`TemplateItem`（线名、轨道、`Repeat::{Count, AutoFill, AutoFit}`）经 `template` 转为 Taffy 的轨道与线名，并检查 repeat 不嵌套、至少一条轨道、自动 repeat 唯一且全部轨道固定；`areas` 解析 CSS 式区域字符串并检查矩形；`GridLine`/`GridLines` 支持线号、跨度、命名线与命名跨度，`Placement` 可转换。`aegle-ui` 增加 `set_column_template`、`set_row_template`、`set_areas`、`set_grid_area`，`set_grid_column`/`set_grid_row` 接受 `impl Into<GridLines>`（原 `Placement` 调用不变）。
- 基线：Taffy 叶节点不报告基线，原先 `Align::Baseline` 实际按底边对齐文字控件。`aegle-layout::compute_with_baselines` 在 PerformLayout 时以最终边框盒尺寸向宿主取首基线；`Control::baseline` 由 Label、Button/Dropdown（居中）、TextField/TextArea 与 NumberField（内边距处，按未滚动位置）、CheckBox/Switch/Radio（标签居中）实现，`Node::baseline` 公开同一值。`compute` 保持原签名。
- 标记：标记值新增常量函数 `Value::Call`。`calc(...)` 在检查时折叠为百分比加 `dp` 的线性组合；`columns`/`rows` 接受字符串线名、`repeat(...)`、`minmax(dp, fr)`、`fit_content(dp)`；新增 Grid 的 `areas` 与子项的 `grid_area`；`grid_column`/`grid_row` 接受线名或区域名字符串及 `[起点, 跨度或结束线名]`。`ui!` 与加载器生成相同的 setter 调用；schema 在 `schema/grid.rs` 中按与 `aegle-layout` 相同的规则检查，因此通过检查的文档在运行时不会因这些规则失败。
- 验证：`aegle/tests/layout.rs` 的 `.aegle` 夹具经 `ui!` 与加载器得到相同且符合手算的结果：`calc(100% - 84dp)` 在 384 宽下为 300，区域 `head`/`nav`、命名线 `content`、repeat 内的线名 `half` 与负线号，以及 `repeat(auto_fill, 40dp)` 在 130 宽下排三列后换行；schema 接受与拒绝用例覆盖两个自动 repeat、自动 repeat 与 `fr` 混用、零次、只有线名、嵌套 repeat、参差与非矩形区域、空区域名、长度相乘与非 Grid 上的 `areas`。`aegle-widgets/tests/baseline.rs` 使用 CJK 测试字体，在一行中让 32px 标签、60 高的按钮、文本框、数字框与复选框的首基线重合（误差 0.01），并确认它们的顶边不同；关闭布局基线回调时该测试失败（基线相差约 15）。
- 未实现：`calc` 只有线性组合，没有 `min()`/`max()`/`clamp()`，网格轨道不接受 `calc`；没有子网格（subgrid）与 `masonry`；基线只取首行，`last baseline` 对齐未暴露。

## 主题 token

- `aegle-theme`（仍为 no_std、无分配）：`Token<T>`（u16 索引）、`TokenKind`/`TokenValue`/`TokenDefault`、`TokenType`（`Color`、`f32` 长度、`Duration`、`Font`）。`Font { families, weight, italic }` 选择字体（不含字号），`Font::DEFAULT` 为常规 sans-serif。14 个 Theme 字段是内置 token（`Theme::ACCENT` 等，`theme.<字段>`），`Theme::token`/`with_token` 与 `ThemeOverride::set_token` 按索引读写。
- `aegle-ui`（`tokens.rs`、`token_handles.rs`）：每线程一份注册表，`register_token`/`token` 校验 `包.名称` 形式与类型，`UiError::Token` 报告未登记或类型不符。每个 Ui 的 `Tokens` 用稀疏表保存全局覆盖、子树覆盖与绑定（`TokenSlot`）。`Ui::set_token`/`token_value`、`Node::set_token`/`token_value`/`unbind_token` 与四种绑定：`bind_color`（Style 的 11 个颜色）、`bind_length`（边框宽度、圆角、焦点宽度、文字控件字号、统一 padding、gap）、`bind_font`（文字控件字体，经新的 `Node::set_font`/`clear_font`/`font` 存于同一稀疏装饰，主题换字号时保留）、`bind_transition`（`motion` 下某项过渡的时长，曲线在绑定时给出）。`set_theme`、局部主题与覆盖、reparent、token 改变后在受影响子树内重新解析；直接 setter 结束对应绑定，`set_style` 与 `cancel_transition` 只结束 Style 绑定，`set_transition`/`clear_transition` 结束时长绑定。
- 原子性：任何一个绑定拒绝新值（如字号为零、字体字重越界）时，`set_token`、`Ui::set_theme`、`Node::set_theme`/`set_theme_override` 与 `reparent` 都恢复改变前的主题、覆盖或父节点与位置并重新解析，然后返回错误。`Tree::reparent_at` 供 reparent 放回原位置。
- 标记：颜色属性与 `border_width`、`radius`、`focus_width`、`font_size`、`padding`、`gap` 接受 `token("包.名称")`（含 `theme.radius` 等内置名），schema 检查名称形式；`ui!` 与加载器在构建时按名查找并调用 `bind_color`/`bind_length`，名称未登记或类型不符时构建返回 `UiError::Token`。字体与过渡时长绑定只有 Rust 接口。
- 验证：`aegle/tests/tokens.rs` 用同一 `.aegle` 夹具经 `ui!` 与加载器，检查默认值随 `set_theme` 变化、子树覆盖优先于 Ui 覆盖、内置 `theme.radius` 的子树覆盖、字号为零时 `set_token` 回滚、直接 setter 结束绑定、padding token 绑定；Rust API 部分检查 reparent 后按新子树解析、`unbind_token` 清除、Caret 绑定在非编辑器上被拒绝、`ThemeOverride` 改变后默认值跟随；另一场景检查 padding/gap 绑定后的布局间距随 token 改变、解绑回到主题默认值，字体绑定、非法字体回滚、容器拒绝字体绑定，过渡时长随 token 改变而 `set_transition` 结束绑定，以及使绑定字号非正的 reparent、`set_theme_override`、`Node::set_theme` 与 `Ui::set_theme` 都被拒绝且主题、父节点和兄弟顺序不变。schema 接受与拒绝用例；`grid` feature 下同样通过。
- 未实现：标记中的字体与过渡时长绑定；`margin`、尺寸等其余布局属性不能绑定；`Ui::set_default_transition` 不接受 token。

## 自定义控件的其他鼠标按键

- 此前原生宿主只转发主键：Wayland 的 `0x110` 之外的按键与 Win32 的右键、中键消息都被丢弃，自定义控件无法实现右键菜单、中键平移或侧键导航。
- `aegle-types::PointerButton { Secondary, Middle, Back, Forward }`；`PointerKind::ButtonDown/ButtonUp(PointerButton)`，`Down`/`Up` 仍只表示主键，默认控件不处理新增种类（Slider 在右键按下时不改变值也不取得焦点）。`Ui` 对其他按键同样运行按下钩子（关闭弹出层）并停止惯性滚动；滚动条拖动期间忽略它们。
- 平台：Wayland 映射 evdev `BTN_RIGHT`/`BTN_MIDDLE`/`BTN_SIDE`/`BTN_EXTRA`（及 `BTN_BACK`/`BTN_FORWARD`）；Win32 处理 `WM_RBUTTON*`、`WM_MBUTTON*`、`WM_XBUTTON*`（返回 TRUE，不再产生 `WM_APPCOMMAND`），原生捕获改为按位记录按住的按键，最后一个释放时才释放捕获。
- `Canvas`：`CanvasEvent::ButtonPress`/`ButtonRelease`；任一按键的首次按下取得焦点与捕获，全部释放后才释放；窗口丢失时 `Cancel` 结束所有按键；正在按下时其他指针的事件被忽略。
- 验证：`aegle-widgets/tests/canvas.rs` 检查右键不影响 Slider；中键按住时捕获跨过一次主键单击、移出后仍收到移动、释放中键后不再收到；侧键按下后离开窗口得到 `Cancel`。Windows 交叉检查通过；未在真实鼠标上验证（无头 Sway 没有指针设备），Win32 未实机运行。

## 剩余工作

- 平台验收：Windows 真实 IME/UIA/硬件 Vulkan 与 ARM64、TSF text store/重转换/触屏键盘；macOS AppKit/Metal；Wayland 客户端窗口装饰（无服务端装饰的 compositor 仍没有标题栏）与真实触摸设备；Windows 触摸与惯性；真实桌面 portal 与 Windows 设置变更的实机验收。
- 组件/绘制：组透明度与区域模糊（需要离屏层）；阴影与渐变的标记写法。
- 文字/无障碍：Unix adapter 的上游 EditableText 等限制；真实屏幕阅读器与候选窗验收。
- 工程验收：MSRV 1.88、Clippy、多 compositor/GPU 与嵌入式完整资源测量；GPU 多窗口只共享实例/设备/管线，图集仍按窗口独立。现有桌面样本不能替代这些证据。
