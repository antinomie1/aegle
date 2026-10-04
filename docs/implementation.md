# 实现状态

完整目标保持不变：模块化保留模式 GUI、GPU 与无 GPU 软件绘制、CJK/原生 IME、无障碍、组件/主题/动画、命令式与标记入口、跨平台及 API 文档。

## 已实现

- Rust 2024 workspace，LGPL-3.0-only；目标 MSRV 1.88，实际验证工具链为 1.96.1。
- aegle-types：no_std 几何与紧凑 RGBA 颜色，无第三方依赖。
- aegle-core：代数 ID、可复用槽位、保留树、索引子节点、结构变更与三通道失效；无第三方依赖。叶节点不分配子节点数组，删除不递归。
- aegle-layout：Taffy 0.14.0 直接适配同一棵保留树，无第二份拓扑；共享默认样式、测量缓存、Flex/Block、可选 Grid。它不依赖字体、窗口或 renderer。
- aegle-scene：no_std + alloc 的局部绘制记录；实色矩形、圆角、居中边框、仿射变换、嵌套裁剪。可选 text 保存定位字形、变体坐标与共享字体句柄，不依赖排版器或栅格器。
- aegle-text：复用 Parley/Fontique 的 Unicode shaping、字体选择、回退、换行与定位；Paragraph 保留文字和排版结果，宽度变化只重排，颜色覆盖无需重新 shaping。显式字体为默认，system-fonts、text-dictionary、text-a11y、scene 独立选用；缺字与无可用字体分别报告。
- Editor：复用同一 TextSystem 的 Parley PlainEditor，提供单/多行、选择/命中、视觉移动、grapheme 删除、精确 UTF-8 替换、只读、组合输入模型及有界差量撤销/重做。预编辑只保留被替换片段，提交值不随预编辑改变；取消恢复原选区。文字、装饰和候选区域来自同一布局。编辑、宽度和样式改变目前仍会重新 shaping，不宣称增量编辑引擎。
- aegle-glyph：复用 Swash/Skrifa，按需生成灰度字形与 COLRv0/嵌入位图，LRU 同时约束图像字节和条目数；缓存不持有字体文件。PNG 位图使用有解码预算的 png crate；库不内嵌字体。
- aegle-render-software：借用 RGBA8 缓冲，tiny-skia 负责抗锯齿覆盖率，线性光 SourceOver 合成器处理透明颜色。默认仅几何；可选 text 接同一 Scene 的字形、变换和裁剪。支持均匀缩放的四分之一像素定位及任意可逆仿射变换的双线性采样，无裁剪文字无需面大小的 mask。

图像缓存预算不包括字体映射、排版、缓存索引及上游栅格 scratch；具体边界见 [资源](resources.md)。合成/过滤使用线性预乘颜色，公共字形彩色图像为非预乘 sRGB RGBA8。

## 可运行的组合

```sh
cargo run -p aegle-layout --example retained --release
cargo run -p aegle-render-software --example software_scene --release
cargo run -p aegle-render-software --features text --example text_scene --release
cargo run -p aegle-render-software --features text --example editor_scene --release
```

这些都是无窗口示例。形状示例将保留树的 Taffy 结果接到局部 Scene；首次建立 12 个记录，仅改按钮背景时重建 1 个记录，输出 `target/aegle-software.png`。

文字示例将 Paragraph 保留在同一棵树的节点中，以 Taffy 测量回调换行，并按最终布局宽度录制字形。真实显示拉丁文字、中文、日文、韩文及裁剪，输出 `target/aegle-text.png`；不是可交互控件或 GUI Hello world。测试字体共约 21 KiB，仅供测试/示例，附 OFL 原始声明和重建脚本。

编辑示例输出 `target/aegle-editor.png`：同一 Editor 先录制选区、预编辑和光标，再提交中文、录制结果，最后撤销/重做并检查已提交值。它以程序调用模拟输入，不代表已经连接真实输入法。

## 本阶段验证

- workspace 默认与 all-features 测试通过；all-features 共 14 个集成场景，全部位于各 crate/tests。新增两个场景覆盖组合取消/提交/撤销、被替换选区与隐藏 caret、组合字符精确撤销和 grapheme 删除、只读与单行边界、历史裁剪、候选区域及装饰像素；原有 CJK、字形预算、合成、布局和树生命周期场景保持通过。
- all-features/all-targets 检查通过，Rustdoc 在 warnings-as-errors 下通过，格式与 diff 检查通过。默认几何 renderer 的 normal dependency tree 不含字体栈或 PNG。
- 实现 4718 行、测试 955 行（16.8%，不含 examples）；最大源文件 418 行，均不超过 500 行。没有 src 内测试。
- Linux Fontconfig 独立探针发现 34 个字体家族；zh-Hans、ja、ko 与混排的四段文字 missing_glyphs/unshaped_bytes 均为 0，使用两种 face。这只验证本机字体环境，不保证其他设备覆盖率。
- 上一阶段的 560×400 文字示例：首次 Taffy 测量 20 次，同约束再次布局为 0 次；字形图像缓存 16908 B / 167 项；Scene 保留分配 5712 B。连续 10 帧像素与字形缓存统计一致；当次 release 10 次暖态离屏重绘为 19.82 ms。
- 当前编辑示例：一次中文替换的历史占用 100 B；字形图像缓存 4026 B / 57 项。以上为对应记录的成本，不包含活动文字/layout、字体、历史容器闲置槽位、renderer 缓冲及进程本身，不代表总 RAM/PSS。
- 本机 release 编辑示例的 100 次短预编辑更新为 551 µs，期间撤销历史为 0 B；最终可执行文件 3359080 B，含示例 PNG 编码和测试字体。这是一次短文本调用观察，不含窗口、真实 IME、绘制或呈现，不代表长文本延迟或嵌入式性能。
- release 示例已运行且图片已目视检查。本机 rustc/LLVM 曾在 ThinLTO 阶段 SIGSEGV；设置 `RUST_MIN_STACK=16777216` 后构建运行通过，未改变仓库 release 配置。
- Clippy 未安装；MSRV、其他平台、实际窗口和 GPU 均未验证。字典 feature 关闭时，上游 ICU 在 debug 下可能记录 CJK 词典缺失；基本显示与换行可用，词语导航仍需单独验收。

## 下一阶段与缺口

下一步连接 Wayland 原生窗口、软件呈现和输入，以 text-input-v3 的事务/焦点时序接入现有编辑器，再接系统无障碍；随后实现其他平台、Vulkan/Metal、组件、主题、动画、标记语言及发布组合。SCTK/calloop 与本机隔离 headless compositor 的条件已调查，尚未进行窗口运行验收。

当前尚无可用 GUI、原生 IME/剪贴板、密码编辑、平台窗口、系统无障碍 adapter、GPU renderer、通用图像命令、主题动画或 DSL。text-a11y 仅启用上游布局能力；合成粗体/斜体、COLRv1、SVG 字形显式报错。跨节点祖先裁剪与绘制组仍待应用层组装。设计文档是目标，不能当作实现证据。
