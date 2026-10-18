---
id: ros2.simulation_ecosystem
title: ROS 2 Humble 仿真、控制与导航生态概览
domain: ros2
status: draft
triggers:
  - ros2_control
  - controller_manager
  - nav2
  - moveit
  - gazebo
  - urdf
  - xacro
source: "https://docs.ros.org/en/humble/Releases/Release-Humble-Hawksbill.html"
verify_notes: "本条目涉及仿真与外围生态框架印象，请根据团队实际使用的仿真器（Gazebo Classic / Ignition Fortress / Isaac Sim）与控制框架核对。"
---

## short
ROS 2 Humble 工程常结合 URDF/xacro 机器人模型、`ros2_control` 硬件抽象与控制器管理器、Nav2 导航栈以及 MoveIt 2 运动规划框架。URDF 定义连杆（link）与关节（joint），由 `robot_state_publisher` 结合关节状态发布 TF 树。

## detail
在复合机器人工程中，语音/大模型交互节点通常作为高层任务编排器（或产生业务指令事件），通过话题、服务或 Action 与底盘导航（Nav2）、机械臂（MoveIt 2 / `ros2_control`）或表情/音频硬件节点协同。

## pitfalls
- Xacro 宏文件需先展开为 URDF XML 才能传给 `robot_state_publisher` 的 `robot_description` 参数。

## reading_hints
看到 `.urdf` / `.xacro`、`controller_manager` 或业务结果/工具调用话题（如 `/voice/tool_calls`、`/voice/business_results`），可重点关注高层交互节点如何与外部控制模块解耦对接。
