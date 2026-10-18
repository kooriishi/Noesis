---
id: ros2.cli_debug
title: ROS 2 常用命令行调试工具与在线排查方法
domain: ros2
status: draft
triggers:
  - ros2 topic
  - ros2 node
  - ros2 service
  - ros2 param
  - rqt_graph
source: "https://docs.ros.org/en/humble/Tutorials/Beginner-CLI-Tools.html"
verify_notes: "请核对 ros2 topic info -v 与 ros2 wtf / doctor 在 Humble 中的常用参数。"
---

## short
ROS 2 CLI 提供运行时自省工具：`ros2 node list/info` 查看节点及其端点，`ros2 topic list/echo/hz/info -v` 检查话题数据、频率与 QoS，`ros2 service call` 手动触发服务，`ros2 param list/get/set` 查看与动态修改参数。

## detail
当静态代码阅读确定了某个节点发布或订阅某个话题后，可在运行环境通过：
- `ros2 topic info -v <topic_name>`：核对当前发布者和订阅者的节点名、可靠性（RELIABLE/BEST_EFFORT）和持久性是否匹配。
- `ros2 topic hz <topic_name>`：验证音频采集帧或传感器数据是否按预期帧率输出。
- `ros2 service call /service_name pkg/srv/Type "{...}"`：模拟上游编排器直接测试单节点行为。

## pitfalls
- 调试终端未执行 `source install/setup.bash` 时，`ros2 topic echo` 无法解析自定义 `.msg` 类型。

## reading_hints
结合代码中的话题名、服务名和 `.msg/.srv` 字段名，可以直接构造 `ros2 topic echo` 或 `ros2 service call` 命令进行单模块隔离验证。
