# 文本、CJK 与 IME

状态：段落显示、CJK 排版与按需字形软件绘制已实现；编辑器、平台 IME 和系统无障碍尚未接入。使用[兼容版本组](dependencies.md)中的 Parley、Fontique、HarfRust、Swash；后续编辑复用 PlainEditor，不从零重写 shaping、bidi 或选择逻辑。

## 当前显示接口

`TextSystem` 共享字体集合及 shaping 临时存储；`Paragraph` 拥有 UTF-8 文本和保留的 Parley 布局。`paragraph/update/restyle` 完成字体选择与 shaping；`reflow` 仅换行和对齐，相同约束直接复用结果。`content_widths` 提供 intrinsic 测量值，宿主通过 Taffy 测量回调接入；最终绘制前采用最终布局宽度。Parley 对混合双向文本的 intrinsic 宽度仍有估计限制。

`glyph_runs` 借用已有字形位置；可选 `aegle-text/scene` 提供 `paint`，将位置和变化轴复制到 scene，字体字节通过 `FontData` 共享。`paint_with_color` 可重建前景颜色而不重新 shaping。scene 不持有文字引擎，光栅化模块不依赖 Parley。

`TextDiagnostics` 分别报告缺字 glyph 和没有任何字体可成形的 UTF-8 字节数。前者显示字体的 `.notdef`；后者使 `paint` 返回 `MissingFont`，避免整段文字静默消失。目前 scene 桥接拒绝合成粗体/斜体，应用应提供真实字重/字形。字形支持灰度轮廓、COLRv0、SBIX/CBDT PNG 及原始 BGRA/alpha 位图；COLRv1、SVG 字形表示返回不支持。注册字体不等于该字体的每种表示均可绘制。

## 显示与内存

Fontique 管理字体匹配与按 script/locale 的 fallback；明确区分简繁中文及日、韩语言提示。locale 可由应用覆盖，缺字按配置字体链回退，最终使用缺字占位并提供诊断，不能保证未安装字体覆盖所有字符。

未来 desktop 组合默认使用系统字体，不附带巨大 CJK 字库。当前独立模块的 `TextSystem::new()` 始终创建显式字体集合，不受 Cargo feature 合并影响；启用 `system-fonts` 后可用 `TextSystem::system()` 选择系统发现，Linux 需要 Fontconfig。嵌入式直接注册共享字体字节及 fallback 清单，打包字体的许可、覆盖和体积由发布清单记录。映射字体仍计入相应内存口径。

按实际出现的 glyph、字号、字体变化轴与光栅化参数生成字形。CPU 字形缓存和 GPU 图集分别有界，按最近使用淘汰，在途资源延迟回收；不预烘焙所有 CJK codepoint。布局结果、字体 metadata 和活动编辑状态也分别计量，不用字形上限代表整个文本系统。

当前默认保留基本 CJK 显示、bidi 和 UAX #14 换行，关闭词典分段数据；`text-dictionary` 启用上游 complex-scripts 数据。未来编辑默认提供 grapheme 导航，中文/日文按词导航和双击选词在关闭词典时采用基础边界行为。关闭该数据也影响泰/老/缅/高棉等上下文分段；调试构建可能输出上游缺少分段模型的诊断，不能宣传为全语言完整编辑支持。

## 编辑范围

以下为尚待实现的编辑契约：首版提供单行/多行纯文本、选择、grapheme 移动、基础词导航、剪贴板、只读、密码、撤销/重做及组合输入。不提供富文档编辑器。撤销合并连续输入；默认历史内存上限 1 MiB，超过时丢弃最旧完整操作，不丢失当前文本。

提交内容、IME 预编辑和选区是明确不同状态。预编辑显示可以影响布局但不当作最终 value_changed；提交后才进入普通编辑记录。焦点离开时按平台协商结束组合，销毁时取消会话，不自行重复提交。

内部 UTF-8 范围在字符边界验证；用户导航按 grapheme，平台需要 UTF-16 等单位时显式转换。转换缓存随文本修订号失效。IME 删除周边文字必须验证范围，不能盲目按 UTF-8 字节截断组合字符。

密码内容不进入检查树、日志或普通剪贴板复制；系统语义遵守受保护文本模式。外部辅助技术的选择/编辑动作走同一编辑模型。

## 平台和无障碍衔接

Wayland 接入 text-input-v3，Windows 接入 TSF 和明确的兼容路径，macOS 实现 NSTextInputClient。文字引擎不代替这些平台协议。候选窗采用当前呈现几何，主题、缩放或动画更新时同步，不重建编辑器。

`text-a11y` 当前仅启用 Parley 的可选 AccessKit 布局接口，不自动创建语义树或系统 adapter。后续无障碍模块负责文字布局节点、选择范围及平台 adapter；文字变化和选择变化发送必要通知，纯颜色或装饰动画不制造朗读噪声。

平台缺少 IME 协议时报告 ImeUnavailable 并保留基础键盘输入；要求组合输入的应用可以将其设为启动必需能力。正式 CJK/IME 验收必须在具备对应协议和真实输入法的环境进行。

来源：[PlainEditor 发布源码](https://docs.rs/crate/parley/0.11.1/source/src/editing/editor.rs)、[Parley analysis](https://docs.rs/crate/parley/0.11.1/source/src/analysis/mod.rs)、[ICU4X CJK 换行说明](https://docs.rs/crate/icu_segmenter/2.1.2/source/src/line.rs)。
