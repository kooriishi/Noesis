---
id: llm.serving_web
title: Web 服务路由（FastAPI）与异步事件循环并发
domain: llm_app
status: draft
triggers:
  - FastAPI
  - APIRouter
  - StreamingResponse
  - async def
source: ""
verify_notes: "请核对 FastAPI 中 async def 路由内调用同步阻塞 SDK 时的线程阻塞行为。"
---

## short
FastAPI 基于 ASGI 异步事件循环。在 `async def` 路由处理函数中，应使用异步客户端（如 `AsyncOpenAI`、`httpx.AsyncClient`）并通过 `StreamingResponse` 推送流式生成；若在 `async def` 中直接调用耗时数秒的同步阻塞 HTTP 请求，会卡死整个事件循环线程。

## detail
如果必须调用同步 SDK 或本地 CPU 密集型推理，应使用普通 `def` 路由（FastAPI 会自动放入线程池执行）或通过 `asyncio.to_thread` 包装。

## pitfalls
- 在 `async def` 路由里直接调用同步 `requests.post` 或同步 `OpenAI().chat.completions.create`，导致单个慢请求阻塞所有并发用户。

## reading_hints
检查路由装饰器下的函数是 `async def` 还是 `def`，以及内部调用的客户端是否为异步版本。
