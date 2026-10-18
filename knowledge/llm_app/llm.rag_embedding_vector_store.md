---
id: llm.rag_embedding_vector_store
title: 向量嵌入（Embedding）与向量数据库（Vector Store）
domain: llm_app
status: draft
triggers:
  - OpenAIEmbeddings
  - HuggingFaceEmbeddings
  - Chroma
  - FAISS
  - Qdrant
  - Milvus
source: ""
verify_notes: "请核对所用向量库的相似度度量方式（Cosine / L2 / Inner Product）。"
---

## short
Embedding 模型将文本映射为高维稠密向量，向量数据库（如 FAISS、Chroma、Qdrant、Milvus）通过近似最近邻（ANN）索引快速查找与查询向量距离最近的 Top-K 文档片段。

## detail
不同 Embedding 模型的输出维度、最大输入长度及是否做归一化各不相同。更换 Embedding 模型后必须全量重建向量索引。

## pitfalls
- 未对未归一化的向量使用内积（IP）代替余弦相似度（Cosine）。
- 将超长文本直接塞给最大支持 512 Token 的嵌入模型，导致后半部分被静默截断。

## reading_hints
定位向量库初始化处传入的 `embedding_function` / `embeddings` 对象及集合名称（`collection_name`）。
