---
id: ros2.dds_rmw
title: DDS 与 RMW 中间件抽象层
domain: ros2
status: draft
triggers:
  - RMW_IMPLEMENTATION
  - ROS_DOMAIN_ID
  - rmw_fastrtps_cpp
  - rmw_cyclonedds_cpp
source: "https://docs.ros.org/en/humble/Concepts/About-Internal-Interfaces.html"
verify_notes: "请核对 Humble 默认 RMW 为 Fast DDS (rmw_fastrtps_cpp) 以及局域网多播发现在企业网受限时的配置。"
---

## short
ROS 2 底层通信基于 DDS（数据分发服务），通过 RMW（ROS 中间件接口）屏蔽不同 DDS 厂商差异。Humble 默认使用 Fast DDS（`rmw_fastrtps_cpp`），可通过 `RMW_IMPLEMENTATION` 切换，并用 `ROS_DOMAIN_ID` 隔离不同机器/进程的发现域。

## detail
在 ROS 2 Humble 中，`rclcpp` 与 `rclpy` 调用统一的 C 语言 `rcl` 层，再通过 `rmw` 适配层对接具体 DDS 实现（如 Fast DDS 或 Cyclone DDS）。同一个局域网内具有相同 `ROS_DOMAIN_ID` 的节点通过 UDP 多播自动发现彼此的话题与服务。若两台机器上的节点“看不见对方”，通常是因为 `ROS_DOMAIN_ID` 不一致、防火墙拦截了 UDP 多播，或一端开启了 `ROS_LOCALHOST_ONLY=1`。

## pitfalls
- 不同节点的 `ROS_DOMAIN_ID` 或 `ROS_LOCALHOST_ONLY` 环境变量不一致会导致无法互相发现。
- 无线局域网或容器网络禁用多播时，默认 DDS 自动发现会失败。
- 同一工作空间混用不同 RMW 实现时可能出现发现或序列化兼容问题。

## reading_hints
在 launch 文件、启动脚本或 Dockerfile 中看到 `ROS_DOMAIN_ID`、`RMW_IMPLEMENTATION`、`FASTRTPS_DEFAULT_PROFILES_FILE` 时，说明项目正在配置底层 DDS 通信域或传输策略。
