---
id: llm.sampling_params
title: 采样与生成控制参数（temperature / top_p / max_tokens）
domain: llm_app
status: draft
triggers:
  - temperature
  - top_p
  - max_tokens
  - stop
  - enable_thinking
source: ""
verify_notes: "请核对所用服务商对 temperature=0 的确定性保障及 enable_thinking 等扩展字段语义。"
---

## short
`temperature` 控制输出概率分布的平滑度（越低越确定，适合代码解析、工具调用与 JSON 输出；越高越多样，适合闲聊创作）；`top_p` 控制核采样累积概率阈值；`max_tokens` 限制单次回复的最大生成长度。

## detail
- 结构化输出、事实问答、代码分析通常设 `temperature` 在 `0.0 ~ 0.3` 之间，以减少幻觉和格式漂移。
- 实时语音对话常同时限制 `max_tokens` 并在系统提示词中要求短句输出，以降低 TTS 合成等待和单轮播报时长。

## pitfalls
- 同时大幅调整 `temperature` 和 `top_p` 会导致采样行为难以预测，通常建议只调其一。
- `max_tokens` 设得过小会导致 JSON 输出在闭合花括号前被截断（`finish_reason: "length"`）。

## reading_hints
在 `chat.completions.create` 或配置文件（如 `cascade.yaml`）中检查 `temperature` 和 `max_tokens`，可判断该调用侧重严谨确定性还是发散生成。
