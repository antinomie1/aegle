# 模块、依赖与构建组合

状态：v0.1 模块设计。types、core、layout、scene、text、glyph、controls、软件 renderer 与 Wayland 平台已建立；表中其余模块及已建立模块的完整职责仍是设计目标，具体进度见[实现状态](implementation.md)。

## 拆分尺度

按可独立使用的能力和真实后端差异拆分。每个模块需有自己的 Interface：类型、所有权、调用顺序、错误和资源成本。模块不通过全局服务定位器寻找其他模块，由调用者显式传入能力。

| crate | 责任及独立用途 | Aegle 内部依赖 |
| --- | --- | --- |
| aegle-types | 几何、颜色、资源 ID、通用错误与能力描述；无平台依赖 | 无 |
| aegle-core | 槽位树、句柄、属性变更、事件路由、焦点 | types |
| aegle-layout | Taffy 低层树适配、Flex/Block 与可选 Grid；不依赖应用 | types、core |
| aegle-text | 字体、保留段落布局、纯文本编辑/组合状态与有界撤销 | types；scene 按 feature 接入 |
| aegle-glyph | Swash 字形光栅化与有界 CPU 字形缓存 | 无 |
| aegle-scene | 二维绘制命令、裁剪及可选字形记录 | types |
| aegle-render-vulkan | Vulkan 实现、上传、图集与呈现 | types、scene |
| aegle-render-software | 无 GPU 栅格绘制，与 GPU 共用 scene/文字资源 | types、scene；glyph 按 text feature 接入 |
| aegle-render-metal | Metal 实现、上传、图集与呈现 | types、scene |
| aegle-platform-wayland | Wayland 窗口、事件、IME、输出与平台偏好 | types |
| aegle-platform-win32 | Win32 窗口、TSF/必要兼容路径及平台偏好 | types |
| aegle-platform-appkit | AppKit 窗口、NSTextInputClient 及平台偏好 | types |
| aegle-shell-wayland | layer-shell 表面策略，共享 Wayland 连接和事件队列 | types、platform-wayland |
| aegle-access | 语义快照/差量和目标平台 AccessKit adapter | types |
| aegle-theme | 有类型的主题 token、局部覆盖与状态取值 | types |
| aegle-motion | 时间、补间、过渡及可选弹簧；可无窗口独立推进 | types |
| aegle-controls | 可复用控件行为、语义动作与基础组合；无默认皮肤 | types；text feature 接 text，树与路由由宿主提供 |
| aegle-widgets | 默认中性极简皮肤和常用组件 | controls、theme、scene；motion 按 feature 接入 |
| aegle-path | Lyon 路径细分，可交给其他绘制宿主 | types、scene |
| aegle-assets | 有界 PNG 解码与可选运行时 SVG 光栅化 | types |
| aegle-markup | 解析、跨度、类型化中间表示与语言校验 | types |
| aegle-macros | ui! 编译与组件元数据生成，仅编译期运行 | markup |
| aegle-loader | 可选运行时加载、表达式执行与显式重载 | types、markup、core |
| aegle-app | 将窗口、UI、布局、绘制、文本与可选能力连接起来 | core、layout、scene；其余按构建组合启用 |
| aegle | 应用便捷入口与重导出，不提供另一套实现 | app；其他按 feature 重导出 |

表中的简称指同名前缀 crate。文字无障碍为 `aegle-text/text-a11y`，映射 Parley 的可选 AccessKit 支持；基础文字模块不强制启用它。平台 adapters 按 target 编译，不能把三平台实现都塞进一个程序。

当前 `aegle-scene` 使用 no_std + alloc，默认只依赖 types；`text` 仅增加轻量的 `linebender_resource_handle`，通过共享 `FontData` 及独立 run 旁表保存字形记录，不引入 Parley 或 Swash。纯几何命令不携带完整字体/run 数据。

`aegle-render-software` 借用调用方像素缓冲，不依赖 core、Taffy 或窗口；默认是纯几何构建，没有字体栈和 PNG 运行依赖。tiny-skia 0.12（仅 std/simd）完成覆盖率栅格化，小型自有实现完成线性光 SourceOver。`text` 显式增加 aegle-glyph，其 PNG 解码器用于字体内嵌位图。软件后端不依赖 aegle-text，其他 shaping 宿主也可提供 scene 字形记录。

