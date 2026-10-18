use crate::verifier::{CitationItem, CitationVerifier, StructuredExplanation};
use chrono::{Duration, Utc};
use noesis_store::{DbStore, ExerciseRecord, GraphEdge, GraphEntity, GraphSymbol};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortAnswerGrade {
    pub score: f64,
    #[serde(default)]
    pub covered: Vec<String>,
    #[serde(default)]
    pub missed: Vec<String>,
    #[serde(default)]
    pub misconceptions: Vec<String>,
    #[serde(default)]
    pub feedback: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sm2ReviewState {
    pub ease: f64,
    pub interval_days: i32,
    pub repetitions: i32,
    pub lapses: i32,
    pub next_review_at: String,
}

/// Maps answer correctness + user hesitation or LLM rubric score to SM-2 quality (1..=5) (Stage D).
pub fn map_answer_to_quality(
    source_kind: &str,
    is_correct: bool,
    hesitation: Option<&str>,
    llm_score: Option<f64>,
) -> u8 {
    if source_kind == "llm" {
        let s = llm_score.unwrap_or(if is_correct { 0.85 } else { 0.2 });
        if s >= 0.8 {
            4
        } else if s >= 0.5 {
            3
        } else {
            1
        }
    } else if !is_correct {
        1
    } else {
        match hesitation.unwrap_or("normal") {
            "easy" => 5,
            "hesitant" => 3,
            _ => 4,
        }
    }
}

/// Stage D SM-2 spaced repetition state transitions:
/// - 1st correct -> 1 day
/// - 2nd correct -> 6 days
/// - 3rd+ correct -> round(prev_interval * ease) (e.g. 6 * 2.5 = 15 days)
/// - Wrong (quality < 3) -> interval = 1 day, repetitions = 0, lapses += 1
/// - Ease floor = 1.3
pub fn compute_sm2_review(
    quality: u8,
    current_ease: f64,
    current_interval: i32,
    current_repetitions: i32,
    current_lapses: i32,
) -> Sm2ReviewState {
    let base_ease = if current_ease < 1.3 { 2.5 } else { current_ease };
    let q = (quality.clamp(0, 5)) as f64;

    if quality >= 3 {
        let next_rep = current_repetitions + 1;
        let next_ease = (base_ease + 0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02)).max(1.3);
        let next_interval = match next_rep {
            1 => 1,
            2 => 6,
            _ => ((current_interval.max(1) as f64) * next_ease).round() as i32,
        };
        let next_review_at = (Utc::now() + Duration::days(next_interval as i64)).to_rfc3339();
        Sm2ReviewState {
            ease: (next_ease * 100.0).round() / 100.0,
            interval_days: next_interval,
            repetitions: next_rep,
            lapses: current_lapses,
            next_review_at,
        }
    } else {
        let next_ease = (base_ease - 0.2).max(1.3);
        let next_review_at = (Utc::now() + Duration::days(1)).to_rfc3339();
        Sm2ReviewState {
            ease: (next_ease * 100.0).round() / 100.0,
            interval_days: 1,
            repetitions: 0,
            lapses: current_lapses + 1,
            next_review_at,
        }
    }
}

/// Checks if two strings differ by only 1 character (to avoid overly confusing 1-char typo distractors when possible).
fn is_single_char_diff(a: &str, b: &str) -> bool {
    let ca: Vec<char> = a.chars().collect();
    let cb: Vec<char> = b.chars().collect();
    if ca.len() != cb.len() {
        return false;
    }
    let diffs = ca.iter().zip(cb.iter()).filter(|(x, y)| x != y).count();
    diffs == 1
}

