---
status: accepted
---

# 标记元素由同一份元素契约描述

标记语言原先在 `aegle-markup` 的 `schema.rs` 中有一张固定的元素表：`Kind` 枚举列出 20 种内置元素，各元素的属性、事件、`self` 字段和子节点规则散落在检查器、`ui!` 的代码生成和 loader 的 `Handle` 枚举里。用 Rust 写的第三方控件无法出现在标记中。

## 决定

每个元素，包括内置元素，都由控件库用 `element!` 声明一份 `ElementSpec`：名称、布局（叶、盒、flex、grid）与子节点规则、可选样式组、属性（类型、是否构造参数、是否有 setter、是否必填）、事件与 `self` 字段，连同创建控件和应用属性的胶水代码。`element!` 为元素实现 `aegle_loader::Element`，并定义一个与元素同名的隐藏宏，内容是这份声明本身。

- 检查器 `check_program(files, specs)` 只认识文档根 `Window` 和传入的规格，没有内置元素表；节点属性（布局、样式、无障碍、动效）仍是 `aegle-markup` 中的统一表，按元素的布局和样式组限定。
- `ui!` 先解析文件得到用到的元素名，在调用处的 Rust 作用域里逐个调用同名宏；每个宏把自己的声明转交给隐藏的 `__ui_resume`，集齐后用与运行时完全相同的检查器检查，再生成代码。错误因此在编译期带文件、行、列报告；不在作用域内的元素由 rustc 报告缺少宏。
- 运行时 `Program::load_with(path, &Elements)` 用元素类型上的 `SPEC` 常量检查，挂载前报告同样的诊断；`Elements::new()` 是内置元素，第三方元素用 `with::<E>()` 登记。
- 两条路径都通过 `Element` 的胶水创建控件、设置元素属性、注册事件和读取字段；节点属性都交给 loader 的同一组 setter。静态文档仍编译为直接调用（不经引擎、不带解析器），只是调用的是这些共享函数。

## 取代的内容

`Kind`、`EventKind`、静态专用的 `check`/`CheckedDocument`、`Handle` 枚举与 `FromHandle`、宏中的布局与动效 setter 生成被删除。ADR 0002 中“静态文档的直接构造与引擎仍是两份代码”的一致性约定随之缩小为节点属性与元素胶水各只有一份实现。

## 代价

`ui!` 在调用处按名称解析元素，因此要求元素在作用域内（`use aegle::prelude::*` 引入内置元素）。宏回调链让每个用到的元素多一次宏展开。元素的值类型限于标量（bool、int、float、fraction、length、string、line、color、choice），复杂值仍走节点属性或 Rust API。
