---
id: ros2.executor_callback_groups
title: 执行器（Executor）与回调组（Callback Group）并发模型
domain: ros2
status: draft
triggers:
  - SingleThreadedExecutor
  - MultiThreadedExecutor
  - CallbackGroup
  - MutuallyExclusive
  - Reentrant
  - create_callback_group
  - spin
source: "https://docs.ros.org/en/humble/Concepts/About-Executors.html"
verify_notes: "请核对 Humble 中默认回调组行为以及在订阅回调中同步等待 Service 响应死锁的典型场景。"
---

## short
ROS 2 节点的定时器、订阅、服务回调由执行器（Executor）调度。每个节点默认只有一个 `MutuallyExclusive`（互斥）回调组，即使使用 `MultiThreadedExecutor`，同一互斥组内的回调也不会并发执行。在回调中同步等待另一个回调结果会造成死锁。

## detail
- `SingleThreadedExecutor`（如 `rclcpp::spin(node)`）在单线程内顺序执行所有就绪回调，若某个回调里执行了耗时 HTTP/SSE 请求或阻塞 I/O，整个节点的其它订阅和心跳都会卡住。
- 若希望多个回调并发执行，需使用 `MultiThreadedExecutor` 并将需要并发的订阅/服务放入 `Reentrant` 回调组或不同的 `MutuallyExclusive` 回调组，或者将长耗时任务放入独立工作线程（如 `std::thread` + 队列）。

## pitfalls
- 在默认单线程执行器或同一 `MutuallyExclusive` 回调组的回调内部调用 `spin_until_future_complete` 或阻塞等待同节点客户端响应会导致死锁。
- 在高频音频回调里直接做同步网络请求会造成音频丢帧。

## reading_hints
看到 `create_callback_group`、`MultiThreadedExecutor`、或者节点内部自己维护 `std::thread` + `std::mutex` + `std::condition_variable` 工作线程（如语音节点将音频帧推入队列异步请求 ASR/LLM），都是在解决回调并发与阻塞问题。
