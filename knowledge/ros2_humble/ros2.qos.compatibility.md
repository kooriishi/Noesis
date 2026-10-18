---
id: ros2.qos.compatibility
title: 发布者与订阅者 QoS 兼容性规则
domain: ros2
status: draft
triggers:
  - create_subscription
  - create_publisher
  - QoS
  - best_effort
  - transient_local
  - tf_static
source: "https://docs.ros.org/en/humble/Concepts/About-Quality-of-Service-Settings.html#qos-compatibilities"
verify_notes: "请对照 Humble 文档核对可靠性与持久性的 2x2 兼容性矩阵。"
---

## short
ROS 2 发布者与订阅者的 QoS 必须满足兼容矩阵才能建立连接：发布者提供的保障级别必须不低于订阅者要求的级别。若发布者为 `BestEffort` 而订阅者要求 `Reliable`，或发布者为 `Volatile` 而订阅者要求 `TransientLocal`，双方将无法通信且默认不抛异常。

## detail
排查“话题名和消息类型都对，但订阅回调一直不触发”时，第一步应检查发布端与订阅端的 QoS 是否兼容。具体规则：
1. 可靠性：Reliable 发布者可匹配 Reliable 或 BestEffort 订阅者；BestEffort 发布者只能匹配 BestEffort 订阅者。
2. 持久性：TransientLocal 发布者可匹配 TransientLocal 或 Volatile 订阅者；Volatile 发布者只能匹配 Volatile 订阅者（例如订阅 `/tf_static` 必须配 TransientLocal 或兼容策略）。

## pitfalls
- 用默认 `rclcpp::QoS(10)`（Reliable）去订阅 `SensorDataQoS`（BestEffort）发布的传感器话题会导致收不到任何消息。
- 话题名拼写相差一个前导斜杠或命名空间也会表现为收不到消息，可用 `ros2 topic info -v <topic>` 同时检查端点数量和 QoS。

## reading_hints
对比同一个话题在发布端 `create_publisher` 和订阅端 `create_subscription` 传入的 QoS 参数是否一致或兼容。
