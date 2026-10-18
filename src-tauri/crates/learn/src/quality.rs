use crate::verifier::StructuredExplanation;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualityReport {
    pub score: f32,
    pub issues: Vec<String>,
    pub is_shallow: bool,
    pub total_bullets: usize,
    pub total_detail_chars: usize,
    pub matched_identifiers_count: usize,
    pub generic_phrases_count: usize,
}

pub struct QualityGate;

impl QualityGate {
    /// Loads generic phrases from project or crate manifest, or falls back to builtins
    pub fn load_generic_phrases(project_root: &Path) -> Vec<String> {
        let candidates = [
            project_root.join("prompts/generic_phrases.txt"),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../prompts/generic_phrases.txt"),
        ];

        for c in &candidates {
            if let Ok(content) = fs::read_to_string(c) {
                let phrases: Vec<String> = content
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .collect();
                if !phrases.is_empty() {
                    return phrases;
                }
            }
        }

        vec![
            "实现了相关功能".to_string(),
            "实现了对应的功能".to_string(),
            "用于处理相关业务".to_string(),
            "完成相应的操作".to_string(),
            "起到关键作用".to_string(),
            "提高代码复用性".to_string(),
            "主要功能是".to_string(),
            "等等".to_string(),
            "相关逻辑".to_string(),
            "相关代码".to_string(),
            "一些变量".to_string(),
            "对数据进行处理".to_string(),
            "调用了相关的函数".to_string(),
        ]
    }

    /// Evaluates the depth and concreteness of a structured explanation
    pub fn evaluate(
        explanation: &StructuredExplanation,
        selected_code: &str,
        depth_mode: Option<&str>,
        generic_phrases: &[String],
    ) -> QualityReport {
        let is_deep = depth_mode.map(|d| d == "deep").unwrap_or(false);
        let mut issues = Vec::new();
        let mut score: f32 = 1.0;

        let sections = [
            ("整体", &explanation.overall),
            ("逻辑", &explanation.logic),
            ("语法", &explanation.syntax),
            ("原理", &explanation.principle),
        ];

        let mut total_bullets = 0;
        let mut total_detail_chars = 0;

        let min_bullets = if is_deep { 4 } else { 3 };
        let min_detail_chars = if is_deep { 120 } else { 60 };

        for (name, sec) in &sections {
            let b_len = sec.bullets.len();
            total_bullets += b_len;
            if b_len < min_bullets {
                issues.push(format!("【{}】要点数量偏少（当前 {} 条，建议至少 {} 条）", name, b_len, min_bullets));
                score -= 0.12;
            }

            let d_len = sec.detail.chars().count();
            total_detail_chars += d_len;
            if d_len < min_detail_chars {
                issues.push(format!("【{}】详述深度不足（当前 {} 字，建议至少 {} 字）", name, d_len, min_detail_chars));
                score -= 0.12;
            }
        }

        // Aggregate full text for phrase and identifier checks
        let mut full_text = String::new();
        for (_, sec) in &sections {
            full_text.push_str(&sec.one_sentence);
            full_text.push(' ');
            for b in &sec.bullets {
                full_text.push_str(&b.text);
                full_text.push(' ');
            }
            full_text.push_str(&sec.detail);
            full_text.push(' ');
        }

        // Check for forbidden generic phrases
        let mut generic_count = 0;
        for phrase in generic_phrases {
            if !phrase.is_empty() && full_text.contains(phrase) {
                generic_count += 1;
                issues.push(format!("命中泛化套话短语：'{}'", phrase));
                score -= 0.10;
            }
        }

        // Check for concrete identifiers from selected_code
        let code_identifiers = Self::extract_code_identifiers(selected_code);
        let mut matched_identifiers = 0;
        for ident in &code_identifiers {
            if full_text.contains(ident) {
                matched_identifiers += 1;
            }
        }

        if !code_identifiers.is_empty() && matched_identifiers == 0 {
            issues.push("未包含任何来自选区代码的具体标识符（变量名、函数名或类型名），内容过于泛化".to_string());
            score -= 0.25;
        }

        let clamped_score = score.clamp(0.0, 1.0);
        let is_shallow = clamped_score < 0.65 || total_bullets < 8;

        QualityReport {
            score: clamped_score,
            issues,
            is_shallow,
            total_bullets,
            total_detail_chars,
            matched_identifiers_count: matched_identifiers,
            generic_phrases_count: generic_count,
        }
    }

    fn extract_code_identifiers(code: &str) -> HashSet<String> {
        let stop_words: HashSet<&'static str> = [
            "let", "mut", "fn", "pub", "struct", "impl", "enum", "trait", "type", "const", "static",
            "if", "else", "match", "for", "while", "loop", "in", "return", "break", "continue",
            "true", "false", "Some", "None", "Ok", "Err", "self", "Self", "use", "mod", "as",
            "async", "await", "std", "vec", "Vec", "String", "str", "Option", "Result", "Box",
            "auto", "void", "int", "char", "float", "double", "bool", "class", "public", "private",
            "protected", "virtual", "override", "const", "nullptr", "NULL", "include",
            "def", "import", "from", "class", "pass", "try", "except", "finally", "with",
            "None", "True", "False", "lambda", "yield", "print", "self",
        ]
        .iter()
        .copied()
        .collect();