/// Selects 3 distinct distractors from `pool` that are not in `correct_set`.
/// Returns `None` if fewer than 3 valid distractors exist in the project.
pub fn pick_distractors(correct_set: &HashSet<String>, pool: &[String]) -> Option<Vec<String>> {
    let mut distinct_candidates: Vec<String> = Vec::new();
    for item in pool {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty()
            || correct_set.contains(&trimmed)
            || distinct_candidates.contains(&trimmed)
        {
            continue;
        }
        distinct_candidates.push(trimmed);
    }

    // Prefer candidates that aren't a 1-char diff from any correct answer if we have enough
    let non_confusing: Vec<String> = distinct_candidates
        .iter()
        .filter(|cand| !correct_set.iter().any(|corr| is_single_char_diff(cand, corr)))
        .cloned()
        .collect();

    let source_list = if non_confusing.len() >= 3 {
        non_confusing
    } else {
        distinct_candidates
    };

    if source_list.len() < 3 {
        return None;
    }

    Some(source_list.into_iter().take(3).collect())
}

/// Deterministically generates `fact` exercises strictly from `confidence == "exact"` graph edges and entities.
pub fn generate_fact_exercises(
    project_id: &str,
    entities: &[GraphEntity],
    edges: &[GraphEdge],
    symbols: &[GraphSymbol],
    max_count: usize,
) -> Vec<ExerciseRecord> {
    let mut out = Vec::new();
    let entity_by_id: HashMap<&str, &GraphEntity> =
        entities.iter().map(|e| (e.id.as_str(), e)).collect();

    // Collect pools by entity kind
    let topic_pool: Vec<String> = entities
        .iter()
        .filter(|e| e.kind == "ros_topic")
        .map(|e| e.name.clone())
        .collect();
    let node_pool: Vec<String> = entities
        .iter()
        .filter(|e| e.kind == "ros_node")
        .map(|e| e.name.clone())
        .collect();
    let llm_target_pool: Vec<String> = entities
        .iter()
        .filter(|e| e.domain == "llm_app")
        .map(|e| e.name.clone())
        .collect();
    let sym_pool: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();

    let now_str = Utc::now().to_rfc3339();

    // 1. ROS2 & LLM exact edges
    for edge in edges {
        if out.len() >= max_count {
            break;
        }
        // HARD RULE: Only use confidence == "exact"
        if edge.confidence != "exact" {
            continue;
        }

        let src_ent = entity_by_id.get(edge.src_id.as_str()).copied();
        let dst_ent = entity_by_id.get(edge.dst_id.as_str()).copied();

        if edge.kind == "publishes" || edge.kind == "subscribes" {
            if let (Some(s_ent), Some(d_ent)) = (src_ent, dst_ent) {
                // Collect all topics published/subscribed by this same node with this edge kind
                let all_correct_for_node: HashSet<String> = edges
                    .iter()
                    .filter(|e| {
                        e.confidence == "exact"
                            && e.src_id == s_ent.id
                            && e.kind == edge.kind
                    })
                    .filter_map(|e| entity_by_id.get(e.dst_id.as_str()).map(|t| t.name.clone()))
                    .collect();

                if let Some(distractors) = pick_distractors(&all_correct_for_node, &topic_pool) {
                    let verb = if edge.kind == "publishes" {
                        "发布"
                    } else {
                        "订阅"
                    };
                    let mut options = vec![d_ent.name.clone()];
                    options.extend(distractors);
                    out.push(ExerciseRecord {
                        id: format!("ex-fact-{}-{}", project_id, edge.id),
                        unit_id: format!("ros2:{}", s_ent.name),
                        exercise_type: "multiple_choice".to_string(),
                        question: format!("在 ROS2 架构图中，节点 `{}` {}了以下哪个话题？", s_ent.name, verb),
                        snippet: format!("// {}:L{}\n{}", edge.file_path, edge.line, edge.evidence),
                        options_json: serde_json::to_string(&options).ok(),
                        reference_answer: d_ent.name.clone(),
                        file_path: edge.file_path.clone(),
                        next_review_at: now_str.clone(),
                        repetition: 0,
                        interval_days: 1,
                        project_id: project_id.to_string(),
                        source_kind: "fact".to_string(),
                        verified_by_program: true,
                        ease: 2.5,
                        lapses: 0,
                        stale: false,
                    });
                }
            }
        } else if edge.kind == "launches" {
            if let (Some(s_ent), Some(d_ent)) = (src_ent, dst_ent) {
                let all_launched: HashSet<String> = edges
                    .iter()
                    .filter(|e| e.confidence == "exact" && e.src_id == s_ent.id && e.kind == "launches")
                    .filter_map(|e| entity_by_id.get(e.dst_id.as_str()).map(|n| n.name.clone()))
                    .collect();
                if let Some(distractors) = pick_distractors(&all_launched, &node_pool) {
                    let mut options = vec![d_ent.name.clone()];
                    options.extend(distractors);
                    out.push(ExerciseRecord {
                        id: format!("ex-fact-{}-{}", project_id, edge.id),
                        unit_id: format!("ros2:launch:{}", s_ent.name),
                        exercise_type: "multiple_choice".to_string(),
                        question: format!("启动文件 `{}` 启动了以下哪个 ROS2 节点？", s_ent.name),
                        snippet: format!("// {}:L{}\n{}", edge.file_path, edge.line, edge.evidence),
                        options_json: serde_json::to_string(&options).ok(),
                        reference_answer: d_ent.name.clone(),
                        file_path: edge.file_path.clone(),
                        next_review_at: now_str.clone(),
                        repetition: 0,
                        interval_days: 1,
                        project_id: project_id.to_string(),
                        source_kind: "fact".to_string(),
                        verified_by_program: true,
                        ease: 2.5,
                        lapses: 0,
                        stale: false,
                    });
                }
            }
        } else if edge.kind == "invokes_llm" || edge.kind == "uses_prompt" || edge.kind == "registers_tool" {
            if let (Some(s_ent), Some(d_ent)) = (src_ent, dst_ent) {
                let mut correct_set = HashSet::new();
                correct_set.insert(d_ent.name.clone());
                if let Some(distractors) = pick_distractors(&correct_set, &llm_target_pool) {
                    let mut options = vec![d_ent.name.clone()];
                    options.extend(distractors);
                    out.push(ExerciseRecord {
                        id: format!("ex-fact-{}-{}", project_id, edge.id),
                        unit_id: format!("llm:{}", s_ent.name),
                        exercise_type: "multiple_choice".to_string(),
                        question: format!(
                            "在大模型应用链路中，`{}` 关联或调用了以下哪个组件（关系：{}）？",
                            s_ent.name, edge.kind
                        ),
                        snippet: format!("// {}:L{}\n{}", edge.file_path, edge.line, edge.evidence),
                        options_json: serde_json::to_string(&options).ok(),
                        reference_answer: d_ent.name.clone(),
                        file_path: edge.file_path.clone(),
                        next_review_at: now_str.clone(),
                        repetition: 0,
                        interval_days: 1,
                        project_id: project_id.to_string(),
                        source_kind: "fact".to_string(),
                        verified_by_program: true,
                        ease: 2.5,
                        lapses: 0,
                        stale: false,
                    });
                }
            }
        } else if edge.kind == "calls" {
            let caller_sym = symbols
                .iter()
                .find(|s| s.id == edge.src_id || s.qualified_name == edge.src_id);
            let callee_sym = symbols
                .iter()
                .find(|s| s.id == edge.dst_id || s.qualified_name == edge.dst_id || s.name == edge.dst_id);
            if let (Some(caller), Some(callee)) = (caller_sym, callee_sym) {
                let all_callers: HashSet<String> = edges
                    .iter()
                    .filter(|e| {
                        e.confidence == "exact"
                            && e.kind == "calls"
                            && (e.dst_id == callee.id || e.dst_id == callee.qualified_name)
                    })
                    .filter_map(|e| {
                        symbols
                            .iter()
                            .find(|s| s.id == e.src_id || s.qualified_name == e.src_id)
                            .map(|s| s.name.clone())
                    })
                    .collect();
                if let Some(distractors) = pick_distractors(&all_callers, &sym_pool) {
                    let mut options = vec![caller.name.clone()];
                    options.extend(distractors);
                    out.push(ExerciseRecord {
                        id: format!("ex-fact-{}-{}", project_id, edge.id),
                        unit_id: format!("sym:{}", callee.qualified_name),
                        exercise_type: "multiple_choice".to_string(),
                        question: format!("函数 `{}` 被以下哪个函数直接调用？", callee.name),
                        snippet: format!("// {}:L{}\n{}", edge.file_path, edge.line, edge.evidence),
                        options_json: serde_json::to_string(&options).ok(),
                        reference_answer: caller.name.clone(),
                        file_path: edge.file_path.clone(),
                        next_review_at: now_str.clone(),
                        repetition: 0,
                        interval_days: 1,
                        project_id: project_id.to_string(),
                        source_kind: "fact".to_string(),
                        verified_by_program: true,
                        ease: 2.5,
                        lapses: 0,
                        stale: false,
                    });
                }
            }
        }
    }

    out
}

