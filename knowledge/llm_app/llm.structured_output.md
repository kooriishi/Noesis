---
id: llm.structured_output
title: 结构化输出（JSON Mode / Schema 校验 / 容错重试）
domain: llm_app
status: draft
triggers:
  - response_format
  - json_object
  - json_schema
  - extractJsonObject
  - serde_json
source: ""
verify_notes: "不同网关对 response_format: json_schema 的支持程度不同，请按实际网关核对。"
---

## short
当下游程序需要解析模型输出时，应通过 `response_format`（JSON 模式或 JSON Schema）配合提示词约束模型输出严格 JSON，并在客户端实现容错提取（剥离 Markdown 代码围栏、花括号平衡扫描）、Schema 校验与失败自动修正重试。

## detail
即便在提示词中要求“只输出 JSON”，部分模型仍可能在 JSON 前后附带解释文字或 ` ```json ` 围栏，或在长输出时因 `max_tokens` 截断。稳健的工程实现从不直接假设 `JSON.parse` 必成功，而是先提取完整 JSON 对象，再校验必填字段，失败时报错或携带解析错误重试。

## pitfalls
- 直接对模型原始文本调 `JSON.parse` 且无围栏剥离与错误提示，一旦模型多说一句话就会解析失败。
- 将模型输出的 JSON 字段未经程序核验就直接当作已验证事实渲染。

## reading_hints
看到 `response_format`、`serde_json::from_str` 或 JSON 提取函数时，检查解析失败时的降级与重试路径是否明确。
