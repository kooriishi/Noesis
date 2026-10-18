---
id: llm.messages_roles
title: 对话消息角色（system / user / assistant / tool）与多轮拼接
domain: llm_app
status: draft
triggers:
  - system_prompt
  - messages
  - role
  - ChatPromptTemplate
source: ""
verify_notes: "请核对部分推理模型或接口对 system 角色位置及连续相同角色的限制。"
---

## short
标准对话接口接收有序 `messages` 数组，包含四种核心角色：`system`（设定全局身份、边界与输出格式）、`user`（用户输入或附带上下文）、`assistant`（模型历史回复或发起的 `tool_calls`）、`tool`（工具执行结果回传）。

## detail
LLM 本身无状态，所谓“多轮对话记忆”是由应用层在每次请求时将 `system` 提示词、历史 `user/assistant` 轮次和最新 `user` 提问重新拼接后一并发送给模型。

## pitfalls
- 多轮历史只追加不清理，或者把庞大的中间工具原始输出永久留在历史消息中，会迅速撑爆上下文。
- 当 `assistant` 消息包含 `tool_calls` 时，紧随其后的消息必须包含对应 `tool_call_id` 的 `tool` 角色消息，否则接口报错。

## reading_hints
查看构造 `messages` 数组的代码，重点关注 `system` 消息从哪里加载、历史保留几轮、以及用户在打断（barge-in）时未播完的回复如何处理。
