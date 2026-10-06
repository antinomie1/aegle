# 工程验收与交付边界

状态：v0.1。实现与验证证据见[实现状态](implementation.md)；本文的预算是工程目标，不是达标声明。

## 需求追踪

| 需求 | 主责任文档 |
| --- | --- |
| R01–R03：Rust、保留模式、Taffy | architecture、dependencies |
| R04、R15：性能与发布依赖 | resources、dependencies |
| R05–R07、R11：模块、后端、平台与 shell | modules、platform-rendering |
| R08–R10：双入口及十行 | rust-api、markup |
| R12、R18–R20：组件、外观、主题与动画 | components-theme-animation、rust-api |
| R13–R14：CJK 与 IME | text-input |
| R17：无障碍 | accessibility |
| R21–R23：一致性、工程质量、高级绘制 | architecture、本文、platform-rendering |

术语由 GLOSSARY 定义，契约以对应专题为准，选型记录仅解释取舍。改变跨模块契约时同步引用它的专题，不复制几套不同解释。

## 只做少量关键联合验证

1. 两种 Hello world 按约定计行；编译与加载路径创建同样的控件语义，最小发布清单可解释。
2. 100 控件含 CJK 的静态/输入场景，测量空闲唤醒、CPU、PSS、GPU 分配和发布体积；缓存不会随不同字符无限增长。
3. IME 输入中切换主题、缩放、动画与焦点，检查预编辑、提交、选择、候选窗和辅助技术读取。
4. 删除仍有监听器、动画和平台动作的控件，验证失效句柄、清理、无残留任务；底层 unsafe 生命周期用针对性工具检查。
5. 第三方风格的 Button/TextField/Switch 只使用公开接口，验证键盘、指针、CJK、IME、主题、动画和系统语义。
6. 三平台普通窗口、目标 Wayland compositor 的 layer-shell、真实输入法及辅助技术；缺少能力的行为与文档一致。

语法/绑定循环、句柄代数、IME 范围转换等算法需要少量边界测试；不为每个 setter、颜色常量或转发函数写镜像测试。不用大量 mock 代替真实协议验收。

## 实现顺序

先完成最小主线程窗口、控件、Taffy、文字与绘制；接着完成 IME/无障碍及生命周期，再接编译型标记、默认皮肤/主题/基本动画；最后加入运行时加载、shell 和可选效果。每阶段都复核资源，不能把无障碍变成永久留待以后。

首版验收前需用实际 Cargo.lock 验证完整依赖闭包与 MSRV、三平台构建、源文件示例、真实硬件和系统协议。未完成这些验证时只可称为设计/原型，不可宣传工业级运行质量或预算已达成。
