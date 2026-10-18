---
id: llm.memory
title: 对话短期记忆、滑动窗口与滚动摘要
domain: llm_app
status: draft
triggers:
  - history_turns
  - summary
  - memory
  - ConversationBuffer
source: ""
verify_notes: "请核对项目短期记忆采用固定轮数窗口还是增量摘要策略。"
---

## short
大模型应用的记忆分为短期记忆（当前会话最近 N 轮消息 + 早期对话的滚动摘要）和长期记忆（跨会话的结构化数据库或向量检索）。通过限制保留轮数（如 `history_turns: 5`）或增量压缩早期对话，可在保持上下文连贯的同时控制 Token 消耗。

## detail
当对话持续几十轮后，全量发送历史消息既慢又贵。常见做法是保留最近 K 条完整消息以维持当前语境，同时将更早的对话压缩为结构化摘要（记录用户目标、关键文件与已定结论），或者在语音交互中按固定轮数滑动窗口裁剪。

## pitfalls
- 裁剪历史消息时从某次工具调用中间截断，导致残留孤立的 `tool` 消息引发 API 报错。
- 摘要任务若同步阻塞主对话请求，会显著增加用户感知的响应延迟。

## reading_hints
关注历史消息容器（如 `std::deque` 或 Python `list`）何时入队、何时按 `history_turns` 或 Token 预算弹出旧消息。
