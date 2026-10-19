import React, { useState, useEffect, useRef, useCallback } from 'react';
import {
  ShieldCheck,
  Save,
  Code,
  Terminal,
  Settings as SettingsIcon,
  X,
  FileText,
  FileCode,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRightClose,
  PanelRightOpen,
  AlertTriangle,
  BookOpen,
} from 'lucide-react';
import { ApiService } from '../services/api';
import { CodeEditor, evictEditorStateCache } from '../editor/CodeEditor';
import { ProjectSidebar } from '../sidebar-project/ProjectSidebar';
import { AiSidebar } from '../sidebar-ai/AiSidebar';
import { ScratchRunner } from '../scratch/ScratchRunner';
import { SettingsModal } from './SettingsModal';
import { useToast } from '../components/feedback';
import { extractJsonObject } from '../utils/llmJson';
import {
  FileNode,
  ProjectMap,
  ExplanationCard,
  ContextManifest,
  RewordingResult,
  ConfusionMark,
  LlmConfig,
  SystemStatus,
  ChatMessage,
  OutboundPreview,
  RunRequest,
  RunResponse,
  ExerciseRecord,
  TabItem,
  AppSettings,
} from '../types';

const DEFAULT_SETTINGS: AppSettings = {
  editorFontSize: 14,
  uiFontSize: 13,
  fontFamily: 'Consolas, "Fira Code", monospace',
  wordWrap: false,
  tabSize: 4,
  theme: 'one-dark',
};

const DEFAULT_LLM: LlmConfig = {
  provider: 'deepseek',
  api_url: 'https://api.deepseek.com',
  api_key: '',
  model: 'deepseek-chat',
  temperature: 0.3,
  max_tokens: 4096,
  has_api_key: false,
  api_key_masked: '',
};

