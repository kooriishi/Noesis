export interface FileNode {
  name: string;
  path: string;
  absolute_path: string;
  is_dir: boolean;
  children?: FileNode[];
  size: number;
  language?: string;
}

export interface FileContentPayload {
  path: string;
  content: string;
  hash: string;
  language: string;
  size: number;
}

export interface TabItem {
  path: string;
  name: string;
  content: string;
  language: string;
  isDirty: boolean;
}

export interface AppSettings {
  editorFontSize: number;
  uiFontSize: number;
  fontFamily: string;
  wordWrap: boolean;
  tabSize: number;
  theme: string;
}

export interface SymbolItem {
  id: string;
  name: string;
  kind: 'function' | 'class' | 'struct' | 'interface' | 'type' | 'enum';
  file_path: string;
  line: number;
  signature?: string;
}

export interface ModuleNode {
  id: string;
  name: string;
  path: string;
  kind?: string;
  is_entry: boolean;
  symbols_count: number;
  dependencies: string[];
}

export type ReadingStepStatus = 'unread' | 'reading' | 'done' | 'skipped';

export interface ReadingStep {
  id?: string;
  step: number;
  stage?: string;
  file_path: string;
  symbol_id?: string;
  line_start?: number;
  line_end?: number;
  title: string;
  rationale: string;
  symbol_count: number;
  priority: number;
  status?: ReadingStepStatus | string;
  completed?: boolean;
}

export interface GraphEntity {
  id: string;
  project_id: string;
  domain: string;
  kind: string;
  name: string;
  file_id: string;
  file_path: string;
  symbol_id?: string;
  line_start: number;
  line_end: number;
  metadata_json: string;
}

export interface GraphEdge {
  id: string;
  project_id: string;
  src_type: string;
  src_id: string;
  dst_type: string;
  dst_id: string;
  kind: string;
  file_id: string;
  file_path: string;
  line: number;
  confidence: string;
  evidence: string;
}

export interface GraphSymbol {
  id: string;
  project_id: string;
  file_id: string;
  file_path: string;
  name: string;
  qualified_name: string;
  kind: string;
  line_start: number;
  line_end: number;
  col_start: number;
  col_end: number;
  signature: string;
  docstring?: string;
  parent_id?: string;
  metadata_json: string;
}

export interface SymbolNeighborhood {
  symbol: GraphSymbol;
  parent?: GraphSymbol;
  children: GraphSymbol[];
  incoming_edges: GraphEdge[];
  outgoing_edges: GraphEdge[];
  callers: GraphSymbol[];
  callees: GraphSymbol[];
  related_entities: GraphEntity[];
}

export interface ProjectMap {
  root: string;
  detected_domains?: string[];
  modules: ModuleNode[];
  entry_points: string[];
  reading_route: ReadingStep[];
  total_symbols: number;
  indexed_files_count: number;
  skipped_files_count?: number;
  entities?: GraphEntity[];
  edges?: GraphEdge[];
}

export type PerspectiveType = 'overall' | 'logic' | 'syntax' | 'principle';

export type CitationVerificationStatus = 'verified' | 'corrected' | 'unverified' | 'knowledge';

export interface CitationItem {
  kind: 'code' | 'knowledge';
  file?: string;
  line_start?: number;
  line_end?: number;
  symbol?: string;
  quote?: string;
  pack_ref?: string;
  verification_status: CitationVerificationStatus;
  verified: boolean;
  corrected_line_start?: number;
  corrected_line_end?: number;
  note?: string;
}

export interface ContextLayerManifestItem {
  layer: string;
  label: string;
  included: boolean;
  tokens: number;
  trimmed: boolean;
  summary: string;
}

export interface ContextManifest {
  basis_hash: string;
  total_tokens: number;
  budget_tokens: number;
  layers: ContextLayerManifestItem[];
  injected_knowledge_ids: string[];
}

export interface SourceRef {
  file_path: string;
  line_start: number;
  line_end: number;
  code_snippet: string;
}

export interface ExplanationCard {
  id: string;
  perspective: PerspectiveType;
  title: string;
  one_sentence: string;
  key_bullets: string[];
  detailed_body: string;
  source: SourceRef;
  source_matches: boolean;
  citations?: CitationItem[];
  basis_hash?: string;
  model?: string;
  prompt_version?: string;
  is_template?: boolean;
  is_expanded?: boolean;
}

export interface StructuredExplanationResponse {
  cards: ExplanationCard[];
  manifest: ContextManifest;
  from_cache: boolean;
  is_template: boolean;
  warning?: string;
  quality?: QualityReport;
  explanation_id?: string;
  depth_mode?: 'standard' | 'deep' | string;
}

export interface RewordingResult {
  mark_id?: string;
  original_category: string;
  reexplain_level?: number;
  analogy: string;
  minimal_example: string;
  prerequisite_concept: string;
  guidance: string;
  self_check_question?: string;
  self_check_answer?: string;
  citations?: CitationItem[];
  similarity_warning?: string;
  is_template?: boolean;
}

