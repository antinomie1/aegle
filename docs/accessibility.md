# 系统无障碍 (Accessibility)

Aegle 的所有自绘控件均具备完整的原生系统无障碍支持。辅助技术（屏幕阅读器、语音控制、测试自动化工具）能够无缝感知控件的角色、名称、取值、状态，并执行点击、滑动、文本选择等语义动作。

---

## 一、 核心概念与分层

| 层级 | 表达内容 | 示例 |
|---|---|---|
| **控件树 (`Ui`)** | 控件逻辑结构、父子拓扑与局部状态 | 一个按钮内部包含图标 `Image` 与文字 `Label` |
| **语义树 (`Semantics`)** | 辅助技术关心的元素及其功能边界 | 上述按钮合并为一个具有名称的 `Button` 节点，装饰性图标被隐藏 |
| **原生系统适配** | 将语义树转换为操作系统的无障碍 API | Linux AT-SPI (D-Bus)、Windows UI Automation、macOS NSAccessibility |

* **按需开启**：默认构建仅在内存中维护轻量语义定义；通过单个 `accessibility` feature 启用 AccessKit 适配器，将语义树实时推送到目标系统的辅助技术总线。

---

## 二、 语义契约与属性导出

自定义控件通过 `Control::semantics` 实现契约：

```rust
impl Control for MySlider {
    fn semantics(&self, cx: &mut SemanticsCx) {
        cx.set_role(Role::Slider);
        cx.set_name("音量");
        cx.set_numeric_value(self.value as f64);
        cx.set_numeric_range(0.0..=100.0);
        cx.set_numeric_step(1.0);
        cx.add_action(SemanticAction::Increment);
        cx.add_action(SemanticAction::Decrement);
    }

    fn action_input(&mut self, action: SemanticAction) -> Outcome {
        match action {
            SemanticAction::Increment => { self.step(1.0); Outcome::dirty() }
            SemanticAction::Decrement => { self.step(-1.0); Outcome::dirty() }
            _ => Outcome::empty(),
        }
    }
}
```

### 核心语义属性
* **角色 (`Role`)**：Button, CheckBox, RadioGroup, TextField, Slider, Table, Dialog 等。
* **状态值**：`checked`、`toggled`、数值范围（最小值/最大值/步长）、文本选区。
* **动作响应 (`SemanticAction`)**：`Default`（点击激活）、`Focus`、`Increment`/`Decrement`、`SetTextSelection`。
* **受保护文本**：密码控件自动声明为密码字段，辅助技术不暴露明文快照，剪贴板与内存检查遵守保护语义。

---

## 三、 平台适配器 (`aegle-access`)

由 `aegle-access` 封装 AccessKit 0.24，针对目标平台编译单一适配器：

```
[aegle-ui / 语义节点]
        ↓ 
  [aegle-access]
   ├── Linux:   UnixAdapter   → AT-SPI 2 (基于 zbus 会话总线)
   ├── Windows: WindowsAdapter→ UI Automation (UIA COM Provider)
   └── macOS:   MacosAdapter  → NSAccessibility (NSView 子类化)
```

1. **增量差量推送**：
   * 窗口首次激活时全量导出整棵语义树；
   * 后续仅在控件产生 `Dirty::SEMANTICS` 时推导差量更新，降低 IPC 流量。
2. **坐标与几何**：
   * 语义边界基于布局导出的逻辑坐标与滚动视口交集裁剪计算。
   * Windows 与 macOS 适配器按 DPI 物理像素换算；Linux AT-SPI 使用窗口局部逻辑像素。
3. **线程安全性**：
   * 辅助技术的跨线程查询和动作经 `Mailbox` 排队，通过平台 `WakeHandle` 唤醒主线程，在所有 UI 内部借用之外安全分发。
