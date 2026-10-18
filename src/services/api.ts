import { invoke, Channel } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import {
  FileContentPayload,
  FileNode,
  ProjectMap,
  ExplanationCard,
  StructuredExplanationResponse,
  RewordingResult,
  ConfusionMark,
  LearningProfileSummary,
  ConversationSessionRecord,
  ConversationMessageRecord,
  LlmConfig,
  ConnectionTestResult,
  SystemStatus,
  ChatMessage,
  OutboundPreview,
  RunRequest,
  RunResponse,
  ExerciseRecord,
  ExerciseAttemptResult,
  GraphSymbol,
  SymbolNeighborhood,
  LspServerStatus,
  LspLocation,
  ExplanationSummary,
  PromptPreset,
  EffectivePromptPreview,
  ConfusionDetailResponse,
  AskStreamEvent,
  AskPreviewResponse,
  FeedbackStats,
} from '../types';

const isTauriEnv = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const ApiService = {
  async getSystemStatus(): Promise<SystemStatus> {
    if (isTauriEnv()) {
      return await invoke<SystemStatus>('get_system_status');
    }
    return { db_degraded: false };
  },

  async getLlmConfig(): Promise<LlmConfig> {
    if (isTauriEnv()) {
      return await invoke<LlmConfig>('get_llm_config');
    }
    return {
      provider: 'deepseek',
      api_url: 'https://api.deepseek.com',
      api_key: '',
      model: 'deepseek-chat',
      temperature: 0.3,
      max_tokens: 4096,
      has_api_key: false,
      api_key_masked: '',
    };
  },

  async setLlmConfig(config: LlmConfig): Promise<LlmConfig> {
    if (isTauriEnv()) {
      return await invoke<LlmConfig>('set_llm_config', { config });
    }
    return {
      ...config,
      has_api_key: Boolean(config.api_key || config.has_api_key),
      api_key_masked: config.api_key ? `sk-****${config.api_key.slice(-4)}` : config.api_key_masked || '',
      api_key: '',
    };
  },

  async setApiKey(apiKey: string): Promise<LlmConfig> {
    if (isTauriEnv()) {
      return await invoke<LlmConfig>('set_api_key', { apiKey });
    }
    return {
      provider: 'deepseek',
      api_url: 'https://api.deepseek.com',
      api_key: '',
      model: 'deepseek-chat',
      temperature: 0.3,
      max_tokens: 4096,
      has_api_key: Boolean(apiKey),
      api_key_masked: apiKey ? `sk-****${apiKey.slice(-4)}` : '',
    };
  },

  async clearApiKey(): Promise<LlmConfig> {
    if (isTauriEnv()) {
      return await invoke<LlmConfig>('clear_api_key');
    }
    return {
      provider: 'deepseek',
      api_url: 'https://api.deepseek.com',
      api_key: '',
      model: 'deepseek-chat',
      temperature: 0.3,
      max_tokens: 4096,
      has_api_key: false,
      api_key_masked: '',
    };
  },

  async testLlmConnection(config: LlmConfig): Promise<ConnectionTestResult> {
    if (isTauriEnv()) {
      return await invoke<ConnectionTestResult>('test_llm_connection', { config });
    }
    return {
      ok: true,
      status_code: 200,
      latency_ms: 15,
      category: 'ok',
      message: '浏览器模拟环境连接测试正常',
    };
  },

  async selectDirectory(): Promise<string | null> {
    if (isTauriEnv()) {
      try {
        const selected = await openDialog({
          directory: true,
          multiple: false,
          title: '选择要分析的代码仓库根目录',
        });
        if (typeof selected === 'string') return selected;
        if (Array.isArray(selected) && (selected as any).length > 0) return (selected as any)[0];
        return null;
      } catch (err) {
        console.warn('Dialog error:', err);
        return prompt('请输入本地项目绝对路径 (例如 F:/Projects/Noesis):');
      }
    } else {
      return prompt('请输入本地项目绝对路径 (例如 F:/Projects/Noesis):');
    }
  },

  async openProject(path: string): Promise<FileNode> {
    if (isTauriEnv()) {
      return await invoke<FileNode>('open_project', { path });
    }
    return {
      name: 'Noesis (Demo)',
      path: '',
      absolute_path: path,
      is_dir: true,
      size: 0,
      children: [
        {
          name: 'src',
          path: 'src',
          absolute_path: `${path}/src`,
          is_dir: true,
          size: 0,
          children: [
            { name: 'main.rs', path: 'src/main.rs', absolute_path: `${path}/src/main.rs`, is_dir: false, size: 240, language: 'rust' },
            { name: 'App.tsx', path: 'src/App.tsx', absolute_path: `${path}/src/App.tsx`, is_dir: false, size: 850, language: 'typescript' }
          ]
        },
        { name: 'Cargo.toml', path: 'Cargo.toml', absolute_path: `${path}/Cargo.toml`, is_dir: false, size: 620, language: 'toml' },
        { name: 'README.md', path: 'README.md', absolute_path: `${path}/README.md`, is_dir: false, size: 1200, language: 'markdown' }
      ]
    };
  },

  async listProjectDir(relDir: string): Promise<FileNode[]> {
    if (isTauriEnv()) {
      return await invoke<FileNode[]>('list_project_dir', { relDir });
    }
    return [];
  },

  async getProjectTree(): Promise<FileNode | null> {
    if (isTauriEnv()) {
      return await invoke<FileNode | null>('get_project_tree');
    }
    return null;
  },

  async readFile(path: string): Promise<FileContentPayload> {
    if (isTauriEnv()) {
      return await invoke<FileContentPayload>('read_file', { path });
    }
    return {
      path,
      content: `// Sample content for ${path}\nfunction main() {\n  console.log("Hello from NOESIS!");\n}`,
      hash: 'mock-hash-1234',
      language: 'typescript',
      size: 78,
    };
  },

  async saveFile(path: string, content: string): Promise<string> {
    if (isTauriEnv()) {
      return await invoke<string>('save_file', { path, content });
    }
    return 'mock-saved-hash';
  },

  async indexProject(): Promise<ProjectMap> {
    if (isTauriEnv()) {
      return await invoke<ProjectMap>('index_project');
    }
    return {
      root: '.',
      modules: [],
      entry_points: ['src/main.rs'],
      reading_route: [
        {
          step: 1,
          file_path: 'src/main.rs',
          title: '系统入口',
          rationale: '项目入口',
          symbol_count: 3,
          priority: 1,
        }
      ],
      total_symbols: 12,
      indexed_files_count: 5,
    };
  },

  async getProjectMap(): Promise<ProjectMap | null> {
    if (isTauriEnv()) {
      return await invoke<ProjectMap | null>('get_project_map');
    }
    return null;
  },

  async setReadingStepStatus(stepId: string, status: string): Promise<void> {
    if (isTauriEnv()) {
      await invoke<void>('set_reading_step_status', { stepId, status });
    }
  },

  async getSymbolNeighborhood(symbolId: string): Promise<SymbolNeighborhood | null> {
    if (isTauriEnv()) {
      return await invoke<SymbolNeighborhood | null>('get_symbol_neighborhood', { symbolId });
    }
    return null;
  },

  async listProjectSymbols(filePath?: string): Promise<GraphSymbol[]> {
    if (isTauriEnv()) {
      return await invoke<GraphSymbol[]>('list_project_symbols', { filePath });
    }
    return [];
  },

  async explainCode(
    filePath: string,
    snippet: string,
    startLine: number,
    endLine: number
  ): Promise<ExplanationCard[]> {
    if (isTauriEnv()) {
      return await invoke<ExplanationCard[]>('explain_code', {
        filePath,
        snippet,
        startLine,
        endLine,
      });
    }
    return [
      {
        id: 'card-1',
        perspective: 'overall',
        title: '整体架构与数据流',
        one_sentence: '本段代码定义了核心入口契约。',
        key_bullets: ['对外暴露稳定调用接口', '规范化状态传递'],
        detailed_body: '整体深度解析正文...',
        source: { file_path: filePath, line_start: startLine, line_end: endLine, code_snippet: snippet },
        source_matches: true,
        is_template: true,
        is_expanded: false,
      }
    ];
  },

  async explainSelectionStructured(
    config: LlmConfig,
    filePath: string,
    snippet: string,
    startLine: number,
    endLine: number,
    forceRefresh: boolean = false,
    depthMode: 'standard' | 'deep' = 'standard'
  ): Promise<StructuredExplanationResponse> {
    const fallbackCards = await this.explainCode(filePath, snippet, startLine, endLine);
    if (isTauriEnv()) {
      const raw: any = await invoke('explain_selection_structured', {
        config,
        filePath,
        snippet,
        startLine,
        endLine,
        forceRefresh,
        depthMode,
      });

      const exp = raw?.explanation;
      const perspectives: Array<'overall' | 'logic' | 'syntax' | 'principle'> = [
        'overall',
        'logic',
        'syntax',
        'principle',
      ];
      let hasAiCards = false;
      if (exp) {
        const topCitations: any[] = Array.isArray(exp.citations) ? exp.citations : [];
        const citById = new Map<string, any>();
        for (const c of topCitations) {
          if (c && c.id) citById.set(c.id, c);
        }

        for (const p of perspectives) {
          const sec = exp[p];
          const card = fallbackCards.find((c) => c.perspective === p);
          if (sec && card) {
            if (sec.title) card.title = sec.title;
            if (sec.one_sentence) card.one_sentence = sec.one_sentence;
            const cardCitations: any[] = [];
            if (Array.isArray(sec.bullets) && sec.bullets.length > 0) {
              card.key_bullets = sec.bullets.map((b: any) => {
                if (typeof b === 'string') return b;
                if (b?.kind === 'knowledge' && b?.pack_ref) {
                  cardCitations.push({
                    kind: 'knowledge',
                    pack_ref: b.pack_ref,
                    verification_status: 'knowledge',
                    verified: b.status === 'knowledge' || b.status === 'verified',
                    note: b.text,
                  });
                }
                if (Array.isArray(b?.cites)) {
                  for (const cid of b.cites) {
                    const target = citById.get(cid);
                    if (target) {
                      cardCitations.push({
                        kind: 'code',
                        file: target.file,
                        line_start: target.line_start,
                        line_end: target.line_end,
                        quote: target.quote,
                        verification_status:
                          target.status === 'verified'
                            ? 'verified'
                            : target.status === 'corrected'
                            ? 'corrected'
                            : 'unverified',
                        verified: target.status === 'verified' || target.status === 'corrected',
                        corrected_line_start: target.corrected_line_start,
                        corrected_line_end: target.corrected_line_end,
                        note: target.reason || target.quote,
                      });
                    }
                  }
                }
                return b?.text || String(b);
              });
            }
            if (sec.detail) card.detailed_body = sec.detail;
            if (cardCitations.length > 0) {
              card.citations = cardCitations;
            }
            card.is_template = false;
            hasAiCards = true;
          }
        }
      }

      const rawManifest = raw?.manifest || {};
      const manifest: any = {
        basis_hash: rawManifest.basis_hash || 'verified-ctx',
        total_tokens: rawManifest.total_tokens || 0,
        budget_tokens: rawManifest.budget_tokens || 16000,
        injected_knowledge_ids: rawManifest.loaded_knowledge_packs || [],
        layers: [
          {
            layer: 'P0',
            label: '内置知识包',
            included: (rawManifest.loaded_knowledge_packs || []).length > 0,
            tokens: 0,
            trimmed: false,
            summary:
              (rawManifest.loaded_knowledge_packs || []).join(', ') || '通用导读提示词',
          },
          {
            layer: 'P1',
            label: '当前选区代码',
            included: true,
            tokens: rawManifest.total_tokens || 0,
            trimmed: false,
            summary: `${filePath}:${startLine}-${endLine}`,
          },
          {
            layer: 'P2',
            label: '领域语义事实',
            included: (rawManifest.domain_facts || []).length > 0,
            tokens: 0,
            trimmed: false,
            summary: `${(rawManifest.domain_facts || []).length} 条实体/拓扑事实`,
          },
          {
            layer: 'P3-P4',
            label: '所属符号与 1-hop 邻域',
            included: Boolean(rawManifest.enclosing_symbol),
            tokens: 0,
            trimmed: (raw?.dropped || []).length > 0,
            summary: rawManifest.enclosing_symbol || `${(rawManifest.neighbors || []).length} 个邻接符号`,
          },
          {
            layer: 'P6',
            label: '个人学习画像注入',
            included: Boolean(rawManifest.learning_profile_injected),
            tokens: 0,
            trimmed: false,
            summary: rawManifest.learning_profile_injected ? '已按过往卡点定制' : '无历史卡点',
          },
        ],
      };

      return {
        cards: fallbackCards,
        manifest,
        from_cache: Boolean(raw?.cached),
        is_template: !hasAiCards,
        warning: !hasAiCards && raw?.raw_unverified_output ? 'AI 返回内容未能解析为结构化四层卡片，已回退模板占位' : undefined,
        quality: raw?.quality,
        explanation_id: raw?.explanation_id,
        depth_mode: depthMode,
      };
    }

    return {
      cards: fallbackCards,
      manifest: {
        basis_hash: 'mock-basis-hash',
        total_tokens: 420,
        budget_tokens: 6000,
        layers: [
          { layer: 'P0', label: '系统提示词与内置知识卡片', included: true, tokens: 180, trimmed: false, summary: '已注入 2 张领域知识卡片' },
          { layer: 'P1', label: '当前选区代码 (带行号)', included: true, tokens: 240, trimmed: false, summary: `${filePath}:${startLine}-${endLine}` },
        ],
        injected_knowledge_ids: [],
      },
      from_cache: false,
      is_template: true,
    };
  },

  async cancelLlmRequest(requestId: string): Promise<boolean> {
    if (isTauriEnv()) {
      return await invoke<boolean>('cancel_llm_request', { requestId });
    }
    return false;
  },

  async markConfusion(
    filePath: string,
    startLine: number,
    endLine: number,
    snippet: string,
    category: string,
    reexplainLevel: number = 1
  ): Promise<RewordingResult> {
    if (isTauriEnv()) {
      return await invoke<RewordingResult>('mark_confusion', {
        filePath,
        startLine,
        endLine,
        snippet,
        category,
        userNote: `level:${reexplainLevel}`,
      });
    }
    return {
      mark_id: `mock-mark-${Date.now()}`,
      original_category: category,
      reexplain_level: reexplainLevel,
      analogy: '把它想象成水流阀门...',
      minimal_example: 'if (!ok) return;',
      prerequisite_concept: '卫语句',
      guidance: '注意检查条件',
      is_template: true,
    };
  },

  async resolveConfusionMark(id: string, level: number = 1): Promise<void> {
    if (isTauriEnv()) {
      await invoke<void>('resolve_confusion_mark', { id, level, resolved: true });
    }
  },

  async getLearningProfile(): Promise<LearningProfileSummary> {
    if (isTauriEnv()) {
      const items: any[] = await invoke<any[]>('get_learning_profile');
      const counts: Record<string, number> = {};
      let openMarks = 0;
      let resolvedMarks = 0;
      for (const item of items || []) {
        const key = item.concept_key || item.domain || 'general';
        counts[key] = (counts[key] || 0) + (item.confused_count || 0);
        openMarks += item.confused_count || 0;
        resolvedMarks += item.understood_count || 0;
      }
      return {
        total_marks: openMarks + resolvedMarks,
        open_marks: openMarks,
        resolved_marks: resolvedMarks,
        counts_by_type: counts,
        recent_open_summaries: [],
      };
    }
    return {
      total_marks: 0,
      open_marks: 0,
      resolved_marks: 0,
      counts_by_type: {},
      recent_open_summaries: [],
    };
  },

  async updateConfusionMarkExplanation(
    id: string,
    aiExplanation: string,
    model: string
  ): Promise<void> {
    if (isTauriEnv()) {
      await invoke<void>('update_confusion_mark_explanation', {
        id,
        aiExplanation,
        model,
      });
    }
  },

  async getConfusionMarks(filePath?: string): Promise<ConfusionMark[]> {
    if (isTauriEnv()) {
      return await invoke<ConfusionMark[]>('get_confusion_marks', { filePath });
    }
    return [];
  },

  async listConversations(): Promise<ConversationSessionRecord[]> {
    if (isTauriEnv()) {
      const list: any[] = await invoke<any[]>('list_conversations');
      return (list || []).map((c) => ({
        id: c.id,
        project_id: c.project_id,
        title: c.title,
        rolling_summary: c.summary,
        created_at: new Date((c.created_at || 0) * 1000).toISOString(),
        updated_at: new Date((c.updated_at || 0) * 1000).toISOString(),
      }));
    }
    return [];
  },

  async createConversation(title?: string): Promise<ConversationSessionRecord> {
    if (isTauriEnv()) {
      const c: any = await invoke<any>('create_conversation', { title: title || '新对话' });
      return {
        id: c.id,
        project_id: c.project_id,
        title: c.title,
        rolling_summary: c.summary,
        created_at: new Date((c.created_at || 0) * 1000).toISOString(),
        updated_at: new Date((c.updated_at || 0) * 1000).toISOString(),
      };
    }
    const now = new Date().toISOString();
    return {
      id: `conv-${Date.now()}`,
      project_id: 'default',
      title: title || '新对话',
      created_at: now,
      updated_at: now,
    };
  },

  async loadConversationMessages(conversationId: string): Promise<ConversationMessageRecord[]> {
    if (isTauriEnv()) {
      const list: any[] = await invoke<any[]>('load_conversation_messages', {
        conversationId,
        limit: 50,
      });
      return (list || []).map((m) => ({
        id: m.id,
        conversation_id: m.conversation_id,
        role: m.role,
        content: m.content,
        citations_json: m.meta_json,
        token_est: m.tokens || 0,
        created_at: new Date((m.created_at || 0) * 1000).toISOString(),
      }));
    }
    return [];
  },

  async appendConversationMessage(
    conversationId: string,
    role: string,
    content: string,
    citationsJson?: string
  ): Promise<ConversationMessageRecord> {
    if (isTauriEnv()) {
      const m: any = await invoke<any>('append_conversation_message', {
        conversationId,
        role,
        content,
        metaJson: citationsJson,
      });
      return {
        id: m.id,
        conversation_id: m.conversation_id,
        role: m.role,
        content: m.content,
        citations_json: m.meta_json,
        token_est: m.tokens || 0,
        created_at: new Date((m.created_at || 0) * 1000).toISOString(),
      };
    }
    return {
      id: `msg-${Date.now()}`,
      conversation_id: conversationId,
      role,
      content,
      citations_json: citationsJson,
      token_est: Math.ceil(content.length / 4),
      created_at: new Date().toISOString(),
    };
  },

  async deleteConversation(conversationId: string): Promise<void> {
    if (isTauriEnv()) {
      await invoke<void>('delete_conversation', { id: conversationId });
    }
  },

  async previewOutbound(config: LlmConfig, messages: ChatMessage[]): Promise<OutboundPreview> {
    if (isTauriEnv()) {
      return await invoke<OutboundPreview>('preview_outbound', { config, messages });
    }
    return {
      target_url: config.api_url,
      model: config.model,
      messages_count: messages.length,
      total_chars: 500,
      contains_sanitized_tokens: false,
      sanitized_preview: messages[0]?.content || '',
      file_references: [],
    };
  },

  async sendLlmChat(config: LlmConfig, messages: ChatMessage[]): Promise<string> {
    if (isTauriEnv()) {
      return await invoke<string>('send_llm_chat', { config, messages });
    }
    return '【本地离线模拟响应】已收到提示词，当前正在离线模式下运行。';
  },

  async runScratchCode(req: RunRequest): Promise<RunResponse> {
    if (isTauriEnv()) {
      return await invoke<RunResponse>('run_scratch_code', { req });
    }
    return {
      stdout: 'Demo output\nSuccess!',
      stderr: '',
      exit_code: 0,
      duration_ms: 12,
      success: true,
    };
  },

  async listDueReviews(): Promise<ExerciseRecord[]> {
    if (isTauriEnv()) {
      return await invoke<ExerciseRecord[]>('list_due_reviews');
    }
    return [];
  },

  async generateProjectExercises(): Promise<ExerciseRecord[]> {
    if (isTauriEnv()) {
      return await invoke<ExerciseRecord[]>('generate_project_exercises');
    }
    return [];
  },

  async submitExerciseAnswer(
    exerciseId: string,
    userAnswer: string,
    quality: number
  ): Promise<ExerciseAttemptResult> {
    if (isTauriEnv()) {
      return await invoke<ExerciseAttemptResult>('submit_exercise_answer', {
        exerciseId,
        userAnswer,
        quality,
      });
    }
    return {
      exercise_id: exerciseId,
      is_correct: true,
      quality,
      score: 1,
      feedback: '回答正确！',
      next_interval_days: 1,
      next_ease_factor: 2.5,
      next_review_at: new Date().toISOString(),
    };
  },

  async detectLspServers(): Promise<LspServerStatus[]> {
    if (isTauriEnv()) {
      return await invoke<LspServerStatus[]>('detect_lsp_servers');
    }
    return [];
  },

  async lspFindDefinition(symbolId: string): Promise<LspLocation | null> {
    if (isTauriEnv()) {
      return await invoke<LspLocation | null>('lsp_find_definition', { symbolId });
    }
    return null;
  },

  async lspFindReferences(symbolId: string): Promise<LspLocation[]> {
    if (isTauriEnv()) {
      return await invoke<LspLocation[]>('lsp_find_references', { symbolId });
    }
    return [];
  },

  async lspGetHover(symbolId: string): Promise<string | null> {
    if (isTauriEnv()) {
      return await invoke<string | null>('lsp_get_hover', { symbolId });
    }
    return null;
  },

  async getExpertSystemPrompt(): Promise<string> {
    if (isTauriEnv()) {
      return await invoke<string>('get_expert_system_prompt');
    }
    return '';
  },

  // ----------------- F3: Explanation History -----------------
  async listExplanations(
    filePath?: string,
    query?: string,
    pinnedOnly?: boolean,
    limit?: number,
    offset?: number
  ): Promise<ExplanationSummary[]> {
    if (isTauriEnv()) {
      return await invoke<ExplanationSummary[]>('list_explanations', {
        filePath,
        query,
        pinnedOnly,
        limit,
        offset,
      });
    }
    return [];
  },

  async getExplanation(id: string): Promise<StructuredExplanationResponse | null> {
    if (isTauriEnv()) {
      const raw: any = await invoke('get_explanation', { id });
      if (!raw) return null;
      // raw is a CachedExplanation
      try {
        const parsed = JSON.parse(raw.json);
        const perspectives: Array<'overall' | 'logic' | 'syntax' | 'principle'> = [
          'overall',
          'logic',
          'syntax',
          'principle',
        ];
        const cards: ExplanationCard[] = [];
        const citById = new Map<string, any>();
        for (const c of parsed.citations || []) {
          if (c && c.id) citById.set(c.id, c);
        }

        const labels: Record<string, string> = {
          overall: '整体架构与数据流',
          logic: '逻辑实现与分支',
          syntax: '核心语法与类型',
          principle: '底层原理与机制',
        };

        for (const p of perspectives) {
          const sec = parsed[p] || {};
          const cardCitations: any[] = [];
          const key_bullets = (sec.bullets || []).map((b: any) => {
            if (typeof b === 'string') return b;
            if (b?.kind === 'knowledge' && b?.pack_ref) {
              cardCitations.push({
                kind: 'knowledge',
                pack_ref: b.pack_ref,
                verification_status: 'knowledge',
                verified: true,
                note: b.text,
              });
            }
            if (Array.isArray(b?.cites)) {
              for (const cid of b.cites) {
                const target = citById.get(cid);
                if (target) {
                  cardCitations.push({
                    kind: 'code',
                    file: target.file,
                    line_start: target.line_start,
                    line_end: target.line_end,
                    quote: target.quote,
                    verification_status: target.status || 'verified',
                    verified: target.status === 'verified' || target.status === 'corrected',
                    note: target.reason || target.quote,
                  });
                }
              }
            }
            return b?.text || String(b);
          });

          cards.push({
            id: `card-${raw.id}-${p}`,
            perspective: p,
            title: sec.title || labels[p],
            one_sentence: sec.one_sentence || '',
            key_bullets,
            detailed_body: sec.detail || '',
            source: {
              file_path: raw.file_path || raw.file_id,
              line_start: 1,
              line_end: 1,
              code_snippet: raw.selected_preview || '',
            },
            source_matches: true,
            is_template: false,
            is_expanded: false,
            citations: cardCitations,
          });
        }

        return {
          cards,
          manifest: {
            basis_hash: raw.basis_hash,
            total_tokens: 0,
            budget_tokens: 16000,
            layers: [],
            injected_knowledge_ids: [],
          },
          from_cache: true,
          is_template: false,
          explanation_id: raw.id,
          depth_mode: raw.depth_mode || 'standard',
        };
      } catch (err) {
        console.error('Failed to parse cached explanation JSON', err);
        return null;
      }
    }
    return null;
  },

  async renameExplanation(id: string, newTitle: string): Promise<boolean> {
    if (isTauriEnv()) {
      return await invoke<boolean>('rename_explanation', { id, newTitle });
    }
    return true;
  },

  async pinExplanation(id: string, pinned: boolean): Promise<boolean> {
    if (isTauriEnv()) {
      return await invoke<boolean>('pin_explanation', { id, pinned });
    }
    return true;
  },

  async deleteExplanation(id: string): Promise<boolean> {
    if (isTauriEnv()) {
      return await invoke<boolean>('delete_explanation', { id });
    }
    return true;
  },

  // ----------------- F4: Ask & Stream -----------------
  async previewAsk(
    question: string,
    file?: string,
    range?: [number, number],
    selectedCode?: string
  ): Promise<AskPreviewResponse> {
    if (isTauriEnv()) {
      return await invoke<AskPreviewResponse>('preview_ask', {
        question,
        file,
        range,
        selectedCode,
      });
    }
    return {
      messages: [{ role: 'user', content: question }],
      token_estimate: 200,
      manifest: {
        basis_hash: 'mock',
        total_tokens: 200,
        budget_tokens: 16000,
        layers: [],
        injected_knowledge_ids: [],
      },
      outbound_preview: {
        target_url: 'https://api.deepseek.com',
        model: 'deepseek-chat',
        messages_count: 1,
        total_chars: question.length,
        contains_sanitized_tokens: false,
        sanitized_preview: question,
        file_references: file ? [file] : [],
      },
    };
  },

  async askProjectQuestion(
    question: string,
    onEvent: (event: AskStreamEvent) => void,
    conversationId?: string,
    file?: string,
    range?: [number, number],
    selectedCode?: string,
    useTools?: boolean
  ): Promise<string> {
    if (isTauriEnv()) {
      const channel = new Channel<AskStreamEvent>();
      channel.onmessage = (event) => onEvent(event);
      return await invoke<string>('ask_project_question', {
        conversationId,
        question,
        file,
        range,
        selectedCode,
        useTools,
        onEvent: channel,
      });
    }
    // Web fallback simulation
    setTimeout(() => {
      onEvent({ type: 'token', token: '当前处于纯网页预览模式，已模拟回答：' });
      onEvent({ type: 'token', token: `针对问题「${question}」，系统已成功捕获上下文并装配。` });
      onEvent({ type: 'done', conversation_id: 'mock-conv' });
    }, 200);
    return 'mock-conv';
  },

  // ----------------- F5: Prompt Presets -----------------
  async listPromptPresets(scope?: string): Promise<PromptPreset[]> {
    if (isTauriEnv()) {
      return await invoke<PromptPreset[]>('list_prompt_presets', { scope });
    }
    return [];
  },

  async savePromptPreset(preset: PromptPreset): Promise<boolean> {
    if (isTauriEnv()) {
      return await invoke<boolean>('save_prompt_preset', { preset });
    }
    return true;
  },

  async setActivePromptPreset(id: string): Promise<boolean> {
    if (isTauriEnv()) {
      return await invoke<boolean>('set_active_prompt_preset', { id });
    }
    return true;
  },

  async deletePromptPreset(id: string): Promise<boolean> {
    if (isTauriEnv()) {
      return await invoke<boolean>('delete_prompt_preset', { id });
    }
    return true;
  },

  async resetPromptToDefault(scope: string): Promise<boolean> {
    if (isTauriEnv()) {
      return await invoke<boolean>('reset_prompt_to_default', { scope });
    }
    return true;
  },

  async previewEffectivePrompt(scope: string): Promise<EffectivePromptPreview> {
    if (isTauriEnv()) {
      return await invoke<EffectivePromptPreview>('preview_effective_prompt', { scope });
    }
    return {
      scope,
      preset_name: '内置默认',
      l1_persona: '',
      l0_locked: '',
      full_prompt: '',
      token_estimate: 0,
    };
  },

  // ----------------- F6: Confusion Detail & Revisions -----------------
  async getConfusionDetail(id: string): Promise<ConfusionDetailResponse | null> {
    if (isTauriEnv()) {
      const raw: any = await invoke('get_confusion_detail', { id });
      if (!raw) return null;
      return {
        mark: raw[0],
        revisions: raw[1] || [],
      };
    }
    return null;
  },

  async reopenConfusionMark(id: string): Promise<boolean> {
    if (isTauriEnv()) {
      return await invoke<boolean>('reopen_confusion_mark', { id });
    }
    return true;
  },

  async deleteConfusionMark(id: string): Promise<boolean> {
    if (isTauriEnv()) {
      return await invoke<boolean>('delete_confusion_mark', { id });
    }
    return true;
  },

  async exportConfusionMarkdown(): Promise<string> {
    if (isTauriEnv()) {
      return await invoke<string>('export_confusion_markdown');
    }
    return '# Noesis 学习档案';
  },

  // ----------------- F7-a: Explanation Feedback -----------------
  async submitExplanationFeedback(
    explanationId: string,
    cardPerspective: string,
    feedback: string,
    note?: string
  ): Promise<void> {
    if (isTauriEnv()) {
      await invoke('submit_explanation_feedback', {
        explanationId,
        cardPerspective,
        feedback,
        note,
      });
    }
  },

  async getFeedbackStats(promptVersion?: string, model?: string): Promise<FeedbackStats> {
    if (isTauriEnv()) {
      return await invoke<FeedbackStats>('get_feedback_stats', {
        promptVersion,
        model,
      });
    }
    return {
      total: 0,
      helpful: 0,
      too_generic: 0,
      wrong: 0,
      too_generic_rate: 0,
      wrong_rate: 0,
    };
  },
};