/// Deterministically generates a `cloze` (fill-in-the-blank) exercise from a code snippet,
/// replacing ALL occurrences of the target identifier in the snippet so the answer is never leaked.
pub fn generate_cloze_exercise(
    project_id: &str,
    file_path: &str,
    snippet: &str,
    target_token: Option<&str>,
) -> Option<ExerciseRecord> {
    let token_re = Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]{3,})\b").ok()?;
    let keywords: HashSet<&str> = [
        "def", "class", "return", "import", "from", "self", "None", "True", "False", "async",
        "await", "void", "auto", "const", "std", "public", "private", "struct", "include", "this",
    ]
    .into_iter()
    .collect();

    let chosen = if let Some(t) = target_token {
        if snippet.contains(t) {
            t.to_string()
        } else {
            return None;
        }
    } else {
        let lower_path = file_path.to_ascii_lowercase();
        let mut found = None;
        for cap in token_re.captures_iter(snippet) {
            let word = &cap[1];
            if !keywords.contains(word) && !lower_path.contains(&word.to_ascii_lowercase()) {
                found = Some(word.to_string());
                break;
            }
        }
        found?
    };

    // Replace ALL occurrences of `chosen` in `snippet` with `____`
    let masked = snippet.replace(&chosen, "____");
    if masked == snippet || masked.contains(&chosen) {
        return None;
    }

    Some(ExerciseRecord {
        id: format!(
            "ex-cloze-{}-{}",
            project_id,
            file_path.replace(['/', '\\', '.'], "_")
        ),
        unit_id: format!("cloze:{}", file_path),
        exercise_type: "fill_blank".to_string(),
        question: format!("请补全 `{}` 代码片段中缺失的关键标识符 `____`：", file_path),
        snippet: masked,
        options_json: None,
        reference_answer: chosen,
        file_path: file_path.to_string(),
        next_review_at: Utc::now().to_rfc3339(),
        repetition: 0,
        interval_days: 1,
        project_id: project_id.to_string(),
        source_kind: "cloze".to_string(),
        verified_by_program: true,
        ease: 2.5,
        lapses: 0,
        stale: false,
    })
}

