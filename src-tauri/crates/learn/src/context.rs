use crate::knowledge::KnowledgeBase;
use chrono::Utc;
use noesis_project::is_sensitive_path;
use noesis_store::{DbStore, GraphEdge, GraphEntity, GraphSymbol};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub const PROMPT_VERSION: &str = "3.0";

pub trait TokenEstimator: Send + Sync {
    fn estimate(&self, text: &str) -> usize;
}

/// Conservative character heuristic estimator:
/// ASCII characters: ~4 chars per token (ceil)
/// Non-ASCII (CJK, symbols): ~1 char per token
#[derive(Debug, Clone, Default)]
pub struct HeuristicTokenEstimator;

impl TokenEstimator for HeuristicTokenEstimator {
    fn estimate(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        let mut ascii_count = 0usize;
        let mut non_ascii_count = 0usize;
        for ch in text.chars() {
            if ch.is_ascii() {
                ascii_count += 1;
            } else {
                non_ascii_count += 1;
            }
        }
        (ascii_count + 3) / 4 + non_ascii_count
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequest {
    /// "explain_selection" | "ask" | "explain_entity" | "re_explain"
    pub kind: String,
    pub project_id: String,
    pub file: Option<String>,
    pub range: Option<(u32, u32)>,
    pub selected_code: Option<String>,
    pub question: Option<String>,
    pub entity_ref: Option<String>,
    pub conversation_id: Option<String>,
    pub budget_tokens: usize,
    #[serde(default = "default_true")]
    pub include_learning_profile: bool,
    #[serde(default)]
    pub re_explain_type: Option<String>,
    #[serde(default)]
    pub re_explain_level: Option<u8>,
    #[serde(default)]
    pub previous_explanation_summary: Option<String>,
    #[serde(default)]
    pub user_note: Option<String>,
    #[serde(default)]
    pub depth_mode: Option<String>,
    #[serde(default)]
    pub custom_persona: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub text: String,
    pub priority: u8,
    pub tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DroppedItem {
    pub item_id: String,
    pub kind: String,
    pub source: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextManifest {
    pub used_files: Vec<String>,
    pub used_entities: Vec<String>,
    pub loaded_knowledge_packs: Vec<String>,
    pub used_learning_profile: bool,
    pub total_tokens: usize,
    pub budget_tokens: usize,
    pub usable_budget_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPack {
    pub items: Vec<ContextItem>,
    pub manifest: ContextManifest,
    pub dropped: Vec<DroppedItem>,
    pub token_estimate: usize,
    pub basis_hash: String,
}

pub struct ContextAssembler;

impl ContextAssembler {
    /// Formats a slice of lines (1-indexed `start_line..=end_line`) with real line numbers.
    pub fn format_numbered_lines(content: &str, start_line: u32, end_line: u32) -> (String, bool) {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return (String::new(), false);
        }
        let s = start_line.max(1) as usize;
        let e = (end_line as usize).min(lines.len()).max(s);
        if s > lines.len() {
            return (String::new(), false);
        }

        let max_lines = 400usize;
        let actual_len = e - s + 1;
        let truncated = actual_len > max_lines;
        let take_end = if truncated { s + max_lines - 1 } else { e };

        let mut out = String::new();
        for idx in s..=take_end {
            out.push_str(&format!("{:>4} | {}\n", idx, lines[idx - 1]));
        }
        if truncated {
            out.push_str(&format!(
                "... [已截断：原选区共 {} 行（第 {} 至 {} 行），仅展示前 {} 行]\n",
                actual_len, start_line, end_line, max_lines
            ));
        }
        (out, truncated)
    }

    /// Assembles a prioritized, token-budgeted `ContextPack` from the workspace index and knowledge packs.
    pub fn assemble(
        project_root: &Path,
        store: Option<&DbStore>,
        kb: Option<&KnowledgeBase>,
        req: &ContextRequest,
        estimator: &dyn TokenEstimator,
        model_name: &str,
    ) -> ContextPack {
        let budget_tokens = if req.budget_tokens == 0 {
            16000
        } else {
            req.budget_tokens
        };
        // In deep mode, reserve 40% for output + 15% safety margin => 45% usable input budget.
        // In standard mode, reserve 25% for output + 15% safety margin => 60% usable input budget.
        let usable_budget = if req.depth_mode.as_deref() == Some("deep") {
            (budget_tokens * 45) / 100
        } else {
            (budget_tokens * 60) / 100
        };

        let mut candidate_items: Vec<ContextItem> = Vec::new();
        let mut dropped: Vec<DroppedItem> = Vec::new();
        let mut used_files: Vec<String> = Vec::new();
        let mut used_entities: Vec<String> = Vec::new();
        let mut loaded_packs: Vec<String> = Vec::new();
        let mut next_id = 1usize;

        let mut alloc_id = || {
            let id = format!("E{}", next_id);
            next_id += 1;
            id
        };

        // Collect all project symbols/entities/edges if store is available
        let all_symbols: Vec<GraphSymbol> = store
            .and_then(|s| s.list_project_symbols(&req.project_id, None).ok())
            .unwrap_or_default();
        let all_entities: Vec<GraphEntity> = store
            .and_then(|s| s.get_entities_by_domain(&req.project_id, None, None).ok())
            .unwrap_or_default();
        let all_edges: Vec<GraphEdge> = store
            .and_then(|s| s.get_edges_filtered(&req.project_id, None).ok())
            .unwrap_or_default();

        // Determine active domains in project
        let mut active_domains: Vec<String> = Vec::new();
        if all_entities.iter().any(|e| e.domain == "ros2") {
            active_domains.push("ros2".to_string());
        }
        if all_entities.iter().any(|e| e.domain == "llm_app") {
            active_domains.push("llm_app".to_string());
        }
        if active_domains.is_empty() {
            active_domains.push("ros2".to_string());
            active_domains.push("llm_app".to_string());
        }

        // Check selected file content and line range
        let mut selected_file_content: Option<String> = None;
        let mut norm_file_path: Option<String> = None;

        if let Some(ref rel_file) = req.file {
            let norm = rel_file.replace('\\', "/");
            if is_sensitive_path(Path::new(&norm)) {
                dropped.push(DroppedItem {
                    item_id: alloc_id(),
                    kind: "selected_code".to_string(),
                    source: norm,
                    reason: "sensitive_file_excluded".to_string(),
                });
            } else {
                let full_path = project_root.join(&norm);
                if let Ok(content) = fs::read_to_string(&full_path) {
                    selected_file_content = Some(content);
                }
                norm_file_path = Some(norm);
            }
        }

        // Gather relevant entities in or connected to the selected range
        let (sel_start, sel_end) = req.range.unwrap_or((1, 200));
        let mut relevant_entities: Vec<GraphEntity> = Vec::new();
        if let Some(ref fpath) = norm_file_path {
            for ent in &all_entities {
                if ent.file_path.replace('\\', "/") == *fpath {
                    if ent.line_start <= sel_end + 15 && ent.line_end + 15 >= sel_start {
                        relevant_entities.push(ent.clone());
                    }
                }
            }
        }
        if let Some(ref eref) = req.entity_ref {
            for ent in &all_entities {
                if &ent.id == eref || &ent.name == eref {
                    if !relevant_entities.iter().any(|e| e.id == ent.id) {
                        relevant_entities.push(ent.clone());
                    }
                }
            }
        }

        // Gather confused concept IDs from store for KnowledgeBase weighting & P6
        let concept_profiles = store
            .and_then(|s| s.list_concept_profile(Some(&req.project_id)).ok())
            .unwrap_or_default();
        let confused_concepts: Vec<String> = concept_profiles
            .iter()
            .filter(|c| c.confused_count > c.resolved_count)
            .flat_map(|c| {
                let mut v = vec![c.id.clone(), c.name.clone()];
                if let Some(ref pr) = c.pack_ref {
                    v.push(pr.clone());
                }
                v
            })
            .collect();

        // --- P0: System Prompt (L1 persona + L0 locked rules) + Re-explain Strategy + Knowledge Packs ---
        let mut sys_prompt = load_layered_system_prompt(project_root, req.custom_persona.as_deref());
        if req.depth_mode.as_deref() == Some("deep") {
            sys_prompt.push_str("\n\n### 模式要求：深入解析模式 (Deep Mode)\n请额外深入推导底层设计权衡、常见工程误区、边界与异常处理、以及与相邻模块的高阶交互，给出翔实的 Markdown 展开。");
        }
        if req.kind == "re_explain" {
            let ctype = req.re_explain_type.as_deref().unwrap_or("overall");
            let level = req.re_explain_level.unwrap_or(1);
            let reexplain_addon = build_reexplain_instruction(
                project_root,
                ctype,
                level,
                req.user_note.as_deref(),
                req.previous_explanation_summary.as_deref(),
                false,
            );
            sys_prompt.push_str("\n\n");
            sys_prompt.push_str(&reexplain_addon);
        }
        let sys_tokens = estimator.estimate(&sys_prompt);
        candidate_items.push(ContextItem {
            id: alloc_id(),
            kind: "system_prompt".to_string(),
            source: format!("prompts/code_guide_expert.system.md@v{}", PROMPT_VERSION),
            text: sys_prompt,
            priority: 0,
            tokens: sys_tokens,
            confidence: None,
        });

        if let Some(kb) = kb {
            let entity_kinds: Vec<String> =
                relevant_entities.iter().map(|e| e.kind.clone()).collect();
            let snippet_ref = req
                .selected_code
                .as_deref()
                .or(selected_file_content.as_deref())
                .unwrap_or("");
            let q_ref = req.question.as_deref().unwrap_or("");
            let include_detail = req.kind == "re_explain"
                && req.re_explain_type.as_deref() == Some("principle")
                || q_ref.contains("原理")
                || q_ref.contains("为什么");

            let snippet_with_kinds = format!("{}\n{}", snippet_ref, entity_kinds.join(" "));
            let selected_packs = kb.select_entries(
                &active_domains,
                &snippet_with_kinds,
                q_ref,
                &confused_concepts,
                include_detail,
                1200,
            );
            if !selected_packs.is_empty() {
                for p in &selected_packs {
                    loaded_packs.push(p.id.clone());
                }
                let pack_text = KnowledgeBase::format_for_prompt(&selected_packs, include_detail);
                let pt = estimator.estimate(&pack_text);
                candidate_items.push(ContextItem {
                    id: alloc_id(),
                    kind: "knowledge_pack".to_string(),
                    source: format!("knowledge ({} entries)", loaded_packs.len()),
                    text: pack_text,
                    priority: 0,
                    tokens: pt,
                    confidence: None,
                });
            }
        }

        // --- P1: User Selected Code (with real line numbers, max 400 lines) ---
        if let Some(ref fpath) = norm_file_path {
            let numbered = if let Some(ref full_content) = selected_file_content {
                let (txt, was_trunc) =
                    Self::format_numbered_lines(full_content, sel_start, sel_end);
                if was_trunc {
                    dropped.push(DroppedItem {
                        item_id: "P1-trunc".to_string(),
                        kind: "selected_code_overflow".to_string(),
                        source: format!("{}:{}-{}", fpath, sel_start + 400, sel_end),
                        reason: "truncated_400_lines".to_string(),
                    });
                }
                txt
            } else if let Some(ref snippet) = req.selected_code {
                let mut out = String::new();
                for (idx, line) in snippet.lines().take(400).enumerate() {
                    out.push_str(&format!("{:>4} | {}\n", sel_start as usize + idx, line));
                }
                out
            } else {
                String::new()
            };

            if !numbered.is_empty() {
                used_files.push(fpath.clone());
                let body = format!(
                    "### 用户选中代码 [{}:{}-{}]\n```\n{}```",
                    fpath, sel_start, sel_end, numbered
                );
                let t = estimator.estimate(&body);
                candidate_items.push(ContextItem {
                    id: alloc_id(),
                    kind: "selected_code".to_string(),
                    source: format!("{}:{}-{}", fpath, sel_start, sel_end),
                    text: body,
                    priority: 1,
                    tokens: t,
                    confidence: Some("exact".to_string()),
                });
            }
        }

        // --- P2: Domain Facts (ROS2 & LLM App facts from the unified graph) ---
        if !relevant_entities.is_empty() {
            let mut facts_text =
                String::from("### 项目图谱领域事实（最高优先级事实依据，严禁猜测）\n");
            for ent in &relevant_entities {
                used_entities.push(ent.id.clone());
                facts_text.push_str(&format!(
                    "- 实体 `{}` (domain={}, kind={}, 位置={}:{}-{}): metadata={}\n",
                    ent.name,
                    ent.domain,
                    ent.kind,
                    ent.file_path,
                    ent.line_start,
                    ent.line_end,
                    ent.metadata_json
                ));

                // Find all connected edges (publishers/subscribers on same topic, launch files, LLM flows)
                for edge in &all_edges {
                    if edge.src_id == ent.id || edge.dst_id == ent.id {
                        facts_text.push_str(&format!(
                            "  * 关联边 [{} | confidence={}]: `{}` -> `{}` (位置={}:{}, 证据=`{}`)\n",
                            edge.kind,
                            edge.confidence,
                            edge.src_id,
                            edge.dst_id,
                            edge.file_path,
                            edge.line,
                            edge.evidence
                        ));
                        // If connected to a topic/service/action or prompt_template, also list other peers on that target!
                        let peer_target = if edge.src_id == ent.id {
                            &edge.dst_id
                        } else {
                            &edge.src_id
                        };
                        for peer_edge in &all_edges {
                            if peer_edge.id != edge.id
                                && (peer_edge.dst_id == *peer_target
                                    || peer_edge.src_id == *peer_target)
                            {
                                facts_text.push_str(&format!(
                                    "    - 同实体其它关联 [{} | confidence={}]: `{}` -> `{}` (位置={}:{})\n",
                                    peer_edge.kind,
                                    peer_edge.confidence,
                                    peer_edge.src_id,
                                    peer_edge.dst_id,
                                    peer_edge.file_path,
                                    peer_edge.line
                                ));
                            }
                        }
                    }
                }
            }
            let ft = estimator.estimate(&facts_text);
            candidate_items.push(ContextItem {
                id: alloc_id(),
                kind: "domain_facts".to_string(),
                source: format!("graph_entities ({})", relevant_entities.len()),
                text: facts_text,
                priority: 2,
                tokens: ft,
                confidence: Some("exact".to_string()),
            });
        }

        // --- P3: Enclosing Symbol Body + Current File Skeleton ---
        let mut enclosing_sym: Option<GraphSymbol> = None;
        if let Some(ref fpath) = norm_file_path {
            let file_syms: Vec<&GraphSymbol> = all_symbols
                .iter()
                .filter(|s| s.file_path.replace('\\', "/") == *fpath)
                .collect();

            // Find enclosing symbol if selection is only part of it
            for sym in &file_syms {
                if sym.line_start <= sel_start
                    && sym.line_end >= sel_end
                    && (sym.line_start < sel_start || sym.line_end > sel_end)
                {
                    enclosing_sym = Some((*sym).clone());
                    break;
                }
            }

            if let (Some(ref sym), Some(ref full_content)) =
                (&enclosing_sym, &selected_file_content)
            {
                let (sym_lines, _) =
                    Self::format_numbered_lines(full_content, sym.line_start, sym.line_end);
                let text = format!(
                    "### 选区所在完整符号 `{}` ({}:{}-{})\n```\n{}```",
                    sym.qualified_name, sym.file_path, sym.line_start, sym.line_end, sym_lines
                );
                let t = estimator.estimate(&text);
                candidate_items.push(ContextItem {
                    id: alloc_id(),
                    kind: "enclosing_symbol".to_string(),
                    source: format!("{}:{}-{}", sym.file_path, sym.line_start, sym.line_end),
                    text,
                    priority: 3,
                    tokens: t,
                    confidence: Some("exact".to_string()),
                });
            }

            if !file_syms.is_empty() {
                let mut skel = format!("### 当前文件符号骨架 `{}`\n", fpath);
                for s in file_syms.iter().take(60) {
                    skel.push_str(&format!(
                        "- [L{}-L{}] {} `{}`: `{}`\n",
                        s.line_start,
                        s.line_end,
                        s.kind,
                        s.qualified_name,
                        s.signature.as_deref().unwrap_or("")
                    ));
                }
                let t = estimator.estimate(&skel);
                candidate_items.push(ContextItem {
                    id: alloc_id(),
                    kind: "file_skeleton".to_string(),
                    source: fpath.clone(),
                    text: skel,
                    priority: 3,
                    tokens: t,
                    confidence: Some("exact".to_string()),
                });
            }
        }

        // --- P4: 1-Hop Callers & Callees (with confidence) ---
        if let Some(ref sym) = enclosing_sym {
            let mut neighbor_text = String::new();
            for edge in &all_edges {
                if edge.kind != "calls" {
                    continue;
                }
                let is_outgoing = edge.src_id == sym.id || edge.src_id == sym.qualified_name;
                let is_incoming = edge.dst_id == sym.id
                    || edge.dst_id == sym.qualified_name
                    || edge.dst_id == sym.name;
                if !is_outgoing && !is_incoming {
                    continue;
                }

                if edge.confidence == "unresolved" {
                    neighbor_text.push_str(&format!(
                        "- [unresolved 调用边] `{}` -> `{}` (位置={}:{}): 动态或外部未解析目标\n",
                        edge.src_id, edge.dst_id, edge.file_path, edge.line
                    ));
                    continue;
                }

                let peer_key = if is_outgoing {
                    &edge.dst_id
                } else {
                    &edge.src_id
                };
                if let Some(peer_sym) = all_symbols.iter().find(|s| {
                    s.id == *peer_key || s.qualified_name == *peer_key || s.name == *peer_key
                }) {
                    let peer_norm = peer_sym.file_path.replace('\\', "/");
                    if is_sensitive_path(Path::new(&peer_norm)) {
                        dropped.push(DroppedItem {
                            item_id: alloc_id(),
                            kind: "neighborhood".to_string(),
                            source: peer_norm,
                            reason: "sensitive_file_excluded".to_string(),
                        });
                        continue;
                    }
                    let direction = if is_outgoing { "调用目标" } else { "调用方" };
                    neighbor_text.push_str(&format!(
                        "- [{} | confidence={}] `{}` ({}:{}-{}): `{}`\n",
                        direction,
                        edge.confidence,
                        peer_sym.qualified_name,
                        peer_norm,
                        peer_sym.line_start,
                        peer_sym.line_end,
                        peer_sym.signature.as_deref().unwrap_or("")
                    ));
                    // Read up to 20 lines of body
                    let peer_full = project_root.join(&peer_norm);
                    if let Ok(peer_content) = fs::read_to_string(&peer_full) {
                        let end_l = (peer_sym.line_start + 19).min(peer_sym.line_end);
                        let (snippet, _) =
                            Self::format_numbered_lines(&peer_content, peer_sym.line_start, end_l);
                        neighbor_text.push_str(&format!("```\n{}```\n", snippet));
                        if !used_files.contains(&peer_norm) {
                            used_files.push(peer_norm);
                        }
                    }
                }
            }
            if !neighbor_text.is_empty() {
                let header = format!("### 一跳调用邻域 ({})\n{}", sym.qualified_name, neighbor_text);
                let t = estimator.estimate(&header);
                candidate_items.push(ContextItem {
                    id: alloc_id(),
                    kind: "neighborhood".to_string(),
                    source: sym.qualified_name.clone(),
                    text: header,
                    priority: 4,
                    tokens: t,
                    confidence: Some("mixed".to_string()),
                });
            }
        }

        // --- P5: Module Summary & Project README Summary ---
        let mut p5_text = String::new();
        if let Some(s) = store {
            if let Ok(modules) = s.list_project_modules(&req.project_id) {
                if !modules.is_empty() {
                    p5_text.push_str("### 项目模块列表\n");
                    for m in modules.iter().take(15) {
                        p5_text.push_str(&format!(
                            "- 模块 `{}` (kind={}, root={})\n",
                            m.name, m.kind, m.root_path
                        ));
                    }
                }
            }
        }
        for readme_name in ["README.md", "readme.md", "README.txt"] {
            let rp = project_root.join(readme_name);
            if rp.is_file() && !is_sensitive_path(Path::new(readme_name)) {
                if let Ok(raw) = fs::read_to_string(&rp) {
                    let preview: String = raw.chars().take(1500).collect();
                    p5_text.push_str(&format!("\n### 项目 README 摘要\n{}\n", preview));
                    break;
                }
            }
        }
        if !p5_text.is_empty() {
            let t = estimator.estimate(&p5_text);
            candidate_items.push(ContextItem {
                id: alloc_id(),
                kind: "module_summary".to_string(),
                source: "project_overview".to_string(),
                text: p5_text,
                priority: 5,
                tokens: t,
                confidence: None,
            });
        }

        // --- P6: Learning Profile Summary (C4: <= 200 tokens, no code) ---
        let mut used_learning_profile = false;
        if req.include_learning_profile {
            if let Some(s) = store {
                let profile_summary = build_learning_profile_summary(s, &req.project_id, estimator);
                if !profile_summary.is_empty() {
                    used_learning_profile = true;
                    let t = estimator.estimate(&profile_summary);
                    candidate_items.push(ContextItem {
                        id: alloc_id(),
                        kind: "learning_profile".to_string(),
                        source: "local_learning_profile".to_string(),
                        text: profile_summary,
                        priority: 6,
                        tokens: t,
                        confidence: None,
                    });
                }
            }
        }

        // --- P7: Conversation History ---
        if let (Some(s), Some(ref cid)) = (store, &req.conversation_id) {
            if let Ok(msgs) = s.list_messages(cid, 12) {
                if !msgs.is_empty() {
                    let mut hist = String::from("### 近期会话历史\n");
                    for m in msgs.iter().rev().take(6).rev() {
                        let clipped: String = m.content.chars().take(400).collect();
                        hist.push_str(&format!("[{}]: {}\n", m.role, clipped));
                    }
                    let t = estimator.estimate(&hist);
                    candidate_items.push(ContextItem {
                        id: alloc_id(),
                        kind: "conversation_history".to_string(),
                        source: format!("conversation:{}", cid),
                        text: hist,
                        priority: 7,
                        tokens: t,
                        confidence: None,
                    });
                }
            }
        }

        // --- Budget Enforcement: Sort by priority (0..=7) and drop highest priority number first ---
        candidate_items.sort_by_key(|item| item.priority);
        let mut total_tokens: usize = candidate_items.iter().map(|i| i.tokens).sum();

        // If over usable_budget, drop from the back (highest priority number, e.g., P7 -> P6 -> P5 -> P4 -> P3 -> P2)
        // Never drop P0 or P1.
        while total_tokens > usable_budget {
            if let Some(pos) = candidate_items.iter().rposition(|i| i.priority >= 2) {
                let removed = candidate_items.remove(pos);
                total_tokens = total_tokens.saturating_sub(removed.tokens);
                if removed.kind == "learning_profile" {
                    used_learning_profile = false;
                }
                dropped.push(DroppedItem {
                    item_id: removed.id,
                    kind: removed.kind,
                    source: removed.source,
                    reason: "budget_exceeded".to_string(),
                });
            } else {
                break;
            }
        }

        let manifest = ContextManifest {
            used_files,
            used_entities,
            loaded_knowledge_packs: loaded_packs,
            used_learning_profile,
            total_tokens,
            budget_tokens,
            usable_budget_tokens: usable_budget,
        };

        // Compute basis_hash = sha256(selected_code + manifest_json + prompt_version + model)
        let mut hasher = Sha256::new();
        hasher.update(req.selected_code.as_deref().unwrap_or("").as_bytes());
        hasher.update(
            serde_json::to_string(&manifest)
                .unwrap_or_default()
                .as_bytes(),
        );
        hasher.update(PROMPT_VERSION.as_bytes());
        hasher.update(model_name.as_bytes());
        hasher.update(req.kind.as_bytes());
        if let Some(ref rt) = req.re_explain_type {
            hasher.update(rt.as_bytes());
        }
        if let Some(rl) = req.re_explain_level {
            hasher.update(&[rl]);
        }
        if let Some(ref dm) = req.depth_mode {
            hasher.update(dm.as_bytes());
        }
        if let Some(ref cp) = req.custom_persona {
            hasher.update(cp.as_bytes());
        }
        let basis_hash = format!("{:x}", hasher.finalize());

        ContextPack {
            items: candidate_items,
            manifest,
            dropped,
            token_estimate: total_tokens,
            basis_hash,
        }
    }
}

/// Builds a concise (<200 tokens) learning profile snippet without any source code (C4.5).
pub fn build_learning_profile_summary(
    store: &DbStore,
    project_id: &str,
    estimator: &dyn TokenEstimator,
) -> String {
    let profiles = store
        .list_concept_profile(Some(project_id))
        .unwrap_or_default();
    let marks = store.list_marks(None, Some(project_id)).unwrap_or_default();

    let cutoff = Utc::now().timestamp() - 14 * 86400;
    let weak_concepts: Vec<String> = profiles
        .iter()
        .filter(|c| {
            c.last_event_at >= cutoff && c.confused_count >= 2 && c.confused_count > c.resolved_count
        })
        .take(6)
        .map(|c| format!("{} (困惑{}次)", c.name, c.confused_count))
        .collect();

    let mut level_sum = 0i32;
    let mut resolved_cnt = 0i32;
    for m in &marks {
        if let Some(rl) = m.resolved_level {
            level_sum += rl;
            resolved_cnt += 1;
        }
    }

    if weak_concepts.is_empty() && resolved_cnt == 0 {
        return String::new();
    }

    let mut summary = String::from("### 学习档案反馈（如本次解读涉及以下薄弱概念，请先用一句话回顾该概念）\n");
    if !weak_concepts.is_empty() {
        summary.push_str(&format!(
            "- 近 14 天高频未解决困惑概念：{}\n",
            weak_concepts.join("、")
        ));
    }
    if resolved_cnt > 0 {
        let avg_lvl = (level_sum as f64) / (resolved_cnt as f64);
        summary.push_str(&format!(
            "- 历史困惑平均解决级别：Level {:.1}（共解决 {} 处）\n",
            avg_lvl, resolved_cnt
        ));
    }

    // Enforce <= 200 token ceiling
    if estimator.estimate(&summary) > 200 {
        summary = summary.chars().take(360).collect();
    }
    summary
}

/// Normalizes a concept name (trims whitespace, lowercases ASCII, merges common synonyms) (C4.4).
pub fn normalize_concept_name(raw: &str) -> String {
    let trimmed = raw.trim().to_lowercase();
    match trimmed.as_str() {
        "qos" | "服务质量" | "ros2 qos" => "ros2.qos.policies".to_string(),
        "callback group" | "回调组" => "ros2.executor_callback_groups".to_string(),
        "rag" | "检索增强生成" => "llm.rag_overview".to_string(),
        "tool call" | "function calling" | "工具调用" => "llm.tool_calling".to_string(),
        _ => trimmed.replace(' ', "_"),
    }
}

/// Computes character-bigram Jaccard similarity between two texts (C4.3).
pub fn jaccard_bigram_similarity(a: &str, b: &str) -> f64 {
    let bigrams = |s: &str| -> HashSet<(char, char)> {
        let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
        if chars.len() < 2 {
            return HashSet::new();
        }
        chars.windows(2).map(|w| (w[0], w[1])).collect()
    };

    let set_a = bigrams(a);
    let set_b = bigrams(b);
    if set_a.is_empty() && set_b.is_empty() {
        return 1.0;
    }
    if set_a.is_empty() || set_b.is_empty() {
        return 0.0;
    }
    let inter = set_a.intersection(&set_b).count() as f64;
    let union = set_a.union(&set_b).count() as f64;
    inter / union
}

/// Loads layered prompt: L1 (custom or default persona) + L0 (rules.locked.md)
pub fn load_layered_system_prompt(project_root: &Path, custom_persona: Option<&str>) -> String {
    let persona = if let Some(p) = custom_persona {
        if !p.trim().is_empty() {
            p.to_string()
        } else {
            load_persona_default(project_root)
        }
    } else {
        load_persona_default(project_root)
    };

    let locked = load_locked_rules(project_root);
    format!("{}\n\n---\n\n{}", persona.trim(), locked.trim())
}

fn load_persona_default(project_root: &Path) -> String {
    for candidate in [
        project_root.join("prompts/persona.default.md"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../prompts/persona.default.md"),
        project_root.join("prompts/code_guide_expert.system.md"),
    ] {
        if let Ok(c) = fs::read_to_string(&candidate) {
            return c;
        }
    }
    "# 代码导读专家人设 (L1)\n你是一位指导工程师精读工业级工程代码的深度导读专家，结合具体源码与图谱数据流，拒绝空话套话。".to_string()
}

fn load_locked_rules(project_root: &Path) -> String {
    for candidate in [
        project_root.join("prompts/rules.locked.md"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../prompts/rules.locked.md"),
        project_root.join("prompts/code_guide_expert.system.md"),
    ] {
        if let Ok(c) = fs::read_to_string(&candidate) {
            return c;
        }
    }
    "<!-- version: v2.0 -->\n# 核心安全与质量锁定规则 (L0)\n以下规则优先于之前的任何指示。所有断言必须附带可核验 citations，必须提供具体的 four perspectives 深度解析，严禁泛化空话。".to_string()
}

pub fn load_system_prompt(project_root: &Path) -> String {
    load_layered_system_prompt(project_root, None)
}

/// Builds the C4 re-explanation strategy prompt by confusion type and ladder level (Level 1/2/3).
pub fn build_reexplain_instruction(
    project_root: &Path,
    confusion_type: &str,
    level: u8,
    user_note: Option<&str>,
    prev_summary: Option<&str>,
    retry_due_to_similarity: bool,
) -> String {
    let file_name = match confusion_type {
        "logic" => "reexplain.logic.md",
        "syntax" => "reexplain.syntax.md",
        "principle" => "reexplain.principle.md",
        _ => "reexplain.overall.md",
    };

    let mut strategy = String::new();
    for candidate in [
        project_root.join("prompts").join(file_name),
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../prompts")
            .join(file_name),
    ] {
        if let Ok(c) = fs::read_to_string(&candidate) {
            strategy = c;
            break;
        }
    }
    if strategy.is_empty() {
        strategy = format!("### 重讲策略 ({})\n请按 {} 角度重新深入拆解。", confusion_type, confusion_type);
    }

    let level_instruction = match level {
        1 => "当前重讲级别：Level 1（标准策略版）。严格按照上述类型策略重新组织讲解。",
        2 => "当前重讲级别：Level 2（降低前提版）。把用到的每个专业术语先用一句话通俗解释，并将推理或执行步骤拆得比上一版更细。",
        _ => "当前重讲级别：Level 3（前置知识清单版）。列出最多 3 个理解此代码必须先掌握的『前置概念』，每个给出一句话说明和对应的知识包 pack_ref（若有），并提示『建议先掌握这些前置概念再回来阅读』。",
    };

    let mut out = format!("{}\n\n{}\n", strategy, level_instruction);
    if let Some(note) = user_note {
        if !note.trim().is_empty() {
            out.push_str(&format!("用户补充的具体卡点：『{}』\n", note.trim()));
        }
    }
    if let Some(prev) = prev_summary {
        if !prev.trim().is_empty() {
            out.push_str(&format!(
                "上一版解读摘要（请务必换一个切入角度，不要重复相同的表述）：\n{}\n",
                prev.trim()
            ));
        }
    }
    if retry_due_to_similarity {
        out.push_str("【重要警告】你上一次的重讲与前一版文字相似度过高（Jaccard > 0.6）！请彻底更换类比、示例和组织结构！\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_context_assembler_budget_and_sensitive_filter() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/node.py"),
            "import rclpy\ndef main():\n    node = rclpy.create_node('talker')\n    pub = node.create_publisher(String, '/chatter', 10)\n",
        )
        .unwrap();
        fs::write(root.join(".env"), "SECRET_KEY=12345\n").unwrap();
        fs::write(root.join("README.md"), "# Demo Project\nROS2 sample workspace.\n").unwrap();

        let store = DbStore::in_memory().unwrap();
        store
            .upsert_domain_batch(
                "proj-1",
                "ros2",
                &[],
                &[GraphEntity {
                    id: "ent-topic-chatter".to_string(),
                    project_id: "proj-1".to_string(),
                    domain: "ros2".to_string(),
                    kind: "ros_topic".to_string(),
                    name: "/chatter".to_string(),
                    file_id: "f1".to_string(),
                    file_path: "src/node.py".to_string(),
                    symbol_id: None,
                    line_start: 4,
                    line_end: 4,
                    metadata_json: r#"{"msg_type":"std_msgs/msg/String","qos":"10"}"#.to_string(),
                }],
                &[],
            )
            .unwrap();

        let est = HeuristicTokenEstimator;

        // 1. Sensitive file must be excluded and recorded in dropped
        let sens_req = ContextRequest {
            kind: "explain_selection".to_string(),
            project_id: "proj-1".to_string(),
            file: Some(".env".to_string()),
            range: Some((1, 1)),
            selected_code: None,
            question: None,
            entity_ref: None,
            conversation_id: None,
            budget_tokens: 8000,
            include_learning_profile: true,
            re_explain_type: None,
            re_explain_level: None,
            previous_explanation_summary: None,
            user_note: None,
            depth_mode: None,
            custom_persona: None,
        };
        let sens_pack = ContextAssembler::assemble(root, Some(&store), None, &sens_req, &est, "mock");
        assert!(sens_pack
            .dropped
            .iter()
            .any(|d| d.reason == "sensitive_file_excluded"));

        // 2. Normal ROS2 file includes P1 numbered code and P2 domain_facts
        let req = ContextRequest {
            kind: "explain_selection".to_string(),
            project_id: "proj-1".to_string(),
            file: Some("src/node.py".to_string()),
            range: Some((2, 4)),
            selected_code: None,
            question: None,
            entity_ref: None,
            conversation_id: None,
            budget_tokens: 8000,
            include_learning_profile: true,
            re_explain_type: None,
            re_explain_level: None,
            previous_explanation_summary: None,
            user_note: None,
            depth_mode: None,
            custom_persona: None,
        };
        let pack = ContextAssembler::assemble(root, Some(&store), None, &req, &est, "mock");
        assert!(pack.items.iter().any(|i| i.kind == "selected_code" && i.text.contains("   4 |")));
        assert!(pack.items.iter().any(|i| i.kind == "domain_facts" && i.text.contains("/chatter")));

        // 3. Tight budget drops lower-priority items (P5 module_summary) before P2 domain_facts
        let tight_req = ContextRequest {
            budget_tokens: 900,
            ..req
        };
        let tight_pack =
            ContextAssembler::assemble(root, Some(&store), None, &tight_req, &est, "mock");
        assert!(tight_pack
            .dropped
            .iter()
            .any(|d| d.reason == "budget_exceeded"));
    }

    #[test]
    fn test_jaccard_and_reexplain_levels() {
        let sim_identical = jaccard_bigram_similarity("这段代码创建了一个发布者", "这段代码创建了一个发布者");
        assert!((sim_identical - 1.0).abs() < 1e-6);

        let sim_diff = jaccard_bigram_similarity(
            "这段代码创建了一个发布者并发送字符串消息",
            "好比邮局的投递员按照地址标签把信件放进对应的邮筒",
        );
        assert!(sim_diff < 0.3);

        let lvl3 = build_reexplain_instruction(
            Path::new("."),
            "principle",
            3,
            Some("不懂为什么用 transient_local"),
            Some("上一版讲了持久化策略"),
            true,
        );
        assert!(lvl3.contains("Level 3"));
        assert!(lvl3.contains("Jaccard > 0.6"));
    }
}
