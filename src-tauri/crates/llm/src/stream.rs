use crate::tools::ToolCallItem;
use crate::ChatMessage;
use noesis_learn::context::{HeuristicTokenEstimator, TokenEstimator};
use noesis_learn::verifier::CitationItem;
use noesis_store::StoredMessageRecord;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const CITATIONS_DELIMITER: &str = "<<<CITATIONS>>>";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreamUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    pub is_estimated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum StreamEvent {
    Start {
        request_id: String,
    },
    Delta {
        text: String,
    },
    Reasoning {
        text: String,
    },
    ToolStep {
        step: usize,
        tool_name: String,
        args_summary: String,
        result_summary: String,
        duration_ms: u64,
    },
    Final {
        content: String,
        citations: Vec<CitationItem>,
        unknowns: Vec<String>,
        has_citations_block: bool,
        verification_rate: f64,
    },
    Error {
        kind: String,
        message: String,
        retryable: bool,
    },
    Done {
        usage: StreamUsage,
    },
}

#[derive(Debug, Clone, Default)]
pub struct SseDeltaBatch {
    pub content_delta: String,
    pub reasoning_delta: String,
    pub done: bool,
    pub usage: Option<StreamUsage>,
}

/// Byte-buffered SSE parser that handles arbitrary TCP chunk boundaries (including multi-byte UTF-8 CJK characters
/// split across two network packets and lines without trailing newline until flush).
#[derive(Default)]
pub struct SseByteParser {
    byte_buf: Vec<u8>,
    accumulated_content: String,
    accumulated_reasoning: String,
    tool_calls_map: BTreeMap<usize, (String, String, String)>, // index -> (id, name, arguments)
    done: bool,
    usage: Option<StreamUsage>,
}

