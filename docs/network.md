# Noesis 网络行为与内网安全规范 (`docs/network.md`)

Noesis 专为企业研发与内网安全环境设计。本文档完整披露软件在运行期间的全部网络行为、脱敏策略与企业网络配置方法。

## 1. 零遥测、零外部资源承诺

1. **零遥测与零埋点**：Noesis 不集成任何第三方统计、崩溃上报或使用行为遥测 SDK（无 Sentry、Google Analytics、PostHog 等）。
2. **零外部静态资源请求**：所有前端图标（Lucide）、字体、语法高亮组件（CodeMirror 6）及领域知识库（`knowledge/ros2_humble`、`knowledge/llm_app`）均静态打包在本地安装程序中。
3. **严格 WebView CSP 隔离**：生产环境 Content Security Policy 锁定为：
   ```text
   default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost
   ```
   前端渲染进程（WebView）**被禁止发起任何外部 HTTP/HTTPS 请求**；所有模型通信均由 Rust 后端进程（`noesis-llm`）统一受控发起。

## 2. 唯一外发网络端点

Noesis 运行期间**唯一**可能产生外部网络流量的行为，是向用户在「设置 -> 模型服务与安全凭据管理」中显式配置的 `api_url` 发送 OpenAI 兼容协议请求（例如 `${api_url}/v1/chat/completions` 或 `${api_url}/v1/models`）。

- 当用户在设置中开启 **「完全离线模式 (`offline_mode: true`)」** 时，`LlmGateway` 会在发起任何 TCP 连接前直接拦截请求，保证 **100% 零网络外发**。
- 在离线模式下，文件浏览、Tree-sitter 多语言 AST 索引、ROS2/LLM 领域语义图谱、两阶段阅读路线生成、确定性事实练习题与填空题（`fact` / `cloze`）、SM-2 复习调度以及本地沙盒运行均可完整工作。

## 3. P0 级外发安全控制 (`OutboundFilter`)

所有发往 `api_url` 的请求消息以及只读工具（`ReadOnlyToolRegistry`）读取的源码结果，在离开本机前必须经过 `OutboundFilter` 强制审查：

1. **敏感文件硬拦截**：禁止读取与外发 `.env`、`.env.*`、`*.pem`、`*.key`、`id_rsa`、`.ssh/*`、`credentials.json`、`secrets.yaml` 等敏感路径。
2. **敏感凭证正则脱敏**：自动将文本中的 API Key（`sk-...`）、Bearer Token、私钥块（`-----BEGIN ... PRIVATE KEY-----`）、密码赋值语句、内网 IPv4 地址及电子邮箱替换为 `[REDACTED_*]` 占位符。
3. **单次外发体积上限**：单次请求总字符数硬上限为 `100,000` 字符，超出即刻阻断。

## 4. 企业代理与自签名根证书配置

针对企业内网隔离网关或私有化部署的大模型服务（如内网 vLLM / Ollama / TGI 网关）：

1. **HTTP/HTTPS/SOCKS5 代理 (`proxy_url`)**：
   - 可在「设置」中配置代理地址（如 `http://proxy.corp.local:8080` 或 `socks5://127.0.0.1:1080`）。
   - 若留空，`reqwest` 默认遵循操作系统环境变量（`HTTPS_PROXY` / `HTTP_PROXY` / `NO_PROXY`）。
2. **自定义根证书 (`custom_ca_cert_path`)**：
   - 支持配置企业自签名 PEM 根证书路径（如 `C:\certs\corp-root-ca.pem` 或 `/etc/ssl/certs/corp-ca.pem`）。
   - `LlmGateway::build_http_client` 会在初始化 TLS 握手前加载并信任该 PEM 证书。
3. **分步连接诊断 (`test_llm_connection`)**：
   - 点击「测试连接」后，系统会分步反馈 `ok`、`auth_failed`（401/403）、`model_not_found`（404）、`rate_limited`（429）、`tls_error`、`dns_or_proxy_error`、`timeout` 等精确诊断结论与延迟毫秒数。
