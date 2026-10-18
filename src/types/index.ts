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
  is_entry: boolean;
  symbols_count: number;
  dependencies: string[];
}

export interface ReadingStep {
  step: number;
  file_path: string;
  title: string;
  rationale: string;
  symbol_count: number;
  priority: number;
  completed?: boolean;
}

export interface ProjectMap {
  root: string;
  modules: ModuleNode[];
  entry_points: string[];
  reading_route: ReadingStep[];
  total_symbols: number;
  indexed_files_count: number;
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
  verified: boolean;
  is_expanded?: boolean;
}

export interface RewordingResult {
  original_category: string;
  analogy: string;
  minimal_example: string;
  prerequisite_concept: string;
  guidance: string;
}

export interface ConfusionMark {
  id: string;
  file_path: string;
  line_start: number;
  line_end: number;
  code_snippet: string;
  category: string;
  explanation?: string;
  created_at: string;
}

export interface LlmConfig {
  provider: string;
  api_url: string;
  api_key: string;
  model: string;
  temperature: number;
  max_tokens: number;
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
