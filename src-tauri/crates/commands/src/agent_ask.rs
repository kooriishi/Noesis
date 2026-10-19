use noesis_llm::tools::{
    run_bounded_agent_loop, AgentLoopResult, AgentModelClient, AgentModelTurn, ReadOnlyToolRegistry,
    ToolCallItem, ToolDefinition,
};
use noesis_llm::{LlmConfig, LlmGateway};
use serde_json::{json, Value};

/// H5-d: 把 `LlmGateway` 包成一个同步的 `AgentModelClient`，让 `run_bounded_agent_loop` 能用真实模型。
///
/// 循环本身是同步的（便于确定性测试），而 HTTP 调用是异步的，所以这里在
/// `spawn_blocking` 线程里用 `Handle::block_on` 把异步请求「落地」成一次同步调用。
pub struct LlmAgentClient<'a> {
    gateway: &'a LlmGateway,
    config: LlmConfig,
    handle: tokio::runtime::Handle,
}

impl<'a> LlmAgentClient<'a> {
    pub fn new(
        gateway: &'a LlmGateway,
        config: LlmConfig,
        handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            gateway,
            config,
            handle,
        }
    }
}

impl<'a> AgentModelClient for LlmAgentClient<'a> {
    fn next_turn(
        &self,
        messages: &[Value],
        tools: &[ToolDefinition],
        force_final: bool,
    ) -> Result<AgentModelTurn, String> {
        let mut body = json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
            "stream": false
        });
        if !tools.is_empty() && !force_final {
            body["tools"] = Value::Array(
                tools
                    .iter()
                    .map(|t| t.to_openai_tool_json())
                    .collect::<Vec<_>>(),
            );
            body["tool_choice"] = json!("auto");
        }

        let cfg = self.config.clone();
        let gateway = self.gateway;
        let resp = self
            .handle
            .block_on(async move { gateway.send_chat_raw(&cfg, body).await })
            .map_err(|e| e.to_string())?;

        let message = &resp["choices"][0]["message"];
        if let Some(calls) = message["tool_calls"].as_array() {
            if !calls.is_empty() {
                let parsed: Vec<ToolCallItem> = calls
                    .iter()
                    .enumerate()
                    .map(|(idx, c)| ToolCallItem {
                        id: c["id"]
                            .as_str()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("call_{}", idx)),
                        name: c["function"]["name"].as_str().unwrap_or_default().to_string(),
                        arguments_json: c["function"]["arguments"]
                            .as_str()
                            .unwrap_or("{}")
                            .to_string(),
                    })
                    .filter(|c| !c.name.is_empty())
                    .collect();
                if !parsed.is_empty() {
                    return Ok(AgentModelTurn::ToolCalls(parsed));
                }
            }
        }

        Ok(AgentModelTurn::FinalAnswer(
            message["content"].as_str().unwrap_or_default().to_string(),
        ))
    }
}

/// H5-d: 智能问答的工具循环。返回最终回答文本 + 每一步工具调用记录。
#[allow(clippy::too_many_arguments)]
pub fn run_ask_agent_loop(
    project_root: &std::path::Path,
    project_id: &str,
    store: Option<&noesis_store::DbStore>,
    active_domains: &[String],
    messages: Vec<Value>,
    gateway: &LlmGateway,
    config: LlmConfig,
    handle: tokio::runtime::Handle,
    cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    mut on_step: impl FnMut(&noesis_llm::tools::AgentStepEvent),
) -> Result<AgentLoopResult, String> {
    let registry = ReadOnlyToolRegistry::new();
    let client = LlmAgentClient::new(gateway, config, handle);
    run_bounded_agent_loop(
        project_root,
        project_id,
        store,
        &registry,
        active_domains,
        messages,
        &client,
        6,
        cancel,
        |ev| on_step(ev),
    )
}
