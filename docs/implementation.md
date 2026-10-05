# 实现状态

完整目标保持不变：模块化保留模式 GUI、GPU 与无 GPU 软件绘制、CJK/原生 IME、无障碍、组件/主题/动画、命令式与标记入口、跨平台及 API 文档。

## 已实现

- Rust 2024 workspace，LGPL-3.0-only；目标 MSRV 1.88，实际验证工具链为 1.96.1。
- aegle-types：no_std 几何与紧凑 RGBA 颜色，无第三方依赖。
- aegle-core：代数 ID、可复用槽位、保留树、索引子节点、结构变更与三通道失效，另有路由快照、默认动作控制及策略化焦点遍历；无第三方依赖。叶节点不分配子节点数组，删除不递归。
- aegle-layout：Taffy 0.14.0 直接适配同一棵保留树，无第二份拓扑；共享默认样式、测量缓存、Flex/Block、可选 Grid。它不依赖字体、窗口或 renderer。
- aegle-scene：no_std + alloc 的局部绘制记录；实色矩形、圆角、居中边框、仿射变换、嵌套裁剪。可选 text 保存定位字形、变体坐标与共享字体句柄，不依赖排版器或栅格器。
- aegle-text：复用 Parley/Fontique 的 Unicode shaping、字体选择、回退、换行与定位；Paragraph 保留文字和排版结果，宽度变化只重排，颜色覆盖无需重新 shaping。显式字体为默认，system-fonts、text-dictionary、text-a11y、scene 独立选用；缺字与无可用字体分别报告。
- Editor：复用同一 TextSystem 的 Parley PlainEditor，提供单/多行、选择/命中、视觉移动、grapheme 删除、精确 UTF-8 替换、只读、组合输入模型及有界差量撤销/重做。预编辑只保留被替换片段，提交值不随预编辑改变；取消恢复原选区。文字、装饰和候选区域来自同一布局。密码模式只排版等量 `•`，明文另存且不记录历史、拒绝 IME 组合。apply_ime 在完整验证后应用删除/提交/预编辑事务，删除与提交合成一次撤销；surrounding 无分配地借出有界周边文字。编辑、宽度和样式改变目前仍会重新 shaping，不宣称增量编辑引擎。
- aegle-controls：共享无分配 Range、Toggle、Slider 和无皮肤 Button 的键盘/指针/语义激活、capture 和取消状态；可选 text 提供复用 Editor 的 TextField。宿主拥有树、命中和焦点；controls 默认只依赖 types，不依赖窗口或 renderer。
- aegle-access：平台回调经 Mailbox/Handlers 排队并唤醒 UI；可选 UnixAdapter/WindowsAdapter 复用 AccessKit AT-SPI/UIA。示例从同一控件树按脏标记导出语义，系统 Focus/Click/SetTextSelection 回到同一焦点、按钮和 Editor。text-a11y 提供文字 run 与有校验的选择转换；不是完整跨平台无障碍。
- aegle-glyph：复用 Swash/Skrifa，按需生成灰度字形与 COLRv0/嵌入位图，LRU 同时约束图像字节和条目数；缓存不持有字体文件。PNG 位图使用有解码预算的 png crate；库不内嵌字体。
- aegle-render-software：借用 RGBA8 缓冲，tiny-skia 负责抗锯齿覆盖率，线性光 SourceOver 合成器处理透明颜色。默认仅几何；可选 text 接同一 Scene 的字形、变换和裁剪。支持均匀缩放的四分之一像素定位及任意可逆仿射变换的双线性采样，无裁剪文字无需面大小的 mask。
- aegle-render-vulkan：独立 Vulkan 1.1 离屏绘制，复用 Scene；GPU 绘制矩形/圆角/居中边框、仿射变换及最多八层裁剪，可选 text 接有界按需灰度/彩色字形图集。RGBA16F 线性混合后由第二遍 GPU 编码预乘 sRGB RGBA8；显式读回、有界设备/记录分配和单次在途提交。已验证硬件与软件 ICD；可选 window 已提供原生 swapchain，App 可显式选择。

- aegle-platform-wayland：一个连接上的多个 xdg-shell 窗口与可选 wlr layer-shell 表面、整数缩放、事件等待、键盘/指针输入、光标、text-input-v3 与按 seat 的非阻塞剪贴板；软件绘制直接借用最多两块有界 SHM 映射。平台不依赖文字/scene/renderer，原生示例把这些模块接到同一控件树和 Editor。尚无触摸、平台偏好、客户端装饰；gpu feature 提供原生句柄租约与共享帧门控。
- aegle-platform-win32：原生多窗口、消息等待、Unicode/指针输入、DPI、IMM 兼容组合、`CF_UNICODETEXT` 剪贴板、GDI 软件与 GPU HWND 租约；已交叉编译，执行证据见本页末尾，TSF/重转换/触屏键盘及真实 Windows 验收未完成。
- aegle-motion：独立无分配 Tween/Transition，标量、Point 与预乘线性 Color 插值、四种 easing；共用 types 的可选 std 颜色转换表。app 的可选 motion 已连接外观过渡、生命周期、语义颜色和 Wayland 帧驱动。几何动画、完成回调与系统偏好监听尚未实现。
- aegle-theme：无分配的有类型配色/尺寸、VisualState、Appearance/Style 和纯函数 Skin；浅色、深色与显式高对比主题。当前没有 token 注册表、局部主题继承或系统偏好监听。
- aegle-app 与 aegle：无窗口 Ui 和可选 Wayland/Win32 软件或 Vulkan 应用宿主，命令式 row/column/scroll_view/text/button/text_field/text_area/check_box/switch/slider/progress、弱句柄、布局 setter、可替换回调及主题切换。每窗口独立树，应用共享字体和 renderer；可选语义能力已接到原生循环。
- aegle-markup 与 aegle-macros：有界静态语法解析/校验和 `ui!` 编译，Window/Column/Row/ScrollView/Text/Button/TextField/TextArea/CheckBox/Switch/Slider/Progress 直接创建同一套保留控件，具名弱句柄绑定 Rust 回调；默认 facade 包含编译宏。运行时表达式、组件导入与 loader 尚未实现。