export const ShellLayout: React.FC = () => {
  // App Settings State (persisted in localStorage)
  const [settings, setSettings] = useState<AppSettings>(() => {
    try {
      const saved = localStorage.getItem('noesis_settings');
      return saved ? { ...DEFAULT_SETTINGS, ...JSON.parse(saved) } : DEFAULT_SETTINGS;
    } catch {
      return DEFAULT_SETTINGS;
    }
  });

  const [isSettingsOpen, setIsSettingsOpen] = useState<boolean>(false);
  const [llmConfig, setLlmConfig] = useState<LlmConfig>(DEFAULT_LLM);
  const [systemStatus, setSystemStatus] = useState<SystemStatus>({ db_degraded: false });

  const updateSettings = (newSettings: AppSettings) => {
    setSettings(newSettings);
    localStorage.setItem('noesis_settings', JSON.stringify(newSettings));
  };

  const updateLlmConfig = async (newConfig: LlmConfig) => {
    // Never write api_key to localStorage (A1)
    localStorage.removeItem('noesis_llm_config');
    try {
      const updated = await ApiService.setLlmConfig(newConfig);
      setLlmConfig(updated);
    } catch (err) {
      console.error('Failed to update LLM config:', err);
      setLlmConfig({ ...newConfig, api_key: '' });
    }
  };

  const handleClearApiKey = async () => {
    localStorage.removeItem('noesis_llm_config');
    try {
      const updated = await ApiService.clearApiKey();
      setLlmConfig(updated);
    } catch (err) {
      console.error('Failed to clear API key:', err);
    }
  };

  // Resizable Panels State
  const [leftWidth, setLeftWidth] = useState<number>(() => {
    const saved = localStorage.getItem('noesis_left_width');
    return saved ? Math.max(180, Math.min(600, Number(saved))) : 280;
  });
  const [rightWidth, setRightWidth] = useState<number>(() => {
    const saved = localStorage.getItem('noesis_right_width');
    return saved ? Math.max(260, Math.min(750, Number(saved))) : 380;
  });
  const [leftCollapsed, setLeftCollapsed] = useState<boolean>(false);
  const [rightCollapsed, setRightCollapsed] = useState<boolean>(false);

  // Resizing state
  const isDraggingLeftRef = useRef(false);
  const isDraggingRightRef = useRef(false);
  const [isResizing, setIsResizing] = useState(false);

  // Multi-Tab Files State
  const [tabs, setTabs] = useState<TabItem[]>([]);
  const [activeTabPath, setActiveTabPath] = useState<string | null>(null);
  const [tabBarEpoch, setTabBarEpoch] = useState<number>(0);
  const hasInitializedRef = useRef<boolean>(false);

  // Project & Map State
  const [currentProject, setCurrentProject] = useState<FileNode | null>(null);
  const [projectMap, setProjectMap] = useState<ProjectMap | null>(null);
  const [isIndexing, setIsIndexing] = useState<boolean>(false);
  const [indexError, setIndexError] = useState<string | null>(null);
  const [lastIndexedAt, setLastIndexedAt] = useState<string | null>(null);

  // AI & Reading State
  const [cards, setCards] = useState<ExplanationCard[]>([]);
  const [contextManifest, setContextManifest] = useState<ContextManifest | null>(null);
  const [fromCache, setFromCache] = useState<boolean>(false);
  const [jumpToRange, setJumpToRange] = useState<{
    filePath: string;
    startLine: number;
    endLine: number;
    nonce: number;
  } | null>(null);
  const [explainError, setExplainError] = useState<string | null>(null);
  const [lastExplainSelection, setLastExplainSelection] = useState<{
    snippet: string;
    startLine: number;
    endLine: number;
  } | null>(null);
  const [confusionMarks, setConfusionMarks] = useState<ConfusionMark[]>([]);
  const [activeRewording, setActiveRewording] = useState<RewordingResult | null>(null);
  const [dueReviews, setDueReviews] = useState<ExerciseRecord[]>([]);
  const [isExplaining, setIsExplaining] = useState<boolean>(false);
  const [activeSidebarTab, setActiveSidebarTab] = useState<'cards' | 'confusion' | 'chat' | 'settings'>('cards');
  const toast = useToast();
  const [contextChip, setContextChip] = useState<{
    file: string;
    range: [number, number];
    snippet: string;
  } | null>(null);
  const [currentExplanationId, setCurrentExplanationId] = useState<string | undefined>(undefined);

  // Scratchpad State
  const [showScratch, setShowScratch] = useState<boolean>(false);
  const [scratchCode, setScratchCode] = useState<string>('');
  const [scratchLanguage, setScratchLanguage] = useState<string>('javascript');

  // G6: Draggable Bottom Drawer State
  const [drawerHeight, setDrawerHeight] = useState<number>(() => {
    const saved = localStorage.getItem('noesis_bottom_drawer_height');
    if (saved) {
      const val = parseInt(saved, 10);
      if (!isNaN(val) && val >= 120) return val;
    }
    return 280;
  });
  const [isDraggingDrawer, setIsDraggingDrawer] = useState(false);
  const drawerDragStartYRef = useRef<number>(0);
  const drawerDragStartHeightRef = useRef<number>(280);

  const handleDrawerPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    setIsDraggingDrawer(true);
    drawerDragStartYRef.current = e.clientY;
    drawerDragStartHeightRef.current = drawerHeight;
  };

  const handleDrawerPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDraggingDrawer) return;
    const deltaY = drawerDragStartYRef.current - e.clientY;
    const maxHeight = Math.max(160, window.innerHeight - 160);
    const newHeight = Math.max(120, Math.min(maxHeight, drawerDragStartHeightRef.current + deltaY));
    setDrawerHeight(newHeight);
    localStorage.setItem('noesis_bottom_drawer_height', String(newHeight));
  };

  const handleDrawerPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (isDraggingDrawer) {
      try {
        e.currentTarget.releasePointerCapture(e.pointerId);
      } catch {}
      setIsDraggingDrawer(false);
    }
  };

  const handleDrawerDoubleClick = () => {
    const maxHeight = Math.max(280, window.innerHeight - 200);
    setDrawerHeight((prev) => {
      let next: number;
      if (prev <= 130) {
        next = 280;
      } else if (prev < maxHeight - 20) {
        next = maxHeight;
      } else {
        next = 120;
      }
      localStorage.setItem('noesis_bottom_drawer_height', String(next));
      return next;
    });
  };

  const activeTab = tabs.find((t) => t.path === activeTabPath);

  // Keyboard shortcut Ctrl+, for settings
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === ',') {
        e.preventDefault();
        setIsSettingsOpen((prev) => !prev);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // Panel Resizer Drag Handlers
  const handleMouseMove = useCallback((e: MouseEvent) => {
    if (isDraggingLeftRef.current) {
      const newWidth = Math.max(180, Math.min(600, e.clientX));
      setLeftWidth(newWidth);
      localStorage.setItem('noesis_left_width', String(newWidth));
    } else if (isDraggingRightRef.current) {
      const newWidth = Math.max(260, Math.min(750, window.innerWidth - e.clientX));
      setRightWidth(newWidth);
      localStorage.setItem('noesis_right_width', String(newWidth));
    }
  }, []);

  const handleMouseUp = useCallback(() => {
    if (isDraggingLeftRef.current || isDraggingRightRef.current) {
      isDraggingLeftRef.current = false;
      isDraggingRightRef.current = false;
      setIsResizing(false);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    }
  }, []);

  useEffect(() => {
    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [handleMouseMove, handleMouseUp]);

  const startResizeLeft = (e: React.MouseEvent) => {
    e.preventDefault();
    isDraggingLeftRef.current = true;
    setIsResizing(true);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  };

  const startResizeRight = (e: React.MouseEvent) => {
    e.preventDefault();
    isDraggingRightRef.current = true;
    setIsResizing(true);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  };

  const resetLayout = () => {
    setLeftWidth(280);
    setRightWidth(380);
    setLeftCollapsed(false);
    setRightCollapsed(false);
    localStorage.setItem('noesis_left_width', '280');
    localStorage.setItem('noesis_right_width', '380');
  };

  // Load initial state on mount (guarded against React.StrictMode double-invocation)
  useEffect(() => {
    if (hasInitializedRef.current) return;
    hasInitializedRef.current = true;

    // Purge any legacy localStorage LLM key (A1)
    localStorage.removeItem('noesis_llm_config');
    ApiService.getLlmConfig().then(setLlmConfig).catch(console.error);
    ApiService.getSystemStatus().then(setSystemStatus).catch(console.error);
    handleOpenProject('./');
    loadConfusionMarks();
    loadDueReviews();
  }, []);

  const handleOpenProject = async (path?: string) => {
    try {
      const targetPath = path || (await ApiService.selectDirectory());
      if (!targetPath) return;

      const tree = await ApiService.openProject(targetPath);
      setCurrentProject(tree);
      toast.success(`成功载入项目: ${tree.name || targetPath}`);

      // When switching to a new project directory, clear stale tabs from previous workspace
      evictEditorStateCache();
      setTabs([]);
      setActiveTabPath(null);
      setTabBarEpoch((n) => n + 1);

      const first = findFirstFile(tree);
      if (first) {
        await handleOpenFile(first.path);
      }

      await handleTriggerIndex();
    } catch (err: any) {
      console.error('Failed to open project:', err);
      toast.error(`打开工程失败: ${err?.message || String(err)}`);
    }
  };

  const findFirstFile = (node: FileNode): FileNode | null => {
    if (!node.is_dir) return node;
    if (node.children) {
      // 1. Prefer README or primary entry files at current level
      const files = node.children.filter((c) => !c.is_dir);
      const preferredFile = files.find((f) =>
        /^(README\.md|lib\.rs|main\.rs|main\.py|index\.tsx|index\.ts|Cargo\.toml|package\.json)$/i.test(
          f.name
        )
      );
      if (preferredFile) return preferredFile;

      // 2. Prefer src/ directory over config directories like capabilities/ or .github/
      const dirs = node.children.filter((c) => c.is_dir);
      const srcDir = dirs.find((d) => d.name === 'src');
      if (srcDir) {
        const inSrc = findFirstFile(srcDir);
        if (inSrc) return inSrc;
      }

      // 3. Any direct file before recursing into other subdirectories
      if (files.length > 0) return files[0];

      for (const d of dirs) {
        if (d.name === 'capabilities' || d.name === 'icons') continue;
        const found = findFirstFile(d);
        if (found) return found;
      }

      for (const c of node.children) {
        const found = findFirstFile(c);
        if (found) return found;
      }
    }
    return null;
  };

  const handleOpenFile = async (filePath: string) => {
    const existing = tabs.find((t) => t.path === filePath);
    if (existing) {
      setActiveTabPath(filePath);
      return;
    }

    try {
      const payload = await ApiService.readFile(filePath);
      const name = filePath.split('/').pop() || filePath;
      const newTab: TabItem = {
        path: filePath,
        name,
        content: payload.content,
        language: payload.language,
        isDirty: false,
      };

      setTabs((prev) => {
        // Deduplicate by path to prevent duplicate React keys under concurrent calls
        const deduped = prev.filter(
          (t, idx, arr) => arr.findIndex((item) => item.path === t.path) === idx
        );
        if (deduped.some((t) => t.path === filePath)) {
          return deduped;
        }
        return [...deduped, newTab];
      });
      setActiveTabPath(filePath);
    } catch (err: any) {
      console.error('Failed to read file:', err);
      toast.error(`无法打开文件 ${filePath}: ${err?.message || String(err)}`);
    }
  };

  const handleJumpToCitation = async (filePath: string, startLine: number, endLine: number) => {
    await handleOpenFile(filePath);
    setJumpToRange({
      filePath,
      startLine,
      endLine,
      nonce: Date.now(),
    });
  };

  const handleCloseTab = (filePath: string, e?: React.MouseEvent) => {
    if (e) {
      e.preventDefault();
      e.stopPropagation();
    }

    const targetTab = tabs.find((t) => t.path === filePath);
    if (targetTab?.isDirty) {
      const confirmClose = window.confirm(
        `文件 "${targetTab.name}" 有未保存的修改，关闭将丢失这些修改。确定关闭吗？`
      );
      if (!confirmClose) return;
    }

    evictEditorStateCache(filePath);
    // Bump tabBarEpoch to force clean reconciliation even if duplicate keys existed prior to HMR
    setTabBarEpoch((n) => n + 1);

    setTabs((prev) => {
      const index = prev.findIndex((t) => t.path === filePath);
      const newTabs = prev.filter((t) => t.path !== filePath);

      setActiveTabPath((currentActive) => {
        if (currentActive === filePath) {
          if (newTabs.length > 0) {
            const safeIndex = index >= 0 ? Math.min(index, newTabs.length - 1) : 0;
            return newTabs[safeIndex].path;
          }
          return null;
        }
        return currentActive;
      });

      return newTabs;
    });
  };

  const handleActiveContentChange = (newContent: string) => {
    if (!activeTabPath) return;
    setTabs((prev) =>
      prev.map((t) =>
        t.path === activeTabPath && t.content !== newContent
          ? { ...t, content: newContent, isDirty: true }
          : t
      )
    );
  };


  const handleSaveActiveFile = async (latestContent?: string) => {
    if (!activeTab) return;
    const contentToSave = latestContent !== undefined ? latestContent : activeTab.content;
    try {
      await ApiService.saveFile(activeTab.path, contentToSave);
      setTabs((prev) =>
        prev.map((t) =>
          t.path === activeTab.path ? { ...t, content: contentToSave, isDirty: false } : t
        )
      );
      toast.success(`文件已保存: ${activeTab.name}`);
    } catch (err: any) {
      toast.error(`保存失败: ${err?.message || String(err)}`);
    }
  };

  const handleTriggerIndex = async () => {
    setIsIndexing(true);
    setIndexError(null);
    try {
      const map = await ApiService.indexProject();
      setProjectMap(map);
      setLastIndexedAt(new Date().toLocaleTimeString());
      toast.success(`工程索引完成：解析出 ${map.total_symbols} 个符号`);
      // Automatically generate deterministic fact & cloze exercises from project graph (Stage D)
      await ApiService.generateProjectExercises().catch(() => {});
      await loadDueReviews();
    } catch (err: any) {
      console.error('Index failed:', err);
      const msg = err?.message || String(err);
      setIndexError(msg);
      toast.error(`索引构建失败: ${msg}`);
    } finally {
      setIsIndexing(false);
    }
  };

  const handleExplainSelection = async (
    snippet: string,
    startLine: number,
    endLine: number,
    forceRefresh: boolean = false,
    depthMode: 'standard' | 'deep' = 'standard'
  ) => {
    if (!activeTab) return;
    setRightCollapsed(false);
    setActiveSidebarTab('cards');
    setIsExplaining(true);
    setExplainError(null);
    setLastExplainSelection({ snippet, startLine, endLine });

    try {
      // Stage C1/C5 structured pipeline (P0-P7 ContextAssembler + CitationVerifier + SQLite basis_hash cache)
      const structured = await ApiService.explainSelectionStructured(
        llmConfig,
        activeTab.path,
        snippet,
        startLine,
        endLine,
        forceRefresh,
        depthMode
      );

      setCards(structured.cards);
      setContextManifest(structured.manifest);
      setFromCache(structured.from_cache);
      setCurrentExplanationId(structured.explanation_id);
      if (structured.warning) {
        setExplainError(structured.warning);
      }
    } catch (err: any) {
      // Fallback to local template cards if structured command throws
      try {
        const fallbackCards = await ApiService.explainCode(
          activeTab.path,
          snippet,
          startLine,
          endLine
        );
        setCards(fallbackCards);
      } catch {
        // ignore
      }
      const msg = err?.message || String(err);
      setExplainError(`解读生成遇到异常: ${msg}`);
      toast.error(`解读生成失败: ${msg}`);
    } finally {
      setIsExplaining(false);
    }
  };

  const handleRetryExplain = (forceRefresh: boolean = true, depthMode: 'standard' | 'deep' = 'standard') => {
    if (lastExplainSelection) {
      handleExplainSelection(
        lastExplainSelection.snippet,
        lastExplainSelection.startLine,
        lastExplainSelection.endLine,
        forceRefresh,
        depthMode
      );
    }
  };

  const handleAskSelection = (snippet: string, startLine: number, endLine: number) => {
    if (!activeTab) return;
    setContextChip({
      file: activeTab.path,
      range: [startLine, endLine],
      snippet: snippet,
    });
    setRightCollapsed(false);
    setActiveSidebarTab('chat');
  };

  const handleLoadHistoryExplanation = (loadedCards: ExplanationCard[], manifest?: ContextManifest) => {
    setCards(loadedCards);
    if (manifest) setContextManifest(manifest);
    setFromCache(true);
  };

  const handleMarkConfusion = async (
    snippet: string,
    startLine: number,
    endLine: number,
    category: string,
    reexplainLevel: number = 1
  ) => {
    if (!activeTab) return;
    setRightCollapsed(false);
    setActiveSidebarTab('confusion');

    try {
      let rewording = await ApiService.markConfusion(
        activeTab.path,
        startLine,
        endLine,
        snippet,
        category,
        reexplainLevel
      );

      const hasKey = Boolean(
        llmConfig.has_api_key || llmConfig.api_key.trim().length > 5 || llmConfig.provider === 'ollama'
      );

      if (hasKey && !llmConfig.offline_mode) {
        const prompt = `用户在阅读文件 "${activeTab.path}" 第 ${startLine}-${endLine} 行代码时，针对【${category}】视角标记了「看不懂」（重讲等级 Level ${reexplainLevel}）：
\`\`\`${activeTab.language}
${snippet}
\`\`\`

请根据《NOESIS 代码深度导读专家》准则的分类降维策略为该代码片段进行针对性重讲（必须采用不同于原解释的全新类比和切入点）：
- overall（整体）：引入生活/工程实体系统类比（如物流分拣机、机场安检流）。
- logic（逻辑）：剥离多余代码，提供 3 行以内的最简最小可运行示例。
- syntax（语法）：用最原始的 if-else 或基础语句等价重写该语法糖。
- principle（底层原理）：指出作者最害怕什么物理代价（爆内存、高延迟、死锁？），反推设计动机。

请以纯 JSON 格式输出，不要有额外文本：
{
  "analogy": "生活或工程实体类比，或者设计动机反推",
  "minimal_example": "3行内极简示例或朴素等价重写",
  "prerequisite_concept": "前置基础概念",
  "guidance": "下一步阅读建议"
}`;

        try {
          const aiResponse = await ApiService.sendLlmChat(llmConfig, [
            { role: 'system', content: '你是一位精通启发式教学的代码深度导读专家。' },
            { role: 'user', content: prompt },
          ]);
          const extracted = extractJsonObject<Record<string, any>>(aiResponse);
          if (extracted.ok) {
            const parsed = extracted.data;
            rewording = {
              mark_id: rewording.mark_id,
              original_category: category,
              reexplain_level: reexplainLevel,
              analogy: parsed.analogy || rewording.analogy,
              minimal_example: parsed.minimal_example || rewording.minimal_example,
              prerequisite_concept: parsed.prerequisite_concept || rewording.prerequisite_concept,
              guidance: parsed.guidance || rewording.guidance,
              is_template: false,
            };

            // Persist AI rewording into SQLite marks table (A6)
            if (rewording.mark_id) {
              await ApiService.updateConfusionMarkExplanation(
                rewording.mark_id,
                JSON.stringify(rewording),
                llmConfig.model
              );
            }
          }
        } catch (chatErr: any) {
          setExplainError(`「看不懂」AI 重讲请求失败：${chatErr?.message || String(chatErr)}`);
        }
      }

      setActiveRewording(rewording);
      await loadConfusionMarks();
      toast.success('已标记代码卡点并生成针对性通俗重讲');
    } catch (err: any) {
      toast.error(`标记卡点失败: ${err?.message || String(err)}`);
    }
  };

  const handleEscalatedReexplain = async (mark: ConfusionMark, nextLevel: number) => {
    await handleOpenFile(mark.file_path);
    await handleMarkConfusion(
      mark.code_snippet,
      mark.line_start,
      mark.line_end,
      mark.category,
      nextLevel
    );
  };

  const loadConfusionMarks = async () => {
    try {
      const marks = await ApiService.getConfusionMarks();
      setConfusionMarks(marks);
    } catch (err: any) {
      console.error('Failed to load confusion marks:', err);
    }
  };

  const loadDueReviews = async () => {
    try {
      const reviews = await ApiService.listDueReviews();
      setDueReviews(reviews);
    } catch (err: any) {
      console.error('Failed to load due reviews:', err);
    }
  };

  const handleSendToScratch = (snippet: string, lang: string) => {
    setScratchCode(snippet);
    setScratchLanguage(lang);
    setShowScratch(true);
  };

  const handleRunScratchCode = async (req: RunRequest): Promise<RunResponse> => {
    return await ApiService.runScratchCode(req);
  };

  const handleSendChat = async (messages: ChatMessage[]): Promise<string> => {
    return await ApiService.sendLlmChat(llmConfig, messages);
  };

  const handlePreviewOutbound = async (messages: ChatMessage[]): Promise<OutboundPreview> => {
    return await ApiService.previewOutbound(llmConfig, messages);
  };

  const handleMarkCardFeedback = (_cardId: string, _type: 'helpful' | 'too_generic' | 'error') => {
    toast.success('卡片反馈已记录，已沉淀至本地评测基线！');
  };

  const isKeyConfigured = Boolean(llmConfig.has_api_key || llmConfig.api_key.trim().length > 5);

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        width: '100vw',
        height: '100vh',
        background: '#11111b',
        color: '#cdd6f4',
        overflow: 'hidden',
        fontSize: `${settings.uiFontSize}px`,
        userSelect: isResizing ? 'none' : 'auto',
      }}
    >
      {/* Top Application Bar */}
      <header
        style={{
          height: '42px',
          background: '#181825',
          borderBottom: '1px solid #313244',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '0 12px',
          userSelect: 'none',
          zIndex: 10,
        }}
      >
        {/* Left: Brand & Sidebar Toggles */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
          <button
            onClick={() => setLeftCollapsed(!leftCollapsed)}
            title={leftCollapsed ? '展开左侧工作区' : '折叠左侧工作区'}
            style={{
              background: 'transparent',
              border: 'none',
              color: '#a6adc8',
              cursor: 'pointer',
              padding: '3px',
              display: 'flex',
            }}
          >
            {leftCollapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
          </button>

          <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
            <span
              style={{
                background: 'linear-gradient(135deg, #89b4fa, #cba6f7)',
                color: '#11111b',
                fontWeight: 900,
                fontSize: '11px',
                padding: '2px 6px',
                borderRadius: '4px',
                letterSpacing: '1px',
              }}
            >
              NOESIS
            </span>
            <span style={{ fontSize: '13px', fontWeight: 600, color: '#cdd6f4' }}>
              代码学习型 IDE
            </span>
          </div>

          <div style={{ width: '1px', height: '14px', background: '#313244' }} />

          <span style={{ fontSize: '12px', color: '#6c7086' }}>
            {currentProject ? currentProject.name : '未加载工作区'}
          </span>
        </div>

        {/* Center: Save / Sandbox */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <button
            onClick={() => handleSaveActiveFile()}
            disabled={!activeTab}
            title="保存当前文件 (Ctrl+S)"
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              background: activeTab?.isDirty ? '#89b4fa' : '#313244',
              color: activeTab?.isDirty ? '#11111b' : '#cdd6f4',
              border: 'none',
              borderRadius: '4px',
              padding: '4px 10px',
              fontSize: '11.5px',
              fontWeight: 500,
              cursor: activeTab ? 'pointer' : 'default',
            }}
          >
            <Save size={13} />
            保存 {activeTab?.isDirty ? '*' : ''}
          </button>

          <button
            onClick={() => setShowScratch(!showScratch)}
            title="切换底部草稿沙盒"
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              background: showScratch ? 'rgba(137, 180, 250, 0.2)' : '#313244',
              color: showScratch ? '#89b4fa' : '#cdd6f4',
              border: '1px solid',
              borderColor: showScratch ? '#89b4fa' : 'transparent',
              borderRadius: '4px',
              padding: '4px 10px',
              fontSize: '11.5px',
              cursor: 'pointer',
              fontWeight: 500,
            }}
          >
            <Terminal size={13} />
            草稿沙盒
          </button>

          <button
            onClick={() => setShowScratch(true)}
            title="间隔复习题库与练习 (F7-g)"
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              background: dueReviews.length > 0 ? 'rgba(249, 226, 175, 0.18)' : '#313244',
              color: dueReviews.length > 0 ? 'var(--noesis-accent-yellow, #f9e2af)' : '#cdd6f4',
              border: '1px solid',
              borderColor: dueReviews.length > 0 ? 'var(--noesis-accent-yellow, #f9e2af)' : 'transparent',
              borderRadius: '4px',
              padding: '4px 8px',
              fontSize: '11.5px',
              cursor: 'pointer',
              fontWeight: 500,
            }}
          >
            <BookOpen size={13} />
            <span>复习练习</span>
            {dueReviews.length > 0 && (
              <span
                style={{
                  background: 'var(--noesis-accent-red, #f38ba8)',
                  color: '#11111b',
                  borderRadius: '10px',
                  padding: '0 5px',
                  fontSize: '10px',
                  fontWeight: 700,
                }}
              >
                {dueReviews.length}
              </span>
            )}
          </button>
        </div>

        {/* Right: Security Badge & Settings & Right Sidebar Toggle */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '5px',
              padding: '3px 8px',
              background: 'rgba(166, 227, 161, 0.1)',
              border: '1px solid rgba(166, 227, 161, 0.3)',
              borderRadius: '12px',
              color: '#a6e3a1',
              fontSize: '11px',
              fontWeight: 600,
            }}
          >
            <ShieldCheck size={13} />
            P0 外发控制生效中
          </div>

          <button
            onClick={() => setIsSettingsOpen(true)}
            title="系统偏好设置 (Ctrl+,)"
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              background: '#313244',
              color: '#cdd6f4',
              border: 'none',
              borderRadius: '4px',
              padding: '4px 8px',
              fontSize: '11.5px',
              cursor: 'pointer',
            }}
          >
            <SettingsIcon size={14} color="#89b4fa" />
            设置
          </button>

          <button
            onClick={() => setRightCollapsed(!rightCollapsed)}
            title={rightCollapsed ? '展开右侧 AI 导读' : '折叠右侧 AI 导读'}
            style={{
              background: 'transparent',
              border: 'none',
              color: '#a6adc8',
              cursor: 'pointer',
              padding: '3px',
              display: 'flex',
            }}
          >
            {rightCollapsed ? <PanelRightOpen size={16} /> : <PanelRightClose size={16} />}
          </button>
        </div>
      </header>

      {/* Main 3-Column Work Area with Resizers */}
      <div style={{ flex: 1, display: 'flex', overflow: 'hidden', position: 'relative' }}>
        {/* Left: Project Explorer & Map */}
        <div
          style={{
            width: `${leftWidth}px`,
            height: '100%',
            flexShrink: 0,
            overflow: 'hidden',
            display: leftCollapsed ? 'none' : 'block',
          }}
        >
          <ProjectSidebar
            currentProject={currentProject}
            projectMap={projectMap}
            activeFilePath={activeTabPath || ''}
            onSelectFile={handleOpenFile}
            onOpenProject={() => handleOpenProject()}
            onTriggerIndex={handleTriggerIndex}
            isIndexing={isIndexing}
            indexError={indexError}
            lastIndexedAt={lastIndexedAt}
          />
        </div>

        {/* Left Resizer Handle */}
        {!leftCollapsed && (
          <div
            onMouseDown={startResizeLeft}
            onDoubleClick={() => setLeftWidth(280)}
            title="双击恢复默认宽度 (280px)"
            style={{
              width: '4px',
              cursor: 'col-resize',
              background: isDraggingLeftRef.current ? '#89b4fa' : 'transparent',
              transition: 'background 0.15s ease',
              zIndex: 30,
              flexShrink: 0,
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = '#89b4fa')}
            onMouseLeave={(e) => {
              if (!isDraggingLeftRef.current) e.currentTarget.style.background = 'transparent';
            }}
          />
        )}

        {/* Center: Multi-Tab Editor + Bottom Docked Scratch Runner */}
        <div
          style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            height: '100%',
            overflow: 'hidden',
            minWidth: '200px',
          }}
        >
          {/* Multi-File Tab Bar */}
          <div
            key={`tab-bar-${tabBarEpoch}`}
            style={{
              height: '34px',
              background: '#181825',
              borderBottom: '1px solid #313244',
              display: 'flex',
              alignItems: 'center',
              overflowX: 'auto',
              padding: '0 4px',
              userSelect: 'none',
            }}
          >
            {tabs.map((tab, idx) => {
              const isActive = tab.path === activeTabPath;
              const isMarkdown = tab.language === 'markdown';
              return (
                <div
                  key={`${tab.path}::${idx}`}
                  onClick={() => setActiveTabPath(tab.path)}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '6px',
                    padding: '6px 10px',
                    background: isActive ? '#1e1e2e' : 'transparent',
                    borderRight: '1px solid #313244',
                    borderTop: isActive ? '2px solid #89b4fa' : '2px solid transparent',
                    color: isActive ? '#cdd6f4' : '#6c7086',
                    cursor: 'pointer',
                    fontSize: '12px',
                    whiteSpace: 'nowrap',
                    borderRadius: '4px 4px 0 0',
                    transition: 'all 0.1s ease',
                  }}
                  onMouseEnter={(e) => {
                    if (!isActive) e.currentTarget.style.color = '#cdd6f4';
                  }}
                  onMouseLeave={(e) => {
                    if (!isActive) e.currentTarget.style.color = '#6c7086';
                  }}
                >
                  {isMarkdown ? (
                    <FileText size={13} color="#fab387" />
                  ) : (
                    <FileCode size={13} color="#a6e3a1" />
                  )}
                  <span>{tab.name}</span>

                  {tab.isDirty && (
                    <span
                      style={{
                        width: '6px',
                        height: '6px',
                        borderRadius: '50%',
                        backgroundColor: '#fab387',
                        marginLeft: '2px',
                      }}
                      title="有未保存修改"
                    />
                  )}

                  <button
                    onClick={(e) => handleCloseTab(tab.path, e)}
                    title="关闭文件"
                    style={{
                      background: 'transparent',
                      border: 'none',
                      color: '#6c7086',
                      borderRadius: '3px',
                      padding: '2px',
                      cursor: 'pointer',
                      display: 'flex',
                      alignItems: 'center',
                      marginLeft: '4px',
                    }}
                    onMouseEnter={(e) => (e.currentTarget.style.color = '#f38ba8')}
                    onMouseLeave={(e) => (e.currentTarget.style.color = '#6c7086')}
                  >
                    <X size={12} />
                  </button>
                </div>
              );
            })}

            {tabs.length === 0 && (
              <span style={{ fontSize: '11px', color: '#6c7086', padding: '0 8px' }}>
                无已打开的文件
              </span>
            )}
          </div>

          {/* Active Tab CodeMirror 6 Editor Container */}
          <div style={{ flex: 1, position: 'relative', overflow: 'hidden' }}>
            {activeTab ? (
              <CodeEditor
                filePath={activeTab.path}
                initialContent={activeTab.content}
                language={activeTab.language}
                fontSize={settings.editorFontSize}
                fontFamily={settings.fontFamily}
                wordWrap={settings.wordWrap}
                jumpToRange={jumpToRange}
                onContentChange={handleActiveContentChange}
                onSave={handleSaveActiveFile}
                onExplainSelection={handleExplainSelection}
                onMarkConfusion={handleMarkConfusion}
                onAskSelection={handleAskSelection}
                onSendToScratch={handleSendToScratch}
              />
            ) : (
              <div
                style={{
                  height: '100%',
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'center',
                  justifyContent: 'center',
                  color: '#6c7086',
                  fontSize: '13px',
                  gap: '12px',
                }}
              >
                <Code size={40} style={{ opacity: 0.3 }} />
                <span>从左侧文件树点击打开代码文件，或导入新项目</span>
              </div>
            )}
          </div>

          {/* Bottom Docked Scratch Runner (G6 Draggable Drawer) */}
          {showScratch && (
            <div style={{ display: 'flex', flexDirection: 'column', flexShrink: 0 }}>
              <div
                onPointerDown={handleDrawerPointerDown}
                onPointerMove={handleDrawerPointerMove}
                onPointerUp={handleDrawerPointerUp}
                onDoubleClick={handleDrawerDoubleClick}
                style={{
                  height: '4px',
                  cursor: 'row-resize',
                  background: isDraggingDrawer ? '#89b4fa' : '#313244',
                  transition: 'background 0.15s ease',
                  zIndex: 25,
                  userSelect: 'none',
                }}
                onMouseEnter={(e) => (e.currentTarget.style.background = '#89b4fa')}
                onMouseLeave={(e) => {
                  if (!isDraggingDrawer) e.currentTarget.style.background = '#313244';
                }}
                title="拖拽调节高度 (120px - 窗口高度-160px)，双击循环切换 120px / 280px / 最大高度"
              />
              <div style={{ height: `${drawerHeight}px`, overflow: 'hidden' }}>
                <ScratchRunner
                  initialCode={scratchCode}
                  initialLanguage={scratchLanguage}
                  onRunCode={handleRunScratchCode}
                  dueReviews={dueReviews}
                  onClose={() => setShowScratch(false)}
                />
              </div>
            </div>
          )}
        </div>

        {/* Right Resizer Handle */}
        {!rightCollapsed && (
          <div
            onMouseDown={startResizeRight}
            onDoubleClick={() => setRightWidth(380)}
            title="双击恢复默认宽度 (380px)"
            style={{
              width: '4px',
              cursor: 'col-resize',
              background: isDraggingRightRef.current ? '#89b4fa' : 'transparent',
              transition: 'background 0.15s ease',
              zIndex: 30,
              flexShrink: 0,
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = '#89b4fa')}
            onMouseLeave={(e) => {
              if (!isDraggingRightRef.current) e.currentTarget.style.background = 'transparent';
            }}
          />
        )}

        {/* Right: AI Guidance, 4-Perspective Cards, Clarifications, and Chat */}
        <div
          style={{
            width: `${rightWidth}px`,
            height: '100%',
            flexShrink: 0,
            overflow: 'hidden',
            display: rightCollapsed ? 'none' : 'block',
          }}
        >
          <AiSidebar
            cards={cards}
            contextManifest={contextManifest}
            fromCache={fromCache}
            explainError={explainError}
            onRetryExplain={handleRetryExplain}
            onJumpToCitation={handleJumpToCitation}
            confusionMarks={confusionMarks}
            onRefreshConfusionMarks={loadConfusionMarks}
            onEscalatedReexplain={handleEscalatedReexplain}
            activeRewording={activeRewording}
            llmConfig={llmConfig}
            isExplaining={isExplaining}
            activeSidebarTab={activeSidebarTab}
            onUpdateLlmConfig={updateLlmConfig}
            onClearApiKey={handleClearApiKey}
            onSendChat={handleSendChat}
            onPreviewOutbound={handlePreviewOutbound}
            onMarkCardFeedback={handleMarkCardFeedback}
            onLoadHistoryExplanation={handleLoadHistoryExplanation}
            contextChip={contextChip}
            onClearContextChip={() => setContextChip(null)}
            currentExplanationId={currentExplanationId}
          />
        </div>
      </div>

      {/* Bottom Status Bar */}
      <footer
        style={{
          height: '24px',
          background: '#11111b',
          borderTop: '1px solid #313244',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '0 12px',
          fontSize: '11px',
          color: '#6c7086',
          userSelect: 'none',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
          <span style={{ display: 'flex', alignItems: 'center', gap: '4px', color: '#a6e3a1' }}>
            <span style={{ width: '6px', height: '6px', borderRadius: '50%', background: '#a6e3a1' }} />
            Rust 内核 (Tauri 2) 在线
          </span>
          {systemStatus.db_degraded && (
            <span
              title={systemStatus.db_error || '数据库降级为内存模式'}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '4px',
                color: '#f38ba8',
                fontWeight: 600,
              }}
            >
              <AlertTriangle size={12} />
              数据库不可用，学习记录不会保存
            </span>
          )}
          <span
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              color: isKeyConfigured ? '#a6e3a1' : '#f9e2af',
            }}
          >
            <span
              style={{
                width: '6px',
                height: '6px',
                borderRadius: '50%',
                background: isKeyConfigured ? '#a6e3a1' : '#f9e2af',
              }}
            />
            {isKeyConfigured
              ? `${llmConfig.provider}: ${llmConfig.model} (${llmConfig.api_key_masked || '已配置密钥'})`
              : `${llmConfig.provider}: 未配置密钥 (模板模式)`}
          </span>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
          <span>标签页: {tabs.length} 个打开</span>
          <span>字号: {settings.editorFontSize}px</span>
          <span>编码: UTF-8</span>
          <span>语言: {activeTab?.language.toUpperCase() || '无'}</span>
          <span>{projectMap ? `已索引 ${projectMap.total_symbols} 个符号` : '准备就绪'}</span>
        </div>
      </footer>

      {/* Global Settings Modal */}
      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        settings={settings}
        onUpdateSettings={updateSettings}
        llmConfig={llmConfig}
        onUpdateLlmConfig={updateLlmConfig}
        onClearApiKey={handleClearApiKey}
        onResetLayout={resetLayout}
      />
    </div>
  );
};
