pub const EXPERT_SYSTEM_PROMPT: &str = include_str!("../../../../prompts/code_guide_expert.system.md");

/// 安全与引用规则（数据隔离 / 引用规则 / 领域事实优先），四层解读、看不懂答疑、智能问答共用。
/// 与输出格式无关，可以安全地拼接到任何提示词前面。
pub const SAFETY_RULES_PROMPT: &str = include_str!("../../../../prompts/rules.safety.md");

/// 旧版「单次返回四视角 JSON」的格式规则，**只允许**给 `explain_selection_structured` 旧流程使用。
pub const JSON_V2_RULES_PROMPT: &str = include_str!("../../../../prompts/rules.json_v2.md");

/// 兼容别名：历史代码里把 LOCKED_RULES_PROMPT 当作「安全 + JSON 格式」的合体。
/// 现在它指向旧版 JSON 规则文件，保证旧流程行为不变。
pub const LOCKED_RULES_PROMPT: &str = JSON_V2_RULES_PROMPT;

pub const DEFAULT_PERSONA_PROMPT: &str = include_str!("../../../../prompts/persona.default.md");
pub const ANNOTATE_LINES_PROMPT: &str = include_str!("../../../../prompts/annotate_lines.md");

pub const PROMPT_OVERALL: &str = include_str!("../../../../prompts/layers/overall.md");
pub const PROMPT_LOGIC: &str = include_str!("../../../../prompts/layers/logic.md");
pub const PROMPT_SYNTAX: &str = include_str!("../../../../prompts/layers/syntax.md");
pub const PROMPT_PRINCIPLE: &str = include_str!("../../../../prompts/layers/principle.md");
pub const PROMPT_CONFUSION: &str = include_str!("../../../../prompts/confusion/reexplain.md");
pub const PROMPT_AGENT_ASK: &str = include_str!("../../../../prompts/agent/ask.md");
