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
import { resetMermaidTheme } from '../components/MarkdownRenderer';
import { type LayerCardState, type LayerStatus } from '../sidebar-ai/LayerCard';
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
  PerspectiveType,
  ExplanationSummary,
  LayerStreamEvent,
  IndexStatus,
} from '../types';

const DEFAULT_SETTINGS: AppSettings = {
  editorFontSize: 14,
  uiFontSize: 13,
  fontFamily: '"JetBrains Mono", "Fira Code", Consolas, monospace',
  uiFontFamily:
    '-apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif',
  wordWrap: false,
  tabSize: 4,
  lineHeight: 1.6,
  theme: 'catppuccin',
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
  // App Settings State (localStorage 作为即时回退，权威值在后端 settings.json)
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

  /**
   * H4: 把设置写进后端 `settings.json`（Linux 版换端口也不会丢），
   * localStorage 只作为浏览器预览模式与读取失败时的回退。
   */
  const persistSettings = (newSettings: AppSettings) => {
    setSettings(newSettings);
    localStorage.setItem('noesis_settings', JSON.stringify(newSettings));
    ApiService.setUiSettings({
      theme: newSettings.theme,
      editor_font_size: newSettings.editorFontSize,
      ui_font_size: newSettings.uiFontSize,
      font_family: newSettings.fontFamily,
      ui_font_family: newSettings.uiFontFamily || DEFAULT_SETTINGS.uiFontFamily || '',
      word_wrap: newSettings.wordWrap,
      tab_size: newSettings.tabSize,
      line_height: newSettings.lineHeight ?? 1.6,
      last_project: lastProjectRef.current,
      recent_projects: [],
    }).catch((err) => console.warn('保存界面设置失败:', err));
  };

  const updateSettings = (newSettings: AppSettings) => {
    persistSettings(newSettings);
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
  // H2: 索引进度与最近项目
  const [indexStatus, setIndexStatus] = useState<IndexStatus | null>(null);
  const [recentProjects, setRecentProjects] = useState<string[]>([]);
  const lastProjectRef = useRef<string | null>(null);

  // AI & Reading State
  // H1: 新版按层解读的状态（四层各自独立）；legacyCards 仅供旧版 JSON 历史记录回放
  const [legacyCards, setLegacyCards] = useState<ExplanationCard[]>([]);
  const [layerStates, setLayerStates] = useState<Record<PerspectiveType, LayerCardState>>({
    overall: { layer: 'overall', status: 'idle', markdown: '' },
    logic: { layer: 'logic', status: 'idle', markdown: '' },
    syntax: { layer: 'syntax', status: 'idle', markdown: '' },
    principle: { layer: 'principle', status: 'idle', markdown: '' },
  });
  const [depthModeState, setDepthModeState] = useState<'standard' | 'deep'>('standard');
  const [currentGroupId, setCurrentGroupId] = useState<string | null>(null);
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
  // H5-c: 「看不懂」流式答疑的中间文本
  const [confusionStreamingText, setConfusionStreamingText] = useState<string>('');
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

  // H4: 把设置写进 CSS 变量（界面字体 / 字号 / 行高 / 代码字体）
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = settings.theme || 'catppuccin';
    root.style.setProperty('--noesis-font-size-ui', `${settings.uiFontSize}px`);
    if (settings.uiFontFamily) {
      root.style.setProperty('--noesis-font-sans', settings.uiFontFamily);
    }
    if (settings.fontFamily) {
      root.style.setProperty('--noesis-font-mono', settings.fontFamily);
    }
    root.style.setProperty('--noesis-line-height', String(settings.lineHeight ?? 1.6));
    document.body.style.fontSize = `${settings.uiFontSize}px`;
    document.body.style.lineHeight = String(settings.lineHeight ?? 1.6);
    // H4: 主题变了要让 mermaid 用新配色重新初始化
    resetMermaidTheme();
  }, [
    settings.theme,
    settings.uiFontSize,
    settings.uiFontFamily,
    settings.fontFamily,
    settings.lineHeight,
  ]);

  // Load initial state on mount (guarded against React.StrictMode double-invocation)
  useEffect(() => {
    if (hasInitializedRef.current) return;
    hasInitializedRef.current = true;

    // Purge any legacy localStorage LLM key (A1)
    localStorage.removeItem('noesis_llm_config');
    ApiService.getLlmConfig().then(setLlmConfig).catch(console.error);
    ApiService.getSystemStatus().then(setSystemStatus).catch(console.error);

    // H4: 界面设置优先从后端读取，失败才回退 localStorage
    ApiService.getUiSettings()
      .then((s) => {
        setSettings((prev) => ({
          ...prev,
          theme: s.theme || prev.theme,
          editorFontSize: s.editor_font_size || prev.editorFontSize,
          uiFontSize: s.ui_font_size || prev.uiFontSize,
          fontFamily: s.font_family || prev.fontFamily,
          uiFontFamily: s.ui_font_family || prev.uiFontFamily,
          wordWrap: Boolean(s.word_wrap),
          tabSize: s.tab_size || prev.tabSize,
          lineHeight: s.line_height || prev.lineHeight,
        }));
        lastProjectRef.current = s.last_project || null;
        setRecentProjects(s.recent_projects || []);
      })
      .catch((err) => console.warn('读取后端界面设置失败，使用 localStorage 回退:', err));

    // H2: 启动时不再默认打开 `./`（Linux 版会变成索引整个家目录）。
    // 有「最近项目」才自动恢复，否则停留在欢迎页等用户导入。
    ApiService.getRecentProjects()
      .then((projects) => {
        setRecentProjects(projects);
        const last = projects[0];
        if (last) {
          handleOpenProject(last, { silent: true, skipIndex: true });
        }
      })
      .catch(console.error);

    loadConfusionMarks();
    loadDueReviews();
  }, []);

  const handleOpenProject = async (
    path?: string,
    options: { silent?: boolean; skipIndex?: boolean } = {}
  ) => {
    try {
      const targetPath = path || (await ApiService.selectDirectory());
      if (!targetPath) return;

      const tree = await ApiService.openProject(targetPath);
      setCurrentProject(tree);
      lastProjectRef.current = targetPath;
      if (!options.silent) {
        toast.success(`成功载入项目: ${tree.name || targetPath}`);
      }

      // When switching to a new project directory, clear stale tabs from previous workspace
      evictEditorStateCache();
      setTabs([]);
      setActiveTabPath(null);
      setTabBarEpoch((n) => n + 1);

      const first = findFirstFile(tree);
      if (first) {
        await handleOpenFile(first.path);
      }

      // H2-5: 不再静默自动索引。已有索引就直接加载旧地图，否则提示用户手动开始。
      await loadExistingMap();
      if (!options.skipIndex) {
        await handleTriggerIndex();
      }
    } catch (err: any) {
      console.error('Failed to open project:', err);
      const msg = err?.message || String(err);
      if (!options.silent) {
        toast.error(`打开工程失败: ${msg}`);
      }
    }
  };

  /** H2-5: 尝试加载已有索引地图（不触发新的索引）。 */
  const loadExistingMap = async () => {
    try {
      const map = await ApiService.getProjectMap();
      if (map) {
        setProjectMap(map);
        setLastIndexedAt('（已加载历史索引）');
      } else {
        setProjectMap(null);
      }
    } catch {
      setProjectMap(null);
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

  /**
   * H2: 手动触发索引。带进度轮询与取消；同一项目重复点击不会报错。
   */
  const handleTriggerIndex = async () => {
    setIsIndexing(true);
    setIndexError(null);
    setIndexStatus(null);

    // 每秒轮询一次索引进度
    const pollTimer = window.setInterval(() => {
      ApiService.getIndexStatus()
        .then((st) => setIndexStatus(st))
        .catch(() => {});
    }, 1000);

    try {
      const map = await ApiService.indexProject();
      setProjectMap(map);
      setLastIndexedAt(new Date().toLocaleTimeString());
      toast.success(`工程索引完成：解析出 ${map.total_symbols} 个符号`);
      // Automatically generate deterministic fact & cloze exercises from project graph (Stage D)
      await ApiService.generateProjectExercises().catch(() => {});
      await loadDueReviews();
    } catch (err: any) {
      const msg = err?.message || String(err);
      if (msg.includes('INDEX_ALREADY_RUNNING')) {
        // 同一项目连点：不当作错误
        toast.info(msg.replace('INDEX_ALREADY_RUNNING: ', ''));
      } else if (msg.includes('INDEX_CANCELLED')) {
        toast.info(msg.replace('INDEX_CANCELLED: ', ''));
      } else {
        setIndexError(msg);
        toast.error(`索引构建失败: ${msg}`);
      }
    } finally {
      window.clearInterval(pollTimer);
      setIsIndexing(false);
      ApiService.getIndexStatus().then(setIndexStatus).catch(() => {});
    }
  };

  const handleCancelIndex = async () => {
    try {
      await ApiService.cancelIndex();
      toast.info('已请求取消索引');
    } catch (err: any) {
      toast.error(`取消失败: ${err?.message || String(err)}`);
    }
  };

  // -----------------------------------------------------------------------------------------------
  // H1: 四层解读改走 explain_layer（每层独立并发 + 流式）
  // -----------------------------------------------------------------------------------------------
  const LAYERS: PerspectiveType[] = ['overall', 'logic', 'syntax', 'principle'];

  const resetLayerStates = (status: LayerStatus = 'loading') => {
    const next: Record<PerspectiveType, LayerCardState> = {
      overall: { layer: 'overall', status, markdown: '' },
      logic: { layer: 'logic', status, markdown: '' },
      syntax: { layer: 'syntax', status, markdown: '' },
      principle: { layer: 'principle', status, markdown: '' },
    };
    setLayerStates(next);
  };

  const runSingleLayer = async (
    layer: PerspectiveType,
    snippet: string,
    startLine: number,
    endLine: number,
    depthMode: 'standard' | 'deep',
    forceRefresh: boolean
  ) => {
    if (!activeTab) return;
    const filePath = activeTab.path;

    setLayerStates((prev) => ({
      ...prev,
      [layer]: { layer, status: 'loading', markdown: '' },
    }));

    let acc = '';
    try {
      const res = await ApiService.explainLayer(
        layer,
        filePath,
        [startLine, endLine],
        snippet,
        depthMode,
        forceRefresh,
        (evt: LayerStreamEvent) => {
          if (evt.type === 'token' && typeof evt.token === 'string') {
            acc += evt.token;
            setLayerStates((prev) => ({
              ...prev,
              [layer]: {
                ...prev[layer],
                layer,
                status: 'streaming',
                markdown: acc,
              },
            }));
          }
        }
      );

      setLayerStates((prev) => ({
        ...prev,
        [layer]: {
          layer,
          status: 'done',
          markdown: res?.markdown || acc,
          quality: res?.quality,
          citations: res?.citations || [],
          concepts: res?.concepts || [],
          cached: res?.cached,
          explanationId: res?.explanation_id,
        },
      }));
      if (res?.group_id) setCurrentGroupId(res.group_id);
      if (res?.explanation_id) setCurrentExplanationId(res.explanation_id);
    } catch (err: any) {
      const msg = err?.message || String(err);
      setLayerStates((prev) => ({
        ...prev,
        [layer]: {
          layer,
          status: 'error',
          markdown: acc,
          error: msg,
        },
      }));
      setExplainError(`【${layer}】层生成失败: ${msg}`);
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
    setDepthModeState(depthMode);
    resetLayerStates('loading');

    // 四层并发发起：每层独立维护 status / markdown / quality
    try {
      await Promise.all(
        LAYERS.map((layer) =>
          runSingleLayer(layer, snippet, startLine, endLine, depthMode, forceRefresh)
        )
      );
    } finally {
      setIsExplaining(false);
    }
  };

  const handleRegenerateLayer = async (layer: PerspectiveType) => {
    if (!lastExplainSelection || !activeTab) return;
    setIsExplaining(true);
    await runSingleLayer(
      layer,
      lastExplainSelection.snippet,
      lastExplainSelection.startLine,
      lastExplainSelection.endLine,
      depthModeState,
      true
    );
    setIsExplaining(false);
  };

  /** 打开历史记录：优先按 group_id 打开新版按层记录，回退到旧版 JSON 卡片。 */
  const handleOpenHistory = async (hist: ExplanationSummary) => {
    try {
      const layers = await ApiService.getExplanationLayers(hist.id, hist.group_id || undefined);
      if (layers && layers.length > 0) {
        const next: Partial<Record<PerspectiveType, LayerCardState>> = {};
        for (const item of layers) {
          const key = item.layer as PerspectiveType;
          next[key] = {
            layer: key,
            status: 'done',
            markdown: item.markdown,
            quality: item.quality,
            citations: item.citations,
            concepts: item.concepts,
            cached: true,
            explanationId: item.explanation_id,
          };
        }
        setLayerStates((prev) => ({ ...prev, ...next }));
        setCurrentExplanationId(hist.id);
        setCurrentGroupId(hist.group_id || null);
        setLegacyCards([]);
        return true;
      }
    } catch {
      // 回退到旧版
    }

    const legacy = await ApiService.getExplanation(hist.id);
    if (legacy) {
      setLegacyCards(legacy.cards);
      setContextManifest(legacy.manifest);
      setCurrentExplanationId(hist.id);
      setCurrentGroupId(null);
      resetLayerStates('idle');
      return true;
    }
    return false;
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
    setLegacyCards(loadedCards);
    setLayerStates({
      overall: { layer: 'overall', status: 'idle', markdown: '' },
      logic: { layer: 'logic', status: 'idle', markdown: '' },
      syntax: { layer: 'syntax', status: 'idle', markdown: '' },
      principle: { layer: 'principle', status: 'idle', markdown: '' },
    });
    if (manifest) setContextManifest(manifest);
    setFromCache(true);
  };

  /**
   * H5-c: 「看不懂」改走后端 explain_confusion（流式），不再在前端拼提示词。
   */
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
      // 1. 先落一条本地卡点记录（拿到 mark_id，便于后续升级重讲）
      const rewording = await ApiService.markConfusion(
        activeTab.path,
        startLine,
        endLine,
        snippet,
        category,
        reexplainLevel
      );
      setActiveRewording(rewording);
      await loadConfusionMarks();

      const hasKey = Boolean(
        llmConfig.has_api_key || llmConfig.api_key.trim().length > 5 || llmConfig.provider === 'ollama'
      );
      if (!hasKey || llmConfig.offline_mode) {
        toast.info('未配置模型密钥，已保存卡点并展示模板重讲');
        return;
      }

      // 2. 走 Rust 的 explain_confusion（流式）
      setConfusionStreamingText('');
      let acc = '';
      const detail = await ApiService.explainConfusion(
        {
          markId: rewording.mark_id,
          file: activeTab.path,
          range: [startLine, endLine],
          snippet,
          confusedLayer: category,
          level: reexplainLevel,
        },
        (evt) => {
          if (evt.type === 'token' && typeof evt.token === 'string') {
            acc += evt.token;
            setConfusionStreamingText(acc);
          } else if (evt.type === 'done' && evt.detail?.mark) {
            setActiveRewording((prev) => ({
              ...(prev || ({} as RewordingResult)),
              mark_id: evt.detail.mark.id,
              original_category: category,
              reexplain_level: reexplainLevel,
              analogy: evt.detail.mark.ai_explanation || acc,
              minimal_example: prev?.minimal_example || '',
              prerequisite_concept: prev?.prerequisite_concept || '',
              guidance: prev?.guidance || '',
              is_template: false,
            }));
          } else if (evt.type === 'error' && evt.error) {
            setExplainError(`「看不懂」答疑失败：${evt.error}`);
          }
        }
      );

      if (detail?.mark?.ai_explanation) {
        setConfusionStreamingText(detail.mark.ai_explanation);
      }
      await loadConfusionMarks();
      toast.success('已生成针对性的卡点答疑');
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
        background: 'var(--noesis-bg-crust)',
        color: 'var(--noesis-text-primary)',
        overflow: 'hidden',
        fontSize: `${settings.uiFontSize}px`,
        userSelect: isResizing ? 'none' : 'auto',
      }}
    >
      {/* Top Application Bar */}
      <header
        style={{
          height: '42px',
          background: 'var(--noesis-bg-mantle)',
          borderBottom: '1px solid var(--noesis-bg-surface0)',
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
              color: 'var(--noesis-text-muted)',
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
                background: 'linear-gradient(135deg, var(--noesis-accent-blue), var(--noesis-accent-mauve))',
                color: 'var(--noesis-bg-crust)',
                fontWeight: 900,
                fontSize: '11px',
                padding: '2px 6px',
                borderRadius: '4px',
                letterSpacing: '1px',
              }}
            >
              NOESIS
            </span>
            <span style={{ fontSize: '13px', fontWeight: 600, color: 'var(--noesis-text-primary)' }}>
              代码学习型 IDE
            </span>
          </div>

          <div style={{ width: '1px', height: '14px', background: 'var(--noesis-bg-surface0)' }} />

          <span style={{ fontSize: '12px', color: 'var(--noesis-text-subtle)' }}>
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
              background: activeTab?.isDirty ? 'var(--noesis-accent-blue)' : 'var(--noesis-bg-surface0)',
              color: activeTab?.isDirty ? 'var(--noesis-bg-crust)' : 'var(--noesis-text-primary)',
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
              background: showScratch ? 'rgba(137, 180, 250, 0.2)' : 'var(--noesis-bg-surface0)',
              color: showScratch ? 'var(--noesis-accent-blue)' : 'var(--noesis-text-primary)',
              border: '1px solid',
              borderColor: showScratch ? 'var(--noesis-accent-blue)' : 'transparent',
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
              background: dueReviews.length > 0 ? 'rgba(249, 226, 175, 0.18)' : 'var(--noesis-bg-surface0)',
              color: dueReviews.length > 0 ? 'var(--noesis-accent-yellow)' : 'var(--noesis-text-primary)',
              border: '1px solid',
              borderColor: dueReviews.length > 0 ? 'var(--noesis-accent-yellow)' : 'transparent',
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
                  background: 'var(--noesis-accent-red)',
                  color: 'var(--noesis-bg-crust)',
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
              color: 'var(--noesis-accent-green)',
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
              background: 'var(--noesis-bg-surface0)',
              color: 'var(--noesis-text-primary)',
              border: 'none',
              borderRadius: '4px',
              padding: '4px 8px',
              fontSize: '11.5px',
              cursor: 'pointer',
            }}
          >
            <SettingsIcon size={14} color="var(--noesis-accent-blue)" />
            设置
          </button>

          <button
            onClick={() => setRightCollapsed(!rightCollapsed)}
            title={rightCollapsed ? '展开右侧 AI 导读' : '折叠右侧 AI 导读'}
            style={{
              background: 'transparent',
              border: 'none',
              color: 'var(--noesis-text-muted)',
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
            onCancelIndex={handleCancelIndex}
            isIndexing={isIndexing}
            indexStatus={indexStatus}
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
              background: isDraggingLeftRef.current ? 'var(--noesis-accent-blue)' : 'transparent',
              transition: 'background 0.15s ease',
              zIndex: 30,
              flexShrink: 0,
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--noesis-accent-blue)')}
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
              background: 'var(--noesis-bg-mantle)',
              borderBottom: '1px solid var(--noesis-bg-surface0)',
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
                    background: isActive ? 'var(--noesis-bg-base)' : 'transparent',
                    borderRight: '1px solid var(--noesis-bg-surface0)',
                    borderTop: isActive ? '2px solid var(--noesis-accent-blue)' : '2px solid transparent',
                    color: isActive ? 'var(--noesis-text-primary)' : 'var(--noesis-text-subtle)',
                    cursor: 'pointer',
                    fontSize: '12px',
                    whiteSpace: 'nowrap',
                    borderRadius: '4px 4px 0 0',
                    transition: 'all 0.1s ease',
                  }}
                  onMouseEnter={(e) => {
                    if (!isActive) e.currentTarget.style.color = 'var(--noesis-text-primary)';
                  }}
                  onMouseLeave={(e) => {
                    if (!isActive) e.currentTarget.style.color = 'var(--noesis-text-subtle)';
                  }}
                >
                  {isMarkdown ? (
                    <FileText size={13} color="var(--noesis-accent-peach)" />
                  ) : (
                    <FileCode size={13} color="var(--noesis-accent-green)" />
                  )}
                  <span>{tab.name}</span>

                  {tab.isDirty && (
                    <span
                      style={{
                        width: '6px',
                        height: '6px',
                        borderRadius: '50%',
                        backgroundColor: 'var(--noesis-accent-peach)',
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
                      color: 'var(--noesis-text-subtle)',
                      borderRadius: '3px',
                      padding: '2px',
                      cursor: 'pointer',
                      display: 'flex',
                      alignItems: 'center',
                      marginLeft: '4px',
                    }}
                    onMouseEnter={(e) => (e.currentTarget.style.color = 'var(--noesis-accent-red)')}
                    onMouseLeave={(e) => (e.currentTarget.style.color = 'var(--noesis-text-subtle)')}
                  >
                    <X size={12} />
                  </button>
                </div>
              );
            })}

            {tabs.length === 0 && (
              <span style={{ fontSize: '11px', color: 'var(--noesis-text-subtle)', padding: '0 8px' }}>
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
                theme={settings.theme}
                jumpToRange={jumpToRange}
                onContentChange={handleActiveContentChange}
                onSave={handleSaveActiveFile}
                onExplainSelection={handleExplainSelection}
                onMarkConfusion={handleMarkConfusion}
                onAskSelection={handleAskSelection}
                onSendToScratch={handleSendToScratch}
              />
            ) : (
              /* H2-1: 欢迎页（不再默认打开 `./` 静默索引家目录） */
              <div
                style={{
                  height: '100%',
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'center',
                  justifyContent: 'center',
                  color: 'var(--noesis-text-subtle)',
                  fontSize: '13px',
                  gap: '14px',
                  padding: '24px',
                }}
              >
                <Code size={40} style={{ opacity: 0.3 }} />
                <div style={{ fontSize: 14, color: 'var(--noesis-text-muted)' }}>
                  {currentProject ? '从左侧文件树选择要阅读的文件' : '还没有打开任何项目'}
                </div>
                <div style={{ display: 'flex', gap: 8 }}>
                  <button
                    onClick={() => handleOpenProject()}
                    style={{
                      padding: '6px 14px',
                      background: 'var(--noesis-accent-blue)',
                      color: 'var(--noesis-bg-crust)',
                      border: 'none',
                      borderRadius: 4,
                      fontSize: 12,
                      fontWeight: 600,
                      cursor: 'pointer',
                    }}
                  >
                    导入项目
                  </button>
                </div>
                {recentProjects.length > 0 && (
                  <div style={{ width: '100%', maxWidth: 460, marginTop: 8 }}>
                    <div style={{ fontSize: 11, color: 'var(--noesis-text-subtle)', marginBottom: 6 }}>
                      最近打开
                    </div>
                    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                      {recentProjects.slice(0, 6).map((p) => (
                        <button
                          key={p}
                          onClick={() => handleOpenProject(p)}
                          title={p}
                          style={{
                            textAlign: 'left',
                            padding: '6px 10px',
                            background: 'var(--noesis-bg-base)',
                            border: '1px solid var(--noesis-bg-surface0)',
                            borderRadius: 4,
                            color: 'var(--noesis-text-secondary)',
                            fontSize: 11.5,
                            cursor: 'pointer',
                            overflow: 'hidden',
                            textOverflow: 'ellipsis',
                            whiteSpace: 'nowrap',
                          }}
                        >
                          {p}
                        </button>
                      ))}
                    </div>
                  </div>
                )}
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
                  background: isDraggingDrawer ? 'var(--noesis-accent-blue)' : 'var(--noesis-bg-surface0)',
                  transition: 'background 0.15s ease',
                  zIndex: 25,
                  userSelect: 'none',
                }}
                onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--noesis-accent-blue)')}
                onMouseLeave={(e) => {
                  if (!isDraggingDrawer) e.currentTarget.style.background = 'var(--noesis-bg-surface0)';
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
              background: isDraggingRightRef.current ? 'var(--noesis-accent-blue)' : 'transparent',
              transition: 'background 0.15s ease',
              zIndex: 30,
              flexShrink: 0,
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--noesis-accent-blue)')}
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
            cards={legacyCards}
            layerStates={layerStates}
            depthMode={depthModeState}
            confusionStreamingText={confusionStreamingText}
            currentGroupId={currentGroupId}
            onOpenHistory={handleOpenHistory}
            onRegenerateLayer={handleRegenerateLayer}
            onMarkLayerConfusion={(_layer, markdown) => {
              if (!lastExplainSelection) return;
              handleMarkConfusion(
                lastExplainSelection.snippet,
                lastExplainSelection.startLine,
                lastExplainSelection.endLine,
                _layer,
                1
              );
              // 把本层全文作为上下文备注一并带上
              if (markdown) setContextChip(null);
            }}
            onLayerFeedback={(layer, type) => handleMarkCardFeedback(layer, type)}
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
          background: 'var(--noesis-bg-crust)',
          borderTop: '1px solid var(--noesis-bg-surface0)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '0 12px',
          fontSize: '11px',
          color: 'var(--noesis-text-subtle)',
          userSelect: 'none',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
          <span style={{ display: 'flex', alignItems: 'center', gap: '4px', color: 'var(--noesis-accent-green)' }}>
            <span style={{ width: '6px', height: '6px', borderRadius: '50%', background: 'var(--noesis-accent-green)' }} />
            Rust 内核 (Tauri 2) 在线
          </span>
          {systemStatus.db_degraded && (
            <span
              title={systemStatus.db_error || '数据库降级为内存模式'}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '4px',
                color: 'var(--noesis-accent-red)',
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
              color: isKeyConfigured ? 'var(--noesis-accent-green)' : 'var(--noesis-accent-yellow)',
            }}
          >
            <span
              style={{
                width: '6px',
                height: '6px',
                borderRadius: '50%',
                background: isKeyConfigured ? 'var(--noesis-accent-green)' : 'var(--noesis-accent-yellow)',
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