/// Grades a deterministic (`fact` or `cloze`) exercise answer without calling an LLM.
pub fn grade_deterministic_answer(user_answer: &str, reference_answer: &str) -> bool {
    let normalize = |s: &str| -> String {
        s.trim()
            .trim_matches(|c| c == '\'' || c == '"' || c == '`')
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    };
    let ua = normalize(user_answer);
    if ua.is_empty() {
        return false;
    }
    reference_answer
        .split('|')
        .any(|alt| normalize(alt) == ua)
}

/// Generates an `llm` short-answer exercise grounded in verified explanation bullets (`verified_by_program = false`).
pub fn generate_llm_rubric_exercise(
    project_id: &str,
    file_path: &str,
    explanation: &StructuredExplanation,
) -> Option<ExerciseRecord> {
    let mut rubric_points: Vec<String> = Vec::new();
    for sec in [
        &explanation.overall,
        &explanation.logic,
        &explanation.syntax,
        &explanation.principle,
    ] {
        for b in &sec.bullets {
            if b.status == "verified" || b.status == "knowledge" {
                rubric_points.push(b.text.clone());
            }
        }
    }
    if rubric_points.is_empty() {
        return None;
    }
    let rubric_summary: Vec<String> = rubric_points.into_iter().take(3).collect();
    let ref_ans = rubric_summary.join("；");

    Some(ExerciseRecord {
        id: format!(
            "ex-llm-{}-{}-{}",
            project_id,
            file_path.replace(['/', '\\', '.'], "_"),
            Utc::now().timestamp_millis()
        ),
        unit_id: format!("llm_short:{}", file_path),
        exercise_type: "short_answer".to_string(),
        question: format!(
            "【由 AI 批改】请简述 `{}` 中这段代码的核心职责与关键处理逻辑：",
            file_path
        ),
        snippet: format!("评分要点（Rubric 共 {} 项）", rubric_summary.len()),
        options_json: serde_json::to_string(&rubric_summary).ok(),
        reference_answer: ref_ans,
        file_path: file_path.to_string(),
        next_review_at: Utc::now().to_rfc3339(),
        repetition: 0,
        interval_days: 1,
        project_id: project_id.to_string(),
        source_kind: "llm".to_string(),
        verified_by_program: false,
        ease: 2.5,
        lapses: 0,
        stale: false,
    })
}

