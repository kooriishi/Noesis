use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

const BUILTIN_ROS2_ENTRIES: &[(&str, &str)] = &[
    (
        "ros2.dds_rmw",
        include_str!("../../../../knowledge/ros2_humble/ros2.dds_rmw.md"),
    ),
    (
        "ros2.qos.policies",
        include_str!("../../../../knowledge/ros2_humble/ros2.qos.policies.md"),
    ),
    (
        "ros2.qos.compatibility",
        include_str!("../../../../knowledge/ros2_humble/ros2.qos.compatibility.md"),
    ),
    (
        "ros2.topic_service_action",
        include_str!("../../../../knowledge/ros2_humble/ros2.topic_service_action.md"),
    ),
    (
        "ros2.executor_callback_groups",
        include_str!("../../../../knowledge/ros2_humble/ros2.executor_callback_groups.md"),
    ),
    (
        "ros2.timers_and_time",
        include_str!("../../../../knowledge/ros2_humble/ros2.timers_and_time.md"),
    ),
    (
        "ros2.parameters",
        include_str!("../../../../knowledge/ros2_humble/ros2.parameters.md"),
    ),
    (
        "ros2.names_namespaces_remap",
        include_str!("../../../../knowledge/ros2_humble/ros2.names_namespaces_remap.md"),
    ),
    (
        "ros2.tf2",
        include_str!("../../../../knowledge/ros2_humble/ros2.tf2.md"),
    ),
    (
        "ros2.lifecycle",
        include_str!("../../../../knowledge/ros2_humble/ros2.lifecycle.md"),
    ),
    (
        "ros2.composition",
        include_str!("../../../../knowledge/ros2_humble/ros2.composition.md"),
    ),
    (
        "ros2.build_system",
        include_str!("../../../../knowledge/ros2_humble/ros2.build_system.md"),
    ),
    (
        "ros2.interfaces",
        include_str!("../../../../knowledge/ros2_humble/ros2.interfaces.md"),
    ),
    (
        "ros2.cli_debug",
        include_str!("../../../../knowledge/ros2_humble/ros2.cli_debug.md"),
    ),
    (
        "ros2.simulation_ecosystem",
        include_str!("../../../../knowledge/ros2_humble/ros2.simulation_ecosystem.md"),
    ),
];

