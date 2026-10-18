---
id: llm.agent_patterns
title: 智能体编排模式（ReAct 循环 / 状态机图 StateGraph）
domain: llm_app
status: draft
triggers:
  - StateGraph
  - add_node
  - add_conditional_edges
  - create_react_agent
  - AgentExecutor
  - VoiceOrchestratorNode
source: ""
verify_notes: "请核对项目采用的编排方式（LangGraph StateGraph、自写 while 循环或 C++ 语音对话状态机）。"
---

## short
智能体（Agent）编排主要有两类：一是基于 `while` 循环的 ReAct 模式（思考 → 调工具 → 观察结果 → 直至完成或达步数上限）；二是显式状态机图（如 LangGraph `StateGraph` 或机器人对话轮次状态机），通过固定节点（Nodes）和条件边（Conditional Edges）控制状态流转。

## detail
在生产级工程中，为了避免完全自由的 Agent 循环不可控，常将主干流程建模为受控状态机（例如：唤醒监听 → 语音活动检测 VAD → ASR 识别 → LLM 生成 → TTS 播报与打断检测），只在特定节点内开放受限工具调用。

## pitfalls
- 状态机缺少超时回退边或异常恢复分支，导致某一步网络超时后会话卡死在中间态。
- 多轮异步事件（如旧轮次的延迟 TTS 数据包）未校验 `turn_id` / `generation_id` 就直接播放，造成新旧回答串音。

## reading_hints
先找出状态结构体（如 `AgentState` 或 `TurnState` / `Correlation`），再顺着状态转移函数或 `add_conditional_edges` 理清正常路径与打断/取消路径。
