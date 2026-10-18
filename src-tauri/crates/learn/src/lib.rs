use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LearnError {
    #[error("Verification error: {0}")]
    Verification(String),
    #[error("Store error: {0}")]
    Store(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Perspective {
    Overall,
    Logic,
    Syntax,
    Principle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRef {
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub code_snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplanationCard {
    pub id: String,
    pub perspective: Perspective,
    pub title: String,
    pub one_sentence: String,
    pub key_bullets: Vec<String>,
    pub detailed_body: String,
    pub source: SourceRef,
    /// Indicates whether the selected snippet matches the actual file content at line_start..=line_end (A5)
    pub source_matches: bool,
    /// True if this card is a local template placeholder rather than AI-generated (A4)
    #[serde(default)]
    pub is_template: bool,
    pub is_expanded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewordingResult {
    #[serde(default)]
    pub mark_id: Option<String>,
    pub original_category: String,
    pub analogy: String,
    pub minimal_example: String,
    pub prerequisite_concept: String,
    pub guidance: String,
    #[serde(default)]
    pub is_template: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseItem {
    pub id: String,
    pub unit_title: String,
    pub exercise_type: String, // "predict_output" | "fill_blank" | "fix_bug"
    pub question: String,
    pub snippet: String,
    pub options: Option<Vec<String>>,
    pub reference_answer: String,
    pub explanation: String,
}

pub struct LearnEngine;

impl LearnEngine {
    pub fn new() -> Self {
        Self
    }

    /// Generates structured 4-perspective template cards for a selected snippet
    pub fn generate_structured_cards(
        file_path: &str,
        snippet: &str,
        start_line: u32,
        end_line: u32,
        source_matches: bool,
    ) -> Vec<ExplanationCard> {
        let clean_snippet = snippet.trim();
        let first_line = clean_snippet.lines().next().unwrap_or("代码片段");

        let source = SourceRef {
            file_path: file_path.to_string(),
            line_start: start_line,
            line_end: end_line,
            code_snippet: snippet.to_string(),
        };

        vec![
            ExplanationCard {
                id: format!("card-overall-{}-{}", start_line, end_line),
                perspective: Perspective::Overall,
                title: "整体架构与数据流".to_string(),
                one_sentence: format!("本段代码定义了 `{}` 的核心入口及数据组织契约。", first_line),
                key_bullets: vec![
                    "职责边界：对外暴露稳定调用接口，内聚具体操作逻辑".to_string(),
                    "依赖上下游：接收上层请求上下文，向底层存储/运行时派发状态".to_string(),
                    "数据流向：从不可信输入规范化转换为内部强类型数据结构".to_string(),
                ],
                detailed_body: format!(
                    "### 整体视角深度解析\n\n该逻辑在文件 `{}` 的第 {}-{} 行执行。\n\n\
                    它扮演着模块间的粘合层，既约束了外部输入结构，也保证了下游模块无需关心协议差异。",
                    file_path, start_line, end_line
                ),
                source: source.clone(),
                source_matches,
                is_template: true,
                is_expanded: false,
            },
            ExplanationCard {
                id: format!("card-logic-{}-{}", start_line, end_line),
                perspective: Perspective::Logic,
                title: "控制流程与边界条件".to_string(),
                one_sentence: "通过防御性检查和前置断言防范空值、越界及并发冲突。".to_string(),
                key_bullets: vec![
                    "分支控制：优先处理失败或快速返回路径，减少代码嵌套深度".to_string(),
                    "状态跃迁：状态机在成功校验后原子更新，保证幂等性".to_string(),
                    "边界处理：考虑了空输入与网络异常回滚机制".to_string(),
                ],
                detailed_body: "### 逻辑控制流详述\n\n\
                    代码采用典型的防御式编程模式。在进入真正业务计算前，先完成了所有的参数校验，\
                    从而将可能存在的异常情况在最早期拦截。"
                    .to_string(),
                source: source.clone(),
                source_matches,
                is_template: true,
                is_expanded: false,
            },
            ExplanationCard {
                id: format!("card-syntax-{}-{}", start_line, end_line),
                perspective: Perspective::Syntax,
                title: "语言特性与惯用写法".to_string(),
                one_sentence: "运用强类型系统、模式匹配或高阶函数提高可读性与类型安全性。".to_string(),
                key_bullets: vec![
                    "惯用模式：充分利用了语言特有的零成本抽象".to_string(),
                    "所有权/作用域：资源在生命周期结束时自动析构回收".to_string(),
                    "错误处理机制：以 Result/Option 或精准错误枚举取代全局断言".to_string(),
                ],
                detailed_body: "### 语法特性剖析\n\n\
                    此处的语法结构体现了语言最佳实践，避免了冗长的防御逻辑，代码表意直接，具备编译期可靠性保障。"
                    .to_string(),
                source: source.clone(),
                source_matches,
                is_template: true,
                is_expanded: false,
            },
            ExplanationCard {
                id: format!("card-principle-{}-{}", start_line, end_line),
                perspective: Perspective::Principle,
                title: "底层原理与算法协议".to_string(),
                one_sentence: "建立在确定性状态机、操作系统调度与轻量级通信契约之上。".to_string(),
                key_bullets: vec![
                    "核心算法：时间复杂度控制在 O(1) ~ O(n)，空间占用高度局部化".to_string(),
                    "系统机制：借助非阻塞 I/O 或事件循环降低上下文切换开销".to_string(),
                    "设计哲学：关注点分离（SoC）与显式优于隐式".to_string(),
                ],
                detailed_body: "### 原理溯源\n\n\
                    本模块设计背后的核心驱动力是降低分布式/异步环境下的耦合度，通过单向数据流减少死锁与竞态风险。"
                    .to_string(),
                source,
                source_matches,
                is_template: true,
                is_expanded: false,
            },
        ]
    }

    /// Programmatically verifies that the source code snippet actually exists in the file within the given line range
    pub fn verify_source_reference(
        file_content: &str,
        start_line: u32,
        end_line: u32,
        expected_snippet: &str,
    ) -> bool {
        let lines: Vec<&str> = file_content.lines().collect();
        let total = lines.len() as u32;
        if start_line == 0 || start_line > total || end_line < start_line || end_line > total {
            return false;
        }

        let s_idx = (start_line - 1) as usize;
        let e_idx = end_line as usize;
        let actual_slice = lines[s_idx..e_idx].join("\n");

        let normalized_actual: String =
            actual_slice.chars().filter(|c| !c.is_whitespace()).collect();
        let normalized_expected: String = expected_snippet
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();

        if normalized_expected.is_empty() {
            return false;
        }

        normalized_actual.contains(&normalized_expected)
            || normalized_expected.contains(&normalized_actual)
    }

    /// Generates template rewording when user marks a snippet as confusing
    pub fn handle_confusion_mark(category: &str, _snippet: &str) -> RewordingResult {
        match category {
            "overall" => RewordingResult {
                mark_id: None,
                original_category: "overall".to_string(),
                analogy: "把它想象成物流中转站的分拣机：不管包裹从哪个货车卸下来，分拣机先扫描条形码，然后把它丢给对应的出库传送带。".to_string(),
                minimal_example: "function dispatch(packet) {\n  const route = getRoute(packet.tag);\n  return route.send(packet.data);\n}".to_string(),
                prerequisite_concept: "软件工程中的关注点分离（Separation of Concerns）与中介者模式。".to_string(),
                guidance: "先不要纠结具体字段，先盯紧：从哪里进来（参数），流向了哪个模块（返回值或下游方法调用）。".to_string(),
                is_template: true,
            },
            "logic" => RewordingResult {
                mark_id: None,
                original_category: "logic".to_string(),
                analogy: "就像去银行办业务的叫号排队逻辑：先看你有没有带身份证（校验），再看账户余额够不够（状态），只要有一步不对就直接叫你补材料，绝不让你白坐半小时。".to_string(),
                minimal_example: "if (!user.isValid()) return Err(\"Invalid\");\nif (balance < amount) return Err(\"No funds\");\nexecuteTransfer();".to_string(),
                prerequisite_concept: "卫语句（Guard Clause）与尽早返回（Early Return）模式。".to_string(),
                guidance: "重点看每一处 `if` 的判断条件：它排除了哪一种极端情况？如果跳过了所有 `if`，主线流程在做什么？".to_string(),
                is_template: true,
            },
            "syntax" => RewordingResult {
                mark_id: None,
                original_category: "syntax".to_string(),
                analogy: "这就像是成语或者行业行话：看似短短几个字符，但包含了约定俗成的展开含义。".to_string(),
                minimal_example: "// 相当于:\n// let val = option.unwrap_or_default();\nconst val = option ?? defaultValue;".to_string(),
                prerequisite_concept: "现代编程语言语法糖、可选链与空值合并运算符。".to_string(),
                guidance: "如果把这些符号替换为最原始的 if-else，代码功能完全一样，写法变简短只是为了减少模板代码。".to_string(),
                is_template: true,
            },
            _ => RewordingResult {
                mark_id: None,
                original_category: "principle".to_string(),
                analogy: "好比计算机的内存分页：它并不需要一次性把整本书放进脑子里，而是看一页翻一页，看完不需要的就先挪出去。".to_string(),
                minimal_example: "class VirtualMemoryCache {\n  get(page) { return this.pages[page] ?? this.swapIn(page); }\n}".to_string(),
                prerequisite_concept: "按需懒加载、时间局部性与空间局部性。".to_string(),
                guidance: "不要看表面的语法，先看设计者最想规避的物理代价是什么（是内存爆了，还是网络卡了？）。".to_string(),
                is_template: true,
            },
        }
    }

    /// Spaced repetition calculator (SM-2 variant)
    pub fn compute_next_review(
        quality_score: u8,
        current_interval: i32,
        repetition: i32,
    ) -> (String, i32, i32) {
        let (next_interval, next_rep) = if quality_score >= 3 {
            let next_r = repetition + 1;
            let next_i = match next_r {
                1 => 1,
                2 => 3,
                3 => 7,
                _ => (current_interval as f64 * 2.2).round() as i32,
            };
            (next_i, next_r)
        } else {
            (1, 0)
        };

        let next_date = Utc::now() + Duration::days(next_interval as i64);
        (next_date.to_rfc3339(), next_interval, next_rep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_structured_cards() {
        let cards = LearnEngine::generate_structured_cards(
            "src/main.rs",
            "pub fn run() -> Result<()> {}",
            10,
            15,
            true,
        );
        assert_eq!(cards.len(), 4);
        assert_eq!(cards[0].perspective, Perspective::Overall);
        assert_eq!(cards[1].perspective, Perspective::Logic);
        assert_eq!(cards[2].perspective, Perspective::Syntax);
        assert_eq!(cards[3].perspective, Perspective::Principle);
        assert!(cards[0].source_matches);
        assert!(cards[0].is_template);
    }

    #[test]
    fn test_verify_source_reference() {
        let code = "line 1\nline 2\nline 3\nline 4\nline 5";
        assert!(LearnEngine::verify_source_reference(code, 1, 3, "line 2"));
        assert!(LearnEngine::verify_source_reference(
            code,
            2,
            4,
            "line 2\nline 3"
        ));
        assert!(!LearnEngine::verify_source_reference(code, 1, 2, "line 5"));
        assert!(!LearnEngine::verify_source_reference(code, 0, 3, "line 1"));
        assert!(!LearnEngine::verify_source_reference(code, 2, 10, "line 2"));
    }

    #[test]
    fn test_handle_confusion_mark() {
        let logic_res = LearnEngine::handle_confusion_mark("logic", "if (!ok) return;");
        assert_eq!(logic_res.original_category, "logic");
        assert!(!logic_res.analogy.is_empty());
        assert!(!logic_res.minimal_example.is_empty());
        assert!(logic_res.is_template);

        let syntax_res = LearnEngine::handle_confusion_mark("syntax", "let x = opt?;");
        assert_eq!(syntax_res.original_category, "syntax");
    }

    #[test]
    fn test_sm2_spaced_repetition() {
        let (_, next_i, next_r) = LearnEngine::compute_next_review(4, 1, 0);
        assert_eq!(next_r, 1);
        assert_eq!(next_i, 1);

        let (_, next_i2, next_r2) = LearnEngine::compute_next_review(5, 1, 1);
        assert_eq!(next_r2, 2);
        assert_eq!(next_i2, 3);

        let (_, fail_i, fail_r) = LearnEngine::compute_next_review(1, 7, 3);
        assert_eq!(fail_r, 0);
        assert_eq!(fail_i, 1);
    }
}
