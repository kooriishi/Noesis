---
id: ros2.topic_service_action
title: 话题、服务与动作（Topic / Service / Action）的选型与底层构成
domain: ros2
status: draft
triggers:
  - create_service
  - create_client
  - rclcpp_action
  - ActionServer
  - ActionClient
  - .msg
  - .srv
  - .action
source: "https://docs.ros.org/en/humble/Concepts/About-Communication-Interfaces.html"
verify_notes: "请核对 Action 在底层展开的 3 个服务与 2 个话题的具体命名后缀。"
---

## short
ROS 2 提供三种核心通信原语：话题（Topic）用于多对多单向连续数据流；服务（Service）用于短暂的同步请求-响应（如打断当前对话轮次 `CancelTurn`）；动作（Action）用于耗时较长、可取消且带周期反馈的任务（如导航或机械臂运动）。

## detail
- **Topic (.msg)**：发布-订阅解耦，适合音频帧流、ASR 实时转写、VAD 状态广播。
- **Service (.srv)**：由请求和响应两部分（`---` 分隔）组成，适合配置切换、会话控制（如 `CancelTurn.srv`、`SetSessionMode.srv`）。服务实现不应长时阻塞。
- **Action (.action)**：由目标（Goal）、结果（Result）、反馈（Feedback）三段组成，底层由发送目标服务、取消目标服务、获取结果服务以及反馈话题、状态话题组合而成。

## pitfalls
- 在耗时数秒以上的任务中使用 Service 会阻塞调用端或导致超时，应改用 Action 或异步事件话题。
- 多个节点注册同名 Service 时，客户端请求落到哪个服务端是不确定的。

## reading_hints
看到 `msg/`、`srv/`、`action/` 目录下的接口文件，分别对应 `create_publisher/subscription`、`create_service/client` 和 `rclcpp_action::create_server/client`。
