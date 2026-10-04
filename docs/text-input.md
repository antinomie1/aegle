# 文本、CJK 与 IME

状态：段落显示、CJK 排版、按需字形软件绘制及保留式纯文本编辑模型已实现；平台 IME、窗口、剪贴板、密码控件和系统无障碍尚未接入。使用[兼容版本组](dependencies.md)中的 Parley、Fontique、HarfRust、Swash；编辑复用 PlainEditor，不从零重写 shaping、bidi 或选择逻辑。

## 当前显示接口

`TextSystem` 共享字体集合及 shaping 临时存储；`Paragraph` 拥有 UTF-8 文本和保留的 Parley 布局。`paragraph/update/restyle` 完成字体选择与 shaping；`reflow` 仅换行和对齐，相同约束直接复用结果。`content_widths` 提供 intrinsic 测量值，宿主通过 Taffy 测量回调接入；最终绘制前采用最终布局宽度。Parley 对混合双向文本的 intrinsic 宽度仍有估计限制。

`glyph_runs` 借用已有字形位置；可选 `aegle-text/scene` 提供 `paint`，将位置和变化轴复制到 scene，字体字节通过 `FontData` 共享。`paint_with_color` 可重建前景颜色而不重新 shaping。scene 不持有文字引擎，光栅化模块不依赖 Parley。

`TextDiagnostics` 分别报告缺字 glyph 和没有任何字体可成形的 UTF-8 字节数。前者显示字体的 `.notdef`；后者使 `paint` 返回 `MissingFont`，避免整段文字静默消失。目前 scene 桥接拒绝合成粗体/斜体，应用应提供真实字重/字形。字形支持灰度轮廓、COLRv0、SBIX/CBDT PNG 及原始 BGRA/alpha 位图；COLRv1、SVG 字形表示返回不支持。注册字体不等于该字体的每种表示均可绘制。

## 显示与内存

Fontique 管理字体匹配与按 script/locale 的 fallback；明确区分简繁中文及日、韩语言提示。locale 可由应用覆盖，缺字按配置字体链回退，最终使用缺字占位并提供诊断，不能保证未安装字体覆盖所有字符。

未来 desktop 组合默认使用系统字体，不附带巨大 CJK 字库。当前独立模块的 `TextSystem::new()` 始终创建显式字体集合，不受 Cargo feature 合并影响；启用 `system-fonts` 后可用 `TextSystem::system()` 选择系统发现，Linux 需要 Fontconfig。嵌入式直接注册共享字体字节及 fallback 清单，打包字体的许可、覆盖和体积由发布清单记录。映射字体仍计入相应内存口径。

按实际出现的 glyph、字号、字体变化轴与光栅化参数生成字形。CPU 字形缓存和 GPU 图集分别有界，按最近使用淘汰，在途资源延迟回收；不预烘焙所有 CJK codepoint。布局结果、字体 metadata 和活动编辑状态也分别计量，不用字形上限代表整个文本系统。

当前默认保留基本 CJK 显示、bidi 和 UAX #14 换行，关闭词典分段数据；`text-dictionary` 启用上游 complex-scripts 数据。编辑器的视觉移动、按词导航和点击选择复用 Parley，删除使用 Unicode extended grapheme 边界。中文/日文按词导航和双击选词在关闭词典时采用基础边界行为；关闭该数据也影响泰/老/缅/高棉等上下文分段。调试构建可能输出上游缺少分段模型的诊断，不能宣传为全语言完整编辑支持。

## 当前编辑接口

`TextSystem::editor` 创建 `Editor`，`TextSystem::edit` 借用短期命令式 driver。已支持单行/多行、方向选区、视觉/词/行移动、点命中选择、grapheme 删除、只读、撤销/重做及预编辑状态。默认单行，拒绝硬换行并禁用软换行；只读保留选择能力，阻止用户编辑并取消预编辑，应用仍可通过 `set_text` 显式替换值。

`Editor::text()` 返回无分配的 `TextValue` 借用视图；需要连续拥有的字符串时才调用 `to_string()`。`display_text()` 包含预编辑，不能充当已提交值。`take_changes()` 合并并取出 value/layout/selection/policy 失效标记，宿主据此更新布局、绘制、平台状态及未来语义树。

