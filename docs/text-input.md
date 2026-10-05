# 文本、CJK 与 IME

状态：段落显示、CJK 排版、按需字形软件绘制、保留式纯文本编辑及 Wayland 原生窗口/text-input-v3 已实现。共享 TextField 行为与原生编辑示例连接了这些模块；通用应用/皮肤层、Windows/macOS 输入、剪贴板和密码控件仍未实现，Unix 无障碍已接入部分查询与选择能力。使用[兼容版本组](dependencies.md)中的 Parley、Fontique、HarfRust、Swash；编辑复用 PlainEditor，不从零重写 shaping、bidi 或选择逻辑。

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

`EditorDriver::apply_ime(ImeEdit)` 是共享的 UTF-8 事务入口：先验证提交、预编辑、两侧删除及长度，再修改文字、历史和失效标记，失败不产生部分编辑。两侧删除与提交合为一次撤销；单纯续写预编辑保留原选区，预编辑期间的两侧删除仍是可撤销的已提交变化。协议空重置移除旧预编辑及其被替换选区，与恢复原内容的显式 `cancel_preedit` 不同。

文档状态和原生会话仍分别管理；TextField 在失焦和必要手动编辑时取消组合，并通过 `Outcome::reset_ime` 要求宿主重置平台会话。未来平台的 UTF-16 等单位须先转换为合法 UTF-8 范围，再进入同一事务接口。

密码模式、系统剪贴板及辅助技术编辑动作仍待集成。密码内容不得进入检查树、日志或普通剪贴板复制；系统语义须遵守受保护文本模式。外部辅助技术的选择/编辑动作将走同一编辑模型。

## 当前 Wayland 原生接口

`aegle-platform-wayland` 在窗口使用的同一连接、队列和事件循环上，为每个 seat 创建一个 text-input-v3 对象，当前绑定协议版本 1。`Wayland::configure_ime(window, Some(ImeRequest))` 保存该窗口所聚焦编辑控件的状态；`None` 结束其会话。平台模块不依赖文字引擎或 renderer。`Event::Ime` 携带窗口、seat 和 `Entered/Left/Update`，序号与原生会话按 seat 独立维护；应用层仍需决定多 seat 对控件焦点和同一编辑器的操作规则。

`ImeRequest` 包含 surrounding、光标/anchor 字节偏移、候选窗矩形、内容提示、用途及变更原因。接口校验 surrounding 最多 4000 UTF-8 字节且无 NUL、两个偏移均在字符边界，矩形有限且尺寸非负，向外取整后可用协议的 i32 坐标表示；版本 2 的提示位返回错误。矩形采用 surface 局部逻辑坐标，宿主须先应用布局和滚动变换，不能再乘 buffer scale。文字或选择的外部变化使用 `ImeCause::Other`，来自 IME 的更新使用 `InputMethod`。

`Editor::surrounding(max_bytes)` 提供最多两个无分配借用切片，包含完整选区及相对字节偏移；组合期间同时排除预编辑和原被替换片段，将选区折叠为光标。选区本身超预算则返回 None，不能截断选区后伪造偏移。Wayland `ImeRequest::surrounding = None` 表示支持组合但不支持周边文字，此时 cursor/anchor 必须为 0。Some/None 切换通过 disable/enable 更新该协议能力；只有原生 API 要求连续字符串时才分配有界副本。

### 批次、同步与会话边界

`preedit_string`、`commit_string` 和 `delete_surrounding_text` 暂存至 `done` 后，以一个 `ImeUpdate` 交给宿主；每次交付后清空暂存值。未出现 preedit 的批次表示空预编辑，不能继续显示上一批文字。预编辑光标必须是两个合法 UTF-8 端点，或 `-1/-1` 表示隐藏；不合法的范围产生错误，不悄悄吸附。`commit: None` 和 `Some("")` 分别表示没有提交事件和显式空提交。

宿主按协议顺序处理：移除旧预编辑到光标、删除选区/预编辑两侧要求的字节、插入提交文字、确定 surrounding，再放入新预编辑及其光标。删除长度不包含原选区；平台层没有文档内容，宿主必须在编辑器边界验证长度与 UTF-8 端点。应先处理完已排队的输入批次，再以最终编辑状态调用 `configure_ime`，避免逐条回复中间状态。

