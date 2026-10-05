# 实现状态

完整目标保持不变：模块化保留模式 GUI、GPU 与无 GPU 软件绘制、CJK/原生 IME、无障碍、组件/主题/动画、命令式与标记入口、跨平台及 API 文档。

## 已实现

- Rust 2024 workspace，LGPL-3.0-only；目标 MSRV 1.88，实际验证工具链为 1.96.1。
- aegle-types：no_std 几何与紧凑 RGBA 颜色，无第三方依赖。
- aegle-core：代数 ID、可复用槽位、保留树、索引子节点、结构变更与三通道失效，另有路由快照、默认动作控制及策略化焦点遍历；无第三方依赖。叶节点不分配子节点数组，删除不递归。
- aegle-layout：Taffy 0.14.0 直接适配同一棵保留树，无第二份拓扑；共享默认样式、测量缓存、Flex/Block、可选 Grid。它不依赖字体、窗口或 renderer。
- aegle-scene：no_std + alloc 的局部绘制记录；实色矩形、圆角、居中边框、仿射变换、嵌套裁剪。可选 text 保存定位字形、变体坐标与共享字体句柄，不依赖排版器或栅格器。
- aegle-text：复用 Parley/Fontique 的 Unicode shaping、字体选择、回退、换行与定位；Paragraph 保留文字和排版结果，宽度变化只重排，颜色覆盖无需重新 shaping。显式字体为默认，system-fonts、text-dictionary、text-a11y、scene 独立选用；缺字与无可用字体分别报告。
- Editor：复用同一 TextSystem 的 Parley PlainEditor，提供单/多行、选择/命中、视觉移动、grapheme 删除、精确 UTF-8 替换、只读、组合输入模型及有界差量撤销/重做。预编辑只保留被替换片段，提交值不随预编辑改变；取消恢复原选区。文字、装饰和候选区域来自同一布局。apply_ime 在完整验证后应用删除/提交/预编辑事务，删除与提交合成一次撤销；surrounding 无分配地借出有界周边文字。编辑、宽度和样式改变目前仍会重新 shaping，不宣称增量编辑引擎。
- aegle-controls：无皮肤 Button 的键盘/指针/语义激活、capture 和取消状态；可选 text 提供复用 Editor 的 TextField。宿主拥有树、命中和焦点；controls 默认只依赖 types，不依赖窗口或 renderer。
- aegle-access：平台回调经 Mailbox/Handlers 排队并唤醒 UI；可选 UnixAdapter 复用 AccessKit AT-SPI。示例从同一控件树按脏标记导出语义，系统 Focus/Click/SetTextSelection 回到同一焦点、按钮和 Editor。text-a11y 提供文字 run 与有校验的选择转换；不是完整跨平台无障碍。
- aegle-glyph：复用 Swash/Skrifa，按需生成灰度字形与 COLRv0/嵌入位图，LRU 同时约束图像字节和条目数；缓存不持有字体文件。PNG 位图使用有解码预算的 png crate；库不内嵌字体。
- aegle-render-software：借用 RGBA8 缓冲，tiny-skia 负责抗锯齿覆盖率，线性光 SourceOver 合成器处理透明颜色。默认仅几何；可选 text 接同一 Scene 的字形、变换和裁剪。支持均匀缩放的四分之一像素定位及任意可逆仿射变换的双线性采样，无裁剪文字无需面大小的 mask。

- aegle-platform-wayland：一个连接上的多个 xdg-shell 窗口、整数缩放、事件等待、键盘/指针输入、光标与 text-input-v3；软件绘制直接借用最多两块有界 SHM 映射。平台不依赖文字/scene/renderer，原生示例把这些模块接到同一控件树和 Editor。尚无 layer-shell、触摸、剪贴板、平台偏好、客户端装饰或 GPU 原生句柄。
- aegle-theme：无分配的有类型配色/尺寸，浅色、深色与显式高对比主题；当前没有 token 注册表、局部主题继承或系统偏好监听。
- aegle-app 与 aegle：无窗口 Ui 和可选 Wayland 软件应用宿主，命令式 row/column/text/button/text_field/text_area、弱句柄、布局 setter、可替换回调及主题切换。每窗口独立树，应用共享字体和 renderer；可选语义能力已接到原生循环。

图像缓存预算不包括字体映射、排版、缓存索引及上游栅格 scratch；具体边界见 [资源](resources.md)。合成/过滤使用线性预乘颜色，公共字形彩色图像为非预乘 sRGB RGBA8。

## 可运行的组合

```sh
cargo run -p aegle --example hello --release
cargo run -p aegle --example controls --release
cargo run -p aegle-layout --example retained --release
cargo run -p aegle-render-software --example software_scene --release
cargo run -p aegle-render-software --features text --example text_scene --release
cargo run -p aegle-render-software --features text --example editor_scene --release
cargo run -p aegle-platform-wayland --example editor --release
```

hello 是 7 行 Rust 加 1 行文档注释的完整应用，controls 演示跨控件回调、CJK 编辑、主题和关闭窗口；两者使用系统字体。layout 与三个 renderer 示例没有窗口。形状示例将保留树的 Taffy 结果接到局部 Scene；首次建立 12 个记录，仅改按钮背景时重建 1 个记录，输出 `target/aegle-software.png`。

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

## 下一阶段与缺口

下一步在已经可用的应用入口上接入编译型标记界面和可复用组件扩展，补齐动画与基础控件；继续完成其他平台、Vulkan/Metal 和无障碍缺口。当前 facade 仅交付 Linux Wayland 软件组合，不能视为整个项目完成。

当前尚无系统剪贴板、密码编辑、其他平台无障碍 adapter、GPU renderer、通用图像命令、动画或 DSL。基础主题已实现，完整 token/局部继承与系统偏好仍缺。Unix adapter 当前为部分支持，text-a11y 已连接文字导出与选择；合成粗体/斜体、COLRv1、SVG 字形显式报错。跨节点祖先裁剪、通用组件插入/自定义皮肤、更多布局属性、原生双击计数与后台 UiProxy 仍待接入。设计文档是目标，不能当作实现证据。
