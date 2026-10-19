use noesis_learn::context::{ContextManifest, DroppedItem};
use noesis_learn::exercises::Sm2ReviewState;
use noesis_learn::quality::QualityReport;
use noesis_learn::verifier::{CitationItem, StructuredExplanation, VerificationSummary};

use noesis_llm::{ChatMessage, OutboundPreview};
use noesis_runner::CompilerInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub db_degraded: bool,
    pub db_error: Option<String>,
    #[serde(default)]
    pub compiler_info: Option<CompilerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredExplainResponse {
    pub explanation: Option<StructuredExplanation>,
    pub verification: VerificationSummary,
    pub manifest: ContextManifest,
    pub dropped: Vec<DroppedItem>,
    pub cached: bool,
    pub raw_unverified_output: Option<String>,
    #[serde(default)]
    pub quality: Option<QualityReport>,
    #[serde(default)]
    pub explanation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AskStreamEvent {
    #[serde(rename = "type")]
    pub event_type: String, // "context_ready" | "token" | "tool_call" | "tool_result" | "citation_check" | "done" | "error"
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_args: Option<String>,
    #[serde(default)]
    pub tool_result: Option<String>,
    #[serde(default)]
    pub citations: Option<Vec<CitationItem>>,
    #[serde(default)]
    pub manifest: Option<ContextManifest>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskPreviewResponse {
    pub messages: Vec<ChatMessage>,
    pub token_estimate: usize,
    pub manifest: ContextManifest,
    pub outbound_preview: OutboundPreview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectivePromptPreview {
    pub scope: String,
    pub preset_name: String,
    pub l1_persona: String,
    pub l0_locked: String,
    pub full_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitExerciseResult {
    pub is_correct: bool,
    pub quality: u8,
    pub review: Sm2ReviewState,
    pub reference_answer: String,
    pub verified_by_program: bool,
    #[serde(default)]
    pub exercise_id: Option<String>,
    #[serde(default)]
    pub feedback: Option<String>,
    #[serde(default)]
    pub next_interval_days: Option<i32>,
    #[serde(default)]
    pub next_ease_factor: Option<f64>,
    #[serde(default)]
    pub next_review_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotatedLine {
    pub line_no: u32,
    pub code: String,
    pub comment: String,
    pub kind: String, // "trivial" | "key" | "hard"
    #[serde(default)]
    pub status: Option<String>, // "verified" | "corrected" | "missing"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnnotatedSnippet {
    pub lines: Vec<AnnotatedLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfusionDetailResponse {
    pub mark: noesis_store::ConfusionMark,
    pub revisions: Vec<noesis_store::ConfusionRevision>,
    pub active_annotated: Option<AnnotatedSnippet>,
}

