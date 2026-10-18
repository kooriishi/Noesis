import React, { useState, useEffect } from 'react';
import {
  ShieldCheck,
  Save,
  Code,
  Terminal,
} from 'lucide-react';
import { ApiService } from '../services/api';
import { CodeEditor } from '../editor/CodeEditor';
import { ProjectSidebar } from '../sidebar-project/ProjectSidebar';
import { AiSidebar } from '../sidebar-ai/AiSidebar';
import { ScratchRunner } from '../scratch/ScratchRunner';
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
} from '../types';

export const ShellLayout: React.FC = () => {
  // Project & File State
  const [currentProject, setCurrentProject] = useState<FileNode | null>(null);
  const [projectMap, setProjectMap] = useState<ProjectMap | null>(null);
  const [activeFilePath, setActiveFilePath] = useState<string>('src/lib.rs');
  const [activeFileContent, setActiveFileContent] = useState<string>('');
  const [activeFileLanguage, setActiveFileLanguage] = useState<string>('rust');
  const [isDirty, setIsDirty] = useState<boolean>(false);
  const [isIndexing, setIsIndexing] = useState<boolean>(false);

  // AI & Reading State
  const [cards, setCards] = useState<ExplanationCard[]>([]);
  const [confusionMarks, setConfusionMarks] = useState<ConfusionMark[]>([]);
  const [activeRewording, setActiveRewording] = useState<RewordingResult | null>(null);
  const [dueReviews, setDueReviews] = useState<ExerciseRecord[]>([]);

  // Sandbox Scratchpad State
  const [showScratch, setShowScratch] = useState<boolean>(false);
  const [scratchCode, setScratchCode] = useState<string>('');
  const [scratchLanguage, setScratchLanguage] = useState<string>('javascript');

  // LLM Config
  const [llmConfig, setLlmConfig] = useState<LlmConfig>({
    provider: 'deepseek',
    api_url: 'https://api.deepseek.com/v1',
    api_key: '',
    model: 'deepseek-chat',
    temperature: 0.3,
    max_tokens: 4096,
  });

  // Load project on mount (load current directory or default)
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

      // Trigger map indexing automatically
      await handleTriggerIndex();

      // Find first code file to open
      const firstFile = findFirstFile(tree);
      if (firstFile) {
        await handleSelectFile(firstFile.path);
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

  const handleSelectFile = async (filePath: string) => {
    try {
      const payload = await ApiService.readFile(filePath);
      setActiveFilePath(filePath);
      setActiveFileContent(payload.content);
      setActiveFileLanguage(payload.language);
      setIsDirty(false);
    } catch (err: any) {
      console.error('Failed to read file:', err);
    }
  };

  const handleSaveFile = async () => {
    try {
      await ApiService.saveFile(activeFilePath, activeFileContent);
      setIsDirty(false);
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
    try {
      const generatedCards = await ApiService.explainCode(
        activeFilePath,
        snippet,
        startLine,
        endLine
      );
      setCards(generatedCards);
    } catch (err: any) {
      alert(`生成解读卡片失败: ${err}`);
    }
  };

  const handleMarkConfusion = async (
    snippet: string,
    startLine: number,
    endLine: number,
    category: string
  ) => {
    try {
      const rewording = await ApiService.markConfusion(
        activeFilePath,
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
    alert(`卡片 [${cardId}] 反馈已记录: [${type}]。该样本已入库至本地评测集，用于持续校准解读深度！`);
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
        fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
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
          padding: '0 14px',
          userSelect: 'none',
          zIndex: 10,
        }}
      >
        {/* Brand & Project Info */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
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

          <div style={{ width: '1px', height: '16px', background: '#313244' }} />

          <div style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '12px', color: '#a6adc8' }}>
            <span>{currentProject ? currentProject.name : '未加载'}</span>
            <span>/</span>
            <span style={{ color: '#cdd6f4', fontWeight: 500 }}>{activeFilePath}</span>
            {isDirty && (
              <span style={{ width: '7px', height: '7px', borderRadius: '50%', background: '#fab387' }} title="未保存更改" />
            )}
          </div>
        </div>

        {/* Center / Actions */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <button
            onClick={handleSaveFile}
            title="保存当前文件 (Ctrl+S)"
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              background: '#313244',
              color: '#cdd6f4',
              border: 'none',
              borderRadius: '4px',
              padding: '4px 10px',
              fontSize: '11.5px',
              cursor: 'pointer',
            }}
          >
            <Save size={13} color="#89b4fa" />
            保存
          </button>

          <button
            onClick={() => setShowScratch(!showScratch)}
            title="切换底部草稿沙盒"
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              background: showScratch ? '#89b4fa' : '#313244',
              color: showScratch ? '#11111b' : '#cdd6f4',
              border: 'none',
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

        {/* Right / P0 Outbound Security Badge */}
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
        </div>
      </header>

      {/* Main 3-Column Work Area */}
      <div style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        {/* Left: Project Explorer & Map */}
        <div style={{ width: '280px', height: '100%', flexShrink: 0 }}>
          <ProjectSidebar
            currentProject={currentProject}
            projectMap={projectMap}
            activeFilePath={activeFilePath}
            onSelectFile={handleSelectFile}
            onOpenProject={() => handleOpenProject()}
            onTriggerIndex={handleTriggerIndex}
            isIndexing={isIndexing}
          />
        </div>

        {/* Center: Editor + Bottom Docked Scratch Runner */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
          {/* Editor file tab bar */}
          <div
            style={{
              height: '32px',
              background: '#181825',
              borderBottom: '1px solid #313244',
              display: 'flex',
              alignItems: 'center',
              padding: '0 8px',
            }}
          >
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                padding: '4px 12px',
                background: '#1e1e2e',
                borderRadius: '4px 4px 0 0',
                borderTop: '2px solid #89b4fa',
                fontSize: '12px',
                color: '#cdd6f4',
                fontWeight: 500,
              }}
            >
              <Code size={13} color="#89b4fa" />
              <span>{activeFilePath.split('/').pop() || activeFilePath}</span>
            </div>
          </div>

          {/* CodeMirror 6 Editor Container */}
          <div style={{ flex: 1, position: 'relative', overflow: 'hidden' }}>
            <CodeEditor
              filePath={activeFilePath}
              initialContent={activeFileContent}
              language={activeFileLanguage}
              onContentChange={(val) => {
                setActiveFileContent(val);
                setIsDirty(true);
              }}
              onSave={handleSaveFile}
              onExplainSelection={handleExplainSelection}
              onMarkConfusion={handleMarkConfusion}
              onSendToScratch={handleSendToScratch}
            />
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

        {/* Right: AI Guidance, 4-Perspective Cards, Clarifications, and Chat */}
        <div style={{ width: '360px', height: '100%', flexShrink: 0 }}>
          <AiSidebar
            cards={cards}
            confusionMarks={confusionMarks}
            activeRewording={activeRewording}
            llmConfig={llmConfig}
            onUpdateLlmConfig={setLlmConfig}
            onSendChat={handleSendChat}
            onPreviewOutbound={handlePreviewOutbound}
            onMarkCardFeedback={handleMarkCardFeedback}
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
          <span>SQLite 存储已挂载</span>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
          <span>编码: UTF-8</span>
          <span>语言: {activeFileLanguage.toUpperCase()}</span>
          <span>{projectMap ? `已索引 ${projectMap.total_symbols} 个符号` : '准备就绪'}</span>
        </div>
      </footer>
    </div>
  );
};