impl SseByteParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds a raw byte chunk from the HTTP stream and returns any newly decoded deltas.
    pub fn feed_bytes(&mut self, chunk: &[u8]) -> Result<SseDeltaBatch, String> {
        self.byte_buf.extend_from_slice(chunk);
        let mut batch = SseDeltaBatch::default();

        while let Some(newline_pos) = self.byte_buf.iter().position(|&b| b == b'\n') {
            let mut line_bytes: Vec<u8> = self.byte_buf.drain(..=newline_pos).collect();
            while matches!(line_bytes.last(), Some(b'\n' | b'\r')) {
                line_bytes.pop();
            }
            self.process_line_bytes(&line_bytes, &mut batch)?;
        }

        Ok(batch)
    }

    /// Flushes any remaining bytes at stream end (if the final line lacked a trailing `\n`).
    pub fn finish(&mut self) -> Result<SseDeltaBatch, String> {
        let mut batch = SseDeltaBatch::default();
        if !self.byte_buf.is_empty() {
            let mut remaining = std::mem::take(&mut self.byte_buf);
            while matches!(remaining.last(), Some(b'\n' | b'\r')) {
                remaining.pop();
            }
            self.process_line_bytes(&remaining, &mut batch)?;
        }
        Ok(batch)
    }

    fn process_line_bytes(
        &mut self,
        line_bytes: &[u8],
        batch: &mut SseDeltaBatch,
    ) -> Result<(), String> {
        if line_bytes.is_empty() {
            return Ok(());
        }
        let line = std::str::from_utf8(line_bytes)
            .map_err(|e| format!("Invalid UTF-8 in SSE line: {}", e))?
            .trim();

        // Ignore SSE comment / heartbeat lines starting with ':'
        if line.is_empty() || line.starts_with(':') {
            return Ok(());
        }

        let Some(data_payload) = line.strip_prefix("data:") else {
            return Ok(());
        };
        let payload = data_payload.trim();
        if payload == "[DONE]" {
            self.done = true;
            batch.done = true;
            return Ok(());
        }

        let val: Value = serde_json::from_str(payload)
            .map_err(|e| format!("SSE JSON parse error: {} in '{}'", e, payload))?;

        if let Some(err_obj) = val.get("error") {
            let msg = err_obj
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Stream error from provider");
            return Err(msg.to_string());
        }

        if let Some(u) = val.get("usage").and_then(|v| v.as_object()) {
            let pt = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let ct = u
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let tt = u
                .get("total_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or((pt + ct) as u64) as usize;
            if tt > 0 {
                let usage = StreamUsage {
                    prompt_tokens: pt,
                    completion_tokens: ct,
                    total_tokens: tt,
                    is_estimated: false,
                };
                self.usage = Some(usage.clone());
                batch.usage = Some(usage);
            }
        }

        if let Some(choices) = val.get("choices").and_then(|v| v.as_array()) {
            for choice in choices {
                if let Some(delta) = choice.get("delta") {
                    if let Some(c) = delta.get("content").and_then(|v| v.as_str()) {
                        if !c.is_empty() {
                            self.accumulated_content.push_str(c);
                            batch.content_delta.push_str(c);
                        }
                    }
                    if let Some(r) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                        if !r.is_empty() {
                            self.accumulated_reasoning.push_str(r);
                            batch.reasoning_delta.push_str(r);
                        }
                    }
                    if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                        for tc in tcs {
                            let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                            let entry = self.tool_calls_map.entry(idx).or_insert_with(|| {
                                (format!("call_{}", idx), String::new(), String::new())
                            });
                            if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                                if !id.is_empty() {
                                    entry.0 = id.to_string();
                                }
                            }
                            if let Some(func) = tc.get("function") {
                                if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                    entry.1.push_str(name);
                                }
                                if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                                    entry.2.push_str(args);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn accumulated_content(&self) -> &str {
        &self.accumulated_content
    }

    pub fn accumulated_reasoning(&self) -> &str {
        &self.accumulated_reasoning
    }

    pub fn take_tool_calls(&self) -> Vec<ToolCallItem> {
        self.tool_calls_map
            .values()
            .filter(|(_, name, _)| !name.is_empty())
            .map(|(id, name, args)| ToolCallItem {
                id: id.clone(),
                name: name.clone(),
                arguments_json: args.clone(),
            })
            .collect()
    }

    pub fn usage_or_estimate(&self, prompt_tokens_est: usize) -> StreamUsage {
        if let Some(ref u) = self.usage {
            u.clone()
        } else {
            let est = HeuristicTokenEstimator;
            let comp = est.estimate(&self.accumulated_content);
            StreamUsage {
                prompt_tokens: prompt_tokens_est,
                completion_tokens: comp,
                total_tokens: prompt_tokens_est + comp,
                is_estimated: true,
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatCitationsTail {
    #[serde(default)]
    pub citations: Vec<CitationItem>,
    #[serde(default)]
    pub unknowns: Vec<String>,
}

/// Splits a chat response on `<<<CITATIONS>>>` into `(markdown_body, Option<ChatCitationsTail>)` (Stage C3.3).
pub fn split_chat_citations(raw_response: &str) -> (String, Option<ChatCitationsTail>) {
    let Some(idx) = raw_response.find(CITATIONS_DELIMITER) else {
        return (raw_response.to_string(), None);
    };
    let body = raw_response[..idx].trim_end().to_string();
    let tail_raw = raw_response[idx + CITATIONS_DELIMITER.len()..].trim();

    if let Ok(parsed) = serde_json::from_str::<ChatCitationsTail>(tail_raw) {
        return (body, Some(parsed));
    }
    if let (Some(s), Some(e)) = (tail_raw.find('{'), tail_raw.rfind('}')) {
        if s < e {
            if let Ok(parsed) = serde_json::from_str::<ChatCitationsTail>(&tail_raw[s..=e]) {
                return (body, Some(parsed));
            }
        }
    }
    (body, Some(ChatCitationsTail::default()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryBudgetPlan {
    pub retained_messages: Vec<ChatMessage>,
    pub used_tokens: usize,
    pub compressed_turns: usize,
    pub new_rolling_summary: Option<String>,
    pub new_summary_upto_message_id: Option<String>,
}

/// Budgets conversation history within 40% of `budget_tokens`, always keeping the latest >= 4 messages,
/// and triggers incremental rolling summary when total history exceeds 60% of `budget_tokens` (Stage C3.5).
pub fn budget_conversation_history(
    all_messages: &[StoredMessageRecord],
    existing_summary: &str,
    existing_summary_upto_id: Option<&str>,
    budget_tokens: usize,
    estimator: &dyn TokenEstimator,
) -> HistoryBudgetPlan {
    let total_budget = if budget_tokens == 0 {
        16000
    } else {
        budget_tokens
    };
    let history_budget = (total_budget * 40) / 100;
    let summary_trigger_threshold = (total_budget * 60) / 100;

    if all_messages.is_empty() {
        return HistoryBudgetPlan {
            retained_messages: Vec::new(),
            used_tokens: 0,
            compressed_turns: 0,
            new_rolling_summary: None,
            new_summary_upto_message_id: None,
        };
    }

    // 1. Always retain at least the latest 4 messages; continue backwards while within history_budget (40%)
    let min_keep = 4usize.min(all_messages.len());
    let mut kept_indices: Vec<usize> = Vec::new();
    let mut used_tokens = estimator.estimate(existing_summary);

    for (rev_count, idx) in (0..all_messages.len()).rev().enumerate() {
        let msg = &all_messages[idx];
        // Strip any <<<CITATIONS>>> tail from older assistant messages to save tokens, keeping body + [cN]
        let (clean_body, _) = split_chat_citations(&msg.content);
        let msg_tokens = if msg.tokens > 0 {
            msg.tokens
        } else {
            estimator.estimate(&clean_body)
        };

        if rev_count < min_keep || used_tokens + msg_tokens <= history_budget {
            kept_indices.push(idx);
            used_tokens += msg_tokens;
        } else {
            break;
        }
    }
    kept_indices.reverse();

    let first_kept_idx = *kept_indices.first().unwrap_or(&0);
    let compressed_turns = first_kept_idx / 2;

    // 2. Check if total history exceeds 60% of budget -> generate incremental rolling summary
    // covering messages before the last 6 messages that haven't been summarized yet.
    let total_raw_tokens: usize = all_messages
        .iter()
        .map(|m| {
            if m.tokens > 0 {
                m.tokens
            } else {
                estimator.estimate(&m.content)
            }
        })
        .sum();

    let mut new_summary: Option<String> = None;
    let mut new_upto_id: Option<String> = None;

    if total_raw_tokens > summary_trigger_threshold && all_messages.len() > 6 {
        let compress_end_exclusive = all_messages.len() - 6;
        let start_idx = existing_summary_upto_id
            .and_then(|upto| all_messages.iter().position(|m| m.id == upto).map(|i| i + 1))
            .unwrap_or(0);

        if start_idx < compress_end_exclusive {
            let slice = &all_messages[start_idx..compress_end_exclusive];
            let summary_text = build_incremental_rolling_summary(existing_summary, slice, estimator);
            let upto_id = all_messages[compress_end_exclusive - 1].id.clone();
            new_summary = Some(summary_text);
            new_upto_id = Some(upto_id);
        }
    }

    let mut retained_messages = Vec::new();
    let effective_summary = new_summary
        .as_deref()
        .unwrap_or(existing_summary)
        .trim();
    if !effective_summary.is_empty() {
        retained_messages.push(ChatMessage {
            role: "system".to_string(),
            content: format!("### 早期对话滚动摘要\n{}", effective_summary),
        });
    }

    for idx in kept_indices {
        let m = &all_messages[idx];
        let (clean_body, _) = split_chat_citations(&m.content);
        retained_messages.push(ChatMessage {
            role: m.role.clone(),
            content: clean_body,
        });
    }

    HistoryBudgetPlan {
        retained_messages,
        used_tokens,
        compressed_turns,
        new_rolling_summary: new_summary,
        new_summary_upto_message_id: new_upto_id,
    }
}

/// Builds a structured rolling summary capped at <= 600 tokens (Stage C3.5).
pub fn build_incremental_rolling_summary(
    old_summary: &str,
    new_messages: &[StoredMessageRecord],
    estimator: &dyn TokenEstimator,
) -> String {
    let mut goals: Vec<String> = Vec::new();
    let mut conclusions: Vec<String> = Vec::new();

    for m in new_messages {
        let first_line = m.content.lines().next().unwrap_or("").trim();
        if first_line.is_empty() {
            continue;
        }
        let clipped: String = first_line.chars().take(80).collect();
        if m.role == "user" {
            if goals.len() < 4 {
                goals.push(clipped);
            }
        } else if conclusions.len() < 4 {
            conclusions.push(clipped);
        }
    }

    let mut combined = String::new();
    if !old_summary.trim().is_empty() {
        let prev_clipped: String = old_summary.trim().chars().take(450).collect();
        combined.push_str(&format!("[既有摘要]: {}\n", prev_clipped));
    }
    if !goals.is_empty() {
        combined.push_str(&format!("- 近期用户目标与问题: {}\n", goals.join("；")));
    }
    if !conclusions.is_empty() {
        combined.push_str(&format!("- 已讨论结论要点: {}\n", conclusions.join("；")));
    }

    // Hard cap at 600 tokens (~900 chars)
    while estimator.estimate(&combined) > 600 && combined.len() > 100 {
        let target_chars = (combined.chars().count() * 8) / 10;
        combined = combined.chars().take(target_chars).collect();
    }
    combined
}

/// Manages active request cancellation tokens so `cancel_request(request_id)` halts streaming within <1s.
#[derive(Clone, Default)]
pub struct CancellationRegistry {
    tokens: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl CancellationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, request_id: &str) -> Arc<AtomicBool> {
        let mut guard = self.tokens.lock().unwrap();
        let token = Arc::new(AtomicBool::new(false));
        guard.insert(request_id.to_string(), token.clone());
        token
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        let guard = self.tokens.lock().unwrap();
        if let Some(tok) = guard.get(request_id) {
            tok.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    pub fn remove(&self, request_id: &str) {
        let mut guard = self.tokens.lock().unwrap();
        guard.remove(request_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_byte_parser_split_utf8_and_tool_calls() {
        let mut parser = SseByteParser::new();

        // "汉" in UTF-8 is 3 bytes: [0xE6, 0xB1, 0x89]
        let line1_part1 = b"data: {\"choices\":[{\"delta\":{\"content\":\"\xE6";
        let line1_part2 = b"\xB1\x89\xE5\xAD\x97\"}}]}\n: keep-alive\n\n";

        let b1 = parser.feed_bytes(line1_part1).unwrap();
        assert_eq!(b1.content_delta, "");

        let b2 = parser.feed_bytes(line1_part2).unwrap();
        assert_eq!(b2.content_delta, "汉字");

        // Segmented tool_calls across two events + final line without newline
        let tc1 = b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_abc\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n";
        let tc2 = b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"a.py\\\"}\"}}]}}]}\ndata: [DONE]";

        parser.feed_bytes(tc1).unwrap();
        parser.feed_bytes(tc2).unwrap();
        let b_end = parser.finish().unwrap();
        assert!(b_end.done);

        let calls = parser.take_tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_abc");
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].arguments_json, r#"{"path":"a.py"}"#);
    }

    #[test]
    fn test_split_chat_citations() {
        let raw = "节点 `talker` 发布了 `/chatter` [c1]。\n<<<CITATIONS>>>\n{\"citations\":[{\"id\":\"c1\",\"file\":\"src/node.py\",\"line_start\":4,\"line_end\":4,\"quote\":\"create_publisher\"}],\"unknowns\":[\"QoS depth from launch\"]}";
        let (body, tail) = split_chat_citations(raw);
        assert_eq!(body, "节点 `talker` 发布了 `/chatter` [c1]。");
        let tail = tail.expect("should parse citations block");
        assert_eq!(tail.citations.len(), 1);
        assert_eq!(tail.citations[0].id, "c1");
        assert_eq!(tail.unknowns.len(), 1);
    }

    #[test]
    fn test_200_turn_synthetic_conversation_budget_and_rolling_summary() {
        let est = HeuristicTokenEstimator;
        let budget_tokens = 2000usize;
        let mut messages: Vec<StoredMessageRecord> = Vec::new();
        let mut rolling_summary = String::new();
        let mut summary_upto: Option<String> = None;
        let mut summary_trigger_count = 0usize;

        for turn in 1..=200 {
            let u_id = format!("m-u-{}", turn);
            let a_id = format!("m-a-{}", turn);
            let u_text = format!(
                "第 {} 轮问题：请解释 src/module_{}.py 里节点与话题的数据流向及参数配置？",
                turn, turn
            );
            let a_text = format!(
                "第 {} 轮回答：该模块通过 create_publisher 发布话题 `/topic_{}` [c1]，由下游订阅处理。",
                turn, turn
            );
            messages.push(StoredMessageRecord {
                id: u_id,
                conversation_id: "conv-1".to_string(),
                role: "user".to_string(),
                content: u_text.clone(),
                meta_json: "{}".to_string(),
                tokens: est.estimate(&u_text),
                created_at: turn as i64 * 2,
            });
            messages.push(StoredMessageRecord {
                id: a_id,
                conversation_id: "conv-1".to_string(),
                role: "assistant".to_string(),
                content: a_text.clone(),
                meta_json: "{}".to_string(),
                tokens: est.estimate(&a_text),
                created_at: turn as i64 * 2 + 1,
            });

            let plan = budget_conversation_history(
                &messages,
                &rolling_summary,
                summary_upto.as_deref(),
                budget_tokens,
                &est,
            );

            // 1. Estimated tokens of retained history must never exceed total budget
            assert!(
                plan.used_tokens <= budget_tokens,
                "Turn {} exceeded budget: {} > {}",
                turn,
                plan.used_tokens,
                budget_tokens
            );

            // 2. Latest 4 messages must always be retained when >= 2 turns exist
            if messages.len() >= 4 {
                assert!(plan.retained_messages.len() >= 4);
                let last_retained = plan.retained_messages.last().unwrap();
                assert!(last_retained.content.contains(&format!("第 {} 轮回答", turn)));
            }

            // 3. Apply incremental rolling summary when triggered
            if let (Some(new_sum), Some(new_upto)) = (
                plan.new_rolling_summary,
                plan.new_summary_upto_message_id,
            ) {
                assert!(est.estimate(&new_sum) <= 600);
                rolling_summary = new_sum;
                summary_upto = Some(new_upto);
                summary_trigger_count += 1;
            }
        }

        assert!(
            summary_trigger_count > 0,
            "Rolling summary must have been triggered during 200 turns"
        );
    }
}
