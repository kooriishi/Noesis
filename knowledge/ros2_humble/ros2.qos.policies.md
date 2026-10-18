---
id: ros2.qos.policies
title: ROS 2 QoS 服务质量策略与预设
domain: ros2
status: draft
triggers:
  - rclcpp::QoS
  - SensorDataQoS
  - SystemDefaultsQoS
  - KeepLast
  - BestEffort
  - Reliable
  - TransientLocal
  - sensor_qos_depth
source: "https://docs.ros.org/en/humble/Concepts/About-Quality-of-Service-Settings.html"
verify_notes: "请核对 SensorDataQoS 默认使用 BestEffort 与 Volatile 的具体队列深度。"
---

## short
ROS 2 QoS 控制话题与服务的传输语义，核心维度包括历史记录（KeepLast/KeepAll）、队列深度（depth）、可靠性（Reliable/BestEffort）和持久性（Volatile/TransientLocal）。常用预设包括默认 QoS（Reliable + KeepLast(10)）和 `SensorDataQoS`（BestEffort，适合高频音频/雷达流）。

## detail
高频传感器流（如音频帧 `AudioFrame`、激光雷达、相机图像）通常更关注低延迟而非重传旧包，因此常采用 `SensorDataQoS` 或较小队列深度；而控制指令、状态机事件（如 `VoiceEvent`、`PlaybackCommand`）不能丢失，通常采用 `Reliable` 可靠传输。`TransientLocal` 则允许晚启动的订阅者收到发布者缓存的最后一条消息（如 `/tf_static` 或机器人描述）。

## pitfalls
- 高频音频或图像流使用过大队列深度加 Reliable 时，网络抖动会导致严重延迟堆积。
- 状态或一次性配置话题若使用 Volatile，晚启动的节点将收不到初始状态。

## reading_hints
在 `create_publisher` 或 `create_subscription` 的第二个参数看到数字（表示 `KeepLast(depth)`）、`rclcpp::SensorDataQoS()`、`.reliable()`、`.best_effort()`、`.transient_local()` 时，即在配置 QoS。