/// Parses the LLM short-answer grading JSON response with fallback tolerance.
pub fn parse_short_answer_grade(raw: &str) -> Result<ShortAnswerGrade, String> {
    let trimmed = raw.trim();
    if let Ok(g) = serde_json::from_str::<ShortAnswerGrade>(trimmed) {
        return Ok(g);
    }
    if let (Some(s), Some(e)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if s < e {
            if let Ok(g) = serde_json::from_str::<ShortAnswerGrade>(&trimmed[s..=e]) {
                return Ok(g);
            }
        }
    }
    Err("Failed to parse short answer grading JSON".to_string())
}

/// Checks if an exercise's evidence quote is still valid in the workspace using `CitationVerifier`.
/// Returns `true` if valid (or line-corrected), or `false` if stale.
pub fn check_exercise_evidence_freshness(
    project_root: &Path,
    file_path: &str,
    line_start: u32,
    line_end: u32,
    evidence_quote: &str,
) -> (bool, Option<(u32, u32)>) {
    let mut exp = StructuredExplanation {
        overall: Default::default(),
        logic: Default::default(),
        syntax: Default::default(),
        principle: Default::default(),
        citations: vec![CitationItem {
            id: "c1".to_string(),
            file: file_path.to_string(),
            line_start,
            line_end,
            quote: evidence_quote.to_string(),
            status: String::new(),
            corrected_line_start: None,
            corrected_line_end: None,
            reason: None,
        }],
        unknowns: vec![],
        concepts: vec![],
    };

    CitationVerifier::verify(project_root, &[], &mut exp);
    let c = &exp.citations[0];
    match c.status.as_str() {
        "verified" => (true, Some((c.line_start, c.line_end))),
        "corrected" => (
            true,
            Some((
                c.corrected_line_start.unwrap_or(c.line_start),
                c.corrected_line_end.unwrap_or(c.line_end),
            )),
        ),
        _ => (false, None),
    }
}

