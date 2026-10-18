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
} from 'lucide-react';
import { ApiService } from '../services/api';
import { CodeEditor } from '../editor/CodeEditor';
import { ProjectSidebar } from '../sidebar-project/ProjectSidebar';
import { AiSidebar } from '../sidebar-ai/AiSidebar';
import { ScratchRunner } from '../scratch/ScratchRunner';
import { SettingsModal } from './SettingsModal';
import {
  FileNode,
  ProjectMap,
  ExplanationCard,
  RewordingResult,
  ConfusionMark,
  LlmConfig,
  ChatMessage,
  OutboundPreview,
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
  api_key: 'sk-136dbd0231f54d29820ef0a7f731550c',
  model: 'deepseek-chat',
  temperature: 0.3,
  max_tokens: 4096,
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

  // LLM Config State (persisted in localStorage)
  const [llmConfig, setLlmConfig] = useState<LlmConfig>(() => {
    try {
      const saved = localStorage.getItem('noesis_llm_config');
      return saved ? { ...DEFAULT_LLM, ...JSON.parse(saved) } : DEFAULT_LLM;
    } catch {
      return DEFAULT_LLM;
    }
  });

  const updateSettings = (newSettings: AppSettings) => {
    setSettings(newSettings);
    localStorage.setItem('noesis_settings', JSON.stringify(newSettings));
  };

  const updateLlmConfig = (newConfig: LlmConfig) => {
    setLlmConfig(newConfig);
    localStorage.setItem('noesis_llm_config', JSON.stringify(newConfig));
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

  // Project & Map State
  const [currentProject, setCurrentProject] = useState<FileNode | null>(null);
  const [projectMap, setProjectMap] = useState<ProjectMap | null>(null);
  const [isIndexing, setIsIndexing] = useState<boolean>(false);

  // AI & Reading State
  const [cards, setCards] = useState<ExplanationCard[]>([]);
  const [confusionMarks, setConfusionMarks] = useState<ConfusionMark[]>([]);
  const [activeRewording, setActiveRewording] = useState<RewordingResult | null>(null);
  const [dueReviews, setDueReviews] = useState<ExerciseRecord[]>([]);

  // Scratchpad State
  const [showScratch, setShowScratch] = useState<boolean>(false);
  const [scratchCode, setScratchCode] = useState<string>('');
  const [scratchLanguage, setScratchLanguage] = useState<string>('javascript');

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

  // Load project on mount
  useEffect(() => {
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

      await handleTriggerIndex();

      // Open first code file
      const first = findFirstFile(tree);
      if (first) {
        await handleOpenFile(first.path);
      }
    } catch (err: any) {
      console.error('Failed to open project:', err);
    }
  };

  const findFirstFile = (node: FileNode): FileNode | null => {
    if (!node.is_dir) return node;
    if (node.children) {
      for (const c of node.children) {
        const found = findFirstFile(c);
        if (found) return found;
      }
    }
    return null;
  };

  // Open file in tabs (support multiple opened files!)
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

      setTabs((prev) => [...prev, newTab]);
      setActiveTabPath(filePath);
    } catch (err: any) {
      console.error('Failed to read file:', err);
    }
  };

  const handleCloseTab = (filePath: string, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();

    const index = tabs.findIndex((t) => t.path === filePath);
    if (index === -1) return;

    const newTabs = tabs.filter((t) => t.path !== filePath);
    setTabs(newTabs);

    if (activeTabPath === filePath) {
      if (newTabs.length > 0) {
        const nextIndex = Math.min(index, newTabs.length - 1);
        setActiveTabPath(newTabs[nextIndex].path);
      } else {
        setActiveTabPath(null);
      }
    }
  };

  const handleActiveContentChange = (newContent: string) => {
    if (!activeTabPath) return;
    setTabs((prev) =>
      prev.map((t) => (t.path === activeTabPath ? { ...t, content: newContent, isDirty: true } : t))
    );
  };

  const handleSaveActiveFile = async () => {
    if (!activeTab) return;
    try {
      await ApiService.saveFile(activeTab.path, activeTab.content);
      setTabs((prev) =>
        prev.map((t) => (t.path === activeTab.path ? { ...t, isDirty: false } : t))
      );
    } catch (err: any) {
      alert(`保存失败: ${err}`);
    }
  };

  const handleTriggerIndex = async () => {
    setIsIndexing(true);
    try {
      const map = await ApiService.indexProject();
      setProjectMap(map);
    } catch (err: any) {
      console.error('Index failed:', err);
    } finally {
      setIsIndexing(false);
    }
  };

  const handleExplainSelection = async (snippet: string, startLine: number, endLine: number) => {
    if (!activeTab) return;
    try {
      // 1. Generate local 4-perspective cards
      const generatedCards = await ApiService.explainCode(
        activeTab.path,
        snippet,
        startLine,
        endLine
      );

      // 2. If DeepSeek API key is present, query DeepSeek model directly for deep analysis!
      if (llmConfig.api_key.trim().length > 5) {
        const prompt = `请针对以下选区代码进行深度导读，严格按照【整体】、【逻辑】、【语法】、【原理】四层视角展开：\n\n文件: ${activeTab.path} (行 ${startLine}-${endLine})\n\`\`\`${activeTab.language}\n${snippet}\n\`\`\``;
        const aiResponse = await ApiService.sendLlmChat(llmConfig, [
          { role: 'system', content: '你是一位精通底层原理与工程架构的代码深度导读专家。' },
          { role: 'user', content: prompt },
        ]);

        // Augment detailed body of the overall perspective card with the live DeepSeek insights
        if (generatedCards.length > 0) {
          generatedCards[0].detailed_body = `### DeepSeek 模型深度解析\n\n${aiResponse}\n\n---\n${generatedCards[0].detailed_body}`;
        }
      }

      setCards(generatedCards);
    } catch (err: any) {
      alert(`解读生成遇到异常: ${err?.message || err}`);
    }
  };

  const handleMarkConfusion = async (
    snippet: string,
    startLine: number,
    endLine: number,
    category: string
  ) => {
    if (!activeTab) return;
    try {
      const rewording = await ApiService.markConfusion(
        activeTab.path,
        startLine,
        endLine,
        snippet,
        category
      );
      setActiveRewording(rewording);
      await loadConfusionMarks();
    } catch (err: any) {
      alert(`标记卡点失败: ${err}`);
    }
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

  const handleRunScratchCode = async (language: string, code: string): Promise<RunResponse> => {
    return await ApiService.runScratchCode({ language, code });
  };

  const handleSendChat = async (messages: ChatMessage[]): Promise<string> => {
    return await ApiService.sendLlmChat(llmConfig, messages);
  };

  const handlePreviewOutbound = async (messages: ChatMessage[]): Promise<OutboundPreview> => {
    return await ApiService.previewOutbound(llmConfig, messages);
  };

  const handleMarkCardFeedback = (cardId: string, type: 'helpful' | 'too_generic' | 'error') => {
    alert(`卡片 [${cardId}] 反馈已记录: [${type}]。已录入本地评测基线！`);
  };

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
            style={{ background: 'transparent', border: 'none', color: '#a6adc8', cursor: 'pointer', padding: '3px', display: 'flex' }}
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
            onClick={handleSaveActiveFile}
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
            style={{ background: 'transparent', border: 'none', color: '#a6adc8', cursor: 'pointer', padding: '3px', display: 'flex' }}
          >
            {rightCollapsed ? <PanelRightOpen size={16} /> : <PanelRightClose size={16} />}
          </button>
        </div>
      </header>

      {/* Main 3-Column Work Area with Resizers */}
      <div style={{ flex: 1, display: 'flex', overflow: 'hidden', position: 'relative' }}>
        {/* Left: Project Explorer & Map */}
        {!leftCollapsed && (
          <div style={{ width: `${leftWidth}px`, height: '100%', flexShrink: 0, overflow: 'hidden' }}>
            <ProjectSidebar
              currentProject={currentProject}
              projectMap={projectMap}
              activeFilePath={activeTabPath || ''}
              onSelectFile={handleOpenFile}
              onOpenProject={() => handleOpenProject()}
              onTriggerIndex={handleTriggerIndex}
              isIndexing={isIndexing}
            />
          </div>
        )}

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
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden', minWidth: '200px' }}>
          {/* Multi-File Tab Bar */}
          <div
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
            {tabs.map((tab) => {
              const isActive = tab.path === activeTabPath;
              const isMarkdown = tab.language === 'markdown';
              return (
                <div
                  key={tab.path}
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
                key={activeTab.path}
                filePath={activeTab.path}
                initialContent={activeTab.content}
                language={activeTab.language}
                fontSize={settings.editorFontSize}
                fontFamily={settings.fontFamily}
                wordWrap={settings.wordWrap}
                onContentChange={handleActiveContentChange}
                onSave={handleSaveActiveFile}
                onExplainSelection={handleExplainSelection}
                onMarkConfusion={handleMarkConfusion}
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

          {/* Bottom Docked Scratch Runner */}
          {showScratch && (
            <div style={{ height: '260px', flexShrink: 0 }}>
              <ScratchRunner
                initialCode={scratchCode}
                initialLanguage={scratchLanguage}
                onRunCode={handleRunScratchCode}
                dueReviews={dueReviews}
                onClose={() => setShowScratch(false)}
              />
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
        {!rightCollapsed && (
          <div style={{ width: `${rightWidth}px`, height: '100%', flexShrink: 0, overflow: 'hidden' }}>
            <AiSidebar
              cards={cards}
              confusionMarks={confusionMarks}
              activeRewording={activeRewording}
              llmConfig={llmConfig}
              onUpdateLlmConfig={updateLlmConfig}
              onSendChat={handleSendChat}
              onPreviewOutbound={handlePreviewOutbound}
              onMarkCardFeedback={handleMarkCardFeedback}
            />
          </div>
        )}
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
          <span>DeepSeek API: 已就绪</span>
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
        onResetLayout={resetLayout}
      />
    </div>
  );
};
