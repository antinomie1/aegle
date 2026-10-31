# 文本、排版与输入法 (IME)

Aegle 的文本策略是：**排版、双向文字（Bidi）、断行与字形覆盖率优先委托给操作系统的原生文字栈，Aegle 自身仅负责段落缓存、增量编辑、光标选区与绘制记录**。这样避免了向二进制中打包数十兆的 ICU 词典和字形数据。

---

## 一、 系统文字栈架构

| 操作系统 | 排版与断行后端 | 字体匹配与发现 | 字形覆盖率光栅 | 加载方式 |
|---|---|---|---|---|
| **Linux** | **Pango** (UAX #14 换行、Bidi) | **Fontconfig** | **FreeType** (`FT_LOAD_COLOR`) | 运行时 `libloading` 动态加载 |
| **Windows** | **DirectWrite** (`backend_directwrite/`) | DirectWrite Font Collection | DirectWrite 8-bit Alpha 纹理 | 动态链接系统 DLL |
| **macOS** | **CoreText** (`backend_coretext/`) | CoreText Font Descriptor | CoreGraphics Alpha 位图 | 动态链接系统 Framework |

* **零数据打包**：默认 release 构建不打包任何字体文件或 HarfBuzz/Swash 代码；`text-parley` 为仅在显式开启时编译的非维护回退。
* **字体身份抽象 (`FontSource`)**：`Scene` 中的字形身份为不透明的 `FontSource::System(SystemFont { backend, id })` 或 `FontSource::Bytes(FontData)`，渲染器不保存任何原生系统指针。
* **彩色字形**：Linux 下 FreeType 直接支持 COLR 与内嵌位图；若启用 `colrv1` 或 `svg` feature，由 `aegle-glyph` 在光栅化前使用 Skrifa/resvg 解析，未开启对应 feature 的彩色字形安全返回 `UnsupportedGlyph`。

---

## 二、 词边界断词 (`aegle-segment`)

用于支持中日韩泰等不以空格分词语言的 **Ctrl/Alt+方向键按词移动** 与 **双击选词**：
* **动态探测**：Linux 和 Windows 运行时探测并调用系统已安装的 ICU 动态库（`libicui18n.so` 或 `icu.dll`）；macOS 调用 `CFStringTokenizer`。
* **不内置近似词典**：未检测到系统分词器时，`aegle_text::segment::available()` 返回 false，按词移动停用，双击仅放置光标，不破坏字符或产生错误。可通过环境变量 `AEGLE_SEGMENT=none` 强制停用以便测试。

---

## 二、 多行文本编辑器 (`Editor`)

`Editor` 是保留模式的纯文本增量编辑器，内部按硬换行切分为独立的段落块：
* **分块增量成形**：单行文本输入仅重新成形被修改的单个段落块；宽度改变仅触发重新折行；颜色/样式改变不触发重新成形。
* **无分配视图**：`Editor::text()` 返回 `TextValue` 借用切片；仅在需要拥有所有权时调用 `to_string()`。
* **稳定修订号 (`revision`)**：仅在实际提交内容改变时递增；预编辑和撤销重做不改变已提交值的语义。
* **有界差量历史**：撤销/重做栈按操作差量存储（默认上限 1 MiB），相邻连续纯输入自动合并，`set_text()` 或超限时安全丢弃旧历史。
* **密码模式**：明文单独存储，排版缓冲仅渲染等长掩码字符（`•`），禁用历史与 IME 组合。
* **空闲预算回收**：共享 `TextSystem` 支持设置未显示段落的内存上限（`set_idle_budget`），在内存受限设备上自动回收视口外的排版缓存。

---

## 三、 组合输入法 (IME) 模型与事务

`EditorDriver::apply_ime(ImeEdit)` 提供跨平台的原子编辑事务：
```rust
pub struct ImeEdit<'a> {
    pub delete_before: usize,   // 光标/预编辑前删除的 UTF-8 字节数
    pub delete_after: usize,    // 光标/预编辑后删除的 UTF-8 字节数
    pub commit: Option<&'a str>, // 本次提交替换的文本
    pub preedit: &'a str,       // 活跃的临时组合文本
    pub cursor: Option<Selection>, // 预编辑内的光标/高亮范围
}
```
* **预编辑稳定性**：`set_preedit` 仅改变局部视觉呈现，不污染已提交的文档内容与撤销历史；取消预编辑立即复原原选区。
* **候选框定位**：控件计算未经裁剪的局部逻辑矩形（`cursor_rect`），由平台层根据 DPI/缩放转换为原生输入法候选窗锚点。

---

## 四、 平台 IME 协议实现

### 1. SDL3 后端 (`aegle-platform-sdl3`)
* **生命周期**：控件获焦时 `SDL_StartTextInput`（若多行则附加 `SDL.textinput.multiline` 属性），失焦时 `SDL_StopTextInput` 并发 `ImeEvent::Left`。
* **候选框位置**：通过 `SDL_SetTextInputArea` 向外取整传递当前光标位置。
* **事件映射**：`SDL_EVENT_TEXT_EDITING` 映射为预编辑更新；`SDL_EVENT_TEXT_INPUT` 映射为事务提交。
* **边界**：SDL3 不支持向输入法同步周围文本（Surrounding Text）和选区范围，相关字段校验后不传递。

### 2. Wayland 原生客户端 (`aegle-platform-wayland`)
* **协议绑定**：每个 seat 独立绑定 `zwp_text_input_v3` 对象。
* **完整周围文本**：通过 `set_surrounding_text` 提交有界周边摘录（最多 4000 字节）与光标/锚点偏移。
* **事务批次**：在 compositor 的 `done` 事件到来时打包提交 `ImeUpdate`，处理预编辑、删除与提交时序。

### 3. Windows 后端 (`aegle-platform-win32`)
* **TSF 文本存储**：每个窗口实现原生 COM 接口 `ITextStoreACP`。
* **主动与被动组合**：支持微软拼音等高级输入法读取周边上下文、执行重转换（Re-conversion），并将变更批次转换为 `ImeUpdate` 投递至 UI 循环。

### 4. macOS 后端 (`aegle-platform-macos`)
* **AppKit 适配**：由窗口的 `NSView` 实现 `NSTextInputClient` 协议，在原生 Objective-C 消息与 Aegle 的 UTF-8 事务之间双向转换。
