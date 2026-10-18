---
id: ros2.timers_and_time
title: 定时器（WallTimer）与仿真时间（use_sim_time）
domain: ros2
status: draft
triggers:
  - create_wall_timer
  - create_timer
  - use_sim_time
  - rclcpp::Clock
  - /clock
source: "https://docs.ros.org/en/humble/Concepts/About-ROS-2-Client-Libraries.html"
verify_notes: "请核对 create_wall_timer 与 create_timer 在开启 use_sim_time=true 时的触发差异。"
---

## short
`create_wall_timer` 始终按操作系统真实物理时钟（Steady/System Clock）周期触发；而 `create_timer`（或 Python `self.create_timer`）在节点参数 `use_sim_time=true` 时会跟随 `/clock` 话题的仿真时间推进。

## detail
在硬件驱动、音频播放节拍控制或看门狗检查中，通常使用 `create_wall_timer` 保证固定物理时间间隔执行。而在 Gazebo 仿真或 rosbag 回放场景中，如果将 `use_sim_time` 设为 `true` 但没有任何节点发布 `/clock` 话题，依赖 ROS 时间的 `now()` 或 `create_timer` 将停滞不前。

## pitfalls
- 开启 `use_sim_time=true` 后若未播放带 `--clock` 的 bag 或未启动仿真器，依赖 ROS 时间的定时器不会触发。
- `create_wall_timer` 不受仿真倍速暂停影响。

## reading_hints
看到 `create_wall_timer(std::chrono::milliseconds(...), ...)` 时，关注该定时回调是在做周期性状态发布、超时看门狗检查还是队列轮询。
