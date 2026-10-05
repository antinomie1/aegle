---
status: accepted
---

# 完整无障碍语义不要求逐控件使用原生控件

用户在 Q10 明确选择完整系统无障碍语义与可检查的控件树，不要求每个控件均由平台原生控件实现。这样，自绘、跨平台一致的组件外观和 GPU 后端可以与系统辅助技术支持并存；相应代价是框架必须提供角色、属性、关系、动作、焦点和变化通知的完整适配，不能只绘制像素或暴露一棵静态树。

本决定不选择 AccessKit 或自研平台适配，不确定语义树的具体存储方式，也不禁止按需接入原生控件。详见[无障碍设计](../accessibility.md)。

## 实现补充

后续实现选择 AccessKit 的独立平台 adapter（Unix AT-SPI、Windows UIA），语义由框架从同一控件树导出，`accessibility` feature 可在不接系统的情况下检查语义树。为控制默认依赖与常驻资源，经用户决定，facade 默认组合不启用系统 adapter，应用以 `unix-accessibility` 或 `windows-accessibility` 显式启用；本决定要求的语义完整性不因此降低，适配范围与限制见[无障碍设计](../accessibility.md)。

