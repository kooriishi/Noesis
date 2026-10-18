---
id: llm.rag_retrieval_rerank
title: 检索召回（Top-K / 混合检索）与重排序（Rerank）
domain: llm_app
status: draft
triggers:
  - search_kwargs
  - rerank
  - CrossEncoder
  - top_k
source: ""
verify_notes: "请核对检索管线中召回 k 值与重排后保留的最终片段数。"
---

## short
纯向量检索擅长语义相似但对专有名词、错误码、函数名等精确字面匹配较弱。工程中常采用“向量检索 + BM25 关键词混合检索”扩大初步召回（如 Top-20），再用交叉编码器（Cross-Encoder Reranker）精排取 Top-5 送入模型。

## detail
双塔 Embedding 模型分别独立编码查询与文档，速度快但交互浅；Reranker 将查询与候选文档拼接后联合注意力打分，精度更高但耗时较长，适合在粗召回后的小集合上精排。

## pitfalls
- 直接把粗召回的几十个片段全部塞进提示词，触发模型“中间迷失（Lost in the Middle）”现象且推高延迟。

## reading_hints
检查 `search_kwargs={"k": ...}` 以及检索结果返回后是否经过二次排序或阈值过滤。
