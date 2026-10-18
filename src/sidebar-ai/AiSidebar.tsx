import React, { useState, useEffect } from 'react';
import {
  Sparkles,
  HelpCircle,
  MessageSquare,
  ShieldCheck,
  AlertCircle,
  ChevronDown,
  ChevronUp,
  Send,
  Eye,
  Loader2,
  RefreshCw,
  Trash2,
  Layers,
  CheckCircle2,
  BookOpen,
  Plus,
  Pin,
  Edit2,
  Search,
  Download,
  PartyPopper,
} from 'lucide-react';
import {
  ExplanationCard,
  PerspectiveType,
  ConfusionMark,
  ContextManifest,
  CitationItem,
  ConversationSessionRecord,
  LlmConfig,
  ChatMessage,
  OutboundPreview,
  ExplanationSummary,
  AskStreamEvent,
  ConfusionDetailResponse,
  AnnotatedSnippet,
} from '../types';
import { ApiService } from '../services/api';
import { MarkdownRenderer } from '../components/MarkdownRenderer';
import { useToast } from '../components/feedback';

interface AiSidebarProps {
  cards: ExplanationCard[];
  contextManifest?: ContextManifest | null;
  fromCache?: boolean;
  explainError?: string | null;
  onRetryExplain?: (forceRefresh?: boolean, depthMode?: 'standard' | 'deep') => void;
  onJumpToCitation?: (filePath: string, startLine: number, endLine: number) => void;
  confusionMarks: ConfusionMark[];
  onRefreshConfusionMarks?: () => void;
  onEscalatedReexplain?: (mark: ConfusionMark, nextLevel: number) => void;
  activeRewording?: any;
  llmConfig?: LlmConfig;
  isExplaining?: boolean;
  activeSidebarTab?: 'cards' | 'confusion' | 'chat' | 'settings';
  onUpdateLlmConfig?: (config: LlmConfig) => void;
  onClearApiKey?: () => void;
  onSendChat?: (messages: ChatMessage[]) => Promise<string>;
  onPreviewOutbound?: (messages: ChatMessage[]) => Promise<OutboundPreview>;
  onMarkCardFeedback?: (cardId: string, type: 'helpful' | 'too_generic' | 'error') => void;
  onLoadHistoryExplanation?: (cards: ExplanationCard[], manifest?: ContextManifest) => void;
  contextChip?: { file: string; range: [number, number]; snippet: string } | null;
  onClearContextChip?: () => void;
  currentExplanationId?: string;
}

