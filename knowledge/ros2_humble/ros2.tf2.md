---
id: ros2.tf2
title: TF2 坐标变换树与常见坐标系约定
domain: ros2
status: draft
triggers:
  - tf2_ros
  - TransformBroadcaster
  - StaticTransformBroadcaster
  - Buffer
  - TransformListener
  - lookupTransform
  - base_link
source: "https://docs.ros.org/en/humble/Concepts/About-Tf2.html"
verify_notes: "请核对 REP-105 中 map -> odom -> base_link 的标准职责划分。"
---

## short
TF2 维护机器人各部件随时间变化的坐标变换树（有向无环树）。动态变换发往 `/tf`（Volatile），静态刚体安装偏置发往 `/tf_static`（TransientLocal）。标准移动机器人坐标系通常遵循 `map -> odom -> base_link -> sensor_frame`。

## detail
- `odom -> base_link` 由里程计连续发布，短期平滑但长期有累计漂移。
- `map -> odom` 由定位模块（如 AMCL 或 SLAM）发布，用于修正里程计漂移。
- 查询坐标变换使用 `tf2_ros::Buffer` + `TransformListener`，调用 `lookupTransform(target_frame, source_frame, time, timeout)`。

## pitfalls
- 两个节点同时发布同一个子坐标系（child_frame_id）的父变换会导致 TF 树抖动错乱。
- 查询最新变换时传入当前系统时间而非 `tf2::TimePointZero`（或传感器时间戳加等待超时）常触发 `ExtrapolationException`。

## reading_hints
看到 `TransformBroadcaster`、`StaticTransformBroadcaster` 或 URDF 中的 `<joint>`、`<link>`，即可梳理机器人的空间坐标树结构。
