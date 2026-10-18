---
id: llm.rag_overview
title: 检索增强生成（RAG）离线写入与在线查询双路径
domain: llm_app
status: draft
triggers:
  - as_retriever
  - similarity_search
  - RetrievalQA
  - create_retrieval_chain
source: ""
verify_notes: "请核对项目中检索管线是否包含查询改写与重排阶段。"
---

## short
RAG（检索增强生成）由两条路径组成：1. **写入路径（Ingest）**：文档加载（Loader）→ 文本切分（Splitter）→ 向量化（Embedding）→ 写入向量库；2. **查询路径（Query）**：用户提问向量化 → 相似度检索 Top-K 片段 →（可选重排 Rerank）→ 拼入提示词交由 LLM 生成带依据的回答。

## detail
RAG 的核心价值是用外部可核验知识库替代模型参数记忆，减少事实幻觉并支持知识实时更新。

## pitfalls
- 写入路径和查询路径使用了不同版本或不同维度的 Embedding 模型，导致检索出的向量相似度完全失效。
- 检索出的文档块未附带源文件名与位置元数据，导致模型无法给出可核验引用。

## reading_hints
阅读 RAG 代码时，先区分哪些函数属于离线建库（调用 `from_documents` / `add_documents`），哪些属于在线检索（调用 `similarity_search` / `as_retriever`）。
