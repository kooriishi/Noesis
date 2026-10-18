---
id: llm.safety_guardrails
title: 提示词注入防护、出站脱敏与最小权限工具沙箱
domain: llm_app
status: draft
triggers:
  - OutboundFilter
  - api_key
  - sanitize
  - is_sensitive_path
source: ""
verify_notes: "请核对项目出站过滤规则覆盖的敏感文件名与密钥前缀模式。"
---

## short
大模型应用安全包括三道防线：1. **出站脱敏（Outbound Filter）**：严禁读取 `.env`/私钥文件，并在发送前剥离 `sk-...`/`ghp_...`/Bearer 等凭据；2. **不可信数据隔离**：将外部文档与工具输出标记为不可信数据，防止间接提示词注入；3. **工具最小权限**：只读工具限制在项目根目录内，禁止任意命令执行或外网越权请求。

## detail
密钥绝不能硬编码在源码、日志或前端 `localStorage` 中，应由后端安全存储管理或从环境变量按需读取。

## pitfalls
- 将真实 API Key 写进配置文件或测试代码并提交到 Git 仓库。
- 工具执行前未先 `canonicalize` 校验根目录前缀，被 `../../` 路径穿越读取系统敏感文件。

## reading_hints
检查所有对外 HTTP 请求和文件读取入口，确认是否统一经过了路径校验与出站敏感信息过滤。
