---
id: ros2.interfaces
title: ROS 2 接口定义（.msg / .srv / .action）与 C++/Python 类型映射
domain: ros2
status: draft
triggers:
  - rosidl_default_generators
  - SharedPtr
  - msg::
  - srv::
  - AudioFrame
  - VoiceEvent
source: "https://docs.ros.org/en/humble/Concepts/About-ROS-Interfaces.html"
verify_notes: "请核对 CamelCase 接口文件名到 C++ snake_case 头文件名的转换规则。"
---

## short
ROS 2 自定义接口通常独立放在 `*_interfaces` 包中。文件名为大驼峰（如 `AudioFrame.msg`、`CancelTurn.srv`），在 C++ 中生成的头文件自动转为下划线蛇形命名（`#include "pkg/msg/audio_frame.hpp"`，类型为 `pkg::msg::AudioFrame`），在 Python 中为 `from pkg.msg import AudioFrame`。

## detail
- `.msg`：每行 `类型 字段名 [默认值]`，或 `类型 常量名=值`。
- `.srv`：上方为 Request 字段，中间以 `---` 分隔，下方为 Response 字段。C++ 服务回调签名为 `(const std::shared_ptr<SrvT::Request> req, std::shared_ptr<SrvT::Response> resp)`。
- `.action`：由 Goal、Result、Feedback 三部分组成，用两行 `---` 分隔。

## pitfalls
- C++ 包含自动生成的接口头文件时，必须把大驼峰文件名转成小写加下划线（例如 `VoiceEvent.msg` 对应 `voice_event.hpp`），否则编译报找不到头文件。
- 修改 `.msg/.srv` 后需重新编译接口包并 `source install/setup.bash`。

## reading_hints
读复杂 ROS 2 系统时，先看 `*_interfaces` 包下的 `.msg` 和 `.srv`，就能掌握整个系统模块间通信的数据契约与常量枚举。
