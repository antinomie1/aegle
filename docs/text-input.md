# 文本、CJK 与 IME

状态：段落显示、CJK 排版、按需字形软件绘制、保留式纯文本编辑及 Wayland 原生窗口/text-input-v3 已实现。文字后端在 Linux（Pango + FreeType，本机运行验证）、Windows（DirectWrite + GDI，仅交叉编译检查）与 macOS（CoreText + CoreGraphics，仅交叉编译检查）三处各有实现，验证程度不同。共享 TextField 行为与原生编辑示例连接了这些模块；通用应用/皮肤层已接入，Windows 经 TSF 文本存储接入输入法（见末节），macOS 输入仍未实现，系统剪贴板与密码模式已接入 Wayland/Win32 应用宿主，Unix 无障碍已接入部分查询与选择能力。

排版引擎默认是**操作系统的文字栈**：Linux 用 Pango（bidi、UAX #14 换行、字体回退）加 Fontconfig 与 FreeType，全部在运行时从发行版的共享库加载；Windows 用 DirectWrite（`backend_directwrite/`），macOS 用 CoreText（`backend_coretext/`）。三者的验证程度不同：Linux 路径在本机运行并通过全部测试与像素核对；Windows 与 macOS 路径只在 Linux 上交叉 `cargo check`/`clippy` 过，**没有在目标系统上运行**（见[实现状态](implementation.md)）。Aegle 自己拥有的是保留段落、光标与选择语义（`document`/`nav`/`edit`）以及字形记录，不重写 shaping、bidi 或换行。`aegle-text/text-parley` 与 `aegle-glyph/swash` 是**默认关闭、不再维护**的回退：只在显式启用时编译 Parley/HarfRust/Skrifa/Swash。三个有系统后端的 target 上它们只作 opt-in；其他 target 没有系统后端，仍默认编译回退实现，避免默认构建静默失去文字。

## 当前显示接口

