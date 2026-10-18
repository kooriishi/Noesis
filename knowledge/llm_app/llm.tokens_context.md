---
id: llm.tokens_context
title: Token 分词机制与上下文窗口预算
domain: llm_app
status: draft
triggers:
  - max_tokens
  - tiktoken
  - context_window
  - token
  - history_turns
source: ""
verify_notes: "不同模型分词器对中英文比例的 token 折算存在差异，请按目标模型核对。"
---

## short
大语言模型以 Token（词元）为单位处理文本。输入提示词（System + History + Context）与模型生成的输出共同占用模型的最大上下文窗口（Context Window）。当对话或检索上下文过长时，必须执行截断、滑动窗口或滚动摘要。

## detail
不同模型使用不同的分词器（Tokenizer），英文通常约 3~4 个字符折合 1 个 Token，中文通常 1~2 个汉字折合 1~2 个 Token。无限制地累加历史消息会导致首字延迟（TTFT）线性或超线性上升、成本剧增，并在超出窗口上限时直接触发 400 错误。

## pitfalls
- 将 `max_tokens`（单次输出上限）误当作整个上下文窗口大小，未给输入留足余量。
- 按字符数直接等价为 Token 数做上限截断，在中文或代码密集场景下容易超限。

## reading_hints
看到 `history_turns`、`max_tokens`、消息列表切片或滚动摘要逻辑时，说明代码正在控制上下文窗口预算。
