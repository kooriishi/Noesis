---
id: ros2.build_system
title: colcon 构建系统、package.xml 与 ament_cmake / ament_python
domain: ros2
status: draft
triggers:
  - ament_cmake
  - ament_python
  - ament_target_dependencies
  - rosidl_generate_interfaces
  - colcon
  - package.xml
source: "https://docs.ros.org/en/humble/Tutorials/Beginner-Client-Libraries/Colcon-Tutorial.html"
verify_notes: "请核对 Humble 中 ament_target_dependencies 与 target_link_libraries 的混用注意事项。"
---

## short
ROS 2 工作空间使用 `colcon build` 按 `package.xml` 声明的依赖拓扑顺序构建各包。C++ 包使用 `ament_cmake`（`CMakeLists.txt`），Python 包使用 `ament_python`（`setup.py`），接口包通过 `rosidl_generate_interfaces` 生成 C++/Python 消息绑定代码。

## detail
- `package.xml` 定义包名及 `<depend>`、`<build_depend>`、`<exec_depend>`，决定了 `colcon` 的包级编译顺序。
- `CMakeLists.txt` 中通过 `find_package` 查找依赖，`add_executable` / `add_library` 定义构建产物，在 Humble 中常见 `ament_target_dependencies`（或 `target_link_libraries`）绑定头文件与库，最后通过 `install(TARGETS ...)` 安装到 `install/` 目录供 `ros2 run` / `ros2 launch` 发现。

## pitfalls
- `CMakeLists.txt` 中写了 `find_package(xxx)` 但忘记在 `package.xml` 中加 `<depend>xxx</depend>`，会导致新环境首次 `colcon build` 时编译顺序错误。
- 新增 `launch/` 或 `config/` 目录后若未在 `CMakeLists.txt` 中写 `install(DIRECTORY launch config DESTINATION share/${PROJECT_NAME})`，运行 `ros2 launch` 时会找不到文件。

## reading_hints
先读 `package.xml` 了解包职责与依赖边界，再读 `CMakeLists.txt` / `setup.py` 找出生成了哪些可执行文件（`add_executable` / `console_scripts`）及其对应源码入口。
