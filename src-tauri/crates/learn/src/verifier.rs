use crate::knowledge::KnowledgeBase;
use noesis_project::is_sensitive_path;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CitationItem {
    pub id: String,
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub quote: String,
    /// Programmatically computed: `"verified" | "corrected" | "failed"`
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub corrected_line_start: Option<u32>,
    #[serde(default)]
    pub corrected_line_end: Option<u32>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExplanationBullet {
    pub text: String,
    /// `"code" | "knowledge"`
    #[serde(default = "default_bullet_kind")]
    pub kind: String,
    #[serde(default)]
    pub cites: Vec<String>,
    #[serde(default)]
    pub pack_ref: Option<String>,
    /// Programmatically computed: `"verified" | "corrected" | "unverified" | "knowledge"`
    #[serde(default)]
    pub status: String,
}

fn default_bullet_kind() -> String {
    "code".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExplanationSection {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub one_sentence: String,
    #[serde(default)]
    pub bullets: Vec<ExplanationBullet>,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StructuredExplanation {
    #[serde(default)]
    pub overall: ExplanationSection,
    #[serde(default)]
    pub logic: ExplanationSection,
    #[serde(default)]
    pub syntax: ExplanationSection,
    #[serde(default)]
    pub principle: ExplanationSection,
    #[serde(default)]
    pub citations: Vec<CitationItem>,
    #[serde(default)]
    pub unknowns: Vec<String>,
    #[serde(default)]
    pub concepts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerificationSummary {
    pub total_citations: usize,
    pub verified_citations: usize,
    pub corrected_citations: usize,
    pub failed_citations: usize,
    pub code_bullets_total: usize,
    pub code_bullets_verified_or_corrected: usize,
    pub code_bullets_unverified: usize,
    pub knowledge_bullets_total: usize,
    pub verification_rate: f64,
}

pub struct CitationVerifier;

impl CitationVerifier {
    /// Programmatically verifies all citations and bullets in `explanation` against files under `project_root`.
    /// Any `status` field provided by the LLM is overwritten.
    pub fn verify(
        project_root: &Path,
        loaded_pack_ids: &[String],
        explanation: &mut StructuredExplanation,
    ) -> VerificationSummary {
        let canonical_root =
            fs::canonicalize(project_root).unwrap_or_else(|_| project_root.to_path_buf());

        let mut cite_status_map: HashMap<String, String> = HashMap::new();
        let mut verified_citations = 0usize;
        let mut corrected_citations = 0usize;
        let mut failed_citations = 0usize;

        for cite in &mut explanation.citations {
            // Reset any LLM-supplied status
            cite.status.clear();
            cite.corrected_line_start = None;
            cite.corrected_line_end = None;
            cite.reason = None;

            verify_single_citation(&canonical_root, cite);
            match cite.status.as_str() {
                "verified" => verified_citations += 1,
                "corrected" => corrected_citations += 1,
                _ => failed_citations += 1,
            }
            cite_status_map.insert(cite.id.clone(), cite.status.clone());
        }

        let mut code_bullets_total = 0usize;
        let mut code_bullets_ok = 0usize;
        let mut code_bullets_unverified = 0usize;
        let mut knowledge_bullets_total = 0usize;

        let sections = [
            &mut explanation.overall,
            &mut explanation.logic,
            &mut explanation.syntax,
            &mut explanation.principle,
        ];

        for sec in sections {
            for bullet in &mut sec.bullets {
                bullet.status.clear();
                bullet.pack_ref =
                    KnowledgeBase::validate_pack_ref(bullet.pack_ref.as_deref(), loaded_pack_ids);

                if bullet.kind == "knowledge" {
                    bullet.status = "knowledge".to_string();
                    knowledge_bullets_total += 1;
                } else {
                    bullet.kind = "code".to_string();
                    code_bullets_total += 1;
                    if bullet.cites.is_empty() {
                        bullet.status = "unverified".to_string();
                        code_bullets_unverified += 1;
                    } else {
                        let mut any_failed = false;
                        let mut any_corrected = false;
                        for cid in &bullet.cites {
                            match cite_status_map.get(cid).map(|s| s.as_str()) {
                                Some("verified") => {}
                                Some("corrected") => any_corrected = true,
                                _ => any_failed = true,
                            }
                        }
                        if any_failed {
                            bullet.status = "unverified".to_string();
                            code_bullets_unverified += 1;
                        } else if any_corrected {
                            bullet.status = "corrected".to_string();
                            code_bullets_ok += 1;
                        } else {
                            bullet.status = "verified".to_string();
                            code_bullets_ok += 1;
                        }
                    }
                }
            }
        }

        let verification_rate = if code_bullets_total > 0 {
            (code_bullets_ok as f64) / (code_bullets_total as f64)
        } else if !explanation.citations.is_empty() {
            ((verified_citations + corrected_citations) as f64)
                / (explanation.citations.len() as f64)
        } else {
            1.0
        };

        VerificationSummary {
            total_citations: explanation.citations.len(),
            verified_citations,
            corrected_citations,
            failed_citations,
            code_bullets_total,
            code_bullets_verified_or_corrected: code_bullets_ok,
            code_bullets_unverified,
            knowledge_bullets_total,
            verification_rate,
        }
    }
}

fn verify_single_citation(canonical_root: &Path, cite: &mut CitationItem) {
    let rel_str = cite.file.trim().replace('\\', "/");
    if rel_str.is_empty() {
        cite.status = "failed".to_string();
        cite.reason = Some("Empty file path".to_string());
        return;
    }

    let rel_path = Path::new(&rel_str);
    // 1. Reject `..` or absolute paths immediately
    if rel_path.is_absolute()
        || rel_path
            .components()
            .any(|c| matches!(c, Component::ParentDir))
    {
        cite.status = "failed".to_string();
        cite.reason = Some("Path traversal or absolute path not allowed".to_string());
        return;
    }

    // 2. Reject sensitive files
    if is_sensitive_path(rel_path) {
        cite.status = "failed".to_string();
        cite.reason = Some("Sensitive file path blocked".to_string());
        return;
    }

    let candidate = canonical_root.join(rel_path);
    let Ok(canonical_file) = fs::canonicalize(&candidate) else {
        cite.status = "failed".to_string();
        cite.reason = Some("File does not exist in workspace".to_string());
        return;
    };

    if !canonical_file.starts_with(canonical_root) || !canonical_file.is_file() {
        cite.status = "failed".to_string();
        cite.reason = Some("Resolved path escapes workspace root".to_string());
        return;
    }

    let Ok(content) = fs::read_to_string(&canonical_file) else {
        cite.status = "failed".to_string();
        cite.reason = Some("Unable to read file as UTF-8".to_string());
        return;
    };

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len() as u32;
    if total_lines == 0 {
        cite.status = "failed".to_string();
        cite.reason = Some("File is empty".to_string());
        return;
    }

    let norm_quote = normalize_ws(&cite.quote);
    if norm_quote.is_empty() {
        cite.status = "failed".to_string();
        cite.reason = Some("Empty quote".to_string());
        return;
    }

    // Check line range bounds and max 200 lines span
    let range_valid = cite.line_start >= 1
        && cite.line_start <= cite.line_end
        && (cite.line_end - cite.line_start) <= 200
        && cite.line_end <= total_lines;

    if range_valid {
        // Exact range check [line_start..=line_end]
        let s_idx = (cite.line_start - 1) as usize;
        let e_idx = cite.line_end as usize;
        let exact_slice = normalize_ws(&lines[s_idx..e_idx].join("\n"));
        if exact_slice.contains(&norm_quote) {
            cite.status = "verified".to_string();
            return;
        }

        // Tolerance window check [line_start - 2 ..= line_end + 2]
        let tol_start = cite.line_start.saturating_sub(2).max(1);
        let tol_end = (cite.line_end + 2).min(total_lines);
        let tol_slice = normalize_ws(&lines[(tol_start - 1) as usize..tol_end as usize].join("\n"));
        if tol_slice.contains(&norm_quote) {
            if let Some((cs, ce)) = find_unique_quote_lines(&lines, &norm_quote) {
                cite.corrected_line_start = Some(cs);
                cite.corrected_line_end = Some(ce);
            } else {
                cite.corrected_line_start = Some(tol_start);
                cite.corrected_line_end = Some(tol_end);
            }
            cite.status = "corrected".to_string();
            cite.reason = Some("Matched within ±2 line tolerance".to_string());
            return;
        }
    } else if (cite.line_end.saturating_sub(cite.line_start)) > 200 {
        cite.status = "failed".to_string();
        cite.reason = Some("Line range exceeds 200 lines limit".to_string());
        return;
    }

    // Whole-file fallback search: only succeed if quote appears uniquely in the file
    if let Some((cs, ce)) = find_unique_quote_lines(&lines, &norm_quote) {
        cite.status = "corrected".to_string();
        cite.corrected_line_start = Some(cs);
        cite.corrected_line_end = Some(ce);
        cite.reason = Some("Line numbers corrected by unique quote match in file".to_string());
    } else {
        cite.status = "failed".to_string();
        cite.reason = Some("Quote not found or ambiguous in file".to_string());
    }
}

fn normalize_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Finds a unique occurrence of `norm_quote` across `lines`, returning 1-indexed `(start_line, end_line)`.
/// Returns `None` if 0 matches or >= 2 matches.
fn find_unique_quote_lines(lines: &[&str], norm_quote: &str) -> Option<(u32, u32)> {
    let mut matches = Vec::new();
    for start_idx in 0..lines.len() {
        let mut acc = String::new();
        for end_idx in start_idx..(start_idx + 30).min(lines.len()) {
            acc.push_str(&normalize_ws(lines[end_idx]));
            if acc.contains(norm_quote) {
                // Check that it didn't already start on `start_idx + 1`
                let without_first = if end_idx > start_idx {
                    normalize_ws(&lines[start_idx + 1..=end_idx].join("\n"))
                } else {
                    String::new()
                };
                if !without_first.contains(norm_quote) {
                    matches.push(((start_idx + 1) as u32, (end_idx + 1) as u32));
                }
                break;
            }
        }
    }
    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}

/// Extracts and parses `StructuredExplanation` from raw LLM text (supports direct JSON,
/// ```json fences, balanced braces, and legacy `key_bullets`/`detailed_body` format).
pub fn extract_and_parse_explanation_json(raw: &str) -> Result<StructuredExplanation, String> {
    let json_str = extract_balanced_json(raw).ok_or_else(|| "No JSON object found in LLM response".to_string())?;
    let val: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("Invalid JSON syntax: {}", e))?;

    let obj = val
        .as_object()
        .ok_or_else(|| "Root JSON value is not an object".to_string())?;

    let parse_sec = |key: &str, default_title: &str| -> ExplanationSection {
        let Some(sec_val) = obj.get(key) else {
            return ExplanationSection {
                title: default_title.to_string(),
                ..Default::default()
            };
        };
        let title = sec_val
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(default_title)
            .to_string();
        let one_sentence = sec_val
            .get("one_sentence")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let detail = sec_val
            .get("detail")
            .or_else(|| sec_val.get("detailed_body"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut bullets = Vec::new();
        if let Some(arr) = sec_val.get("bullets").and_then(|v| v.as_array()) {
            for b in arr {
                if let Some(s) = b.as_str() {
                    bullets.push(ExplanationBullet {
                        text: s.to_string(),
                        kind: "code".to_string(),
                        cites: vec![],
                        pack_ref: None,
                        status: String::new(),
                    });
                } else if let Some(b_obj) = b.as_object() {
                    let text = b_obj
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let kind = b_obj
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("code")
                        .to_string();
                    let cites = b_obj
                        .get("cites")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let pack_ref = b_obj
                        .get("pack_ref")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    bullets.push(ExplanationBullet {
                        text,
                        kind,
                        cites,
                        pack_ref,
                        status: String::new(),
                    });
                }
            }
        } else if let Some(legacy_arr) = sec_val.get("key_bullets").and_then(|v| v.as_array()) {
            for b in legacy_arr {
                if let Some(s) = b.as_str() {
                    bullets.push(ExplanationBullet {
                        text: s.to_string(),
                        kind: "code".to_string(),
                        cites: vec![],
                        pack_ref: None,
                        status: String::new(),
                    });
                }
            }
        }

        ExplanationSection {
            title,
            one_sentence,
            bullets,
            detail,
        }
    };

    let citations: Vec<CitationItem> = obj
        .get("citations")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let unknowns: Vec<String> = obj
        .get("unknowns")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let concepts: Vec<String> = obj
        .get("concepts")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    Ok(StructuredExplanation {
        overall: parse_sec("overall", "整体架构与职责"),
        logic: parse_sec("logic", "控制流程与边界"),
        syntax: parse_sec("syntax", "语言特性与写法"),
        principle: parse_sec("principle", "底层原理与权衡"),
        citations,
        unknowns,
        concepts,
    })
}

pub fn extract_balanced_json(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() && trimmed.starts_with('{') {
        return Some(trimmed.to_string());
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let mut start_idx = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (i, &ch) in chars.iter().enumerate() {
        if start_idx.is_none() {
            if ch == '{' {
                start_idx = Some(i);
                depth = 1;
            }
            continue;
        }
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && in_string {
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let s = start_idx.unwrap();
                    let candidate: String = chars[s..=i].iter().collect();
                    if serde_json::from_str::<serde_json::Value>(&candidate).is_ok() {
                        return Some(candidate);
                    }
                    start_idx = None;
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_citation_verifier_25_cases() {
        let temp_dir = std::env::temp_dir().join(format!(
            "noesis_verifier_test_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        fs::create_dir_all(temp_dir.join("src")).unwrap();

        let mut sample_lines = Vec::new();
        sample_lines.push("use rclcpp::Node;".to_string()); // L1
        sample_lines.push("class VoiceNode : public rclcpp::Node {".to_string()); // L2
        sample_lines.push("public:".to_string()); // L3
        sample_lines.push("  void init_pub() {".to_string()); // L4
        sample_lines.push("    pub_ = create_publisher<AudioFrame>(\"/audio/out\", 10);".to_string()); // L5
        sample_lines.push("  }".to_string()); // L6
        sample_lines.push("  void duplicate_line() { int x = 1; }".to_string()); // L7
        sample_lines.push("  void duplicate_line_2() { int x = 1; }".to_string()); // L8
        for i in 9..=250 {
            sample_lines.push(format!("  // padding line {}", i));
        }
        sample_lines.push("  void tail_unique_fn() { return; }".to_string()); // L251

        fs::write(temp_dir.join("src/node.cpp"), sample_lines.join("\n")).unwrap();
        fs::write(temp_dir.join(".env"), "SECRET_KEY=123").unwrap();

        struct Case {
            name: &'static str,
            file: &'static str,
            line_start: u32,
            line_end: u32,
            quote: &'static str,
            expected_status: &'static str,
        }

        let cases = vec![
            Case { name: "1_exact_single_line", file: "src/node.cpp", line_start: 5, line_end: 5, quote: "create_publisher<AudioFrame>(\"/audio/out\", 10)", expected_status: "verified" },
            Case { name: "2_exact_multi_line", file: "src/node.cpp", line_start: 4, line_end: 6, quote: "void init_pub() {\n    pub_ = create_publisher<AudioFrame>(\"/audio/out\", 10);\n  }", expected_status: "verified" },
            Case { name: "3_whitespace_difference", file: "src/node.cpp", line_start: 5, line_end: 5, quote: "pub_ =   create_publisher<AudioFrame>( \"/audio/out\", 10 );", expected_status: "verified" },
            Case { name: "4_off_by_one_plus_1", file: "src/node.cpp", line_start: 6, line_end: 6, quote: "create_publisher<AudioFrame>", expected_status: "corrected" },
            Case { name: "5_off_by_one_minus_1", file: "src/node.cpp", line_start: 4, line_end: 4, quote: "create_publisher<AudioFrame>", expected_status: "corrected" },
            Case { name: "6_off_by_two_lines", file: "src/node.cpp", line_start: 3, line_end: 3, quote: "create_publisher<AudioFrame>", expected_status: "corrected" },
            Case { name: "7_wrong_line_unique_in_file", file: "src/node.cpp", line_start: 50, line_end: 52, quote: "class VoiceNode : public rclcpp::Node", expected_status: "corrected" },
            Case { name: "8_wrong_line_tail_unique", file: "src/node.cpp", line_start: 10, line_end: 10, quote: "void tail_unique_fn()", expected_status: "corrected" },
            Case { name: "9_out_of_bounds_line_but_unique_quote", file: "src/node.cpp", line_start: 999, line_end: 1000, quote: "use rclcpp::Node;", expected_status: "corrected" },
            Case { name: "10_non_existent_quote", file: "src/node.cpp", line_start: 1, line_end: 5, quote: "non_existent_function_call()", expected_status: "failed" },
            Case { name: "11_ambiguous_quote_wrong_line", file: "src/node.cpp", line_start: 20, line_end: 20, quote: "int x = 1;", expected_status: "failed" },
            Case { name: "12_ambiguous_quote_exact_line_7", file: "src/node.cpp", line_start: 7, line_end: 7, quote: "int x = 1;", expected_status: "verified" },
            Case { name: "13_path_traversal_parent", file: "../outside.cpp", line_start: 1, line_end: 1, quote: "use", expected_status: "failed" },
            Case { name: "14_path_traversal_nested", file: "src/../../outside.cpp", line_start: 1, line_end: 1, quote: "use", expected_status: "failed" },
            Case { name: "15_sensitive_dotenv", file: ".env", line_start: 1, line_end: 1, quote: "SECRET_KEY", expected_status: "failed" },
            Case { name: "16_sensitive_pem", file: "src/cert.pem", line_start: 1, line_end: 1, quote: "BEGIN", expected_status: "failed" },
            Case { name: "17_range_exceeds_200_lines", file: "src/node.cpp", line_start: 1, line_end: 210, quote: "use rclcpp::Node;", expected_status: "failed" },
            Case { name: "18_inverted_line_range_unique_fallback", file: "src/node.cpp", line_start: 10, line_end: 5, quote: "void tail_unique_fn()", expected_status: "corrected" },
            Case { name: "19_inverted_line_range_non_existent", file: "src/node.cpp", line_start: 10, line_end: 5, quote: "missing_symbol", expected_status: "failed" },
            Case { name: "20_empty_quote", file: "src/node.cpp", line_start: 1, line_end: 1, quote: "   ", expected_status: "failed" },
            Case { name: "21_empty_file_path", file: "", line_start: 1, line_end: 1, quote: "use", expected_status: "failed" },
            Case { name: "22_non_existent_file", file: "src/ghost.cpp", line_start: 1, line_end: 1, quote: "use", expected_status: "failed" },
            Case { name: "23_llm_spoofed_verified_status_on_bad_quote", file: "src/node.cpp", line_start: 1, line_end: 2, quote: "hallucinated_api()", expected_status: "failed" },
            Case { name: "24_backslash_windows_relative_path", file: r"src\node.cpp", line_start: 2, line_end: 2, quote: "class VoiceNode", expected_status: "verified" },
            Case { name: "25_exact_line_1", file: "src/node.cpp", line_start: 1, line_end: 1, quote: "use rclcpp::Node;", expected_status: "verified" },
        ];

        for c in &cases {
            let mut exp = StructuredExplanation {
                citations: vec![CitationItem {
                    id: "c1".to_string(),
                    file: c.file.to_string(),
                    line_start: c.line_start,
                    line_end: c.line_end,
                    quote: c.quote.to_string(),
                    status: "verified".to_string(), // Attempt spoof; verifier must overwrite!
                    ..Default::default()
                }],
                overall: ExplanationSection {
                    bullets: vec![ExplanationBullet {
                        text: "Bullet text".to_string(),
                        kind: "code".to_string(),
                        cites: vec!["c1".to_string()],
                        pack_ref: None,
                        status: "verified".to_string(),
                    }],
                    ..Default::default()
                },
                ..Default::default()
            };

            let summary = CitationVerifier::verify(&temp_dir, &["ros2.qos.policies".to_string()], &mut exp);
            assert_eq!(
                exp.citations[0].status, c.expected_status,
                "Case '{}' failed: got status '{}', reason: {:?}",
                c.name, exp.citations[0].status, exp.citations[0].reason
            );
            if c.expected_status == "failed" {
                assert_eq!(exp.overall.bullets[0].status, "unverified");
                assert_eq!(summary.verification_rate, 0.0);
            } else {
                assert_eq!(exp.overall.bullets[0].status, c.expected_status);
                assert_eq!(summary.verification_rate, 1.0);
            }
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
