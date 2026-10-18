---
id: ros2.names_namespaces_remap
title: 节点命名、命名空间与话题重映射（Remapping）
domain: ros2
status: draft
triggers:
  - remappings
  - namespace
  - PushRosNamespace
  - --ros-args
  - -r
source: "https://docs.ros.org/en/humble/Concepts/About-ROS-2-Names.html"
verify_notes: "请核对相对名称、绝对名称（/开头）与私有名称（~/开头）在命名空间下的展开规则。"
---

## short
ROS 2 名称分为绝对名称（以 `/` 开头，不受命名空间影响）、相对名称（无前导 `/`，自动拼上节点 namespace）和私有名称（以 `~/` 开头，拼上节点完整路径）。Launch 文件可通过 `remappings=[('原名', '新名')]` 在不改源码的情况下重定向话题或服务。

## detail
在阅读多模块协同工程时，源码里的字符串可能只是默认话题名，或者通过参数配置读取；而 `launch/*.launch.py` 中的 `namespace` 和 `remappings` 会改变运行时的实际话题拓扑。

## pitfalls
- 将本应随多机命名空间隔离的话题写成了以 `/` 开头的绝对路径，导致多机器人实例话题冲突。
- Launch 中做了 `remappings`，但看代码时只搜原话题名导致找不到下游订阅者。

## reading_hints
检查话题名是否以 `/` 或 `~/` 开头，并结合 `.launch.py` 中的 `remappings` 和 `config/*.yaml` 中的话题配置确认最终话题路径。
