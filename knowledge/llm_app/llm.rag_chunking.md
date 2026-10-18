---
id: llm.rag_chunking
title: 文档分块策略（chunk_size / chunk_overlap / 结构切分）
domain: llm_app
status: draft
triggers:
  - RecursiveCharacterTextSplitter
  - chunk_size
  - chunk_overlap
  - split_documents
source: ""
verify_notes: "请核对项目中具体设置的 chunk_size 与 chunk_overlap 数值。"
---

## short
文档分块（Chunking）决定检索的基本粒度：`chunk_size` 控制单块最大长度，`chunk_overlap` 控制相邻块的重叠区以避免跨边界语义被切断。代码与 Markdown 文档宜优先按语法结构（类/函数、标题层级）切分。

## detail
`chunk_size` 过小会导致单个片段缺失主语或前置条件；`chunk_size` 过大则会把大量无关噪声混入同一个向量，降低嵌入向量的区分度并浪费提示词预算。

## pitfalls
- `chunk_overlap` 设为 0 时，恰好跨在分块边界上的关键句子或函数定义会被劈成两半。
- 用普通按字符切分器直接切源代码，把一个函数体切得支离破碎。

## reading_hints
查看切分器初始化参数中的 `chunk_size`、`chunk_overlap` 和 `separators` 列表。
