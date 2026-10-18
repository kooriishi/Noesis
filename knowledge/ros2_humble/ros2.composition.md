---
id: ros2.composition
title: 组件化节点（Composition）与进程内零拷贝通信
domain: ros2
status: draft
triggers:
  - RCLCPP_COMPONENTS_REGISTER_NODE
  - ComposableNodeContainer
  - ComposableNode
  - use_intra_process_comms
source: "https://docs.ros.org/en/humble/Concepts/About-Composition.html"
verify_notes: "请核对 use_intra_process_comms 开启零拷贝对 std::unique_ptr 发布与订阅签名的要求。"
---

## short
组件化（Composition）允许将多个 `rclcpp::Node` 编译为共享库（`.so`/`.dll`）并加载进同一个容器进程（`ComponentContainer`），配合 `use_intra_process_comms(true)` 和 `std::unique_ptr<MsgT>` 实现进程内零拷贝内存传递。

## detail
对于高频音频帧、点云或图像流水线，跨进程序列化与拷贝开销较大。组件化要求节点提供接受 `const rclcpp::NodeOptions &` 的构造函数，并通过 `RCLCPP_COMPONENTS_REGISTER_NODE` 宏注册插件，从而既可独立启动，也可在 launch 中通过 `ComposableNodeContainer` 合入单进程运行。

## pitfalls
- 开启 `use_intra_process_comms(true)` 后，若发布或订阅仍传值/共享指针而非 `std::unique_ptr`，仍会发生内存拷贝。
- 同一容器内某个组件发生段错误（Segfault）会导致整个容器内所有节点一同崩溃。

## reading_hints
在源文件末尾看到 `RCLCPP_COMPONENTS_REGISTER_NODE(...)` 或构造函数接收 `const rclcpp::NodeOptions & options`，说明该节点支持组件化加载。