const BUILTIN_LLM_ENTRIES: &[(&str, &str)] = &[
    (
        "llm.tokens_context",
        include_str!("../../../../knowledge/llm_app/llm.tokens_context.md"),
    ),
    (
        "llm.sampling_params",
        include_str!("../../../../knowledge/llm_app/llm.sampling_params.md"),
    ),
    (
        "llm.messages_roles",
        include_str!("../../../../knowledge/llm_app/llm.messages_roles.md"),
    ),
    (
        "llm.structured_output",
        include_str!("../../../../knowledge/llm_app/llm.structured_output.md"),
    ),
    (
        "llm.tool_calling",
        include_str!("../../../../knowledge/llm_app/llm.tool_calling.md"),
    ),
    (
        "llm.agent_patterns",
        include_str!("../../../../knowledge/llm_app/llm.agent_patterns.md"),
    ),
    (
        "llm.memory",
        include_str!("../../../../knowledge/llm_app/llm.memory.md"),
    ),
    (
        "llm.rag_overview",
        include_str!("../../../../knowledge/llm_app/llm.rag_overview.md"),
    ),
    (
        "llm.rag_chunking",
        include_str!("../../../../knowledge/llm_app/llm.rag_chunking.md"),
    ),
    (
        "llm.rag_embedding_vector_store",
        include_str!("../../../../knowledge/llm_app/llm.rag_embedding_vector_store.md"),
    ),
    (
        "llm.rag_retrieval_rerank",
        include_str!("../../../../knowledge/llm_app/llm.rag_retrieval_rerank.md"),
    ),
    (
        "llm.prompt_caching",
        include_str!("../../../../knowledge/llm_app/llm.prompt_caching.md"),
    ),
    (
        "llm.streaming_api",
        include_str!("../../../../knowledge/llm_app/llm.streaming_api.md"),
    ),
    (
        "llm.serving_web",
        include_str!("../../../../knowledge/llm_app/llm.serving_web.md"),
    ),
    (
        "llm.eval_observability",
        include_str!("../../../../knowledge/llm_app/llm.eval_observability.md"),
    ),
    (
        "llm.safety_guardrails",
        include_str!("../../../../knowledge/llm_app/llm.safety_guardrails.md"),
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub id: String,
    pub title: String,
    pub domain: String,
    /// `draft | reviewed`
    pub status: String,
    pub triggers: Vec<String>,
    pub source: String,
    pub verify_notes: String,
    pub short: String,
    pub detail: String,
    pub pitfalls: Vec<String>,
    pub reading_hints: String,
}

#[derive(Debug, Clone, Default)]
pub struct KnowledgeBase {
    entries: Vec<KnowledgeEntry>,
    by_id: HashMap<String, usize>,
}

impl KnowledgeBase {
    pub fn load_builtin() -> Self {
        let mut kb = Self::default();
        for (_, raw) in BUILTIN_ROS2_ENTRIES.iter().chain(BUILTIN_LLM_ENTRIES.iter()) {
            if let Ok(entry) = parse_knowledge_markdown(raw) {
                let idx = kb.entries.len();
                kb.by_id.insert(entry.id.clone(), idx);
                kb.entries.push(entry);
            }
        }
        kb
    }

    pub fn load_from_dir(dir: &Path) -> Result<Self, String> {
        let mut kb = Self::default();
        for entry in WalkDir::new(dir).follow_links(false).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let content = fs::read_to_string(entry.path()).map_err(|e| e.to_string())?;
            let parsed = parse_knowledge_markdown(&content)?;
            if kb.by_id.contains_key(&parsed.id) {
                return Err(format!("Duplicate knowledge entry id: {}", parsed.id));
            }
            let idx = kb.entries.len();
            kb.by_id.insert(parsed.id.clone(), idx);
            kb.entries.push(parsed);
        }
        kb.validate_all()?;
        Ok(kb)
    }

    pub fn validate_all(&self) -> Result<(), String> {
        let mut seen = HashSet::new();
        for entry in &self.entries {
            if entry.id.trim().is_empty() {
                return Err("Knowledge entry has empty id".to_string());
            }
            if !seen.insert(entry.id.clone()) {
                return Err(format!("Duplicate knowledge entry id: {}", entry.id));
            }
            if entry.short.trim().is_empty() {
                return Err(format!("Knowledge entry '{}' has empty short section", entry.id));
            }
            if entry.short.chars().count() > 260 {
                return Err(format!(
                    "Knowledge entry '{}' short section exceeds length limit ({} chars)",
                    entry.id,
                    entry.short.chars().count()
                ));
            }
            if !matches!(entry.status.as_str(), "draft" | "reviewed") {
                return Err(format!(
                    "Knowledge entry '{}' has invalid status '{}'",
                    entry.id, entry.status
                ));
            }
        }
        Ok(())
    }

    pub fn entries(&self) -> &[KnowledgeEntry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&KnowledgeEntry> {
        self.by_id.get(id).and_then(|&i| self.entries.get(i))
    }

    /// Selects up to 5 relevant knowledge entries within `max_tokens` (~1200 tokens),
    /// scoring by trigger matches in `code_snippet`, `query`, `entity_kinds`, and boosting `confused_concepts`.
    pub fn select_entries(
        &self,
        active_domains: &[String],
        code_snippet: &str,
        query: &str,
        confused_concepts: &[String],
        include_detail: bool,
        max_tokens: usize,
    ) -> Vec<KnowledgeEntry> {
        let combined_haystack = format!("{}\n{}", code_snippet, query).to_lowercase();
        let confused_lower: HashSet<String> = confused_concepts
            .iter()
            .map(|c| c.to_lowercase())
            .collect();

        let mut scored: Vec<(usize, &KnowledgeEntry)> = Vec::new();
        for entry in &self.entries {
            if !active_domains.is_empty() && !active_domains.contains(&entry.domain) {
                continue;
            }
            let mut score = 0usize;
            for trig in &entry.triggers {
                let tl = trig.to_lowercase();
                if !tl.is_empty() && combined_haystack.contains(&tl) {
                    score += 3;
                }
            }
            if confused_lower.contains(&entry.id.to_lowercase())
                || confused_lower
                    .iter()
                    .any(|c| entry.title.to_lowercase().contains(c))
            {
                score += 4;
            }
            if score > 0 {
                scored.push((score, entry));
            }
        }

        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));

        let mut selected = Vec::new();
        let mut used_tokens = 0usize;
        for (_, entry) in scored {
            if selected.len() >= 5 {
                break;
            }
            let text_len = if include_detail {
                entry.short.chars().count() + entry.detail.chars().count() + 60
            } else {
                entry.short.chars().count() + 30
            };
            if used_tokens + text_len > max_tokens && !selected.is_empty() {
                break;
            }
            used_tokens += text_len;
            selected.push(entry.clone());
        }
        selected
    }

    pub fn format_for_prompt(entries: &[KnowledgeEntry], include_detail: bool) -> String {
        if entries.is_empty() {
            return String::new();
        }
        let mut out = String::from("### 领域知识包条目 (Knowledge Pack)\n");
        for e in entries {
            let status_label = if e.status == "reviewed" {
                "已核对"
            } else {
                "未核对草稿"
            };
            out.push_str(&format!(
                "- **[{}]** {} ({}): {}\n",
                e.id, e.title, status_label, e.short
            ));
            if include_detail && !e.detail.is_empty() {
                out.push_str(&format!("  详述: {}\n", e.detail));
                if !e.pitfalls.is_empty() {
                    out.push_str(&format!("  常见坑: {}\n", e.pitfalls.join("；")));
                }
            }
        }
        out
    }

    /// Validates `pack_ref`: only returns `Some(pack_ref)` if it is in `loaded_pack_ids`.
    pub fn validate_pack_ref(pack_ref: Option<&str>, loaded_pack_ids: &[String]) -> Option<String> {
        let r = pack_ref?.trim();
        if r.is_empty() {
            return None;
        }
        if loaded_pack_ids.iter().any(|id| id == r) {
            Some(r.to_string())
        } else {
            None
        }
    }
}