        let mut idents = HashSet::new();
        let mut current = String::new();

        for ch in code.chars() {
            if ch.is_alphanumeric() || ch == '_' {
                current.push(ch);
            } else {
                if current.len() >= 3 && !stop_words.contains(current.as_str()) {
                    let first = current.chars().next().unwrap();
                    if !first.is_numeric() {
                        idents.insert(current.clone());
                    }
                }
                current.clear();
            }
        }
        if current.len() >= 3 && !stop_words.contains(current.as_str()) {
            let first = current.chars().next().unwrap();
            if !first.is_numeric() {
                idents.insert(current);
            }
        }

        idents
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verifier::{ExplanationBullet, ExplanationSection};

    fn b(text: &str, kind: &str) -> ExplanationBullet {
        ExplanationBullet {
            text: text.to_string(),
            kind: kind.to_string(),
            cites: vec![],
            pack_ref: None,
            status: String::new(),
        }
    }

    #[test]
    fn test_quality_gate_passes_concrete_explanation() {
        let explanation = StructuredExplanation {
            overall: ExplanationSection {
                title: "整体".to_string(),
                one_sentence: "ROS2 雷达节点订阅者回调函数处理点云。".to_string(),
                bullets: vec![
                    b("订阅 scan 话题的 LaserScan 消息", "code"),
                    b("将点云投影到 costmap_ 局部代价地图中", "code"),
                    b("由 ROS2 SingleThreadedExecutor 调度执行", "knowledge"),
                    b("属于机器人环境感知模块的关键链条", "code"),
                ],
                detail: "### 整体设计\n当激光雷达发布 `/scan` 话题时，节点通过 DDS 传输层接收点云并写入 costmap_。该模块衔接传感器驱动与导航路径规划系统，具有极强的软实时处理要求。".to_string(),
            },
            logic: ExplanationSection {
                title: "逻辑".to_string(),
                one_sentence: "三步完成过滤与栅格投影。".to_string(),
                bullets: vec![
                    b("过滤 ranges 数组中的无效极值", "code"),
                    b("调用 tf_buffer_ 转换到 odom 坐标系", "code"),
                    b("若抛出 TransformException 则安全跳过", "code"),
                ],
                detail: "### 步进逻辑\n1. 极值过滤：遍历 ranges 数组滤除 inf/nan。\n2. TF 转换：查询雷达到里程计位姿并变换。\n3. 栅格打点：计算坐标并更新代价地图。".to_string(),
            },
            syntax: ExplanationSection {
                title: "语法".to_string(),
                one_sentence: "使用 std::bind 绑定成员函数回调。".to_string(),
                bullets: vec![
                    b("std::bind(&Node::scanCallback, this, _1) 绑定非静态成员", "code"),
                    b("std::placeholders::_1 匹配入参", "code"),
                    b("推荐改用 Lambda 表达式获取更好的内联优化", "code"),
                ],
                detail: "std::bind 在现代 C++ 中可以被捕获 Lambda 完美取代，使用 Lambda 能够避免额外的类型擦除开销并让编译器充分进行内联优化。".to_string(),
            },
            principle: ExplanationSection {
                title: "原理".to_string(),
                one_sentence: "ROS2 节点执行器回调队列与 Best Effort QoS 机制。".to_string(),
                bullets: vec![
                    b("采用 SensorDataQoS 配置，允许网络丢包但保障低延迟", "knowledge"),
                    b("耗时操作应分配至 MutuallyExclusiveCallbackGroup", "knowledge"),
                    b("Loaned Message 机制可实现进程内零拷贝传输", "knowledge"),
                ],
                detail: "在高频传感器场景下，Best Effort 策略永远保证获取最新的点云帧，即使遇到网络拥塞丢包，后续新帧也会立刻覆盖旧数据，避免重传阻塞。".to_string(),
            },
            ..Default::default()
        };

        let code = "sub_ = create_subscription<LaserScan>(\"/scan\", 10, std::bind(&Node::scanCallback, this, _1)); costmap_->setCost(x, y, 254);";
        let generic_phrases = vec!["实现了相关功能".to_string(), "等等".to_string()];

        let report = QualityGate::evaluate(&explanation, code, Some("standard"), &generic_phrases);
        assert!(!report.is_shallow, "Concrete explanation must pass quality gate");
        assert!(report.score >= 0.7);
        assert_eq!(report.generic_phrases_count, 0);
    }

    #[test]
    fn test_quality_gate_rejects_generic_shallow_explanation() {
        let shallow_explanation = StructuredExplanation {
            overall: ExplanationSection {
                title: "整体".to_string(),
                one_sentence: "这段代码实现了相关功能。".to_string(),
                bullets: vec![
                    b("用于处理相关业务逻辑。", "code"),
                ],
                detail: "".to_string(),
            },
            ..Default::default()
        };

        let code = "auto subscriber = node->create_subscription<sensor_msgs::msg::LaserScan>(\"/scan\", 10, cb);";
        let generic_phrases = vec!["实现了相关功能".to_string(), "用于处理相关业务".to_string()];

        let report = QualityGate::evaluate(&shallow_explanation, code, Some("standard"), &generic_phrases);
        assert!(report.is_shallow, "Shallow generic explanation must be rejected");
        assert!(report.score < 0.65);
        assert!(report.issues.len() >= 2);
    }
}
