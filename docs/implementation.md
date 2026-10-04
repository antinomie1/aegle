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
- aegle-glyph：复用 Swash/Skrifa，按需生成灰度字形与 COLRv0/嵌入位图，LRU 同时约束图像字节和条目数；缓存不持有字体文件。PNG 位图使用有解码预算的 png crate；库不内嵌字体。
- aegle-render-software：借用 RGBA8 缓冲，tiny-skia 负责抗锯齿覆盖率，线性光 SourceOver 合成器处理透明颜色。默认仅几何；可选 text 接同一 Scene 的字形、变换和裁剪。支持均匀缩放的四分之一像素定位及任意可逆仿射变换的双线性采样，无裁剪文字无需面大小的 mask。

- aegle-platform-wayland：一个连接上的多个 xdg-shell 窗口、整数缩放、事件等待、键盘/指针输入、光标与 text-input-v3；软件绘制直接借用最多两块有界 SHM 映射。平台不依赖文字/scene/renderer，原生示例把这些模块接到同一控件树和 Editor。尚无 layer-shell、触摸、剪贴板、平台偏好、客户端装饰或 GPU 原生句柄。

图像缓存预算不包括字体映射、排版、缓存索引及上游栅格 scratch；具体边界见 [资源](resources.md)。合成/过滤使用线性预乘颜色，公共字形彩色图像为非预乘 sRGB RGBA8。

## 可运行的组合

```sh
cargo run -p aegle-layout --example retained --release
cargo run -p aegle-render-software --example software_scene --release
cargo run -p aegle-render-software --features text --example text_scene --release
cargo run -p aegle-render-software --features text --example editor_scene --release
cargo run -p aegle-platform-wayland --example editor --release
```

前四个是无窗口示例。形状示例将保留树的 Taffy 结果接到局部 Scene；首次建立 12 个记录，仅改按钮背景时重建 1 个记录，输出 `target/aegle-software.png`。

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

`aegle-platform-wayland --example editor` 是可交互的软件绘制窗口：一棵 Tree/Taffy 树保存文字标签、TextField、Button 和局部 scene，Route/Focus 管理路由及遍历，按钮回调修改文本框。CJK、键盘编辑、拖选、滚动、撤销及 IME 使用共享行为与同一 Editor，绘制直接写 SHM。示例使用有限覆盖的 OFL 测试字体，无系统字体发现成本；它不是未来的10行 facade API，未接系统无障碍或主题组件。

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

## 下一阶段与缺口

下一步把已共享的控件行为、路由和原生输入组成应用层，并接入系统无障碍；随后完成其他平台、Vulkan/Metal、组件、主题、动画、标记语言及发布组合。原生示例已可运行，但完整 GUI facade 仍未交付。

当前尚无系统剪贴板、密码编辑、系统无障碍 adapter、GPU renderer、通用图像命令、主题动画或 DSL。text-a11y 仅启用上游布局能力；合成粗体/斜体、COLRv1、SVG 字形显式报错。跨节点祖先裁剪与绘制组仍待应用层组装。设计文档是目标，不能当作实现证据。