export interface ConfusionMark {
  id: string;
  project_id?: string;
  file_path: string;
  line_start: number;
  line_end: number;
  code_snippet: string;
  category: string;
  confusion_type?: string;
  status?: string;
  reexplain_level?: number;
  explanation?: string;
  ai_explanation?: string;
  model?: string;
  created_at: string;
  resolved_at?: string;
}

export interface LearningProfileSummary {
  total_marks: number;
  open_marks: number;
  resolved_marks: number;
  counts_by_type: Record<string, number>;
  recent_open_summaries: string[];
}

export interface ConversationSessionRecord {
  id: string;
  project_id: string;
  title: string;
  rolling_summary?: string;
  created_at: string;
  updated_at: string;
}

export interface ConversationMessageRecord {
  id: string;
  conversation_id: string;
  role: string;
  content: string;
  citations_json?: string;
  token_est: number;
  created_at: string;
}

export interface LlmConfig {
  provider: string;
  api_url: string;
  api_key: string;
  model: string;
  temperature: number;
  max_tokens: number;
  has_api_key?: boolean;
  api_key_masked?: string;
  proxy_url?: string;
  custom_ca_cert_path?: string;
  offline_mode?: boolean;
}

export interface ConnectionTestResult {
  ok: boolean;
  status_code?: number;
  latency_ms: number;
  category: string;
  message: string;
}

export interface SystemStatus {
  db_degraded: boolean;
  db_error?: string;
}

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

export interface OutboundPreview {
  target_url: string;
  model: string;
  messages_count: number;
  total_chars: number;
  contains_sanitized_tokens: boolean;
  sanitized_preview: string;
  file_references: string[];
}

export interface RunRequest {
  language: string;
  code: string;
  timeout_seconds?: number;
  stdin?: string;
  cpp_standard?: string;
  extra_args?: string;
}

export interface RunResponse {
  stdout: string;
  stderr: string;
  exit_code: number;
  duration_ms: number;
  success: boolean;
  truncated?: boolean;
  phase?: 'compile' | 'run' | string;
  compile_stdout?: string;
  compile_stderr?: string;
}

export interface ExerciseRecord {
  id: string;
  unit_id: string;
  exercise_type: string;
  question: string;
  snippet: string;
  options_json?: string;
  reference_answer: string;
  file_path: string;
  next_review_at: string;
  repetition: number;
  interval_days: number;
  ease_factor?: number;
}

export interface ExerciseAttemptResult {
  exercise_id: string;
  is_correct: boolean;
  quality: number;
  score: number;
  feedback: string;
  next_interval_days: number;
  next_ease_factor: number;
  next_review_at: string;
}

export interface LspServerStatus {
  language: string;
  binary: string;
  installed: boolean;
  install_hint: string;
}

export interface LspLocation {
  file_path: string;
  line_start: number;
  line_end: number;
  col_start: number;
  col_end: number;
  confidence: string;
  source: string;
}

export interface QualityReport {
  overall_score: number;
  passed: boolean;
  issues: string[];
}

export interface ExplanationSummary {
  id: string;
  project_id: string;
  file_path: string;
  range: string;
  title: string;
  selected_preview: string;
  basis_hash: string;
  model: string;
  depth_mode?: 'standard' | 'deep' | string;
  pinned: boolean;
  created_at: number;
  last_opened_at?: number;
}

export interface PromptPreset {
  id: string;
  name: string;
  scope: 'ask' | 'explain' | 'reexplain' | string;
  persona_text: string;
  created_at: number;
  updated_at: number;
  is_active: boolean;
  is_builtin: boolean;
}

export interface EffectivePromptPreview {
  scope: string;
  preset_name: string;
  l1_persona: string;
  l0_locked: string;
  full_prompt: string;
  token_estimate: number;
}

export interface ConfusionRevision {
  id: string;
  mark_id: string;
  level: number;
  created_at: number;
  model: string;
  explanation_json: string;
  annotated_json?: string;
  similarity_score: number;
}

export interface AnnotatedLine {
  line_no: number;
  code: string;
  comment: string;
  is_hard: boolean;
}

export interface AnnotatedSnippet {
  lines: AnnotatedLine[];
  summary: string;
}

export interface ConfusionDetailResponse {
  mark: ConfusionMark;
  revisions: ConfusionRevision[];
}

export interface AskStreamEvent {
  type: 'context_ready' | 'token' | 'tool_call' | 'tool_result' | 'citation_check' | 'done' | 'error' | string;
  token?: string;
  tool_name?: string;
  tool_args?: string;
  tool_result?: string;
  citations?: CitationItem[];
  manifest?: ContextManifest;
  error?: string;
  conversation_id?: string;
}

export interface AskPreviewResponse {
  messages: ChatMessage[];
  token_estimate: number;
  manifest: ContextManifest;
  outbound_preview: OutboundPreview;
}

export interface FeedbackStats {
  total: number;
  helpful: number;
  too_generic: number;
  wrong: number;
  too_generic_rate: number;
  wrong_rate: number;
}

