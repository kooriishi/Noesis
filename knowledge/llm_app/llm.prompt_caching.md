---
id: llm.prompt_caching
title: 提示词前缀缓存（Prompt Caching）与消息排布设计
domain: llm_app
status: draft
triggers:
  - cache_control
  - prompt_tokens_details
  - cached_tokens
source: ""
verify_notes: "不同服务商的自动前缀缓存最小 Token 阈值与计费倍率不同，请对照所用网关核对。"
---

## short
现代推理引擎与云端模型服务支持基于公共前缀的 KV Cache 复用（Prompt Caching）。组装请求时，应将固定不变的内容（System 提示词、工具列表、静态领域知识）放在最前面，而将每次变化的动态选区或最新提问放在末尾，以最大化缓存命中率并降低首字延迟。

## detail
如果在 System 提示词开头插入了当前秒级时间戳或随机请求 ID，会导致整个前缀从第一个 Token 起全部缓存失效，每次请求都被迫重新计算全部预填充（Prefill）。

## pitfalls
- 在 System Prompt 顶部拼入每次请求都变化的动态时间戳或随机数，彻底破坏前缀缓存命中。

## reading_hints
观察提示词组装顺序是否遵循“静态系统指令在前 → 稳定工具与项目背景在中 → 动态对话与选区在后”。
