---
id: llm.eval_observability
title: 大模型应用评测（Evaluation）与链路追踪（Trace ID）
domain: llm_app
status: draft
triggers:
  - trace_id
  - session_id
  - turn_id
  - generation_id
  - Correlation
source: ""
verify_notes: "请核对项目中 Correlation (session_id / turn_id / generation_id / trace_id) 的关联传递方式。"
---

## short
多阶段大模型/语音系统（采集 → VAD → ASR → LLM → TTS → 播报）必须通过统一的关联标识（如 `session_id`、`turn_id`、`generation_id`、`trace_id`）贯穿全链路日志与事件，才能准确统计各阶段延迟（TTFT、端到端耗时）并过滤已被打断的过期轮次数据。

## detail
除了在线链路追踪外，提示词或检索策略修改后应通过离线评测集（对比参考要点覆盖率、错误主张数与引用核验率）做回归验证，避免“改好一个例子却弄坏十个场景”。

## pitfalls
- 异步流水线各节点之间只传裸文本/裸音频而不传 `turn_id` / `generation_id`，导致无法区分某段音频属于哪一轮对话。

## reading_hints
看到 `Correlation`、`trace_id_prefix`、`turn_id`、`generation_id` 时，关注它是如何在请求发起时生成、在事件广播中透传、以及在收到新轮次时用来淘汰旧轮次任务的。