`aegle-text` 默认启用 Parley std，并复用其已有的 ICU 分段包处理 grapheme 删除；系统字体、词典、文字无障碍和 scene 桥接分别可选。段落与 Editor 共用 TextSystem 字体/shaping 上下文，Editor 包装 PlainEditor 并补充稳定提交值、可取消组合和有界 delta 历史；不另建编辑引擎或转发 crate。scene 桥接共用字形绘制，额外记录选择、预编辑和光标；平台 IME、剪贴板及系统语义由后续平台/应用层连接。

`aegle-glyph` 独立接受共享字体句柄，复用 Swash、Skrifa、hashbrown 与 lru-slab，不自建字体解析器或通用缓存框架。缓存不保留字体字节；段落、编辑器及 scene 的字体句柄维持各自资源寿命。

`aegle-platform-wayland` 复用 SCTK、wayland-client 与 calloop 管理同一连接、多个普通窗口和原生输入。平台只依赖 types；TextSystem、Editor、Scene 和 renderer 在可执行示例中组合，不成为平台的发布依赖。软件呈现直接借出有界 SHM 像素；text-input-v3 以带 seat 身份的事务传递给宿主。尚未实现 layer-shell 或 GPU surface 接口。

`aegle-controls` 默认只有无分配的 Button 状态及借用 Input/Outcome；`text` 增加复用 Editor 的 TextField。它不依赖 core、布局、主题、renderer 或窗口。宿主在自己的树中保存行为状态，负责命中、焦点和 capture；键盘、指针及语义激活经过同一默认行为。Wayland editor 示例使用 core 的 Route/Focus 连接这套行为，不再另写编辑快捷键与 IME 文本替换。完整系统无障碍仍需 adapter。

没有独立的“每个控件 crate”或“每个颜色类型 crate”。当一个模块的多种选择只影响内部小函数时使用 feature，不为包装一个转发函数增加新的包。

## 依赖方向

```mermaid
flowchart TD
  Facade[aegle 便捷入口] --> App[aegle-app 组装]
  App --> Core[core]
  App --> Layout[layout / Taffy]
  App --> Text[text + glyph]
  App --> Platform[目标平台 + 可选 shell]
  App --> Access[可选 access]
  App --> Render[scene + 一个 renderer]
  Facade --> Widgets[widgets / controls]
  Widgets --> Core
  Widgets --> Visual[theme / motion / scene]
  Loader[可选 loader] --> Core
  Loader --> Markup[markup]
  Macros[构建期 macros] --> Markup
```

这是一张依赖图，不是每次更新必须经过的多层调用链。core 不依赖 app、widgets、loader、GPU 或操作系统库；renderer 不依赖控件或标记语言。

## 默认组合与可裁剪组合

| 组合 | 包含 | 不自动包含 |
| --- | --- | --- |
| `aegle` 默认 desktop | 目标平台、目标 GPU、Taffy Flex/Block、CJK 文本/编辑、无障碍、默认组件、主题、基本补间、编译宏 | 运行时加载、shell、SVG、路径特效、Grid、词典分词、弹簧 |
| `default-features = false` | 不自动选择窗口/renderer；使用者显式加所需功能或直接用独立模块 | 便捷默认组合 |
| `runtime-ui` | loader、所注册组件的类型描述与宿主动作 | 通用脚本 VM、文件监视器 |
| `shell` | Linux layer-shell；共享既有 Wayland backend | 托盘、通知、D-Bus 业务服务、全局快捷键 |
| `text-dictionary` | 中日词典分词及相关复杂文字分段数据 | 网络字体 |
| `vector` / `svg` / `effects` | 分别增加路径、SVG 和阴影/模糊能力 | 三者不互相暗中全部开启 |

默认桌面组合启用无障碍与减少动态效果支持。嵌入式应用可以显式不编译 OS 无障碍 adapter；不能将这种构建宣传为完整无障碍构建。省掉平台 adapter 不要求删除控件的基本语义定义。

Cargo feature 在依赖图中会统一：不能承诺同一程序里的某个控件使用有字典的 Parley，另一个控件因此完全不承担其静态数据成本。发布检查必须查看实际闭包和最终文件，而不是只查看 facade 清单。

## 扩展契约

第三方可以注册控件、主题 token、动作和新的 renderer/platform adapter，使用稳定 Rust 接口；不提供动态二进制插件 ABI。组件库依赖 core/controls/theme/scene 等实际需要的模块，不必依赖整个 aegle。只改外观的组件复用行为，只有新增交互时才实现新的行为。

首版只承诺文档所列 capability。扩展能力不存在时返回结构化错误或使用明确的组件降级方案，不依赖反射查找“可能存在”的隐藏服务。
