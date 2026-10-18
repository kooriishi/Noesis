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
  is_template?: boolean;
  is_expanded?: boolean;
}

export interface RewordingResult {
  mark_id?: string;
  original_category: string;
  analogy: string;
  minimal_example: string;
  prerequisite_concept: string;
  guidance: string;
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
  explanation?: string;
  ai_explanation?: string;
  model?: string;
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
}

export interface RunResponse {
  stdout: string;
  stderr: string;
  exit_code: number;
  duration_ms: number;
  success: boolean;
  truncated?: boolean;
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
}