同一启用会话中的旧 serial 更新仍须应用。`ImeUpdate::current = false` 时，后端只缓存新的 surrounding 等状态，等待与最新 commit 计数匹配的 `done`；匹配后由宿主应用本批并重新提供最终状态，后端不提前发送旧缓存。计数包含 disable 的 commit，并按 u32 环绕。完全相同的配置不再发送，避免空批次往返产生持续更新。

取消会话是独立的生命周期操作：`configure_ime(None)` 即使在等待匹配 serial 时也立即 disable/commit。同一窗口内切换编辑控件必须先传 `None`，再启用新控件；每次 enable 记录序号边界，取消会话后排队到达的旧批次不会编辑新控件，也不会阻止新会话启用；显式 None 还会删除已分发到应用事件队列但尚未取出的该窗口 Update。`leave` 清除本地焦点和预编辑暂存；下一次 `enter` 先结束仍启用的旧服务端会话，再按当前控件重新启用并完整发送内容类型、surrounding 和矩形。无编辑控件的窗口保持禁用。

### 当前集成与验证边界

`cargo run -p aegle-platform-wayland --example editor --release` 使用同一 Tree/Taffy 保存 TextField、Button 和文字标签。Route/Focus 处理路由及 Tab 顺序，controls 处理按键、选择、capture 和 IME，保留 scene/software renderer 显示 CJK 与焦点状态；按钮回调可以修改文本框。`controls_support` 只负责示例组装、原生归一化及绘制，不另建编辑事务。示例为活动键盘 seat 设置单一逻辑焦点域；多 seat 产品策略尚未作为通用 App API 交付。示例字体仅覆盖有限清单，正式应用应配置所需字体。

已通过隔离 Sway 中的原生协议验证：测试输入法使用 input-method-v2，经真实 compositor 将 CJK 预编辑、提交、周边删除和批次重置传递给本库的 text-input-v3；还验证了同会话旧 serial 延迟同步、焦点往返，以及取消后延迟批次与新会话的隔离。测试程序模拟输入法协议端点，不是 fcitx/IBus 用户操作验收；真实输入法切换、候选列表交互、复杂组合和桌面集成仍待端到端验证。详细运行环境与证据见[实现状态](implementation.md)。

## 平台和无障碍衔接

Wayland 已接入上述 text-input-v3；Windows 的 TSF/兼容路径及 macOS 的 NSTextInputClient 仍为待实现目标。文字引擎不代替这些平台协议。候选窗采用当前呈现几何，主题、缩放或动画更新时同步，不重建编辑器。当前 app 的局部字号改变重排同一 Editor，保留组合和选择；皮肤/局部配色只覆盖绘制，不重启 IME 会话。动画几何仍未实现。

`text-a11y` 提供 `EditorDriver::accessibility` 导出显示文字 run、几何、预编辑下划线和选区；`select_accessibility` 校验最近导出的身份/cluster 索引并作用于同一个 Editor。布局重建后旧映射无效；活动组合期间明确返回 CompositionActive，不能把显示范围直接套到取消后的已提交值。Unix 示例通过独立 aegle-access 接到 AT-SPI 查询、选择和焦点，当前上游缺少 EditableText，不能声称支持辅助技术文字替换；详见[无障碍边界](accessibility.md)。

平台缺少 IME 协议时报告 ImeUnavailable 并保留基础键盘输入；要求组合输入的应用可以将其设为启动必需能力。正式 CJK/IME 验收必须在具备对应协议和真实输入法的环境进行。

来源：[PlainEditor 发布源码](https://docs.rs/crate/parley/0.11.1/source/src/editing/editor.rs)、[Parley analysis](https://docs.rs/crate/parley/0.11.1/source/src/analysis/mod.rs)、[ICU4X CJK 换行说明](https://docs.rs/crate/icu_segmenter/2.3.0/source/src/line.rs)、[text-input-v3 协议](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/unstable/text-input/text-input-unstable-v3.xml)。