/// Refreshes all exercises for a project after re-indexing, marking broken evidence as `stale = true`.
pub fn refresh_project_exercises_freshness(project_root: &Path, store: &DbStore, project_id: &str) {
    if let Ok(exercises) = store.list_due_exercises() {
        for ex in exercises {
            if !project_id.is_empty() && ex.project_id != project_id {
                continue;
            }
            if ex.file_path.is_empty() || !ex.snippet.starts_with("// ") {
                continue;
            }
            let lines: Vec<&str> = ex.snippet.lines().collect();
            if lines.len() >= 2 {
                let quote = lines[1..].join("\n");
                let (fresh, _) =
                    check_exercise_evidence_freshness(project_root, &ex.file_path, 1, 200, &quote);
                if !fresh {
                    let _ = store.mark_exercise_stale(&ex.id, true);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sm2_15_cases_table() {
        // (case_id, quality, ease, interval, reps, lapses, expected_interval, expected_reps, expected_lapses, min_ease, max_ease)
        let cases: Vec<(&str, u8, f64, i32, i32, i32, i32, i32, i32, f64, f64)> = vec![
            ("01_first_correct_q4", 4, 2.5, 0, 0, 0, 1, 1, 0, 2.49, 2.51),
            ("02_second_correct_q4", 4, 2.5, 1, 1, 0, 6, 2, 0, 2.49, 2.51),
            ("03_third_correct_q4_15days", 4, 2.5, 6, 2, 0, 15, 3, 0, 2.49, 2.51),
            ("04_fourth_correct_q5", 5, 2.5, 15, 3, 0, 39, 4, 0, 2.59, 2.61),
            ("05_hesitant_q3_first", 3, 2.5, 0, 0, 0, 1, 1, 0, 2.35, 2.37),
            ("06_hesitant_q3_second", 3, 2.36, 1, 1, 0, 6, 2, 0, 2.21, 2.23),
            ("07_hesitant_q3_third", 3, 2.22, 6, 2, 0, 12, 3, 0, 2.07, 2.09),
            ("08_wrong_q1_resets_from_rep3", 1, 2.5, 15, 3, 0, 1, 0, 1, 2.29, 2.31),
            ("09_wrong_q1_second_lapse", 1, 2.3, 1, 0, 1, 1, 0, 2, 2.09, 2.11),
            ("10_ease_floor_on_wrong", 1, 1.35, 6, 2, 3, 1, 0, 4, 1.30, 1.30),
            ("11_ease_floor_on_q3", 3, 1.32, 6, 2, 2, 8, 3, 2, 1.30, 1.30),
            ("12_easy_q5_first", 5, 2.5, 0, 0, 0, 1, 1, 0, 2.59, 2.61),
            ("13_easy_q5_second", 5, 2.6, 1, 1, 0, 6, 2, 0, 2.69, 2.71),
            ("14_easy_q5_third", 5, 2.7, 6, 2, 0, 17, 3, 0, 2.79, 2.81),
            ("15_recovery_after_lapse", 4, 2.1, 1, 0, 2, 1, 1, 2, 2.09, 2.11),
        ];

        assert_eq!(cases.len(), 15);
        for (name, q, ease, ivl, reps, lapses, exp_ivl, exp_reps, exp_lapses, min_e, max_e) in
            cases
        {
            let st = compute_sm2_review(q, ease, ivl, reps, lapses);
            assert_eq!(st.interval_days, exp_ivl, "interval mismatch on {}", name);
            assert_eq!(st.repetitions, exp_reps, "reps mismatch on {}", name);
            assert_eq!(st.lapses, exp_lapses, "lapses mismatch on {}", name);
            assert!(
                st.ease >= min_e && st.ease <= max_e,
                "ease {} not in [{}..={}] on {}",
                st.ease,
                min_e,
                max_e,
                name
            );
        }
    }

    #[test]
    fn test_fact_exercises_only_exact_and_valid_distractors() {
        let entities = vec![
            GraphEntity {
                id: "node1".into(),
                project_id: "p1".into(),
                domain: "ros2".into(),
                kind: "ros_node".into(),
                name: "iat_publish_node".into(),
                file_id: "f1".into(),
                file_path: "src/iat.cpp".into(),
                symbol_id: None,
                line_start: 10,
                line_end: 20,
                metadata_json: "{}".into(),
            },
            GraphEntity {
                id: "t1".into(),
                project_id: "p1".into(),
                domain: "ros2".into(),
                kind: "ros_topic".into(),
                name: "/voice_words".into(),
                file_id: "f1".into(),
                file_path: "src/iat.cpp".into(),
                symbol_id: None,
                line_start: 15,
                line_end: 15,
                metadata_json: "{}".into(),
            },
            GraphEntity {
                id: "t2".into(),
                project_id: "p1".into(),
                domain: "ros2".into(),
                kind: "ros_topic".into(),
                name: "/tts_topic".into(),
                file_id: "f1".into(),
                file_path: "src/tts.cpp".into(),
                symbol_id: None,
                line_start: 15,
                line_end: 15,
                metadata_json: "{}".into(),
            },
            GraphEntity {
                id: "t3".into(),
                project_id: "p1".into(),
                domain: "ros2".into(),
                kind: "ros_topic".into(),
                name: "/cmd_vel".into(),
                file_id: "f1".into(),
                file_path: "src/nav.cpp".into(),
                symbol_id: None,
                line_start: 15,
                line_end: 15,
                metadata_json: "{}".into(),
            },
            GraphEntity {
                id: "t4".into(),
                project_id: "p1".into(),
                domain: "ros2".into(),
                kind: "ros_topic".into(),
                name: "/odom_raw".into(),
                file_id: "f1".into(),
                file_path: "src/odom.cpp".into(),
                symbol_id: None,
                line_start: 15,
                line_end: 15,
                metadata_json: "{}".into(),
            },
        ];

        let edges = vec![
            GraphEdge {
                id: "e-exact".into(),
                project_id: "p1".into(),
                src_type: "entity".into(),
                src_id: "node1".into(),
                dst_type: "entity".into(),
                dst_id: "t1".into(),
                kind: "publishes".into(),
                file_id: "f1".into(),
                file_path: "src/iat.cpp".into(),
                line: 15,
                confidence: "exact".into(),
                evidence: "create_publisher<String>(\"/voice_words\", 10)".into(),
            },
            GraphEdge {
                id: "e-heuristic".into(),
                project_id: "p1".into(),
                src_type: "entity".into(),
                src_id: "node1".into(),
                dst_type: "entity".into(),
                dst_id: "t2".into(),
                kind: "subscribes".into(),
                file_id: "f1".into(),
                file_path: "src/iat.cpp".into(),
                line: 18,
                confidence: "heuristic".into(),
                evidence: "create_subscription(topic_var)".into(),
            },
        ];

        let generated = generate_fact_exercises("p1", &entities, &edges, &[], 10);
        // Only the exact edge must produce an exercise; heuristic edge is ignored
        assert_eq!(generated.len(), 1);
        assert_eq!(generated[0].reference_answer, "/voice_words");
        assert!(generated[0].verified_by_program);

        let opts: Vec<String> =
            serde_json::from_str(generated[0].options_json.as_deref().unwrap()).unwrap();
        assert_eq!(opts.len(), 4);
        let unique_opts: HashSet<String> = opts.into_iter().collect();
        assert_eq!(unique_opts.len(), 4);
    }

    #[test]
    fn test_cloze_no_answer_leak() {
        let code = "pub = node.create_publisher(String, '/chatter', 10)\nnode.get_logger().info(str(create_publisher))";
        let ex = generate_cloze_exercise("p1", "src/node.py", code, Some("create_publisher"))
            .expect("should generate cloze");
        assert!(!ex.snippet.contains("create_publisher"));
        assert!(ex.snippet.contains("____"));
        assert!(grade_deterministic_answer("create_publisher", &ex.reference_answer));
        assert!(!grade_deterministic_answer("create_subscription", &ex.reference_answer));
    }
}
