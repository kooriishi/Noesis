import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import {
  FileContentPayload,
  FileNode,
  ProjectMap,
  ExplanationCard,
  RewordingResult,
  ConfusionMark,
  LlmConfig,
  ConnectionTestResult,
  SystemStatus,
  ChatMessage,
  OutboundPreview,
  RunRequest,
  RunResponse,
  ExerciseRecord,
  GraphSymbol,
  SymbolNeighborhood,
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

  async markConfusion(
    filePath: string,
    startLine: number,
    endLine: number,
    snippet: string,
    category: string
  ): Promise<RewordingResult> {
    if (isTauriEnv()) {
      return await invoke<RewordingResult>('mark_confusion', {
        filePath,
        startLine,
        endLine,
        snippet,
        category,
      });
    }
    return {
      mark_id: `mock-mark-${Date.now()}`,
      original_category: category,
      analogy: '把它想象成水流阀门...',
      minimal_example: 'if (!ok) return;',
      prerequisite_concept: '卫语句',
      guidance: '注意检查条件',
      is_template: true,
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

  async getExpertSystemPrompt(): Promise<string> {
    if (isTauriEnv()) {
      return await invoke<string>('get_expert_system_prompt');
    }
    return '';
  },
};
