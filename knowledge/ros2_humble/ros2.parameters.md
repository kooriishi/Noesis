---
id: ros2.parameters
title: ROS 2 节点参数声明、YAML 格式与覆盖优先级
domain: ros2
status: draft
triggers:
  - declare_parameter
  - get_parameter
  - ros__parameters
  - DeclareLaunchArgument
  - LaunchConfiguration
source: "https://docs.ros.org/en/humble/Concepts/About-ROS-2-Parameters.html"
verify_notes: "请核对 launch 文件中多份 YAML 与命令行 -p 参数的覆盖先后顺序。"
---

## short
ROS 2 节点默认必须先调用 `declare_parameter` 声明参数及默认值，再通过 `get_parameter` 读取。配置文件采用 `<node_name>: ros__parameters:`（或通配符 `/**: ros__parameters:`）层级结构，支持在 launch 启动时注入覆盖。

## detail
在大型 ROS 2 工程中，话题名、模型路径、超时阈值、提示词等配置通常不在 C++/Python 代码里写死，而是由节点启动时通过 `declare_parameter` 声明默认值，再由 `config/*.yaml` 统一注入。注意 YAML 顶层键必须与运行时的节点名（或 `/**`）一致，否则参数不会生效而静默退回代码里的默认值！

## pitfalls
- YAML 顶层节点名与 launch 中重命名后的 `name=` 不匹配时，整份 YAML 参数会被静默忽略（除非使用 `/**`）。
- 整数与浮点数类型在 YAML 中写错（如 `1` 写成 `1.0` 或反之）会导致 `ParameterTypeException`。

## reading_hints
读节点构造函数时，顺着 `declare_parameter("xxx", default_val)` 找到对应的 `config/*.yaml`，优先看 YAML 中的真实生效值而非代码里的硬编码默认值。
