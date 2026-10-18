---
id: llm.tool_calling
title: 工具调用（Tool / Function Calling）协议与执行闭环
domain: llm_app
status: draft
triggers:
  - tool_calls
  - bind_tools
  - StructuredTool
  - "@tool"
  - tool_call_topic
source: ""
verify_notes: "请核对所用模型对并行 tool_calls（一次返回多个工具调用）的支持情况。"
---

## short
工具调用（Tool Calling）并非由模型直接执行代码，而是：应用向模型声明工具列表（名称、描述、参数 JSON Schema）→ 模型决定是否产出 `tool_calls`（函数名与参数 JSON）→ 应用程序在本地安全执行函数 → 将结果作为 `role: "tool"` 消息回传模型继续生成最终回答。

## detail
一个完整的工具调用循环必须包含：工具 Schema 注册、参数反序列化与路径/权限校验、单次执行超时控制、结果长度截断、重复调用检测以及最大循环步数上限（防止模型陷入死循环）。

## pitfalls
- 未限制最大循环步数或未检测完全相同参数的重复调用，导致模型反复调同一工具耗尽配额。
- 直接信任模型生成的路径或命令参数而未做沙箱与根目录校验，存在目录穿越或命令注入风险。

## reading_hints
查找 `tools=`、`@tool`、`tool_calls` 分发字典或 ROS 2 中的 `/voice/tool_calls` 话题，追踪工具从模型意图产生到本地执行再到结果回传的全链路。
