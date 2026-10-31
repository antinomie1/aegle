# 组件、主题与动画系统

Aegle 采用行为（`aegle-controls`）、视觉皮肤（`aegle-theme`）与渲染调度（`aegle-ui`）解耦的架构，确保所有内置与第三方控件共用统一的状态机、无障碍语义与动画管道。

---

## 一、 控件体系与交互行为

### 1. 行为与外观分离
* **`aegle-controls`**：保存交互状态机（Hover/Press/Focus/Active/Drag/Range），不含具体绘制与布局。
* **`aegle-widgets`**：组合行为与布局，提供跨平台一致的中性极简（类 Metro）默认外观。
* **纯函数皮肤 (`Skin`)**：签名统一为 `fn(&Theme, VisualState) -> Appearance`，支持三级作用域覆盖：
  1. 控件类型默认皮肤；
  2. 子树类型皮肤（`Node::set_kind_skin`）；
  3. 节点专属皮肤（`Node::set_skin`）。

### 2. 默认组件清单
* **基础容器**：`Row`、`Column`、`ScrollView`（视口滚动）、`Grid` / `Stack`（网格与重叠层，`grid` feature）。
* **文本与输入**：`Label`（单/多行标签）、`TextField`（单/多行文本编辑）、`NumberField`（步进数值输入）。
* **操作与切换**：`Button`（支持主次与强调变体）、`CheckBox`（支持半选状态）、`Switch`（滑动开关）、`RadioButton`（单选互斥）。
* **范围与进度**：`Slider`（水平/竖直连续或步长滑块）、`Progress`（确定与不确定进度条）。
* **选择与展示**：`Dropdown`（下拉选择）、`Tabs`（标签页切换）、`Table`（虚拟行表格）、`ListView`（等高/动态虚拟列表）、`ImageView`（图像展示）、`Canvas`（自定义绘制与交互）。
* **浮层与导航**：`Popup`（锚点浮层）、`Menu` / `MenuBar`（菜单项、复选菜单、子菜单）、`Tooltip`（悬停提示）、`Dialog`（模态居中卡片对话框）、`Drawer`（边缘抽屉导航）。

---

## 二、 主题与 Token 系统 (`aegle-theme`)

### 1. 主题结构 (`Theme`)
`Theme` 是无堆分配的值类型，预设浅色（`Theme::light()`）、深色（`Theme::dark()`）与高对比度（`Theme::high_contrast()`）：
* **核心颜色**：`background`（窗体背景）、`surface`（卡片表面）、`control`（控件底色）、`foreground`（正文文本）、`muted`（弱化提示）、`border`（边框）、`divider`（分隔线）、`accent` / `on_accent`（强调色与强调文字）、`hover`、`pressed`。
* **尺寸基线**：默认控件高度 32 dp，边框 1 dp，圆角 4 dp，内边距与间隙基线 8 dp。

### 2. Token 动态绑定
* **类型化 Token**：支持颜色、长度、字体、阴影、动画时长等全局注册。
* **动态响应**：控件样式属性（如背景色、字号、圆角、阴影）可绑定至 Token（如 `bind_color(Token::BACKGROUND)`），当全局或子树通过 `ThemeOverride` 变更主题时，绑定属性自动更新，无需逐个控件遍历赋值。

---

## 三、 动画与过渡系统 (`aegle-motion`)

Aegle 内置连续动画引擎，所有过渡共享主时钟推进，空闲状态零定时器唤醒。

### 1. 过渡动画 (`Transition`)
* **支持属性**：`Background`、`Foreground`、`BorderColor`、`BorderWidth`、`Radius`、`Opacity`、`Offset`（平移）、`Scale`、`Rotation`、`Shadow`。
* **平滑重定向**：在动画播放途中更改目标值时，新动画自动以**当前呈现值（`presented`）**作为起始点平滑过渡，杜绝突变跳帧。
* **作用域控制**：
  * `Node::with_transition(transition, || { ... })`：为代码块内的修改指定临时过渡策略。
  * `Node::snap(|| { ... })`：跳过动画，立即快照定位至目标值。
  * `Node::on_transition_end`：接收过渡完成通知。

### 2. 动画曲线与物理弹簧
* **缓动曲线 (`Easing`)**：支持线性、EaseIn、EaseOut、EaseInOut、三次 Bézier 曲线。
* **阻尼弹簧 (`Spring`)**：支持刚度（stiffness）与阻尼比（damping ratio），自动计算系统收敛稳定时间，并作为动画时长。
* **系统偏好跟随**：系统声明“减少动态效果”（`reduced_motion`）时，所有非显式强制的过渡立即直接生效，不进行逐帧插值。

---

## 四、 视觉规范与中性外观参数

* **边框与焦点环**：边框均向内绘制，不侵占布局外边距；焦点可见环（Focus Ring）仅在键盘导航操作时激活显示，鼠标交互默认隐藏焦点环。
* **滚动条行为**：仅在内容溢出时于边缘绘制 4 dp 胶囊滚动条，悬停或拖拽时扩展至 8 dp，无多余淡入淡出轮询。
* **浮层自适应裁剪**：所有弹出窗口、下拉菜单与对话框在测量时自动受限于当前窗口逻辑视口，超出屏幕时自动反转贴边或内部开启滚动，不截断重要内容。