export const AiSidebar: React.FC<AiSidebarProps> = ({
  cards,
  contextManifest,
  fromCache: _fromCache = false,
  explainError,
  onRetryExplain,
  onJumpToCitation,
  confusionMarks,
  onRefreshConfusionMarks,
  onEscalatedReexplain: _onEscalatedReexplain,
  activeRewording: _activeRewording,
  llmConfig: _llmConfig,
  isExplaining = false,
  activeSidebarTab,
  onUpdateLlmConfig: _onUpdateLlmConfig,
  onClearApiKey: _onClearApiKey,
  onSendChat: _onSendChat,
  onPreviewOutbound: _onPreviewOutbound,
  onLoadHistoryExplanation,
  contextChip,
  onClearContextChip,
  currentExplanationId,
}) => {
  const toast = useToast();
  const [activeTab, setActiveTab] = useState<'cards' | 'confusion' | 'chat' | 'settings'>('cards');
  const [showManifest, setShowManifest] = useState(false);

  // F2 Depth mode
  const [depthMode, setDepthMode] = useState<'standard' | 'deep'>('standard');

  // F3 History state
  const [histories, setHistories] = useState<ExplanationSummary[]>([]);
  const [isHistoryPickerOpen, setIsHistoryPickerOpen] = useState(false);
  const [historySearchQuery, setHistorySearchQuery] = useState('');
  const [historyPinnedOnly, setHistoryPinnedOnly] = useState(false);

  // Multi-conversation state (Stage C3 & F4)
  const [sessions, setSessions] = useState<ConversationSessionRecord[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string>('');

  // Cards state: details default to expanded
  const [activePerspective, setActivePerspective] = useState<PerspectiveType | 'all'>('all');
  const [expandedCards, setExpandedCards] = useState<Record<string, boolean>>({});

  // Chat state (F4)
  const [chatInput, setChatInput] = useState('');
  const [chatMessages, setChatMessages] = useState<Array<ChatMessage & { citations?: CitationItem[] }>>([
    {
      role: 'assistant',
      content:
        '👋 你好！我是 NOESIS 代码深度导读助手。在编辑器中选中任意代码片段，即可生成「整体、逻辑、语法、原理」四层解读，或向我进行工程上下文针对性问答！',
    },
  ]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [previewData, setPreviewData] = useState<OutboundPreview | null>(null);

  // Confusion state (F6)
  const [selectedConfusionId, setSelectedConfusionId] = useState<string>('');
  const [confusionDetail, setConfusionDetail] = useState<ConfusionDetailResponse | null>(null);
  const [isLoadingDetail, setIsLoadingDetail] = useState(false);
  const [celebrationMarkId, setCelebrationMarkId] = useState<string | null>(null);
  const [confusionFilter, setConfusionFilter] = useState<'all' | 'unresolved' | 'resolved'>('all');

  useEffect(() => {
    if (activeSidebarTab) {
      setActiveTab(activeSidebarTab);
    }
  }, [activeSidebarTab]);

  useEffect(() => {
    if (activeTab === 'cards') {
      loadHistoryList();
    } else if (activeTab === 'confusion') {
      ApiService.getLearningProfile().catch(() => {});
      if (confusionMarks.length > 0 && !selectedConfusionId) {
        setSelectedConfusionId(confusionMarks[0].id);
      }
    } else if (activeTab === 'chat') {
      loadConversationList();
    }
  }, [activeTab, confusionMarks.length]);

  useEffect(() => {
    if (selectedConfusionId) {
      loadConfusionDetail(selectedConfusionId);
    }
  }, [selectedConfusionId]);

  const loadHistoryList = async () => {
    try {
      const list = await ApiService.listExplanations(
        undefined,
        historySearchQuery || undefined,
        historyPinnedOnly
      );
      setHistories(list);
    } catch (err) {
      console.error('Failed to load histories:', err);
    }
  };

  const loadConfusionDetail = async (id: string) => {
    setIsLoadingDetail(true);
    try {
      const detail = await ApiService.getConfusionDetail(id);
      setConfusionDetail(detail);
    } catch (err) {
      console.error('Failed to load confusion detail:', err);
    } finally {
      setIsLoadingDetail(false);
    }
  };

  const loadConversationList = async () => {
    try {
      const list = await ApiService.listConversations();
      setSessions(list);
      if (list.length > 0 && !activeSessionId) {
        setActiveSessionId(list[0].id);
        await loadSessionMessages(list[0].id);
      }
    } catch {
      // ignore
    }
  };

  const loadSessionMessages = async (sessionId: string) => {
    try {
      const records = await ApiService.loadConversationMessages(sessionId);
      if (records.length > 0) {
        setChatMessages(
          records.map((r) => {
            let citations: CitationItem[] = [];
            try {
              if (r.citations_json) {
                const parsed = JSON.parse(r.citations_json);
                if (Array.isArray(parsed)) citations = parsed;
                else if (parsed?.citations) citations = parsed.citations;
              }
            } catch {}
            return {
              role: (r.role === 'user' || r.role === 'system' ? r.role : 'assistant') as
                | 'system'
                | 'user'
                | 'assistant',
              content: r.content,
              citations,
            };
          })
        );
      } else {
        setChatMessages([
          {
            role: 'assistant',
            content: '👋 已开启新对话！你可以针对当前工程代码或架构设计继续提问。',
          },
        ]);
      }
    } catch {
      // ignore
    }
  };

  const handleCreateNewSession = async () => {
    try {
      const created = await ApiService.createConversation('新问答会话');
      setSessions((prev) => [created, ...prev]);
      setActiveSessionId(created.id);
      setChatMessages([
        {
          role: 'assistant',
          content: '👋 已开启新对话！你可以针对当前工程代码或架构设计继续提问。',
        },
      ]);
      toast.success('已开启新问答会话');
    } catch {
      // ignore
    }
  };

  const handleDeleteCurrentSession = async () => {
    if (!activeSessionId) return;
    try {
      await ApiService.deleteConversation(activeSessionId);
      const remaining = sessions.filter((s) => s.id !== activeSessionId);
      setSessions(remaining);
      if (remaining.length > 0) {
        setActiveSessionId(remaining[0].id);
        await loadSessionMessages(remaining[0].id);
      } else {
        setActiveSessionId('');
        setChatMessages([
          {
            role: 'assistant',
            content: '👋 会话已清空。随时输入问题开启新探索！',
          },
        ]);
      }
      toast.info('会话已删除');
    } catch {
      // ignore
    }
  };

  // Toggle card expansion: default to expanded
  const toggleExpand = (id: string) => {
    setExpandedCards((prev) => ({
      ...prev,
      [id]: prev[id] === undefined ? false : !prev[id],
    }));
  };

  // F2 Feedback
  const handleCardFeedback = async (
    cardId: string,
    perspective: string,
    type: 'helpful' | 'too_generic' | 'wrong'
  ) => {
    const expId = currentExplanationId || cardId;
    try {
      await ApiService.submitExplanationFeedback(expId, perspective, type);
      const textMap = { helpful: '👍 很有帮助', too_generic: '😐 讲得太泛', wrong: '⚠ 存在偏差' };
      toast.success(`感谢反馈 [${textMap[type]}]！已记录至评测基线`);
    } catch (err: any) {
      toast.error(`反馈记录失败: ${err?.message || err}`);
    }
  };

  // F3 History Actions
  const handleSelectHistoryItem = async (hist: ExplanationSummary) => {
    try {
      const expRes = await ApiService.getExplanation(hist.id);
      if (expRes && onLoadHistoryExplanation) {
        onLoadHistoryExplanation(expRes.cards, expRes.manifest);
        toast.success(`已切换至历史卡片: ${hist.title}`);
      }
      setIsHistoryPickerOpen(false);
    } catch (err: any) {
      toast.error(`加载历史卡片失败: ${err?.message || err}`);
    }
  };

  const handlePinHistory = async (e: React.MouseEvent, hist: ExplanationSummary) => {
    e.stopPropagation();
    try {
      await ApiService.pinExplanation(hist.id, !hist.pinned);
      toast.success(hist.pinned ? '已取消钉住' : '已钉住卡片');
      await loadHistoryList();
    } catch (err: any) {
      toast.error(`操作失败: ${err?.message || err}`);
    }
  };

  const handleRenameHistory = async (e: React.MouseEvent, hist: ExplanationSummary) => {
    e.stopPropagation();
    const newTitle = window.prompt('请输入新的卡片标题：', hist.title);
    if (!newTitle || !newTitle.trim()) return;
    try {
      await ApiService.renameExplanation(hist.id, newTitle.trim());
      toast.success('已更新卡片标题');
      await loadHistoryList();
    } catch (err: any) {
      toast.error(`重命名失败: ${err?.message || err}`);
    }
  };

  const handleDeleteHistory = async (e: React.MouseEvent, hist: ExplanationSummary) => {
    e.stopPropagation();
    try {
      await ApiService.deleteExplanation(hist.id);
      toast.info(`已删除卡片「${hist.title}」`);
      await loadHistoryList();
    } catch (err: any) {
      toast.error(`删除失败: ${err?.message || err}`);
    }
  };

  // F4 Send Question with SSE Streaming
  const handleSendStream = async (overridePrompt?: string) => {
    const textToSend = overridePrompt || chatInput.trim();
    if (!textToSend || isStreaming) return;

    const userMsg = { role: 'user' as const, content: textToSend };
    setChatMessages((prev) => [...prev, userMsg]);
    if (!overridePrompt) setChatInput('');
    setIsStreaming(true);

    const asstIndex = chatMessages.length + 1;
    setChatMessages((prev) => [
      ...prev,
      { role: 'assistant', content: '', citations: [] },
    ]);

    let accumulated = '';
    try {
      await ApiService.askProjectQuestion(
        textToSend,
        (evt: AskStreamEvent) => {
          if (evt.type === 'token' && evt.token) {
            accumulated += evt.token;
            setChatMessages((prev) => {
              const copy = [...prev];
              if (copy[asstIndex]) {
                copy[asstIndex] = { ...copy[asstIndex], content: accumulated };
              }
              return copy;
            });
          } else if (evt.type === 'citation_check' && evt.citations) {
            setChatMessages((prev) => {
              const copy = [...prev];
              if (copy[asstIndex]) {
                copy[asstIndex] = { ...copy[asstIndex], citations: evt.citations };
              }
              return copy;
            });
          } else if (evt.type === 'context_ready' && evt.manifest) {
            // Context ready
          } else if (evt.type === 'done') {
            if (evt.conversation_id && !activeSessionId) {
              setActiveSessionId(evt.conversation_id);
              loadConversationList();
            }
          }
        },
        activeSessionId || undefined,
        contextChip?.file,
        contextChip?.range,
        contextChip?.snippet
      );
    } catch (err: any) {
      setChatMessages((prev) => {
        const copy = [...prev];
        if (copy[asstIndex]) {
          copy[asstIndex] = {
            ...copy[asstIndex],
            content: accumulated
              ? `${accumulated}\n\n[⚠️ 生成中断: ${err?.message || err}]`
              : `❌ 请求出错: ${err?.message || err}`,
          };
        }
        return copy;
      });
      toast.error(`智能问答请求失败: ${err?.message || err}`);
    } finally {
      setIsStreaming(false);
    }
  };

  const handlePreview = async () => {
    if (!chatInput.trim()) return;
    try {
      const p = await ApiService.previewAsk(
        chatInput.trim(),
        contextChip?.file,
        contextChip?.range,
        contextChip?.snippet
      );
      setPreviewData(p.outbound_preview);
      toast.info('P0 安全预检已生成');
    } catch (err: any) {
      toast.error(`安全预检拦截: ${err?.message || err}`);
    }
  };

  // F6 Confusion handlers
  const handleResolveConfusion = async (markId: string) => {
    try {
      await ApiService.resolveConfusionMark(markId);
      setCelebrationMarkId(markId);
      setTimeout(() => setCelebrationMarkId(null), 3000);
      toast.success('🎉 恭喜弄懂！已记录掌握，并同步加入巩固复习库', {
        label: '立即做题',
        onClick: () => {
          // Switch to exercises if available
          setActiveTab('confusion');
        },
      });
      if (onRefreshConfusionMarks) onRefreshConfusionMarks();
      await ApiService.getLearningProfile();
      await loadConfusionDetail(markId);
    } catch (err: any) {
      toast.error(`操作失败: ${err?.message || err}`);
    }
  };

  const handleReopenConfusion = async (markId: string) => {
    try {
      await ApiService.reopenConfusionMark(markId);
      toast.info('已重新标记为待解决卡点');
      if (onRefreshConfusionMarks) onRefreshConfusionMarks();
      await loadConfusionDetail(markId);
    } catch (err: any) {
      toast.error(`重新标记失败: ${err?.message || err}`);
    }
  };

  const handleExportMarkdown = async () => {
    try {
      const md = await ApiService.exportConfusionMarkdown();
      navigator.clipboard.writeText(md);
      toast.success('已导出全量卡点与精读汇编并复制到剪贴板！');
    } catch (err: any) {
      toast.error(`导出失败: ${err?.message || err}`);
    }
  };

  const renderCitationBadge = (cit: CitationItem, idx: number) => {
    const isVerified = cit.verification_status === 'verified';
    const isCorrected = cit.verification_status === 'corrected';
    const startLine = (isCorrected && cit.corrected_line_start ? cit.corrected_line_start : cit.line_start) ?? 1;
    const endLine = (isCorrected && cit.corrected_line_end ? cit.corrected_line_end : cit.line_end) ?? startLine;

    const badgeStyle = isVerified
      ? {
          bg: 'var(--noesis-status-verified-bg, rgba(166, 227, 161, 0.14))',
          fg: 'var(--noesis-status-verified-fg, #a6e3a1)',
          prefix: '✓ 依据已核验',
        }
      : isCorrected
      ? {
          bg: 'var(--noesis-status-corrected-bg, rgba(137, 180, 250, 0.14))',
          fg: 'var(--noesis-status-corrected-fg, #89b4fa)',
          prefix: '✓ 已修正行号',
        }
      : {
          bg: 'var(--noesis-status-unverified-bg, rgba(250, 179, 135, 0.15))',
          fg: 'var(--noesis-status-unverified-fg, #fab387)',
          prefix: '⚠ 引用待核实',
        };

    return (
      <button
        key={idx}
        onClick={() => {
          if (cit.file && onJumpToCitation) {
            onJumpToCitation(cit.file, startLine, endLine);
          }
        }}
        title={cit.note || `${cit.file}:${startLine}-${endLine} (${cit.quote || ''})`}
        style={{
          display: 'inline-flex',
          alignItems: 'center',
          gap: '4px',
          padding: '2px 6px',
          borderRadius: '4px',
          fontSize: '10px',
          background: badgeStyle.bg,
          color: badgeStyle.fg,
          border: '1px solid rgba(255,255,255,0.08)',
          cursor: cit.file ? 'pointer' : 'default',
        }}
      >
        <span>{badgeStyle.prefix}</span>
        <span>
          {cit.file ? `${cit.file.split('/').pop()}:${startLine}-${endLine}` : ''}
        </span>
      </button>
    );
  };

  const filteredCards =
    activePerspective === 'all'
      ? cards
      : cards.filter((c) => c.perspective === activePerspective);

  const filteredConfusionMarks = confusionMarks.filter((m) => {
    if (confusionFilter === 'unresolved') return !m.resolved_at;
    if (confusionFilter === 'resolved') return Boolean(m.resolved_at);
    return true;
  });

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        background: 'var(--noesis-bg-mantle, #181825)',
        borderLeft: '1px solid var(--noesis-bg-surface0, #313244)',
      }}
    >
      {/* Header Tabs */}
      <div
        style={{
          display: 'flex',
          background: 'var(--noesis-bg-crust, #11111b)',
          borderBottom: '1px solid var(--noesis-bg-surface0, #313244)',
        }}
      >
        {[
          { id: 'cards', label: '四层解读', icon: Sparkles },
          { id: 'confusion', label: '看不懂答疑', icon: HelpCircle },
          { id: 'chat', label: '智能问答', icon: MessageSquare },
        ].map((tab) => {
          const Icon = tab.icon;
          const isActive = activeTab === tab.id;
          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id as any)}
              style={{
                flex: 1,
                padding: '9px 0',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                gap: '5px',
                background: isActive ? 'var(--noesis-bg-mantle, #181825)' : 'transparent',
                color: isActive ? 'var(--noesis-accent-blue, #89b4fa)' : 'var(--noesis-text-subtle, #6c7086)',
                border: 'none',
                borderBottom: isActive ? '2px solid var(--noesis-accent-blue, #89b4fa)' : '2px solid transparent',
                fontSize: '11.5px',
                fontWeight: isActive ? 600 : 400,
                cursor: 'pointer',
              }}
            >
              <Icon size={14} />
              {tab.label}
            </button>
          );
        })}
      </div>

      {/* Content Area */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '12px' }}>
        {/* ===================== TAB 1: CARDS ===================== */}
        {activeTab === 'cards' && (
          <div>
            {/* F3 History Picker Dropdown Bar */}
            <div style={{ position: 'relative', marginBottom: '10px' }}>
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  background: 'var(--noesis-bg-base, #1e1e2e)',
                  border: '1px solid var(--noesis-bg-surface0, #313244)',
                  borderRadius: 'var(--noesis-radius-sm, 6px)',
                  padding: '6px 10px',
                  fontSize: '11.5px',
                }}
              >
                <button
                  onClick={() => {
                    setIsHistoryPickerOpen(!isHistoryPickerOpen);
                    loadHistoryList();
                  }}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '6px',
                    background: 'transparent',
                    border: 'none',
                    color: 'var(--noesis-text-primary, #cdd6f4)',
                    cursor: 'pointer',
                    fontSize: '11.5px',
                    fontWeight: 500,
                    flex: 1,
                    textAlign: 'left',
                  }}
                >
                  <BookOpen size={13} color="var(--noesis-accent-blue, #89b4fa)" />
                  <span style={{ textOverflow: 'ellipsis', overflow: 'hidden', whiteSpace: 'nowrap' }}>
                    {histories.find((h) => h.id === currentExplanationId)?.title || '历史卡片档案 (F3)'}
                  </span>
                  <ChevronDown size={13} style={{ marginLeft: 'auto' }} />
                </button>

                {/* F2 Depth mode toggle pill */}
                <div style={{ display: 'flex', background: 'var(--noesis-bg-surface0, #313244)', borderRadius: '12px', padding: '2px', marginLeft: '8px' }}>
                  <button
                    onClick={() => {
                      setDepthMode('standard');
                      if (onRetryExplain) onRetryExplain(true, 'standard');
                    }}
                    style={{
                      padding: '2px 7px',
                      fontSize: '10px',
                      borderRadius: '10px',
                      border: 'none',
                      background: depthMode === 'standard' ? 'var(--noesis-accent-blue, #89b4fa)' : 'transparent',
                      color: depthMode === 'standard' ? 'var(--noesis-bg-crust, #11111b)' : 'var(--noesis-text-muted, #a6adc8)',
                      fontWeight: 600,
                      cursor: 'pointer',
                    }}
                  >
                    标准
                  </button>
                  <button
                    onClick={() => {
                      setDepthMode('deep');
                      if (onRetryExplain) onRetryExplain(true, 'deep');
                    }}
                    style={{
                      padding: '2px 7px',
                      fontSize: '10px',
                      borderRadius: '10px',
                      border: 'none',
                      background: depthMode === 'deep' ? 'var(--noesis-accent-mauve, #cba6f7)' : 'transparent',
                      color: depthMode === 'deep' ? 'var(--noesis-bg-crust, #11111b)' : 'var(--noesis-text-muted, #a6adc8)',
                      fontWeight: 600,
                      cursor: 'pointer',
                    }}
                  >
                    深入 (F2)
                  </button>
                </div>
              </div>

              {/* History Picker Dropdown Menu */}
              {isHistoryPickerOpen && (
                <div
                  style={{
                    position: 'absolute',
                    top: '100%',
                    left: 0,
                    right: 0,
                    zIndex: 50,
                    marginTop: '4px',
                    background: 'var(--noesis-bg-base, #1e1e2e)',
                    border: '1px solid var(--noesis-bg-surface0, #313244)',
                    borderRadius: 'var(--noesis-radius-md, 6px)',
                    boxShadow: 'var(--noesis-shadow-lg, 0 12px 30px rgba(0,0,0,0.5))',
                    maxHeight: '260px',
                    display: 'flex',
                    flexDirection: 'column',
                    overflow: 'hidden',
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'center', padding: '6px 8px', borderBottom: '1px solid var(--noesis-bg-surface0, #313244)', gap: '6px' }}>
                    <Search size={12} color="var(--noesis-text-subtle, #6c7086)" />
                    <input
                      type="text"
                      placeholder="搜索历史卡片..."
                      value={historySearchQuery}
                      onChange={(e) => {
                        setHistorySearchQuery(e.target.value);
                        loadHistoryList();
                      }}
                      style={{
                        flex: 1,
                        background: 'transparent',
                        border: 'none',
                        color: 'var(--noesis-text-primary, #cdd6f4)',
                        fontSize: '11px',
                        outline: 'none',
                      }}
                    />
                    <button
                      onClick={() => {
                        setHistoryPinnedOnly(!historyPinnedOnly);
                        loadHistoryList();
                      }}
                      style={{
                        padding: '2px 6px',
                        fontSize: '10px',
                        borderRadius: '4px',
                        border: 'none',
                        background: historyPinnedOnly ? 'var(--noesis-accent-yellow, #f9e2af)' : 'var(--noesis-bg-surface0, #313244)',
                        color: historyPinnedOnly ? 'var(--noesis-bg-crust, #11111b)' : 'var(--noesis-text-muted, #a6adc8)',
                        cursor: 'pointer',
                      }}
                    >
                      📌 收藏
                    </button>
                  </div>

                  <div style={{ overflowY: 'auto', flex: 1 }}>
                    {histories.length === 0 ? (
                      <div style={{ padding: '12px', textAlign: 'center', fontSize: '11px', color: 'var(--noesis-text-subtle, #6c7086)' }}>
                        暂无历史卡片记录
                      </div>
                    ) : (
                      histories.map((h) => (
                        <div
                          key={h.id}
                          onClick={() => handleSelectHistoryItem(h)}
                          style={{
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'space-between',
                            padding: '7px 10px',
                            borderBottom: '1px solid rgba(255,255,255,0.03)',
                            cursor: 'pointer',
                            fontSize: '11px',
                            background: h.id === currentExplanationId ? 'rgba(137, 180, 250, 0.12)' : 'transparent',
                          }}
                        >
                          <div style={{ flex: 1, overflow: 'hidden', marginRight: '6px' }}>
                            <div style={{ color: 'var(--noesis-text-primary, #cdd6f4)', fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                              {h.title}
                            </div>
                            <div style={{ color: 'var(--noesis-text-subtle, #6c7086)', fontSize: '10px' }}>
                              {h.file_path.split('/').pop()}:{h.range} · {h.depth_mode === 'deep' ? '深入' : '标准'}
                            </div>
                          </div>
                          <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                            <button
                              onClick={(e) => handlePinHistory(e, h)}
                              title={h.pinned ? '取消钉住' : '钉住卡片'}
                              style={{ background: 'transparent', border: 'none', color: h.pinned ? 'var(--noesis-accent-yellow, #f9e2af)' : 'var(--noesis-text-subtle, #6c7086)', cursor: 'pointer', padding: '2px' }}
                            >
                              <Pin size={12} />
                            </button>
                            <button
                              onClick={(e) => handleRenameHistory(e, h)}
                              title="重命名"
                              style={{ background: 'transparent', border: 'none', color: 'var(--noesis-text-subtle, #6c7086)', cursor: 'pointer', padding: '2px' }}
                            >
                              <Edit2 size={12} />
                            </button>
                            <button
                              onClick={(e) => handleDeleteHistory(e, h)}
                              title="删除"
                              style={{ background: 'transparent', border: 'none', color: 'var(--noesis-accent-red, #f38ba8)', cursor: 'pointer', padding: '2px' }}
                            >
                              <Trash2 size={12} />
                            </button>
                          </div>
                        </div>
                      ))
                    )}
                  </div>
                </div>
              )}
            </div>

            {/* Perspective Filter Pills + Manifest Toggle */}
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: '4px', marginBottom: '10px', flexWrap: 'wrap' }}>
              <div style={{ display: 'flex', gap: '4px', flexWrap: 'wrap' }}>
                {[
                  { id: 'all', label: '全部视角' },
                  { id: 'overall', label: '整体' },
                  { id: 'logic', label: '逻辑' },
                  { id: 'syntax', label: '语法' },
                  { id: 'principle', label: '原理' },
                ].map((p) => {
                  const isSelected = activePerspective === p.id;
                  return (
                    <button
                      key={p.id}
                      onClick={() => setActivePerspective(p.id as any)}
                      style={{
                        padding: '3px 8px',
                        borderRadius: '12px',
                        fontSize: '11px',
                        border: '1px solid',
                        borderColor: isSelected ? 'var(--noesis-accent-blue, #89b4fa)' : 'var(--noesis-bg-surface0, #313244)',
                        background: isSelected ? 'rgba(137, 180, 250, 0.15)' : 'var(--noesis-bg-base, #1e1e2e)',
                        color: isSelected ? 'var(--noesis-accent-blue, #89b4fa)' : 'var(--noesis-text-muted, #a6adc8)',
                        cursor: 'pointer',
                      }}
                    >
                      {p.label}
                    </button>
                  );
                })}
              </div>

              {contextManifest && (
                <button
                  onClick={() => setShowManifest(!showManifest)}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '4px',
                    padding: '3px 8px',
                    borderRadius: '12px',
                    fontSize: '11px',
                    border: '1px solid var(--noesis-bg-surface0, #313244)',
                    background: showManifest ? 'rgba(137, 180, 250, 0.15)' : 'transparent',
                    color: 'var(--noesis-accent-blue, #89b4fa)',
                    cursor: 'pointer',
                  }}
                >
                  <Layers size={11} />
                  上下文清单 ({contextManifest.total_tokens}t)
                </button>
              )}
            </div>

            {/* Collapsible Context Manifest Drawer */}
            {showManifest && contextManifest && (
              <div
                style={{
                  background: 'var(--noesis-bg-crust, #11111b)',
                  border: '1px solid var(--noesis-bg-surface0, #313244)',
                  borderRadius: '6px',
                  padding: '8px 10px',
                  marginBottom: '12px',
                  fontSize: '11px',
                }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', color: 'var(--noesis-accent-blue, #89b4fa)', fontWeight: 600, marginBottom: '6px' }}>
                  <span>上下文清单 (P0–P7 预算: {contextManifest.total_tokens}/{contextManifest.budget_tokens} tokens)</span>
                  <span style={{ color: 'var(--noesis-text-subtle, #6c7086)', fontSize: '10px' }}>hash: {contextManifest.basis_hash.slice(0, 10)}</span>
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
                  {contextManifest.layers.map((layer) => (
                    <div
                      key={layer.layer}
                      style={{
                        display: 'flex',
                        justifyContent: 'space-between',
                        alignItems: 'center',
                        color: layer.included ? 'var(--noesis-text-primary, #cdd6f4)' : 'var(--noesis-text-subtle, #6c7086)',
                        fontSize: '10.5px',
                      }}
                    >
                      <span>
                        <strong>[{layer.layer}]</strong> {layer.label}: {layer.summary}
                      </span>
                      <span style={{ color: layer.trimmed ? 'var(--noesis-accent-peach, #fab387)' : 'var(--noesis-accent-green, #a6e3a1)', whiteSpace: 'nowrap', marginLeft: '6px' }}>
                        {layer.tokens} tok {layer.trimmed ? '(已裁剪)' : ''}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {isExplaining && (
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px',
                  padding: '10px 12px',
                  marginBottom: '12px',
                  borderRadius: '6px',
                  background: 'rgba(137, 180, 250, 0.1)',
                  border: '1px solid var(--noesis-accent-blue, #89b4fa)',
                  color: 'var(--noesis-accent-blue, #89b4fa)',
                  fontSize: '11.5px',
                }}
              >
                <Loader2 size={14} style={{ animation: 'spin 1s linear infinite' }} />
                <span>正在组装上下文并核验源码引用以生成四层深度剖析...</span>
              </div>
            )}

            {explainError && !isExplaining && (
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  gap: '8px',
                  padding: '10px 12px',
                  marginBottom: '12px',
                  borderRadius: '6px',
                  background: 'rgba(243, 139, 168, 0.12)',
                  border: '1px solid rgba(243, 139, 168, 0.45)',
                  color: 'var(--noesis-accent-red, #f38ba8)',
                  fontSize: '11.5px',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: '6px', flex: 1 }}>
                  <AlertCircle size={15} style={{ flexShrink: 0 }} />
                  <span>{explainError}</span>
                </div>
                {onRetryExplain && (
                  <button
                    onClick={() => onRetryExplain(true, depthMode)}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '4px',
                      padding: '4px 8px',
                      background: 'var(--noesis-accent-red, #f38ba8)',
                      color: 'var(--noesis-bg-crust, #11111b)',
                      border: 'none',
                      borderRadius: '4px',
                      fontSize: '11px',
                      fontWeight: 600,
                      cursor: 'pointer',
                    }}
                  >
                    <RefreshCw size={12} />
                    重试
                  </button>
                )}
              </div>
            )}

            {filteredCards.length > 0 ? (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
                {filteredCards.map((card) => {
                  // F2: Details open by default unless explicitly closed
                  const isExpanded = expandedCards[card.id] !== false;
                  return (
                    <div
                      key={card.id}
                      style={{
                        background: 'var(--noesis-bg-base, #1e1e2e)',
                        border: '1px solid var(--noesis-bg-surface0, #313244)',
                        borderRadius: 'var(--noesis-radius-md, 8px)',
                        padding: '12px',
                      }}
                    >
                      {/* Card Header */}
                      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: '6px', flexWrap: 'wrap' }}>
                        <span
                          style={{
                            fontSize: '10.5px',
                            fontWeight: 'bold',
                            padding: '2px 6px',
                            borderRadius: '4px',
                            background:
                              card.perspective === 'overall'
                                ? 'rgba(137, 180, 250, 0.2)'
                                : card.perspective === 'logic'
                                ? 'rgba(166, 227, 161, 0.2)'
                                : card.perspective === 'syntax'
                                ? 'rgba(249, 226, 175, 0.2)'
                                : 'rgba(243, 139, 168, 0.2)',
                            color:
                              card.perspective === 'overall'
                                ? 'var(--noesis-accent-blue, #89b4fa)'
                                : card.perspective === 'logic'
                                ? 'var(--noesis-accent-green, #a6e3a1)'
                                : card.perspective === 'syntax'
                                ? 'var(--noesis-accent-yellow, #f9e2af)'
                                : 'var(--noesis-accent-red, #f38ba8)',
                          }}
                        >
                          {card.perspective.toUpperCase()} · {card.title}
                        </span>

                        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                          {card.source_matches ? (
                            <span style={{ fontSize: '10px', color: 'var(--noesis-accent-green, #a6e3a1)', display: 'flex', alignItems: 'center', gap: '3px' }}>
                              <ShieldCheck size={12} />
                              选区原文一致
                            </span>
                          ) : (
                            <span style={{ fontSize: '10px', color: 'var(--noesis-accent-peach, #fab387)', display: 'flex', alignItems: 'center', gap: '3px' }}>
                              <AlertCircle size={12} />
                              选区待核对
                            </span>
                          )}
                        </div>
                      </div>

                      {/* One sentence */}
                      <div style={{ fontSize: '13px', fontWeight: 600, color: 'var(--noesis-text-primary, #cdd6f4)', marginTop: '8px', lineHeight: '1.4' }}>
                        {card.one_sentence}
                      </div>

                      {/* Key bullets with Markdown Renderer (F2) */}
                      <ul style={{ margin: '8px 0', paddingLeft: '16px', fontSize: '12px', color: 'var(--noesis-text-muted, #a6adc8)', lineHeight: '1.5' }}>
                        {card.key_bullets.map((b, i) => (
                          <li key={i} style={{ marginBottom: '4px' }}>
                            <MarkdownRenderer content={b} fontSize={12} />
                          </li>
                        ))}
                      </ul>

                      {/* Citations Badges */}
                      {card.citations && card.citations.length > 0 && (
                        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '5px', marginTop: '6px', marginBottom: '6px' }}>
                          {card.citations.map((cit, idx) => renderCitationBadge(cit, idx))}
                        </div>
                      )}

                      {/* Expanded Details View (Default Open) */}
                      {isExpanded && card.detailed_body && (
                        <div
                          style={{
                            marginTop: '10px',
                            paddingTop: '10px',
                            borderTop: '1px solid var(--noesis-bg-surface0, #313244)',
                            fontSize: '12px',
                            color: 'var(--noesis-text-secondary, #bac2de)',
                            lineHeight: '1.6',
                          }}
                        >
                          <MarkdownRenderer content={card.detailed_body} fontSize={12} />
                        </div>
                      )}

                      {/* Card Footer Actions & Feedback (F2 / F7-a) */}
                      <div
                        style={{
                          display: 'flex',
                          justifyContent: 'space-between',
                          alignItems: 'center',
                          marginTop: '10px',
                          paddingTop: '8px',
                          borderTop: '1px solid rgba(255,255,255,0.05)',
                        }}
                      >
                        <button
                          onClick={() => toggleExpand(card.id)}
                          style={{
                            display: 'flex',
                            alignItems: 'center',
                            gap: '4px',
                            background: 'transparent',
                            border: 'none',
                            color: 'var(--noesis-accent-blue, #89b4fa)',
                            fontSize: '11px',
                            cursor: 'pointer',
                            fontWeight: 500,
                          }}
                        >
                          {isExpanded ? <ChevronUp size={13} /> : <ChevronDown size={13} />}
                          {isExpanded ? '收起详述' : '展开详细原理解析'}
                        </button>

                        <div style={{ display: 'flex', gap: '5px' }}>
                          <button
                            onClick={() => handleCardFeedback(card.id, card.perspective, 'helpful')}
                            title="标记有深度"
                            style={{
                              background: 'var(--noesis-bg-surface0, #313244)',
                              border: 'none',
                              borderRadius: '4px',
                              padding: '3px 7px',
                              color: 'var(--noesis-accent-green, #a6e3a1)',
                              fontSize: '10px',
                              cursor: 'pointer',
                            }}
                          >
                            👍 有用
                          </button>
                          <button
                            onClick={() => handleCardFeedback(card.id, card.perspective, 'too_generic')}
                            title="标记讲得太泛"
                            style={{
                              background: 'var(--noesis-bg-surface0, #313244)',
                              border: 'none',
                              borderRadius: '4px',
                              padding: '3px 7px',
                              color: 'var(--noesis-accent-yellow, #f9e2af)',
                              fontSize: '10px',
                              cursor: 'pointer',
                            }}
                          >
                            😐 太泛
                          </button>
                          <button
                            onClick={() => handleCardFeedback(card.id, card.perspective, 'wrong')}
                            title="标记有源码依据偏差"
                            style={{
                              background: 'var(--noesis-bg-surface0, #313244)',
                              border: 'none',
                              borderRadius: '4px',
                              padding: '3px 7px',
                              color: 'var(--noesis-accent-red, #f38ba8)',
                              fontSize: '10px',
                              cursor: 'pointer',
                            }}
                          >
                            ⚠ 有误
                          </button>
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              <div style={{ textAlign: 'center', padding: '36px 12px', color: 'var(--noesis-text-subtle, #6c7086)', fontSize: '12px' }}>
                在左侧或编辑区选中任意代码段，即可一键生成四层结构化导读。
              </div>
            )}
          </div>
        )}

        {/* ===================== TAB 2: CONFUSION (F6) ===================== */}
        {activeTab === 'confusion' && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
            {/* Top Bar: Learning Profile & Export */}
            <div
              style={{
                background: 'var(--noesis-bg-base, #1e1e2e)',
                border: '1px solid var(--noesis-bg-surface0, #313244)',
                borderRadius: 'var(--noesis-radius-md, 8px)',
                padding: '10px',
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
              }}
            >
              <div>
                <div style={{ fontSize: '12px', fontWeight: 600, color: 'var(--noesis-text-primary, #cdd6f4)' }}>
                  学习卡点与精读档案 (F6)
                </div>
                <div style={{ fontSize: '11px', color: 'var(--noesis-text-muted, #a6adc8)' }}>
                  累计记录: <strong>{confusionMarks.length}</strong> 处 · 待解决: <strong>{confusionMarks.filter((m) => !m.resolved_at).length}</strong> 处
                </div>
              </div>

              <button
                onClick={handleExportMarkdown}
                title="导出为 Markdown 学习档案"
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '4px',
                  padding: '5px 10px',
                  background: 'var(--noesis-bg-surface0, #313244)',
                  color: 'var(--noesis-accent-blue, #89b4fa)',
                  border: 'none',
                  borderRadius: 'var(--noesis-radius-sm, 4px)',
                  fontSize: '11px',
                  cursor: 'pointer',
                }}
              >
                <Download size={12} />
                导出 Markdown
              </button>
            </div>

            {/* Filter pills */}
            <div style={{ display: 'flex', gap: '6px' }}>
              {[
                { id: 'all', label: '全部' },
                { id: 'unresolved', label: '待解决' },
                { id: 'resolved', label: '已解决' },
              ].map((f) => (
                <button
                  key={f.id}
                  onClick={() => setConfusionFilter(f.id as any)}
                  style={{
                    padding: '2px 8px',
                    borderRadius: '10px',
                    fontSize: '10.5px',
                    border: '1px solid',
                    borderColor: confusionFilter === f.id ? 'var(--noesis-accent-blue, #89b4fa)' : 'var(--noesis-bg-surface0, #313244)',
                    background: confusionFilter === f.id ? 'rgba(137, 180, 250, 0.15)' : 'transparent',
                    color: confusionFilter === f.id ? 'var(--noesis-accent-blue, #89b4fa)' : 'var(--noesis-text-muted, #a6adc8)',
                    cursor: 'pointer',
                  }}
                >
                  {f.label}
                </button>
              ))}
            </div>

            {/* List & Detail Grid */}
            {filteredConfusionMarks.length > 0 ? (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
                {filteredConfusionMarks.map((m) => {
                  const isSelected = selectedConfusionId === m.id;
                  const isResolved = Boolean(m.resolved_at);
                  const isCelebrated = celebrationMarkId === m.id;
                  return (
                    <div
                      key={m.id}
                      onClick={() => setSelectedConfusionId(m.id)}
                      style={{
                        background: 'var(--noesis-bg-base, #1e1e2e)',
                        border: isSelected
                          ? '1px solid var(--noesis-accent-blue, #89b4fa)'
                          : '1px solid var(--noesis-bg-surface0, #313244)',
                        borderRadius: 'var(--noesis-radius-md, 8px)',
                        padding: '12px',
                        cursor: 'pointer',
                        transition: 'all 0.15s ease',
                      }}
                    >
                      {/* Mark Header */}
                      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '6px' }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                          <span
                            style={{
                              padding: '2px 6px',
                              borderRadius: '4px',
                              fontSize: '10.5px',
                              fontWeight: 600,
                              background: 'rgba(249, 226, 175, 0.15)',
                              color: 'var(--noesis-accent-yellow, #f9e2af)',
                            }}
                          >
                            {m.category || '代码卡点'}
                          </span>
                          <span style={{ fontSize: '11px', color: 'var(--noesis-accent-blue, #89b4fa)' }}>
                            {m.file_path.split('/').pop()}:{m.line_start}-{m.line_end}
                          </span>
                        </div>

                        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                          {!isResolved ? (
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                handleResolveConfusion(m.id);
                              }}
                              style={{
                                display: 'flex',
                                alignItems: 'center',
                                gap: '3px',
                                padding: '3px 8px',
                                background: isCelebrated ? 'var(--noesis-accent-yellow, #f9e2af)' : 'rgba(166, 227, 161, 0.15)',
                                color: isCelebrated ? 'var(--noesis-bg-crust, #11111b)' : 'var(--noesis-accent-green, #a6e3a1)',
                                border: '1px solid var(--noesis-accent-green, #a6e3a1)',
                                borderRadius: '4px',
                                fontSize: '10.5px',
                                fontWeight: 600,
                                cursor: 'pointer',
                              }}
                            >
                              {isCelebrated ? <PartyPopper size={12} /> : <CheckCircle2 size={12} />}
                              {isCelebrated ? '攻克成功！' : '看懂了'}
                            </button>
                          ) : (
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                handleReopenConfusion(m.id);
                              }}
                              style={{
                                padding: '2px 6px',
                                background: 'transparent',
                                border: '1px solid var(--noesis-bg-surface0, #313244)',
                                color: 'var(--noesis-text-muted, #a6adc8)',
                                borderRadius: '4px',
                                fontSize: '10px',
                                cursor: 'pointer',
                              }}
                            >
                              重新激活
                            </button>
                          )}
                        </div>
                      </div>

                      {/* Code Snippet preview */}
                      <pre
                        style={{
                          margin: '6px 0',
                          padding: '6px 8px',
                          background: 'var(--noesis-bg-crust, #11111b)',
                          borderRadius: '4px',
                          fontSize: '11px',
                          color: 'var(--noesis-text-primary, #cdd6f4)',
                          overflowX: 'auto',
                          fontFamily: 'var(--noesis-font-mono, monospace)',
                        }}
                      >
                        {m.code_snippet}
                      </pre>

                      {/* If Selected: show detailed revisions ladder and line-by-line annotations */}
                      {isSelected && confusionDetail && (
                        <div style={{ marginTop: '10px', paddingTop: '10px', borderTop: '1px solid var(--noesis-bg-surface0, #313244)' }}>
                          {isLoadingDetail ? (
                            <div style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '11px', color: 'var(--noesis-accent-blue, #89b4fa)' }}>
                              <Loader2 size={12} style={{ animation: 'spin 1s linear infinite' }} />
                              加载精读详情与逐行注释...
                            </div>
                          ) : (
                            <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                              {/* Revisions ladder */}
                              {confusionDetail.revisions.length > 0 && (
                                <div style={{ display: 'flex', gap: '6px' }}>
                                  {confusionDetail.revisions.map((rev) => (
                                    <span
                                      key={rev.id}
                                      style={{
                                        padding: '2px 6px',
                                        background: rev.level === 1 ? 'rgba(137, 180, 250, 0.15)' : rev.level === 2 ? 'rgba(249, 226, 175, 0.15)' : 'rgba(243, 139, 168, 0.15)',
                                        color: rev.level === 1 ? 'var(--noesis-accent-blue, #89b4fa)' : rev.level === 2 ? 'var(--noesis-accent-yellow, #f9e2af)' : 'var(--noesis-accent-red, #f38ba8)',
                                        borderRadius: '4px',
                                        fontSize: '10px',
                                        fontWeight: 600,
                                      }}
                                    >
                                      Level {rev.level} · {rev.level === 1 ? '通俗类比' : rev.level === 2 ? '极简示例' : '物理代价'}
                                    </span>
                                  ))}
                                </div>
                              )}

                              {/* Latest Revision explanation content */}
                              {confusionDetail.revisions.slice(-1).map((lastRev) => {
                                let parsed: any = null;
                                try {
                                  parsed = JSON.parse(lastRev.explanation_json);
                                } catch {}

                                let annot: AnnotatedSnippet | null = null;
                                if (lastRev.annotated_json) {
                                  try {
                                    annot = JSON.parse(lastRev.annotated_json);
                                  } catch {}
                                }

                                return (
                                  <div key={lastRev.id} style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
                                    {parsed?.analogy && (
                                      <div style={{ fontSize: '12px', color: 'var(--noesis-text-primary, #cdd6f4)', lineHeight: '1.4' }}>
                                        <strong>💡 通俗类比：</strong>
                                        <MarkdownRenderer content={parsed.analogy} fontSize={12} />
                                      </div>
                                    )}

                                    {/* Line-by-line Annotated Code View (F6) */}
                                    {annot && annot.lines && annot.lines.length > 0 && (
                                      <div style={{ marginTop: '6px' }}>
                                        <div style={{ fontSize: '11px', fontWeight: 600, color: 'var(--noesis-accent-teal, #94e2d5)', marginBottom: '4px' }}>
                                          逐行语义精读 (点击跳行):
                                        </div>
                                        <div
                                          style={{
                                            background: 'var(--noesis-bg-crust, #11111b)',
                                            borderRadius: '4px',
                                            padding: '4px 6px',
                                            border: '1px solid var(--noesis-bg-surface0, #313244)',
                                            maxHeight: '180px',
                                            overflowY: 'auto',
                                            fontFamily: 'var(--noesis-font-mono, monospace)',
                                            fontSize: '10.5px',
                                          }}
                                        >
                                          {annot.lines.map((ln) => (
                                            <div
                                              key={ln.line_no}
                                              onClick={() => onJumpToCitation && onJumpToCitation(m.file_path, ln.line_no, ln.line_no)}
                                              style={{
                                                display: 'flex',
                                                alignItems: 'center',
                                                gap: '8px',
                                                padding: '2px 4px',
                                                cursor: 'pointer',
                                                background: ln.is_hard ? 'rgba(243, 139, 168, 0.12)' : 'transparent',
                                                borderRadius: '2px',
                                              }}
                                            >
                                              <span style={{ color: 'var(--noesis-text-subtle, #6c7086)', width: '28px', textAlign: 'right' }}>
                                                {ln.line_no}
                                              </span>
                                              <span style={{ color: 'var(--noesis-text-primary, #cdd6f4)', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                                                {ln.code}
                                              </span>
                                              <span style={{ color: 'var(--noesis-accent-lavender, #b4befe)', fontSize: '10px' }}>
                                                // {ln.comment}
                                              </span>
                                              {ln.is_hard && (
                                                <span style={{ padding: '1px 4px', background: 'var(--noesis-accent-red, #f38ba8)', color: 'var(--noesis-bg-crust, #11111b)', borderRadius: '2px', fontSize: '9px', fontWeight: 700 }}>
                                                  难点
                                                </span>
                                              )}
                                            </div>
                                          ))}
                                        </div>
                                      </div>
                                    )}
                                  </div>
                                );
                              })}
                            </div>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            ) : (
              <div style={{ textAlign: 'center', padding: '36px 12px', color: 'var(--noesis-text-subtle, #6c7086)', fontSize: '12px' }}>
                目前没有记录的代码卡点。在代码行号或选区点击「看不懂」随时沉淀难点！
              </div>
            )}
          </div>
        )}

        {/* ===================== TAB 3: CHAT (F4) ===================== */}
        {activeTab === 'chat' && (
          <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
            {/* Session switcher */}
            <div style={{ display: 'flex', alignItems: 'center', gap: '6px', marginBottom: '8px' }}>
              <select
                value={activeSessionId}
                onChange={(e) => {
                  const id = e.target.value;
                  setActiveSessionId(id);
                  if (id) loadSessionMessages(id);
                }}
                style={{
                  flex: 1,
                  padding: '4px 8px',
                  background: 'var(--noesis-bg-base, #1e1e2e)',
                  border: '1px solid var(--noesis-bg-surface0, #313244)',
                  borderRadius: '4px',
                  color: 'var(--noesis-text-primary, #cdd6f4)',
                  fontSize: '11px',
                }}
              >
                {sessions.length === 0 && <option value="">当前临时会话</option>}
                {sessions.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.title}
                  </option>
                ))}
              </select>
              <button
                onClick={handleCreateNewSession}
                title="新建独立会话"
                style={{
                  padding: '4px 8px',
                  background: 'var(--noesis-bg-surface0, #313244)',
                  border: 'none',
                  borderRadius: '4px',
                  color: 'var(--noesis-accent-blue, #89b4fa)',
                  fontSize: '11px',
                  cursor: 'pointer',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '3px',
                }}
              >
                <Plus size={12} />
                新会话
              </button>
              {activeSessionId && (
                <button
                  onClick={handleDeleteCurrentSession}
                  title="删除当前会话"
                  style={{
                    padding: '4px 6px',
                    background: 'transparent',
                    border: '1px solid rgba(243, 139, 168, 0.3)',
                    borderRadius: '4px',
                    color: 'var(--noesis-accent-red, #f38ba8)',
                    cursor: 'pointer',
                  }}
                >
                  <Trash2 size={12} />
                </button>
              )}
            </div>

            {/* Messages Scroll Area */}
            <div
              style={{
                flex: 1,
                overflowY: 'auto',
                display: 'flex',
                flexDirection: 'column',
                gap: '10px',
                marginBottom: '10px',
              }}
            >
              {chatMessages.map((m, idx) => (
                <div
                  key={idx}
                  style={{
                    alignSelf: m.role === 'user' ? 'flex-end' : 'flex-start',
                    maxWidth: '90%',
                    background: m.role === 'user' ? 'var(--noesis-accent-blue, #89b4fa)' : 'var(--noesis-bg-base, #1e1e2e)',
                    border: m.role === 'user' ? 'none' : '1px solid var(--noesis-bg-surface0, #313244)',
                    color: m.role === 'user' ? 'var(--noesis-bg-crust, #11111b)' : 'var(--noesis-text-primary, #cdd6f4)',
                    padding: '8px 12px',
                    borderRadius: 'var(--noesis-radius-md, 8px)',
                    fontSize: '12.5px',
                    lineHeight: '1.5',
                  }}
                >
                  {m.role === 'user' ? (
                    <div style={{ whiteSpace: 'pre-wrap' }}>{m.content}</div>
                  ) : (
                    <div>
                      <MarkdownRenderer content={m.content} fontSize={12} />
                      {/* Citation badges for assistant response (F4) */}
                      {m.citations && m.citations.length > 0 && (
                        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '4px', marginTop: '8px', paddingTop: '6px', borderTop: '1px solid rgba(255,255,255,0.06)' }}>
                          {m.citations.map((c, cIdx) => renderCitationBadge(c, cIdx))}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              ))}
              {isStreaming && (
                <div style={{ alignSelf: 'flex-start', color: 'var(--noesis-accent-blue, #89b4fa)', fontSize: '11px', display: 'flex', alignItems: 'center', gap: '5px' }}>
                  <Loader2 size={13} style={{ animation: 'spin 1s linear infinite' }} />
                  <span>正在流式检索并组装上下文...</span>
                </div>
              )}
            </div>

            {/* Context Chips (F4) */}
            {contextChip && (
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '6px',
                  padding: '4px 8px',
                  background: 'rgba(137, 180, 250, 0.12)',
                  border: '1px solid var(--noesis-accent-blue, #89b4fa)',
                  borderRadius: '4px',
                  fontSize: '11px',
                  color: 'var(--noesis-accent-blue, #89b4fa)',
                  marginBottom: '6px',
                }}
              >
                <span
                  onClick={() => onJumpToCitation && onJumpToCitation(contextChip.file, contextChip.range[0], contextChip.range[1])}
                  style={{ cursor: 'pointer', textDecoration: 'underline', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
                >
                  📎 携带代码上下文: {contextChip.file.split('/').pop()}:{contextChip.range[0]}-{contextChip.range[1]}
                </span>
                <button
                  onClick={onClearContextChip}
                  style={{ background: 'transparent', border: 'none', color: 'var(--noesis-accent-blue, #89b4fa)', cursor: 'pointer', padding: 0 }}
                >
                  ✕
                </button>
              </div>
            )}

            {/* Domain Quick-Question Pills (F4) */}
            <div style={{ display: 'flex', gap: '4px', overflowX: 'auto', paddingBottom: '6px', marginBottom: '4px' }}>
              {[
                '解析此节点话题收发关系',
                '分析此回调函数的线程安全性',
                '分析此 Prompt 的 RAG 链路',
                '检查工具调用的入参与异常处理',
              ].map((pill, pIdx) => (
                <button
                  key={pIdx}
                  onClick={() => handleSendStream(pill)}
                  style={{
                    whiteSpace: 'nowrap',
                    padding: '3px 8px',
                    borderRadius: '12px',
                    background: 'var(--noesis-bg-base, #1e1e2e)',
                    border: '1px solid var(--noesis-bg-surface0, #313244)',
                    color: 'var(--noesis-accent-lavender, #b4befe)',
                    fontSize: '10.5px',
                    cursor: 'pointer',
                  }}
                >
                  {pill}
                </button>
              ))}
            </div>

            {/* Preview Banner */}
            {previewData && (
              <div style={{ background: 'var(--noesis-bg-crust, #11111b)', border: '1px solid var(--noesis-accent-blue, #89b4fa)', borderRadius: '6px', padding: '8px', marginBottom: '8px', fontSize: '11px' }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', color: 'var(--noesis-accent-blue, #89b4fa)', fontWeight: 'bold' }}>
                  <span>P0 外发安全预检 ({previewData.total_chars} 字符)</span>
                  <button onClick={() => setPreviewData(null)} style={{ background: 'transparent', border: 'none', color: 'var(--noesis-text-subtle, #6c7086)', cursor: 'pointer' }}>✕</button>
                </div>
                <div style={{ color: 'var(--noesis-accent-green, #a6e3a1)', margin: '4px 0' }}>
                  已安全脱敏: {previewData.contains_sanitized_tokens ? '是 (已擦除敏感密钥/内网IP)' : '未发现敏感字段'}
                </div>
              </div>
            )}

            {/* Input Bar */}
            <div style={{ display: 'flex', gap: '6px' }}>
              <input
                type="text"
                value={chatInput}
                onChange={(e) => setChatInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.nativeEvent.isComposing && !e.shiftKey) {
                    e.preventDefault();
                    handleSendStream();
                  }
                }}
                placeholder="针对当前选区代码或架构设计提问..."
                style={{
                  flex: 1,
                  padding: '8px 10px',
                  background: 'var(--noesis-bg-base, #1e1e2e)',
                  border: '1px solid var(--noesis-bg-surface0, #313244)',
                  borderRadius: 'var(--noesis-radius-sm, 6px)',
                  color: 'var(--noesis-text-primary, #cdd6f4)',
                  fontSize: '12px',
                  outline: 'none',
                }}
              />
              <button
                onClick={handlePreview}
                title="P0 安全预检外发内容"
                style={{
                  padding: '0 8px',
                  background: 'var(--noesis-bg-surface0, #313244)',
                  border: 'none',
                  borderRadius: '6px',
                  color: 'var(--noesis-text-muted, #a6adc8)',
                  cursor: 'pointer',
                }}
              >
                <Eye size={15} />
              </button>
              <button
                onClick={() => handleSendStream()}
                disabled={isStreaming || !chatInput.trim()}
                style={{
                  padding: '0 12px',
                  background: 'var(--noesis-accent-blue, #89b4fa)',
                  border: 'none',
                  borderRadius: '6px',
                  color: 'var(--noesis-bg-crust, #11111b)',
                  cursor: isStreaming || !chatInput.trim() ? 'default' : 'pointer',
                  fontWeight: 600,
                  opacity: isStreaming || !chatInput.trim() ? 0.6 : 1,
                }}
              >
                <Send size={14} />
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