pub fn parse_knowledge_markdown(raw: &str) -> Result<KnowledgeEntry, String> {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return Err("Missing YAML frontmatter start '---'".to_string());
    }
    let after_first = &trimmed[3..];
    let Some(end_idx) = after_first.find("\n---") else {
        return Err("Missing YAML frontmatter closing '---'".to_string());
    };
    let yaml_block = &after_first[..end_idx];
    let body_block = &after_first[end_idx + 4..];

    let mut id = String::new();
    let mut title = String::new();
    let mut domain = String::new();
    let mut status = "draft".to_string();
    let mut triggers = Vec::new();
    let mut source = String::new();
    let mut verify_notes = String::new();
    let mut in_triggers = false;

    for line in yaml_block.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(stripped) = l.strip_prefix("- ") {
            if in_triggers {
                triggers.push(stripped.trim().trim_matches('"').trim_matches('\'').to_string());
            }
            continue;
        }
        in_triggers = false;
        if let Some((k, v)) = l.split_once(':') {
            let key = k.trim();
            let val = v.trim().trim_matches('"').trim_matches('\'');
            match key {
                "id" => id = val.to_string(),
                "title" => title = val.to_string(),
                "domain" => domain = val.to_string(),
                "status" => status = val.to_string(),
                "source" => source = val.to_string(),
                "verify_notes" => verify_notes = val.to_string(),
                "triggers" => in_triggers = true,
                _ => {}
            }
        }
    }

    let mut short = String::new();
    let mut detail = String::new();
    let mut pitfalls = Vec::new();
    let mut reading_hints = String::new();
    let mut current_sec = "";

    for line in body_block.lines() {
        let t = line.trim();
        if let Some(h) = t.strip_prefix("## ") {
            current_sec = h.trim();
            continue;
        }
        if t.is_empty() {
            continue;
        }
        match current_sec {
            "short" => {
                if !short.is_empty() {
                    short.push(' ');
                }
                short.push_str(t);
            }
            "detail" => {
                if !detail.is_empty() {
                    detail.push('\n');
                }
                detail.push_str(t);
            }
            "pitfalls" => {
                let item = t.strip_prefix("- ").unwrap_or(t).trim().to_string();
                pitfalls.push(item);
            }
            "reading_hints" => {
                if !reading_hints.is_empty() {
                    reading_hints.push(' ');
                }
                reading_hints.push_str(t);
            }
            _ => {}
        }
    }

    Ok(KnowledgeEntry {
        id,
        title,
        domain,
        status,
        triggers,
        source,
        verify_notes,
        short,
        detail,
        pitfalls,
        reading_hints,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_knowledge_packs_validate_and_select() {
        let kb = KnowledgeBase::load_builtin();
        assert_eq!(
            kb.entries().len(),
            31,
            "Should load 15 ROS2 Humble + 16 LLM App entries"
        );
        kb.validate_all().expect("All builtin entries must pass format validation");

        // Trigger selection test 1: ROS2 SensorDataQoS + create_subscription
        let selected_ros2 = kb.select_entries(
            &["ros2".to_string()],
            "sub_ = create_subscription<AudioFrame>(topic, rclcpp::SensorDataQoS(), cb);",
            "",
            &[],
            false,
            1200,
        );
        let ids: Vec<String> = selected_ros2.iter().map(|e| e.id.clone()).collect();
        assert!(ids.contains(&"ros2.qos.policies".to_string()));
        assert!(ids.contains(&"ros2.qos.compatibility".to_string()));

        // Trigger selection test 2: LLM App RecursiveCharacterTextSplitter + chunk_size
        let selected_llm = kb.select_entries(
            &["llm_app".to_string()],
            "splitter = RecursiveCharacterTextSplitter(chunk_size=500, chunk_overlap=50)",
            "",
            &[],
            false,
            1200,
        );
        let llm_ids: Vec<String> = selected_llm.iter().map(|e| e.id.clone()).collect();
        assert!(llm_ids.contains(&"llm.rag_chunking".to_string()));

        // Pack ref validation test: unlisted pack_ref is ignored
        assert_eq!(
            KnowledgeBase::validate_pack_ref(Some("ros2.qos.policies"), &ids),
            Some("ros2.qos.policies".to_string())
        );
        assert_eq!(
            KnowledgeBase::validate_pack_ref(Some("non.existent.pack"), &ids),
            None
        );
    }
}
