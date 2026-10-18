---
id: llm.streaming_api
title: SSE 流式输出（Server-Sent Events）、分句缓冲与打断取消
domain: llm_app
status: draft
triggers:
  - CompleteStream
  - qwen_sse
  - sentence_delimiters
  - stream
  - EventSource
source: ""
verify_notes: "请核对流式 SSE 增量块中跨网络包切分 UTF-8 字符及语音合成按标点分句的逻辑。"
---

## short
流式对话通过 HTTP SSE（`data: {...}\n\n`，结束标志 `data: [DONE]`）或 WebSocket 逐块返回生成的增量文本（`delta.content`）。在实时语音机器人或 IDE 对话中，流式输出能将首字/首句响应延迟从数秒缩短至百毫秒级，并支持中途即时取消（Cancel / Barge-in）。

## detail
在“ASR → LLM(SSE) → TTS”级联语音架构（如 `CascadeNode`）中，客户端不会等 LLM 整段话生成完才调 TTS，而是边收 SSE `delta` 边按标点符号（`sentence_delimiters: ["。", "！", "？"]`）切分出完整短句，立即送入 TTS 边合边播；一旦检测到用户打断，立刻通过取消标志中止 SSE 流与播放队列。

## pitfalls
- 对每个网络字节块单独做 UTF-8 解码，当一个 3 字节汉字恰好被 TCP 分包切在两个块中间时会产生乱码或崩溃。
- 用户取消或开启新轮次后未丢弃旧流回调，导致旧回复继续写入界面或播报。

## reading_hints
看到 `CompleteStream`、`data: [DONE]`、`sentence_delimiters` 或 `cancel_requested` 原子标志时，重点追踪流式增量缓冲、按句切分和打断清理逻辑。