图像缓存预算不包括字体映射、排版、缓存索引及上游栅格 scratch；具体边界见 [资源](resources.md)。合成/过滤使用线性预乘颜色，公共字形彩色图像为非预乘 sRGB RGBA8。

## 可运行的组合

```sh
cargo run -p aegle --example hello --release
cargo run -p aegle --example controls --release
cargo run -p aegle --example hello_markup --release
cargo run -p aegle --example markup_controls --release
cargo run -p aegle --example components --release
cargo run -p aegle-render-vulkan --example geometry --release -- /tmp/aegle-vulkan.ppm
cargo run -p aegle-render-vulkan --features text --example text_scene --release -- /tmp/aegle-vulkan-text.ppm
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

## 前一阶段基础验证

- workspace 默认与 all-features 测试通过；all-features 共 14 个集成场景，全部位于各 crate/tests。新增两个场景覆盖组合取消/提交/撤销、被替换选区与隐藏 caret、组合字符精确撤销和 grapheme 删除、只读与单行边界、历史裁剪、候选区域及装饰像素；原有 CJK、字形预算、合成、布局和树生命周期场景保持通过。
- all-features/all-targets 检查通过，Rustdoc 在 warnings-as-errors 下通过，格式与 diff 检查通过。默认几何 renderer 的 normal dependency tree 不含字体栈或 PNG。
- 实现 4718 行、测试 955 行（16.8%，不含 examples）；最大源文件 418 行，均不超过 500 行。没有 src 内测试。
- Linux Fontconfig 独立探针发现 34 个字体家族；zh-Hans、ja、ko 与混排的四段文字 missing_glyphs/unshaped_bytes 均为 0，使用两种 face。这只验证本机字体环境，不保证其他设备覆盖率。
- 上一阶段的 560×400 文字示例：首次 Taffy 测量 20 次，同约束再次布局为 0 次；字形图像缓存 16908 B / 167 项；Scene 保留分配 5712 B。连续 10 帧像素与字形缓存统计一致；当次 release 10 次暖态离屏重绘为 19.82 ms。
- 当前编辑示例：一次中文替换的历史占用 100 B；字形图像缓存 4026 B / 57 项。以上为对应记录的成本，不包含活动文字/layout、字体、历史容器闲置槽位、renderer 缓冲及进程本身，不代表总 RAM/PSS。
- 本机 release 编辑示例的 100 次短预编辑更新为 551 µs，期间撤销历史为 0 B；最终可执行文件 3359080 B，含示例 PNG 编码和测试字体。这是一次短文本调用观察，不含窗口、真实 IME、绘制或呈现，不代表长文本延迟或嵌入式性能。
- release 示例已运行且图片已目视检查。本机 rustc/LLVM 曾在 ThinLTO 阶段 SIGSEGV；设置 `RUST_MIN_STACK=16777216` 后构建运行通过，未改变仓库 release 配置。
- Clippy 未安装；MSRV、其他平台和 GPU 均未验证。上一阶段只有离屏路径，原生窗口验证见下文。字典 feature 关闭时，上游 ICU 在 debug 下可能记录 CJK 词典缺失；基本显示与换行可用，词语导航仍需单独验收。

## 原生 Wayland 集成

`aegle-platform-wayland --example editor` 是可交互的软件绘制窗口：一棵 Tree/Taffy 树保存文字标签、TextField、Button 和局部 scene，Route/Focus 管理路由及遍历，按钮回调修改文本框。CJK、键盘编辑、拖选、滚动、撤销及 IME 使用共享行为与同一 Editor，绘制直接写 SHM。示例使用有限覆盖的 OFL 测试字体，无系统字体发现成本；它不是未来的10行 facade API，可选启用 Unix 系统无障碍，未接主题组件。

在隔离的 Sway 1.12/wlroots 0.20 headless + Pixman compositor 上验证，不连接用户桌面。原生窗口截图已目视检查，CJK 字形、选择字段与裁剪正常；两次启动的首帧均成功。真实协议探针通过 fake input-method-v2 和持有的 virtual-keyboard 对 text-input-v3 发送 CJK 预编辑、隐藏光标、删除/提交同批次、空预编辑重置、stale/current serial、跨窗口焦点、立即取消与旧会话延迟结果，暴露并修复了焦点重启和取消会话串写问题。这是实际客户端/服务端协议验证，尚未进行 fcitx/IBus 的完整真人候选窗验收。

native lifecycle 集成场景验证 configure 前不能绘制、失败帧不提交、frame callback 门控、缓冲复用、映射预算与零预算拒绝、空闲等待、销毁与失效 ID。需要专用 compositor 的测试标记 ignored，常规测试不会自行连接桌面。跨 compositor、嵌入式 PSS/CPU、真实硬件/GPU 和其他操作系统尚未验收。

原生窗口阶段检查（引入共享控件之前）：

- workspace all-features 的14个现有集成场景通过；两个默认 ignored 的 Wayland 集成场景在专用 compositor 上显式运行通过。所有测试位于各 crate/tests。
- all-features/all-targets 检查、warnings-as-errors Rustdoc、格式和 diff 检查通过。实现6805行，测试1380行，占16.86%；最大源文件483行，没有 src 内测试。
- 独立键盘探针验证 compositor repeat rate=0 仍投递首个按键，25 Hz 重复期间 Shift 将 a 转为 A，release/失焦/窗口移除取消重复；按键保持期间销毁 backend 后，文件描述符恢复到5个基线。常驻额外 XKB keymap/state 成本见[依赖](dependencies.md)，未测量其完整字节数。
- release 原生编辑示例构建并运行通过，3秒无输入运行仅呈现2帧（初始/激活变化），字形图像9013 B / 84项。可执行文件4372424 B，包含有限测试字体；`ldd` 列出 libxkbcommon、libgcc_s、libm、libc 和系统 loader，不含 Fontconfig、libwayland-client 或 GPU loader。该结果不等于完整 GUI、PSS 或嵌入式性能。

运行专用 compositor 测试时需自行设定隔离的 `XDG_RUNTIME_DIR` 与 `WAYLAND_DISPLAY`，再执行：

```sh
AEGLE_TEST_COMPOSITOR=private cargo test -p aegle-platform-wayland --tests -- --ignored --test-threads=1
```

IME 场景还需要 compositor 提供 input-method-v2、virtual-keyboard-v1；不能把用户正在使用的输入法会话当作测试环境。软件生命周期测试需要可见表面的 frame callbacks。

## 共享控件与 IME 事务验证

新增的4个集成场景覆盖路由/焦点生命周期、按钮跨输入源的 capture/取消、TextField 编辑/失焦/禁用/只读，以及原子 IME 的验证、选区、历史和周边文字。workspace all-features 共18个普通集成场景通过；两个需要隔离 compositor 的原生场景也显式运行通过，新增验证包含有/无 surrounding 的会话切换与已排队旧更新取消。没有 src 内测试。

私有 Sway/Pixman 上的组合示例验证了 Tab/Shift+Tab、Space 激活、修改另一控件的应用回调、键盘输入、按钮拖出释放取消及重新点击激活；18帧的有输入探针观察到预期两次激活。截图已目视检查。另一个外部临时探针检查12996组 IME 编辑/取消/撤销/重做组合和1486个周边文字摘取；不把探针保留为膨胀的测试套件，也不代替真实输入法验收。

- all-features/all-targets 检查、严格 Rustdoc、格式和 diff 检查通过。
- 实现8107行，测试1795行，占18.13%（不含 examples）；最大源文件483行。
- 新 release 组合示例4520216 B，比上一阶段原生单字段示例增加147792 B；增加了控件树/Taffy、路由/焦点和按钮/文本框行为，不能当作同场景性能对比。动态依赖仍为 libxkbcommon、libgcc_s、libm、libc 和 loader。
- release 组合示例3秒无输入运行呈现2帧，字形图像11629 B / 121项；这是短时无持续重绘检查，未测量完整空闲 CPU/PSS。
- 本机编译与运行不等于 MSRV、其他操作系统或完整无障碍验收；Clippy 仍未安装。

## Unix 无障碍与文字选择验证

通过 `--features example-accessibility` 在原生 editor 示例中启用 AT-SPI。`aegle-access` 不保存第二份控件树；初次/重新激活全量导出，其后提交语义脏控件，布局与滚动使用绘制的同一逻辑几何。平台线程只排队并使用懒创建的 Wayland wake handle 唤醒 UI；按钮纯 hover/pressed 不再连带语义失效。

私有 Sway/Pixman 与 `dbus-run-session` 上的真实 AT-SPI 方法调用验证：7个系统可查询元素（应用根加6个逻辑节点）的角色/名称/父子关系，CJK 文本及 Unicode scalar 数量，文本框和按钮焦点，选择一个中文字、设置 caret、按钮激活清空文字，以及停用后重新启用时的完整树和焦点恢复。Registry/Status 为最小测试服务，不是真实屏幕阅读器验收；未连接用户桌面。探针还发现并修正了示例 Label 应使用 value 导出名称的映射问题。

- workspace all-features 共20个常规集成场景通过；两个原生 Wayland 场景显式运行通过，含新增后台线程唤醒。all-targets/all-features、默认 access 构建、严格 Rustdoc、fmt/diff 检查通过。
- 实现8450行、测试1974行（18.94%，不含 examples），最大源文件483行。新文本场景覆盖 CJK/组合字符、run 端点、只读、无效/过期选择、预编辑与空文本。临时 RTL/bidi 探针验证12种混排/换行场景中的799个位置，不将其扩张成仓库测试套件。
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
- 实现11240行、测试2230行，占16.56%（不含 examples）；最大源文件483行。没有 src 内测试。

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
- 私有 Sway/Pixman 上运行 release markup_controls，验证 CJK 显示、Tab/Shift+Tab、清空按钮、键盘编辑、深色主题和关闭回调，浅色/深色截图已目视检查。它复用既有输入、IME、绘制与语义实现；本轮未重复真实 IME/AT-SPI 协议探针，不声称新增协议支持。
- 同一工具链/profile、Wayland + 系统字体 + markup、关闭 Unix adapter 的发布文件：hello 与 hello_markup 均为 3,925,184 B，controls 与 markup_controls 均为 3,962,048 B。此样本标记入口没有增加最终文件大小，不代表所有界面均有相同大小，也不是 RAM/CPU 测量。解析器、AST 与 syn/quote 等仅在编译主机运行。
- 实现 12463 行、测试 2441 行，占 16.38%（不含 examples）；最大源文件 483 行。独立复核未发现需修复的问题。MSRV/Clippy/其他 OS/真实输入法候选窗与屏幕阅读器的未验收范围保持不变。

## 局部外观与可复用皮肤验证

默认控件和自定义皮肤使用同一套行为、文本、IME 与语义。`set_skin` 接受无捕获的纯主题/状态函数；Style 是有类型的局部覆盖，按节点放在稀疏表中。已有控件可通过普通 Rust 工厂函数复用外观，不需要注册器或组件宏。局部字号在主题变化及文字替换后保留；字号改变重排同一编辑器，配色改变只覆盖绘制。边框和独立焦点环在控件范围内绘制；容器圆角不隐式裁剪子树。

`.aegle` 支持六/八位颜色字面量及背景、前景、状态背景、边框、圆角、焦点环、选择/caret 和字号属性。复核发现状态属性适用范围与实际行为不一致，已同步收紧 schema 和运行时：hover/focus 限按钮/编辑器，pressed 限按钮，selection/caret 限编辑器。不适用的设置明确拒绝，不接受后悄悄忽略。

- workspace all-features 共 25 个常规集成场景通过；新增一个场景覆盖局部样式、无效值不修改状态、主题/字号/预编辑共存、语义前景、按压/禁用优先级、回调文字替换与字体恢复。原有标记测试加入颜色与适用范围检查，生成代码在真实 Ui 测试中执行；没有逐 setter 测试套件。
- all-targets/all-features、无默认 feature 的应用场景与仅 markup 的 facade 场景、严格 Rustdoc、格式和 diff 检查通过；所有测试在 crates/*/tests。实现 13086 行、测试 2606 行，占 16.61%（不含 examples）；最大源文件 483 行。
- `components` 示例采用 `.aegle` 结构、一个可复用按钮工厂和主题皮肤；私有 Sway/Pixman 上验证 CJK 显示、Tab/Shift+Tab、清空、编辑、深色主题及关闭回调，浅色/深色截图已检查。它只是圆角填色组件示例，不是完整 MD3。当前未新增 OS 协议，也未重复完整原生 IME/AT-SPI 探针。
- 外部小型状态探针验证自定义皮肤悬停/离开时的前景与语义差量一致、离开确实触发重绘，以及后续状态返回无效半径时报错、清除皮肤后可恢复。未扩张正式测试套件。
- Wayland + 系统字体 + markup、无 Unix adapter 的本机 release 文件：hello 为 3,929,280 B（较前阶段增加 4096 B），controls 为 3,962,048 B（不变），components 为 3,982,528 B。没有新发布依赖。大小受链接/对齐影响，不能据此宣称零运行成本。
- 同一800×480私有输出上的 controls 单次快照：RSS/PSS 为28,796/11,526 KiB、1线程；随后3秒 CPU tick 增量为0（CLK_TCK=100）。这只是一份短时进程样本，不是峰值、稳定基准或嵌入式验收。当前目标上的 `size_of`：Appearance 36 B、Style 104 B、VisualState 6 B；不含稀疏表桶、字号/函数指针和 allocator 开销。

## 外观过渡与帧驱动验证

新增独立 `aegle-motion`，没有第三方依赖、定时器或堆分配；Tween 支持标量、Point 与预乘线性 Color，以及四种 easing。软件 renderer 和 motion 共用 types/color-math 的8200 B转换表数据，types 默认仍是 no_std；现有软件合成像素场景保持通过。

app 的可选 motion 维护稀疏过渡策略及活动表，Node 支持目标/呈现查询、中途改目标、取消、完成和清除策略。原生按钮/编辑器默认120ms EaseOut；无窗口 Ui 默认关闭，可手动推进单调时钟。`.aegle` 支持整数毫秒 transition 与 easing，静态设置完成后才安装策略。外观变化不重建 Editor，前景采样同步语义。此次复核修复了减少动态效果丢失最后重绘、失焦轮廓直接消失，以及取消失焦过渡后重新启动的问题。

- workspace all-features 共27个常规集成场景通过。新增两组综合场景覆盖补间端点/有限值/线性透明颜色、改目标连续性、实际焦点轮廓、取消/完成、时钟倒退、隐藏/删除、减少动态效果、CJK 预编辑保留与语义前景；已有标记场景扩展时长和首帧无动画检查。测试全部在 crates/*/tests。
- all-targets/all-features、app/facade 无默认 feature、独立 motion 与 types 默认 no_std 检查、严格 Rustdoc、fmt/diff 检查通过。最后的焦点取消修复后，相关 motion 场景在有/无 accessibility 两种组合重新通过。实现13747行、测试2832行，占17.08%（不含 examples），最大源文件483行。
- 外部小探针在私有 Sway/Pixman 上运行400ms过渡，实测406ms、27次 Wayland frame 请求；全程 app.dispatch(None)，没有后续输入或主动 wake。结束并排空已有平台事件后，600ms等待完整阻塞，新增 frame 请求和皮肤采样均为0。还验证了动画中删除控件、关闭窗口、晚建窗口共享时钟及减少动态效果；不把此桌面协议样本当作硬件帧时保证。
- Wayland + 系统字体 + markup + motion、关闭 Unix adapter 的本机 release：hello 3,949,760 B、controls 3,982,528 B、components 4,003,008 B，分别比前阶段增加20,480 B。没有新增第三方运行依赖，发布配置保持相同。
- 800×480私有输出的 release controls 单次快照：RSS/PSS 28,776/11,450 KiB，1线程，随后3秒 CPU tick 增量0（CLK_TCK=100）。波动范围内的单次样本不能解释为内存改善，也不是峰值、持续动画成本或嵌入式验收。

本阶段没有增加几何动画、完成回调、系统偏好监听或新的原生 IME/AT-SPI 协议。未重复完整真实输入法/屏幕阅读器验收；Clippy、MSRV、其他 OS 与 GPU 的未验证范围保持不变。

## 切换、滑块与进度组件验证

新增复选框、开关、水平滑块与确定进度条，Rust 与 `.aegle` 入口共用相同构造器、行为、主题、过渡和语义。Toggle 复用 Button 生命周期，Slider 与 Progress 共用有限 Range/步长/clamp；段落访问由字号、文字与主题更新共用。没有新增第三方依赖。程序 setter 不触发用户修改回调，互相同步不会形成反馈循环。

- workspace all-features 共29个常规场景通过；新增两组综合场景覆盖范围失败保持旧值、浮点步进/最大端点、多指针与捕获取消、键盘/语义同状态、禁用祖先、回调销毁、CJK 预编辑保持，以及局部 gap 下主题变化的布局失效和极小尺寸焦点绘制。旧标记场景扩展四种 typed handle、乱序范围属性、clamp/step。没有 src 内测试。
- all-features/all-targets、无默认 features 的 app/controls、仅 markup+motion 的 facade 场景、严格 Rustdoc、格式与 diff 检查通过。实现14951行、测试3172行，占17.50%（不含 examples）；最大源文件483行。
- 私有 Sway/Pixman 上的 release widgets 示例验证 checkbox/switch 同步启禁 CJK 字段、键盘输入、滑块同步进度、浅深主题与关闭，截图已检查。根据视觉复核将默认未填充轨道改用主题 border，完成部分4dp、轨道2dp；进度不只靠色相区分。外部4536组小尺寸/空标签/边框/圆角场景检查 scene 构造与裁剪，未扩展正式测试套件。
- 私有 D-Bus 的实际 AT-SPI 查询/写入验证 CheckBox/Checked、Switch→ToggleButton/Pressed、Slider/Progress 的 Value。10..20/step3 的滑块初值10，写15得到16并经回调同步 Progress，写100得到20，写12得到13；祖先禁用后交互不能改变值。Slider 在 Unix 通过 Value/MinimumIncrement 调整，没有独立增减 Action。上游禁用 ProgressIndicator 的 Enabled/Sensitive 状态传播仍有缺口，详见[无障碍](accessibility.md#当前切换与数值控件)，未声称完整系统一致性。
- 相同 release 配置，Wayland + 系统字体 + markup + motion、关闭 Unix adapter：hello 3,962,048 B、controls 3,994,816 B、components 4,015,296 B，较上一阶段各增加12,288 B；新 widgets 为4,019,392 B。
- widgets 在800×480私有输出的单次快照：RSS/PSS 28,532/11,315 KiB、1线程，随后3秒 CPU tick 增量0（CLK_TCK=100）。它不是100控件基准，且与之前示例内容不同，不能当作内存下降证明；不包含 Unix worker，未进行嵌入式或跨平台性能验收。

本阶段实现二态、水平数值控件；三态、竖向、无限进度、滚轮调值、值/手柄位移动画未实现。MSRV/Clippy/其他 OS、GPU、真实屏幕阅读器与输入法完整验收的缺口仍保留。

## 保留滚动容器与跨节点裁剪验证

新增 ScrollView 和同名静态标记节点，复用 Taffy 的滚动范围、Element 既有偏移和软件 renderer 的裁剪 mask，没有新增依赖或 crate。纯滚动更新窗口几何及祖先 clip，复用局部 scene 与文字布局；横纵滚轮剩余量从编辑器/内层视口传到外层。控件命中、捕获释放、静止指针悬停、焦点揭示、IME 和语义使用同一几何。`visit_scenes` 的第三参数是必须由宿主应用的窗口逻辑裁剪。

- workspace all-features 共30个常规集成场景通过。仅新增一组滚动生命周期场景，覆盖嵌套余量、保留记录、裁掉的捕获按钮、显式 refocus、CJK 组合/候选范围、语义动作、隐藏恢复、删除后范围缩小、横向及有限值边界。现有 renderer 场景追加外部/内部 clip 相交、文字、预算、空/非法 clip 和调用间隔离；控件场景补悬停更新不改滑块值。
- all-targets/all-features、无默认功能的滚动场景、markup+motion 无窗口 facade 场景及严格 Rustdoc/格式/diff 检查通过。实现15647行、测试3436行，占18.01%（不含 examples），最大源文件483行，没有 src 内测试。
- 外部 Taffy 探针确认 padding、嵌套滚动溢出隔离、隐藏恢复；AccessKit schema/consumer 探针验证嵌套语义边界等于 Ui 几何、四方向 Item/Page、SetScrollOffset、普通标签滚入、禁用/无效数据拒绝及 IME 组合保持。此阶段未重复真实 AT-SPI 滚动或真人输入法验收。
- 测试发现并修复两处边界：内部滚动的 TextArea/自裁剪控件使用 Taffy Hidden overflow，避免其文字扩大外层内容范围；完全离屏的候选锚点依次夹到控件、祖先和窗口，保持组合而不返回视口外坐标。可完整容纳的编辑器整体滚入，过大的轴以 caret 为准。
- 私有800×640 Sway/Pixman 中，release scrolling 通过真实 virtual-pointer 轴事件、Tab 连续滚入内外视口、顶部/末尾回调及关闭；滚动前后截图已目视检查。初次仅使用 swaymsg 的测试未产生 pointer capability，改用独立虚拟指针后轴输入生效，未连接用户桌面。
- 同一 release 配置、Wayland + 系统字体 + markup + motion、关闭 Unix adapter：hello 3,970,240 B、controls 4,007,104 B、components 4,023,488 B、widgets 4,027,584 B，较上一阶段分别增加8192/12288/8192/8192 B；新 scrolling 为4,019,392 B。
- scrolling 在800×480私有输出的一次进程快照：RSS/PSS 29,364/10,292 KiB，1线程，随后3秒 CPU tick 增量0（CLK_TCK=100）。示例内容、输出和系统映射均影响该样本；这不是对比改善、峰值、100控件、默认无障碍或嵌入式验收。

此阶段没有滚动条、惯性/触摸、独立滚动容器键盘导航或列表虚拟化。所有离屏控件仍保留。系统无障碍离屏过滤、ScrollHint/ScrollToPoint 和 HiDPI 验收限制见[无障碍](accessibility.md#当前滚动语义)，其他平台/GPU仍未实现。

## Vulkan 离屏几何验证

新增独立 `aegle-render-vulkan`，消费已有 Scene，尚未接入 App。矩形/圆角/居中描边、仿射与八层裁剪由 GPU 光栅化，CPU 仅生成有界记录；没有上传软件栅格化的整帧。采用 ash 0.38、bytemuck 1.25 和构建期 Naga 30，不引入 wgpu。两遍 GPU 绘制保留线性混合与预乘 sRGB 输出语义，接口与预算见 [Vulkan](vulkan.md)。

- workspace all-features 的30个常规集成场景、all-targets/all-features、独立无文字 Vulkan library 检查、严格 Rustdoc 和格式检查通过。新增一个默认 ignored 的综合 Vulkan 场景，覆盖混合/裁剪/仿射/作用域、预算、失败帧拒绝提交和恢复，以及 resize/释放/重建，没有 src 内测试。实现17500行、测试3611行，占17.10%（不含 examples/build.rs）；最大源文件483行。
- 同一场景分别在 AMD Radeon RX 6800 XT（RADV NAVI21）和 llvmpipe（LLVM 22.1.8）上显式通过。Khronos validation layer 1.4.363 仅解压至临时目录；两次均确认 loader 实际插入该层并启用同步验证，没有 VUID、Validation Error/Warning 或 SYNC-HAZARD。Lavapipe 属于软件 ICD，单列记录，不当作硬件加速证据。
- Lavapipe 上的临时像素探针验证0.2px细矩形、小数外部 clip、八层重复 clip、2730个避开抗锯齿边缘的旋转/反射/斜切采样，以及局部单位与设备缩放为 `1e±19` 的场景。探针发现并修复局部距离平方溢出与固定 AA 阈值失真：上传前将局部原点/长度等比归一化。记录预算320 B下的一条 clip + 一条 draw 也通过，空余容量可在另一个向量需要空间时回收；未增加另一套正式测试文件。
- 硬件上运行800×480 release geometry 示例并检查生成图片。发布可执行文件512,872 B，包含示例 PPM 写出及 SPIR-V；不含 Vulkan loader、驱动或其依赖，也不包含文本/窗口/App。normal 依赖没有字体栈或运行时 shader 编译器，bytemuck 的 derive 过程宏同样只在编译期运行。Vulkan loader 通过动态加载打开，不能仅凭 ldd 没列出它就声称无系统依赖。
- 临时 release 成本探针在 RX 6800 XT 上一次构造100个不透明圆角矩形，800×480；初始化15.115 ms、首帧1.330 ms，预热20帧后300帧共29.829 ms，平均0.099 ms、P95 0.102 ms。帧样本计量 begin/draw/finish/wait 的主机 wall time，包含提交及 fence 等待，没有逐帧读回；不是 GPU timestamp、窗口呈现或嵌入式帧时保证。
- 该硬件探针计时前后显式设备分配均5,529,664 B，CPU记录capacity均16,384 B；最后一次读回耗时3.692 ms（含首次 staging 分配/copy/wait/map），分配随后为7,065,664 B。相同探针的 Lavapipe 单次样本为平均1.363 ms、P95 1.686 ms，读回前/后分配4,608,064/6,144,064 B。差异来自驱动的实际分配要求；这些数字不包括调用方像素Vec、驱动内部对象及映射，不是进程 RAM/PSS，也不能证明达到资源目标。

本阶段没有字形图集、native surface/swapchain、GPU App、通用图像或路径绘制；没有改动现有 Wayland 输入、IME 与无障碍协议，因此未重复原生桌面验收。MSRV、Clippy、其他 OS/GPU/嵌入式设备及真实输入法/屏幕阅读器的完整验收仍缺。

## Vulkan 按需文字与共享光栅策略验证

新增可选 `aegle-render-vulkan/text`，与几何按相同记录顺序绘制，同用变换和八层裁剪。R8灰度与RGBA8_SRGB彩色页按需分配；每字形透明边、页LRU、当前帧固定、取消脏页回滚和独立上传上限保证不会用尚未上传或已被覆盖的条目。GPU缓存使用完整GlyphKey；软件和GPU共用RasterTransform，减少两个后端之间的字号/相位/仿射差异。没有新增第三方包；复用已有glyph/hashbrown依赖，默认几何构建无字体栈，text的normal依赖无Parley/Naga。

- 全workspace all-features仍为30个常规场景；all-targets/all-features、Vulkan无默认feature、glyph/software有/无feature场景、严格Rustdoc和格式检查通过。新增一个224行ignored文字综合场景，仅扩展既有glyph场景验证共享key/变换。实现18886行、测试3890行，占17.08%（不含examples/build.rs），最大源文件489行。
- RX 6800 XT与Lavapipe分别通过几何和文字综合场景，并开启Khronos层和同步验证，无Vulkan验证错误/警告。文字场景验证CJK/quarterphase、透明颜色、COLRv0/PNG字形、过滤/反射/旋转/clip、几何文字穿插顺序，以及CPU缓存仅一个条目时的GPU命中、取消帧、整页淘汰、预算/超大字形错误、resize和释放重建；与软件逐通道对照容差3。debug下未启用中日词典的既有ICU诊断仍会出现，不代表字形或Vulkan失败。
- 独立复核的临时小探针发现完全裁掉的字形仍因解析几何AA外扩而占用图集，单条目配置错误返回AtlasFull。修复后轴向clip使用真实像素覆盖边界，字形只保留自身过滤支持范围；同一探针及正式文字场景都验证一条目可绘制唯一可见字形。页数上限同时为目标/clip/upload/readback预留五个Vk内存分配；上传缓冲在fence完成即释放，避免只等下一帧。
- 800×480 release text_scene已在RX硬件运行并检查图片，包含CJK三语、裁剪、四相位和仿射文字；库不内嵌字体，示例使用有OFL许可的测试子集。当前App仍软件呈现，本阶段未改窗口/IME/无障碍协议，未重复真人输入法、原生桌面或屏幕阅读器验收。
- 相同示例场景的临时release成本探针：scene/font准备0.220 ms、renderer初始化14.317 ms、首帧提交及wait为1.911 ms；预热20帧后300帧的begin/draw/finish/wait平均0.166 ms、P95 0.218 ms，无逐帧读回。首次末帧读回另为4.287 ms。它是单次桌面GPU主机wall-time样本，不是GPU timestamp、窗口延迟或嵌入式性能保证。
- 该探针计时前后显式设备分配均5,791,872 B（含1张262,144 B字形页），绘制/clip容量33,792 B；127个图集条目，CPU上传capacity40,960 B、128个region槽、CPU字形像素29,004 B。等待后staging为0，首次读回后设备分配7,327,872 B。空字形及被裁掉未入图集字形仍会查CPU缓存，raster_requests在300帧中由547增至6547；未发生图集上传，不将此计数误称为重新光栅化。所有计量均不含完整driver/font/shaping/allocator成本，不能当作PSS或资源目标已达标。

## Vulkan 窗口、App 与 Windows 接入

本轮按用户限定并行完成 Vulkan WSI、App/Wayland 集成、Win32 平台，未开展 macOS。App 的同一 retained Ui 复用两种 renderer，文字、滚动裁剪、输入、IME、主题、动画与语义不另建状态。默认保留软件组合；native,vulkan 可单独构建，不带 tiny-skia。Windows adapter 在隐藏 HWND 上安装，DPI 通过语义根变换与 IMM 光标矩形共同同步。

- Linux workspace 全 features 的31个常规场景通过；all-targets 检查、严格 Rustdoc、格式/diff 检查通过。首轮同时链接多个大型 debug 示例耗尽可用内存，限制为2个构建任务后通过；这不是运行路径的内存样本。
- 新增一项默认 ignored 的 Vulkan 窗口生命周期场景，分别在 Lavapipe 与 RX 6800 XT（RADV NAVI21）上通过。私有 Sway/Pixman 或 GLES2 compositor，不连接用户桌面；开启 Khronos validation 1.4.363 与同步检查，无验证错误。覆盖呈现、resize、弃帧/失败帧拒绝、zero extent、release/recreate、SHM占用为0及原生租约销毁。
- App 既有双窗口 native 场景在上述软/硬 Vulkan ICD 通过：CJK、回调关闭、递归 dispatch 拒绝、旧句柄失效及最后窗口退出。Vulkan-only release scrolling 在 RX 硬件通过真实滚轮、Tab 焦点显露、滚动按钮与关闭，截图目视确认中文/日文/韩文和嵌套裁剪；没有验证层诊断。当前二进制4,028,032 B（native,vulkan,system-fonts,markup，不含系统库、驱动、adapter或motion），不是默认desktop体积。
- 原生800×480硬件探针显示显式设备分配3,686,464 B，swapchain估计6,144,000 B（驱动返回4图）；Lavapipe显式3,072,064 B、相同WSI估计。此为简单几何探针，无整帧readback；WSI实际驱动内存、字体、UIA及进程PSS不包括其中，不代表嵌入式预算达标。
- Windows x86_64-pc-windows-gnu：Rust 1.96.1 + 同版本 Debian rust-src、临时提取的 MinGW交叉构建，facade全features/all-targets检查与App/平台native测试可执行文件链接通过，Win32严格Rustdoc通过；源文件与工具链未安装进系统。Rust标准库通过 -Z build-std 验证当前工具链，未据此宣称MSRV1.88通过。真实Windows11/ARM64仍待验收。


- Wine10 + 私有 Xvfb 兼容环境执行 Win32 native 综合场景通过：隐藏创建、软件像素、Unicode代理对、IMM上下文启停、resize、持续redraw期间消息公平性、两窗关闭/租约寿命及wake。App同一个双窗口场景分别以软件、Vulkan（Lavapipe经Wine Win32 WSI）运行通过；UIA adapter安装/销毁已实际执行，但没有系统客户端文本/动作查询验收。这是兼容层及软件ICD证据。该Xvfb不支持硬件RADV所需DRI3，硬件Windows/Wine呈现未验证；库本身没有新增X11后端。
- 实现21777行、测试4153行，占16.02%（不含examples/build.rs）；最大源文件489行，测试都在crate的tests目录。新增小型Win32边界/native场景与一个Vulkan native场景，App沿用已有native场景切换平台/renderer。

## 当前进度与剩余工作

本轮限定范围的实现与证据见上节；此前暂停的完整 GUI 目标仍未完成。本轮收尾后不自动开始 macOS 或其他里程碑。

- 平台验收：Windows 真实 IME/UIA/硬件 Vulkan 与 ARM64、TSF text store/重转换/触屏键盘；macOS AppKit/Metal；Wayland 触摸、fractional scale、系统偏好与客户端装饰。
- 组件/绘制：惯性、列表虚拟化、自定义 painter 扩展、更多基础组件与布局属性、通用图像/路径/特效；后台 UiProxy。
- 标记语言：目前只有静态结构/字面量与Rust回调，state/绑定/事件块/条件/列表/组件导入/运行时加载仍缺。
- 主题/动画：完整token/局部继承、系统偏好、几何动画与完成回调。
- 文字/无障碍：Unix adapter 的上游 EditableText 等限制；真实屏幕阅读器与候选窗验收；合成粗体/斜体、COLRv1/SVG字形明确不支持。
- 工程验收：MSRV1.88、Clippy、多compositor/GPU与嵌入式完整资源测量；GPU多窗口共享设备/图集尚未实现。现有桌面样本不能替代这些证据。

## 直角默认控件与覆盖式滚动条

默认主题圆角改为 0，复选框、开关、滑块、进度条与按钮/编辑器统一使用主题圆角，因此默认全部直角；正的主题 radius 仍统一圆化。ScrollView 与多行编辑器新增覆盖式滚动条，不占布局、不新增依赖或计时器。视口的滑块保存在自身单独记录中，`visit_scenes` 按后序子树终点在子树之后输出，滚动只重录该小记录，不重录子控件或重排文字。拖动在窗口坐标中处理，经同一 `scroll_to`/几何更新路径同步裁剪、命中、IME 与语义。

- workspace all-features 测试通过；新增一个滚动条场景覆盖最上层记录、拖到两端、拖动不激活下方按钮，以及不溢出时指针归还给控件。Vulkan 与软件后端复用同一 Scene，未新增 renderer 路径；本阶段未做真人窗口拖动验收。

## 文字清晰度与 Vulkan 低功耗路径

- 字形：hinting 的轴向文字基线取整到设备像素，水平保留四分之一相位；横笔不再因小数基线跨两行发虚，每字形相位变体由 16 个降为 4 个。灰度覆盖率经共用 `aegle_glyph::mask_contrast` 曲线 `c + c(1-c)k`，k 由前景亮度给出：深色字加重、浅色字减轻，软件整数实现与 Vulkan shader 同式。浅/深底 12–20px 中英文样张在两个 renderer 上目视检查；GPU/软件 ±3 级对比测试通过。整像素基线下空格等无像素字形不驻留图集，每帧回到 CPU 缓存命中，测试已按此更新。
- Vulkan 批处理：图元记录改为 storage buffer，相邻且 pipeline/图集页相同的图元合并为一次 instanced draw，移除逐图元 push constant 与 scissor。release 离屏 800×480 探针（2000 矩形 + 30 行中英文，约 3800 图元，预热 40 帧后 300 帧，主机 wall time 含 fence 等待）：RX 6800 XT 平均 1.412 → 0.448 ms，Lavapipe 23.894 → 4.198 ms；两种驱动的读回像素与改动前逐字节相同。图元缓冲按每图元 112 B 计入设备预算。
- 直接 sRGB 窗口：不透明窗口默认直接在 BGRA8/RGBA8_SRGB swapchain 上绘制，没有 RGBA16F 目标与编码 pass；`Options::transparent` 保留原透明路径。私有 headless Sway（RADV 用 GLES2，Lavapipe 用 Pixman）同一场景 300 帧：设备分配 RADV 4,350,736 → 664,336 B、Lavapipe 3,582,736 → 664,336 B；每帧工作时间 RADV 0.579 → 0.555 ms、Lavapipe 4.441 → 3.918 ms；Lavapipe 进程 CPU 每帧 30.5 → 26.2 ms（含其光栅线程），RADV 均为 0.433 ms。两条路径截图最大通道差 1 级，占 2.1% 通道。
- 未指定设备时改为优先集成 GPU；本机没有集成 GPU，未实测该选择。Vulkan 窗口生命周期场景在 RADV 与 Lavapipe 上通过；Vulkan 版 scrolling 示例目视确认直角控件、覆盖式滚动条与 CJK 文字。本轮机器未安装 Khronos validation layer，没有验证层诊断。以上为桌面样本，不是嵌入式功耗或帧时保证。

## 剪贴板、密码编辑与 layer-shell

- 剪贴板：TextField 的 Ctrl+C/X/V 只产生请求，App 在刷新前交给平台，不新增依赖或线程。Wayland 每 seat 一个 data device，选区数据以一份 `Rc<[u8]>` 保留到被其他客户端取代，写出/读取都是 4 KiB 一次的 calloop 非阻塞管道，读取上限 4 MiB；Win32 同步读写 `CF_UNICODETEXT`。
- 密码：`set_password`/标记 `password` 让 PlainEditor 只排版 `•`，明文仅存一份；无撤销历史、拒绝 IME 组合与复制/剪切，语义导出 `PasswordInput` 与遮盖值。测试字体按重建脚本补入 U+2022。
- layer-shell：`WindowOptions::layer` 在同一窗口表中创建 wlr layer 表面，复用输入、IME、SHM/Vulkan 呈现和帧门控；compositor 缺少协议时创建报错。不另设 shell crate。
- 验证：workspace 全 features 测试通过；ui 场景覆盖剪切/粘贴（换行剥离、独立撤销）、密码输入拒绝复制与 IME 且语义树不含明文。私有 Pixman Sway 上 Wayland native 场景验证 TOP|LEFT|RIGHT 锚定面板拉伸到输出宽度、高度 96，ime 场景以真实键盘 serial 设置 180,000 B CJK 选区并经非阻塞管道读回一致。wtype 驱动的应用级探针在软件与 Vulkan（Lavapipe）下完成跨编辑器复制/粘贴、密码框拒绝复制后粘贴仍为原值，截图确认 layer 面板与遮盖显示。Wine10 + 私有 Xvfb 执行 Win32 native 场景的 UTF-16 代理对剪贴板往返；Wine 下所有者窗口的剪贴板消息会先于已置位的 wake 被派发，wake 在下一次 dispatch 送达，测试按此接受。真实 Windows 与 GNOME 等无 layer-shell 的 compositor 未验收。
- 既有 Wayland native 场景在平铺 Sway 下会因窗口被放大超过 2 MiB SHM 预算失败（未改动的 HEAD 同样复现）；私有 Sway 以浮动规则运行，未改测试预算。
