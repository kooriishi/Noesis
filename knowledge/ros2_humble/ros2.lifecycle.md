---
id: ros2.lifecycle
title: 生命周期节点（LifecycleNode）与受控状态机
domain: ros2
status: draft
triggers:
  - LifecycleNode
  - rclcpp_lifecycle
  - on_configure
  - on_activate
  - on_deactivate
  - on_cleanup
source: "https://docs.ros.org/en/humble/Concepts/About-Lifecycle.html"
verify_notes: "请核对四个主要状态（Unconfigured, Inactive, Active, Finalized）与过渡回调函数名。"
---

## short
`rclcpp_lifecycle::LifecycleNode` 为节点提供标准化的四态生命周期状态机：`Unconfigured`（未配置）→ `Inactive`（已加载资源但未激活发布）→ `Active`（正常运行）→ `Finalized`（已销毁），确保多节点按确定顺序初始化与启停。

## detail
普通 `rclcpp::Node` 在构造函数里直接开始收发话题，难以保证复杂系统中各模块就绪顺序。LifecycleNode 将资源分配放在 `on_configure()`，将开启发布者/定时器放在 `on_activate()`，停止输出放在 `on_deactivate()`，广泛用于 Nav2 及语音/控制主干服务。

## pitfalls
- `LifecyclePublisher` 在节点处于 `Inactive` 状态时调用 `publish()` 不会实际发出消息。
- 在构造函数而非 `on_configure` 中执行重型硬件初始化会失去生命周期受控管理的效果。

## reading_hints
继承自 `rclcpp_lifecycle::LifecycleNode` 的类，阅读顺序应按 `on_configure` → `on_activate` → `on_deactivate` → `on_cleanup` 追踪其资源生命周期。