内部 UTF-8 范围在字符边界验证。`select` 为用户选择，会按 Parley 的 shaping cluster 吸附；`replace` 用于精确协议替换，保留合法字节端点，不把删除一个组合字符扩大为删除基字符。无选区的 backspace/delete 使用与 Parley 同版本 ICU 的 extended grapheme 分段；它从文本起点扫描，不宣称大型文档常数时间编辑。

历史记录保存替换片段及前后选区，不逐次复制完整文本；连续相邻纯插入可合并。宿主在粘贴、焦点变化或输入停顿时调用 `break_undo_group`，编辑器本身不建计时器。默认历史预算 1 MiB，常规超限淘汰最旧完整操作；单次操作大于预算或禁用历史时仍编辑当前值，但清除旧历史以防错误回放。`set_text` 清空历史，详细计量见[资源](resources.md)。

`scene` feature 提供 `Editor::paint`，依次记录选择背景、字形、预编辑下划线及光标。`EditorPaint` 控制颜色及装饰显隐，前景变色不重新 shaping；聚焦、光标闪烁、滚动和裁剪由宿主负责。绘制、命中、选择矩形及 `ime_rect` 共用同一布局。`ime_rect` 返回未裁剪的局部逻辑坐标，宿主必须施加当前滚动/呈现变换。

当前 PlainEditor 在内容、宽度/对齐或样式变化时重新 shaping；相同 reflow 约束跳过工作，不能套用只读 Paragraph 的仅换行成本。普通替换使用一次重建；端点落在 shaping cluster 内的精确标量替换借助上游 composition API，可能多次重建。未实现增量文档排版或富文档编辑器。

## 组合输入模型

`set_preedit` 改变显示，不改变已提交值或历史。首次组合只保存被替换片段及原方向选区，`TextValue` 通过前缀、原片段、后缀维持稳定提交值；没有第二份完整文档。相对预编辑的光标范围按 UTF-8 验证，`None` 隐藏光标。

`cancel_preedit` 或空预编辑恢复原内容及原选区；`commit` 将结果记为一次撤销操作，空提交表示删除原范围，与取消不同。普通插入、移动、选择、外部替换和撤销要求先明确提交或取消活动组合，避免宿主无意改变输入法会话。预编辑只标记布局/选择变化，实际提交值变化才设置 value 标记。

以上是可被平台驱动的状态模型，不表示输入法协议已接通。后续焦点离开时按平台协商结束组合，销毁时取消会话，不自行重复提交。平台需要 UTF-16 等单位时显式转换并验证范围；周边文字删除不能直接使用未经校验的偏移。

密码模式、系统剪贴板及辅助技术编辑动作仍待集成。密码内容不得进入检查树、日志或普通剪贴板复制；系统语义须遵守受保护文本模式。外部辅助技术的选择/编辑动作将走同一编辑模型。

## 平台和无障碍衔接

Wayland 接入 text-input-v3，Windows 接入 TSF 和明确的兼容路径，macOS 实现 NSTextInputClient。文字引擎不代替这些平台协议。候选窗采用当前呈现几何，主题、缩放或动画更新时同步，不重建编辑器。

`text-a11y` 当前仅启用 Parley 的可选 AccessKit 布局接口，不自动创建语义树或系统 adapter。后续无障碍模块负责文字布局节点、选择范围及平台 adapter；文字变化和选择变化发送必要通知，纯颜色或装饰动画不制造朗读噪声。

平台缺少 IME 协议时报告 ImeUnavailable 并保留基础键盘输入；要求组合输入的应用可以将其设为启动必需能力。正式 CJK/IME 验收必须在具备对应协议和真实输入法的环境进行。

来源：[PlainEditor 发布源码](https://docs.rs/crate/parley/0.11.1/source/src/editing/editor.rs)、[Parley analysis](https://docs.rs/crate/parley/0.11.1/source/src/analysis/mod.rs)、[ICU4X CJK 换行说明](https://docs.rs/crate/icu_segmenter/2.3.0/source/src/line.rs)。