`TextSystem` 共享系统字形栈的一次性初始化与字体集合；`Paragraph` 拥有 UTF-8 文本和保留的排版结果。`layout` 模块的 `PlatformSystem`/`Layout` 是所有排版调用的唯一入口，按 target 选择系统栈或回退实现。`paragraph/update/restyle` 完成字体选择与排版，段落保留去色的 shaping 键，只改颜色时不重新排版（绘制取段落颜色）；`Paragraph::release` 释放排版并保留文字、样式与约束，`TextSystem::restore` 重新成形，宿主据 `TextSystem::set_idle_budget` 回收未显示的段落（见[资源](resources.md#当前字体与-cpu-字形边界)）；`reflow` 仅换行和对齐，相同约束直接复用结果。`content_widths` 提供 intrinsic 测量值，宿主通过 Taffy 测量回调接入；最终绘制前采用最终布局宽度。段落与编辑器按布局方向左对齐或右对齐（`Node::set_layout_direction`），混排文本的基础方向仍由系统栈按内容检测（Pango 的段落方向）。`Paragraph::first_baseline` 与编辑器的同名方法给出首行基线，文字控件据此按最终尺寸与绘制位置（居中或内边距处）向 Taffy 报告首基线，供 `Align::Baseline`。`LineHeight` 的三种取值都换算成目标行高。Pango 的 `pango_layout_set_line_spacing` 乘的是每行自身高度，因此先按首行高度求出目标倍数（0 表示自然行距），混合字号的行按各自高度缩放，这与逐行固定行高的排印模型有意不同；DirectWrite 用 `DWRITE_LINE_SPACING_METHOD_UNIFORM` 直接设置基线距离，三种取值同样先换算成目标高度（`offsets::line_height`，与平台无关并有单测）；CoreText 用 `CTParagraphStyle` 的最小/最大行高。

`glyph_runs` 借用已有字形位置；可选 `aegle-text/scene` 提供 `paint`，将位置和变化轴复制到 scene。一个 run 的字体身份是后端无关的 `FontSource`：`FontSource::Bytes(FontData)` 是应用/宿主共享的字体字节，`FontSource::System(SystemFont { backend, id })` 是平台后端注册的字体对象，`backend` 区分 DirectWrite（`SystemFont::DWRITE`）与 CoreText（`SystemFont::CORETEXT`），`id` 是该后端注册表的键。scene 只把它当不透明身份，从不保存原生指针；渲染器和图集按它分派。`paint_with_color` 可重建前景颜色而不重新 shaping。scene 不持有文字引擎，光栅化模块不依赖排版库：`aegle-glyph` 按 `FontSource` 分派，`Bytes` 走 Linux 的 FreeType 或 Rust 回退，`System` 走该 backend 注册的 `SystemRasterizer`（Windows 的 GDI、macOS 的 CoreGraphics）。

`TextDiagnostics` 分别报告缺字 glyph 和没有任何字体可成形的 UTF-8 字节数。三个系统栈的缺字表现并不相同：Linux 的 Pango 给出 unknown glyph，它自带占位宽度并由 Pango 自己画成占位框，Aegle 不重复绘制（不换成 `.notdef`，否则字形与 advance 不一致），诊断照常计数；Windows 的 DirectWrite 总会回退，缺字表现为字形索引 0（`.notdef`），DirectWrite 画出的就是该框，因此计数与像素一致，`unshaped_bytes` 只在布局创建失败时统计非换行字节；macOS 的 CoreText 同样总会回退，缺字由 CTLine 的字形索引 0 表示。完全没有字体成形的字节数仍使 `paint` panic，避免整段文字静默消失。字体缺少所需字重或倾斜时，合成建议随 `GlyphRun` 传到光栅器：轮廓外扩 em/32 并按角度错切，仍是同一字形键与图集；位图字体不合成。

彩色字形按平台如实区分：Linux 的 FreeType `FT_LOAD_COLOR` 覆盖 COLR 层与内嵌 PNG/BGRA 位图（输出直通 sRGB）；**Windows 的 GDI 轮廓路径拿不到 COLR/CBDT/SVG**，这些字形返回 `GlyphError::UnsupportedGlyph`，不画成黑白近似；macOS 的 `CTFontCreatePathForGlyph` 只给轮廓，彩色字形同样返回 `UnsupportedGlyph`。Windows/macOS 的彩色字形接入 DirectWrite 的 glyph-run analysis / CoreText 的彩色 run 是后续工作。`colrv1`/`svg` feature 增加 COLRv1 渐变层与不含文字的 OpenType-SVG，**并作用于所有字形路径**：光栅化前先由 `aegle-glyph/src/color.rs` 判断字形是否带这两种表示，有则用 Skrifa + tiny-skia / resvg 解码，无则走平台光栅器。它们不启用 `swash`、也不替换平台光栅器——这两点曾经是耦合的，导致开 `svg` 就把 Linux 的 FreeType 换成 Rust 光栅器（字形像素随之改变）。未启用时对应表示返回 `UnsupportedGlyph` 而不是画成灰度轮廓。注册字体不等于该字体的每种表示均可绘制。

## 显示与内存

字体匹配与 fallback 由各平台系统栈负责：Linux 是 Fontconfig/Pango，Windows 是 DirectWrite 的 font collection 与 system fallback（每个 format 都经 `IDWriteTextFormat1::SetFontFallback` 挂上），macOS 是 CoreText。三者都区分简繁中文及日、韩语言提示。locale 可由应用覆盖，缺字按配置字体链回退，最终使用缺字占位并提供诊断，不能保证未安装字体覆盖所有字符。Rust 回退实现（`text-parley`）下由 Fontique 承担同样的职责。

未来 desktop 组合默认使用系统字体，不附带巨大 CJK 字库。当前独立模块的 `TextSystem::new()` 始终创建显式字体集合，不受 Cargo feature 合并影响：Linux 下它给 Pango 一个空的 Fontconfig 配置，只加入显式注册的字体文件；Windows 下它用 DirectWrite 的 font set 从注册字体建私有 collection（DirectWrite 只接受文件引用，所以每个注册字体写一次临时文件，随 `TextSystem` 一起删除）；macOS 下用 `CTFontManagerRegisterGraphicsFont` 注册。三处都把这些字体登记为对应 generic family 的替代（`set_generic_families`）。启用 `system-fonts` 后可用 `TextSystem::system()` 使用发行版的字体发现。嵌入式直接注册共享字体字节及 fallback 清单，打包字体的许可、覆盖和体积由发布清单记录。映射字体仍计入相应内存口径。

按实际出现的 glyph、字号、字体变化轴与光栅化参数生成字形。CPU 字形缓存和 GPU 图集分别有界，按最近使用淘汰，在途资源延迟回收；不预烘焙所有 CJK codepoint。布局结果、字体 metadata 和活动编辑状态也分别计量，不用字形上限代表整个文本系统。

`aegle-glyph/scene` 提供两类 renderer 共用的 `RasterTransform/GlyphOrigin` 策略：取仿射矩阵较大列长度确定设备字号，正向均匀轴向变换采用 hinting、整像素基线与水平四分之一像素相位，其他变换保留原点并过滤采样。共用 `mask_contrast` 按前景亮度调整灰度覆盖率 `c + c(1-c)k`：深色字加重、浅色字减轻，抵消线性混合造成的浅底细字与深底粗字；0 和满覆盖不变。它只计算字形光栅参数和 bitmap 到设备的变换，不改变排版 advance；软件后端已使用同一接口。设备基线超出 ±1,048,576 时明确返回坐标错误，最终图像边界和裁剪仍由各后端处理。

`aegle-text` 不携带任何词典数据：显示、换行、bidi 与删除由系统栈负责（Linux 是 Pango 的 UAX #14 换行与 `pango_get_log_attrs` 的 extended grapheme 边界；Windows 是 DirectWrite 的换行与 Uniscribe `ScriptBreak` 的 `fCharStop`，先 `ScriptItemize` 取得光标所在 script item 的分析；macOS 是 CoreText 的换行与 `CFStringGetRangeOfComposedCharactersAtIndex`），按词移动与双击选词只有一个边界来源——系统分词器（`aegle-segment`：Linux/Windows 的系统 ICU、macOS 的 `CFStringTokenizer`）。有它时按词移动与选词是词典结果（实测「中文分词测试」→ 中文/分词/测试、「東京都に行きます」→ 東京/都/に/行き/ます、「ภาษาไทยทดสอบ」→ ภาษา/ไทย/ทดสอบ）；**没有它时这两个操作不做任何事**——不退回规则边界，也不把每个汉字当成一个词：按词移动不移动，双击只把插入点放到点击处。`aegle_text::segment::available()` 报告当前进程是否可用。

默认构建不链接任何词典数据，泰/老/缅/高棉的**行内换行**仍由系统栈的规则引擎处理（Pango 自带字典时其行为随发行版而定），按词移动与选词永远只来自系统分词器。回退实现（`text-parley`）关闭了 complex-scripts 词典，这几个script的行内换行退回 UAX #14，长段落可能不折断。按词移动与选词对双向文本同样只来自系统分词器，边界按逻辑（字节）顺序，因此 RTL 段落按逻辑顺序移动。系统分词器查找与 `AEGLE_SEGMENT=none` 的调试开关见 [API 指南](developer/api.md)。

## 当前编辑接口

`TextSystem::editor` 创建 `Editor`，`TextSystem::edit` 借用短期命令式 driver。已支持单行/多行、方向选区、视觉/词/行移动、点命中选择、grapheme 删除、只读、撤销/重做及预编辑状态。默认单行，拒绝硬换行并禁用软换行；只读保留选择能力，阻止用户编辑并取消预编辑，应用仍可通过 `set_text` 显式替换值。

`Editor::text()` 返回无分配的 `TextValue` 借用视图；需要连续拥有的字符串时才调用 `to_string()`。`display_text()` 包含预编辑，不能充当已提交值。`take_changes()` 合并并取出 value/layout/selection/policy 失效标记，宿主据此更新布局、绘制、平台状态及未来语义树。`revision()` 是只增不减的已提交值修订号（预编辑不推进），比较前后两次读数即知其间值是否改变；aegle-ui 据此在用户输入改变文字后排队 TextField 的 `on_change`（标记中的 `changed`），程序 `set_text` 不经过输入路径，因此不触发。

内部 UTF-8 范围在字符边界验证。`select` 为用户选择，会按当前排版结果的 cluster/字符边界吸附；`replace` 用于精确协议替换，保留合法字节端点，不把删除一个组合字符扩大为删除基字符。无选区的 backspace/delete 使用系统栈的 extended grapheme 分段，只扫描光标所在的一行；在行首退格或行尾删除移除整个换行（CRLF 为一个）。

历史记录保存替换片段及前后选区，不逐次复制完整文本；连续相邻纯插入可合并。宿主在粘贴、焦点变化或输入停顿时调用 `break_undo_group`，编辑器本身不建计时器。默认历史预算 1 MiB，常规超限淘汰最旧完整操作；单次操作大于预算或禁用历史时仍编辑当前值，但清除旧历史以防错误回放。`set_text` 清空历史，详细计量见[资源](resources.md)。

`scene` feature 提供 `Editor::paint`，依次记录选择背景、字形、预编辑下划线及光标。`EditorPaint` 控制颜色及装饰显隐，前景变色不重新 shaping；聚焦、光标闪烁、滚动和裁剪由宿主负责。绘制、命中、选择矩形及 `ime_rect` 共用同一布局。`ime_rect` 返回未裁剪的局部逻辑坐标，宿主必须施加当前滚动/呈现变换。

Editor 自己保存显示缓冲与每个硬换行段落的排版结果（段落连同其后的换行符成形，排版时跳过换行后的空行，因此语义 run、选区与光标几何与整段布局相同）。替换只重新成形包含编辑或旧预编辑的段落，以及因插入、删除换行而合并或拆开的相邻段落，后面段落只平移字节偏移与位置；精确 UTF-8 替换直接作用于缓冲，不再借助上游 composition API。改宽度或对齐只重新断行与对齐，无换行宽度时各段在最宽一段内按对齐方式摆放；只改颜色不成形，`EditChanges::paint` 通知宿主重录；字体、字号等其余样式改变重新成形全部段落。段内移动、命中与按词/行选择调用 `Layout` 的 `move_cursor`/`hit_test`/`line_range_*`，越过段首尾时走到相邻段落，上下移动保留水平位置；系统栈返回的字节偏移一律先归到 UTF-8 字符边界（`pango_layout_xy_to_index` 的 trailing 位表示"落在该字符后半"，不是加一个字节），宿主发布 IME surrounding 时不会交出半个字符的偏移。单行编辑器只有一个段落，每次编辑仍重新成形整行；仍不是富文本编辑器。

## 组合输入模型

`set_preedit` 改变显示，不改变已提交值或历史。首次组合只保存被替换片段及原方向选区，`TextValue` 通过前缀、原片段、后缀维持稳定提交值；没有第二份完整文档。相对预编辑的光标范围按 UTF-8 验证，`None` 隐藏光标。

`cancel_preedit` 或空预编辑恢复原内容及原选区；`commit` 将结果记为一次撤销操作，空提交表示删除原范围，与取消不同。普通插入、移动、选择、外部替换和撤销要求先明确提交或取消活动组合，避免宿主无意改变输入法会话。预编辑只标记布局/选择变化，实际提交值变化才设置 value 标记。

`EditorDriver::apply_ime(ImeEdit)` 是共享的 UTF-8 事务入口：先验证提交、预编辑、两侧删除及长度，再修改文字、历史和失效标记，失败不产生部分编辑。两侧删除与提交合为一次撤销；单纯续写预编辑保留原选区，预编辑期间的两侧删除仍是可撤销的已提交变化。协议空重置移除旧预编辑及其被替换选区，与恢复原内容的显式 `cancel_preedit` 不同。

文档状态和原生会话仍分别管理；TextField 在失焦和必要手动编辑时取消组合，并通过 `Outcome::reset_ime` 要求宿主重置平台会话。未来平台的 UTF-16 等单位须先转换为合法 UTF-8 范围，再进入同一事务接口。

`set_password(true)` 把已提交值移入 Editor 自己的字符串，显示缓冲只排版等量 `•`；选择、命中、`display_text` 与 `selected_text` 都是遮盖缓冲的字节偏移，替换按遮盖字符索引同步到明文。`text()` 返回明文。切换时取消组合、光标移到末尾并清空撤销历史；密码模式不记录历史、拒绝 IME 组合（`TextError::Password`），TextField 不向宿主请求 IME 会话。

TextField 在 Ctrl（macOS 为 Command）+C/X/V 时只返回 `Outcome::clipboard` 请求，不访问系统服务；应用经 `Node::copy_text` 发出的写入请求（标记的 `copy(e)` 同此）走同一条路径；密码模式或空选区不复制/剪切，只读不剪切/粘贴。App 在刷新前把请求交给平台：Wayland 每 seat 一个 `wl_data_device`，以最近按键/按钮 serial 设置选区，写出与读取都是 calloop 非阻塞管道，读取上限 `CLIPBOARD_LIMIT` 4 MiB，结果作为 `Event::Clipboard` 回到原 seat 的焦点控件；Win32 同步读写 `CF_UNICODETEXT`，读出的文字同样作为 `Event::Clipboard` 进入事件队列，两个平台经同一条宿主路径粘贴。粘贴作为一次独立撤销，单行编辑器丢弃换行类字符。辅助技术编辑动作仍待集成。

## 当前 Wayland 原生接口

`aegle-platform-wayland` 在窗口使用的同一连接、队列和事件循环上，为每个 seat 创建一个 text-input-v3 对象，当前绑定协议版本 1。契约方法 `configure_ime(window, Some(ImeRequest))` 保存该窗口所聚焦编辑控件的状态；`None` 结束其会话。平台模块不依赖文字引擎或 renderer。`Event::Ime` 携带窗口、seat 和 `Entered/Left/Update`，序号与原生会话按 seat 独立维护，协议的提交序号与“当前”判断留在平台内部；宿主让最近聚焦窗口的 seat 驱动该窗口的编辑器。

契约 `ImeRequest` 包含 surrounding、光标/anchor 字节偏移、候选窗矩形、是否多行与变更是否来自输入法，两个平台相同。`ImeRequest::validate` 统一校验 surrounding 最多 4000 UTF-8 字节且无 NUL、两个偏移均在字符边界、矩形有限且尺寸非负；Wayland 另要求向外取整后可用协议的 i32 坐标表示，Win32 按 DPI 换算到物理像素后同样检查。矩形采用窗口局部逻辑坐标，宿主须先应用布局和滚动变换，不能再乘 buffer scale。Wayland 把 `multiline` 映射为内容提示，把 `input_method` 映射为 `InputMethod`/`Other` 变更原因。

`Editor::surrounding(max_bytes)` 提供最多两个无分配借用切片，包含完整选区及相对字节偏移；组合期间同时排除预编辑和原被替换片段，将选区折叠为光标。选区本身超预算则返回 None，不能截断选区后伪造偏移。`ImeRequest::surrounding = None` 表示支持组合但不支持周边文字，此时 cursor/anchor 必须为 0。Some/None 切换通过 disable/enable 更新该协议能力；只有原生 API 要求连续字符串时才分配有界副本。

### 批次、同步与会话边界

`preedit_string`、`commit_string` 和 `delete_surrounding_text` 暂存至 `done` 后，以一个 `ImeUpdate` 交给宿主；每次交付后清空暂存值。未出现 preedit 的批次表示空预编辑，不能继续显示上一批文字。预编辑光标必须是两个合法 UTF-8 端点，或 `-1/-1` 表示隐藏；不合法的范围产生错误，不悄悄吸附。`commit: None` 和 `Some("")` 分别表示没有提交事件和显式空提交。

宿主按协议顺序处理：移除旧预编辑到光标、删除选区/预编辑两侧要求的字节、插入提交文字、确定 surrounding，再放入新预编辑及其光标。删除长度不包含原选区；平台层没有文档内容，宿主必须在编辑器边界验证长度与 UTF-8 端点。应先处理完已排队的输入批次，再以最终编辑状态调用 `configure_ime`，避免逐条回复中间状态。

同一启用会话中的旧 serial 更新仍须应用。`ImeUpdate::current = false` 时，后端只缓存新的 surrounding 等状态，等待与最新 commit 计数匹配的 `done`；匹配后由宿主应用本批并重新提供最终状态，后端不提前发送旧缓存。计数包含 disable 的 commit，并按 u32 环绕。完全相同的配置不再发送，避免空批次往返产生持续更新。

取消会话是独立的生命周期操作：`configure_ime(None)` 即使在等待匹配 serial 时也立即 disable/commit。同一窗口内切换编辑控件必须先传 `None`，再启用新控件；每次 enable 记录序号边界，取消会话后排队到达的旧批次不会编辑新控件，也不会阻止新会话启用；显式 None 还会删除已分发到应用事件队列但尚未取出的该窗口 Update。`leave` 清除本地焦点和预编辑暂存；下一次 `enter` 先结束仍启用的旧服务端会话，再按当前控件重新启用并完整发送内容类型、surrounding 和矩形。无编辑控件的窗口保持禁用。

### 当前集成与验证边界

`cargo run -p aegle --example controls --release` 在原生窗口中组合 TextField、Button 和文字标签：`aegle-controls` 处理按键、选择、capture 和 IME，引擎处理 Tab 顺序与焦点；按钮回调可以修改文本框。原生 App 为活动键盘 seat 设置单一逻辑焦点域；多 seat 产品策略尚未作为通用 App API 交付。示例字体仅覆盖有限清单，正式应用应配置所需字体。

已通过隔离 Sway 中的原生协议验证：测试输入法使用 input-method-v2，经真实 compositor 将 CJK 预编辑、提交、周边删除和批次重置传递给本库的 text-input-v3；还验证了同会话旧 serial 延迟同步、焦点往返，以及取消后延迟批次与新会话的隔离。测试程序模拟输入法协议端点，不是 fcitx/IBus 用户操作验收；真实输入法切换、候选列表交互、复杂组合和桌面集成仍待端到端验证。详细运行环境与证据见[实现状态](implementation.md)。

## 平台和无障碍衔接

Wayland 已接入上述 text-input-v3，Windows 经 TSF 文本存储以同一批次形式接入（见[当前 Windows 输入边界](#当前-windows-输入边界)）；macOS 的 NSTextInputClient 仍为待实现目标。文字引擎不代替这些平台协议。候选窗采用当前呈现几何，主题、缩放或动画更新时同步，不重建编辑器。当前 app 的局部字号改变重排同一 Editor，保留组合和选择；皮肤/局部配色只覆盖绘制，不重启 IME 会话。动画几何仍未实现。

`text-a11y` 提供 `EditorDriver::accessibility` 导出显示文字 run、几何、预编辑下划线和选区；系统栈下 run 由 Aegle 按段落与预编辑边界切分（预编辑恰好被下划线覆盖），回退实现转发 Parley 的 run；`select_accessibility` 校验最近导出的身份/cluster 索引并作用于同一个 Editor。布局重建后旧映射无效；活动组合期间明确返回 CompositionActive，不能把显示范围直接套到取消后的已提交值。Unix 示例通过独立 aegle-access 接到 AT-SPI 查询、选择和焦点，当前上游缺少 EditableText，不能声称支持辅助技术文字替换；详见[无障碍边界](accessibility.md)。

平台缺少 IME 协议时报告 ImeUnavailable 并保留基础键盘输入；要求组合输入的应用可以将其设为启动必需能力。正式 CJK/IME 验收必须在具备对应协议和真实输入法的环境进行。

来源：[Pango 布局 API](https://docs.gtk.org/Pango/pango/Layout.html)、[Pango 断行属性](https://docs.gtk.org/Pango/method.get_log_attrs.html)、[Fontconfig 配置 API](https://fontconfig.pages.freedesktop.org/fontconfig/fontconfig-FcConfigCreate.html)、[FreeType 位图与彩色字形](https://freetype.org/freetype2/docs/reference/ft2-bitmap_handling.html)、[text-input-v3 协议](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/unstable/text-input/text-input-unstable-v3.xml)。

## 当前应用层滚动与候选区域

ScrollView 仅平移既有控件几何，Editor 继续拥有自己的文字滚动和组合状态。滚轮先由命中的编辑器消费，到边界后剩余的横纵位移交给祖先滚动容器；内部文字溢出不计入外层内容高度。聚焦、选择或编辑会沿祖先从内到外滚入控件；编辑器大于视口的轴改为保证 caret 可见，纯滚轮不强行拉回焦点。

候选矩形依次应用编辑器内部偏移、控件窗口位置和祖先偏移，再夹到控件、祖先可见矩形及窗口范围。手动将活动编辑器滚出视口时保持组合会话，候选锚点收缩到最近边界的零面积矩形；不因此发送 IME reset。重新编辑会揭示 caret。此几何策略已有无窗口组合场景验证，真实输入法对离屏零面积候选锚点的呈现仍需真人验收。

## 窗口级按键与输入时间

`Ui::on_key`（原生窗口经 `window.ui().on_key`）在焦点控件和 Tab 遍历之前收到每个按键事件，处理器返回 bool；多个处理器按注册顺序执行，第一个返回 true 的处理器消费该键，编辑器不再收到。它只看到普通按键：Wayland 上输入法组合期间的按键由 compositor 交给输入法，Win32 上 `VK_PROCESSKEY` 与组合中的按键在平台层已被过滤，所以快捷键处理器不会截走组合输入；`KeyEvent::editing` 告诉处理器焦点在编辑器中，处理器应把普通字符留给文字输入。Win32 的 WM_CHAR 文字作为 `Key::Unidentified` 加文字的事件同样先经过处理器。Menu 键（Wayland `Menu` keysym、Win32 `VK_APPS`）是 `Key::ContextMenu`，F1–F24 是 `Key::Function(n)`；控件库钩子不使用时，Menu 键与 Shift+F10 向焦点控件（没有焦点时为根）请求上下文菜单，F10 由默认控件库用于聚焦菜单栏。

按键和指针事件的平台时间（Wayland 毫秒时间戳、Win32 `GetMessageTime`）按窗口映射到 `Instant`：以相邻事件的差值推进，任何晚于投递时刻的结果把锚点移回投递时刻，因此映射收敛到观察到的最小投递延迟，不会超前于时钟，32 位回绕按有符号差值处理。

## 当前 Windows 输入边界

Win32 普通文本由 WM_CHAR/WM_UNICHAR 产生，UTF-16 surrogate pair 合并为 Unicode scalar；按键与文字分开送入同一 Ui/Editor，保留 AltGr/dead-key 的系统翻译。

输入法经 Text Services Framework（TSF）接入。`Win32::connect` 在 OLE 初始化后为 UI 线程激活一个 `ITfThreadMgr`；每个窗口有一个文档管理器，其唯一上下文由窗口自己的 `ITextStoreACP` 文本存储支撑（`text_store.rs`，同一 COM 对象也是 `ITfContextOwnerCompositionSink`）。启用编辑会话时窗口关联该文档；没有编辑器、会话取消或窗口失焦时关联一个没有上下文的空文档，输入法不在其中组合。窗口不关联 IMM 上下文，只经 TSF 组合：Windows 自带的中日韩输入法都是 TSF 输入法，仅实现 IMM32 的旧式第三方输入法不受支持。TSF 激活失败时 `ime_available` 为 false，编辑器请求输入法得到 `Error::ImeUnavailable`，不静默退化为纯键盘输入。

文本存储的文档是宿主提供的周边摘录（至多 `MAX_SURROUNDING` = 4000 UTF-8 字节，组合期间不含预编辑），组合文字内联其中：输入法可读取光标前后文字用于预测，也可在已提交文字上开始组合（重转换）。宿主是唯一权威：

- 每次锁结束后，存储把文档相对宿主所显示内容的变化表达为一次 `ImeUpdate`：在宿主选区（组合中为预编辑）两侧删除 `delete_before`/`delete_after` 字节、以 `commit` 替换、在提交之后显示新组合及其光标，与 text-input-v3 批次同形，App 原样交给 `Ui::ime`。变化取共同前缀与后缀，并以选区和组合范围为上限，重复文字（在 "a|a" 中组合 "a"）不会错位；只在第二个代码单元不同的代理对整体替换。组合之后的文字也被改动的编辑无法用该模型表达，报告为 `Event::Error`，不错位应用。
- 宿主在组合之外发布状态（`configure_ime`）时，其摘录与选区替换文档，并依次通知 TSF `OnTextChange`、`OnSelectionChange`；组合期间文档归输入法，只更新候选矩形并通知 `OnLayoutChange`。
- `GetTextExt` 对任何范围都返回宿主的光标矩形（物理像素，屏幕坐标），候选窗跟随光标，而非逐字符定位；`GetACPFromPoint`、嵌入对象与文字属性不提供。
- 锁同步授予；持锁期间的异步请求在当前锁结束前授予，同步请求得到 `TS_E_SYNCHRONOUS`。输入法给出的位置在边界校验（不越界、不拆分代理对），插入文字须为合法 UTF-16，文档上限 512 Ki 个 UTF-16 单元。

取消会话（切换编辑器、`configure_ime(None)`）与窗口失焦都先停止报告再终止组合，然后关联空文档；失焦另发出 `ImeEvent::Left`，已排队的旧会话更新被丢弃。输入法终止组合时留在文档里的文字不会提交：重新启用时先以宿主状态替换文档，再恢复报告并关联文档。这一顺序来自测试中发现的问题——先关联、后发布时，Microsoft Pinyin 在新会话中读到残留的 "shi"，被报告为提交。

验证（2026-10-11，Windows 11 Pro for Workstations 26100 桌面会话，简体中文 Microsoft Pinyin）：

- `aegle-platform-win32/tests/tsf.rs` 以文本服务身份经真实 TSF 运行时请求编辑会话，不依赖已安装输入法：读取摘录和 UTF-16 光标；`GetTextExt` 等于光标矩形的屏幕坐标；光标处组合到提交；对已提交文字重转换（`delete_after` 加预编辑，再提交新文字）；代理对替换与原位替换已提交的 emoji（`delete_before` 4 字节）；清空组合删除预编辑；无法表达的组合报告错误；取消会话不报告，重新启用后文档等于宿主文字。
- `aegle-platform-win32/tests/ime.rs` 以 `SendInput` 发送真实按键（只在测试窗口处于前台时输入）：组合按键不进入宿主、逐键预编辑、首候选只提交一次且不产生 WM_CHAR、Esc 取消、取消会话不提交残留、窗口失焦结束组合不提交、回到窗口后重新组合。
- `aegle-app/tests/ime_windows.rs` 经原生 App 把拼音提交进 TextField：预编辑不改变已提交值、焦点移到按钮不提交残留、回到字段后在光标处追加；`aegle-app/tests/tsf_windows.rs` 让文本服务读取 TextField 的文字与光标，把 "世界" 重转换为 "时节"，字段值随之改变，再次读取得到宿主重新发布的状态。
- 候选窗位置：组合 "nihao" 时截屏核对（缩放 100%），候选列表左上角在宿主报告的光标矩形左下角。
- 未覆盖：日文/韩文输入法及其重转换按键（本机只装了 Microsoft Pinyin，重转换只经测试文本服务验证）、触屏键盘、InputScope（密码框仍由 Editor 拒绝组合）、组合显示属性（下划线按 Editor 默认绘制）、非 100% 缩放下的候选窗。

```sh
cargo test -p aegle-platform-win32 --test tsf --test ime -- --ignored --test-threads=1
AEGLE_TEST_COMPOSITOR=private cargo test -p aegle-app --features windows,software --test ime_windows --test tsf_windows -- --ignored --test-threads=1
```
