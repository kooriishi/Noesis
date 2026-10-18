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
  Sliders,
  Loader2,
  RefreshCw,
  Trash2,
  Wifi,
} from 'lucide-react';
import {
  ExplanationCard,
  PerspectiveType,
  RewordingResult,
  ConfusionMark,
  LlmConfig,
  ChatMessage,
  OutboundPreview,
  ConnectionTestResult,
} from '../types';
import { ApiService } from '../services/api';
import { MarkdownRenderer } from '../components/MarkdownRenderer';

interface AiSidebarProps {
  cards: ExplanationCard[];
  explainError?: string | null;
  onRetryExplain?: () => void;
  confusionMarks: ConfusionMark[];
  activeRewording: RewordingResult | null;
  llmConfig: LlmConfig;
  isExplaining?: boolean;
  activeSidebarTab?: 'cards' | 'confusion' | 'chat' | 'settings';
  onUpdateLlmConfig: (config: LlmConfig) => void;
  onClearApiKey?: () => void;
  onSendChat: (messages: ChatMessage[]) => Promise<string>;
  onPreviewOutbound: (messages: ChatMessage[]) => Promise<OutboundPreview>;
  onMarkCardFeedback: (cardId: string, type: 'helpful' | 'too_generic' | 'error') => void;
}

export const AiSidebar: React.FC<AiSidebarProps> = ({
  cards,
  explainError,
  onRetryExplain,
  confusionMarks,
  activeRewording,
  llmConfig,
  isExplaining = false,
  activeSidebarTab,
  onUpdateLlmConfig,
  onClearApiKey,
  onSendChat,
  onPreviewOutbound,
  onMarkCardFeedback,
}) => {
  const [activeTab, setActiveTab] = useState<'cards' | 'confusion' | 'chat' | 'settings'>('cards');
  const [pendingKeyInput, setPendingKeyInput] = useState('');
  const [connTestResult, setConnTestResult] = useState<ConnectionTestResult | null>(null);
  const [isTestingConn, setIsTestingConn] = useState(false);

  useEffect(() => {
    if (activeSidebarTab) {
      setActiveTab(activeSidebarTab);
    }
  }, [activeSidebarTab]);

  const [activePerspective, setActivePerspective] = useState<PerspectiveType | 'all'>('all');
  const [expandedCards, setExpandedCards] = useState<Record<string, boolean>>({});

  // Chat state
  const [chatInput, setChatInput] = useState('');
  const [chatMessages, setChatMessages] = useState<ChatMessage[]>([
    {
      role: 'assistant',
      content:
        '👋 你好！我是 NOESIS 代码深度导读助手。在编辑器中选中任意代码片段，即可一键生成「整体、逻辑、语法、原理」四层解读，或标记看不懂换个通俗讲法！',
    },
  ]);
  const [isSending, setIsSending] = useState(false);
  const [previewData, setPreviewData] = useState<OutboundPreview | null>(null);

  const toggleExpand = (id: string) => {
    setExpandedCards((prev) => ({ ...prev, [id]: !prev[id] }));
  };

  const handleSend = async () => {
    if (!chatInput.trim() || isSending) return;
    const newMsg: ChatMessage = { role: 'user', content: chatInput.trim() };
    const updated = [...chatMessages, newMsg];
    setChatMessages(updated);
    setChatInput('');
    setIsSending(true);

    try {
      const resp = await onSendChat(updated);
      setChatMessages((prev) => [...prev, { role: 'assistant', content: resp }]);
    } catch (err: any) {
      setChatMessages((prev) => [
        ...prev,
        { role: 'assistant', content: `❌ 请求出错: ${err?.message || err}` },
      ]);
    } finally {
      setIsSending(false);
    }
  };

  const handlePreview = async () => {
    if (!chatInput.trim()) return;
    const testMsg: ChatMessage[] = [...chatMessages, { role: 'user', content: chatInput.trim() }];
    try {
      const p = await onPreviewOutbound(testMsg);
      setPreviewData(p);
    } catch (err: any) {
      alert(`安全预检拦截: ${err}`);
    }
  };

  const handleSaveApiKey = () => {
    if (!pendingKeyInput.trim()) return;
    onUpdateLlmConfig({ ...llmConfig, api_key: pendingKeyInput.trim() });
    setPendingKeyInput('');
  };

  const handleTestConnection = async () => {
    setIsTestingConn(true);
    setConnTestResult(null);
    try {
      const cfg = pendingKeyInput.trim()
        ? { ...llmConfig, api_key: pendingKeyInput.trim() }
        : llmConfig;
      const res = await ApiService.testLlmConnection(cfg);
      setConnTestResult(res);
    } catch (err: any) {
      setConnTestResult({
        ok: false,
        latency_ms: 0,
        category: 'network',
        message: err?.message || String(err),
      });
    } finally {
      setIsTestingConn(false);
    }
  };

  const filteredCards =
    activePerspective === 'all'
      ? cards
      : cards.filter((c) => c.perspective === activePerspective);

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        background: '#181825',
        borderLeft: '1px solid #313244',
      }}
    >
      {/* Header Tabs */}
      <div
        style={{
          display: 'flex',
          background: '#11111b',
          borderBottom: '1px solid #313244',
        }}
      >
        {[
          { id: 'cards', label: '四层解读', icon: Sparkles },
          { id: 'confusion', label: '看不懂答疑', icon: HelpCircle },
          { id: 'chat', label: '智能问答', icon: MessageSquare },
          { id: 'settings', label: '模型/安全', icon: Sliders },
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
                background: isActive ? '#181825' : 'transparent',
                color: isActive ? '#89b4fa' : '#6c7086',
                border: 'none',
                borderBottom: isActive ? '2px solid #89b4fa' : '2px solid transparent',
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
        {activeTab === 'cards' && (
          <div>
            {/* Perspective Filter Pills */}
            <div style={{ display: 'flex', gap: '4px', marginBottom: '12px', flexWrap: 'wrap' }}>
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
                      borderColor: isSelected ? '#89b4fa' : '#313244',
                      background: isSelected ? 'rgba(137, 180, 250, 0.15)' : '#1e1e2e',
                      color: isSelected ? '#89b4fa' : '#a6adc8',
                      cursor: 'pointer',
                    }}
                  >
                    {p.label}
                  </button>
                );
              })}
            </div>

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
                  border: '1px solid #89b4fa',
                  color: '#89b4fa',
                  fontSize: '11.5px',
                }}
              >
                <Loader2 size={14} style={{ animation: 'spin 1s linear infinite' }} />
                <span>正在根据选区代码与工程上下文调用大模型进行四层深度剖析...</span>
              </div>
            )}

            {/* Explicit Error Banner + Retry Button (A4) */}
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
                  color: '#f38ba8',
                  fontSize: '11.5px',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: '6px', flex: 1 }}>
                  <AlertCircle size={15} style={{ flexShrink: 0 }} />
                  <span>{explainError}</span>
                </div>
                {onRetryExplain && (
                  <button
                    onClick={onRetryExplain}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '4px',
                      padding: '4px 8px',
                      background: '#f38ba8',
                      color: '#11111b',
                      border: 'none',
                      borderRadius: '4px',
                      fontSize: '11px',
                      fontWeight: 600,
                      cursor: 'pointer',
                      flexShrink: 0,
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
                  const isExpanded = !!expandedCards[card.id];
                  return (
                    <div
                      key={card.id}
                      style={{
                        background: '#1e1e2e',
                        border: '1px solid #313244',
                        borderRadius: '8px',
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
                                ? '#89b4fa'
                                : card.perspective === 'logic'
                                ? '#a6e3a1'
                                : card.perspective === 'syntax'
                                ? '#f9e2af'
                                : '#f38ba8',
                          }}
                        >
                          {card.perspective.toUpperCase()} · {card.title}
                        </span>

                        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                          {/* Explicit Template Watermark (A4) */}
                          {card.is_template && (
                            <span
                              style={{
                                fontSize: '10px',
                                padding: '1px 6px',
                                borderRadius: '4px',
                                background: 'rgba(249, 226, 175, 0.15)',
                                border: '1px solid rgba(249, 226, 175, 0.4)',
                                color: '#f9e2af',
                                fontWeight: 600,
                              }}
                            >
                              模板占位，非 AI 解读
                            </span>
                          )}

                          {/* Source Reference Match Indicator (A5) */}
                          {card.source_matches ? (
                            <span
                              style={{
                                fontSize: '10px',
                                color: '#a6e3a1',
                                display: 'flex',
                                alignItems: 'center',
                                gap: '3px',
                              }}
                            >
                              <ShieldCheck size={12} />
                              选区原文一致
                            </span>
                          ) : (
                            <span
                              style={{
                                fontSize: '10px',
                                color: '#fab387',
                                display: 'flex',
                                alignItems: 'center',
                                gap: '3px',
                              }}
                            >
                              <AlertCircle size={12} />
                              选区原文待核对
                            </span>
                          )}
                        </div>
                      </div>

                      {/* One sentence summary */}
                      <div
                        style={{
                          fontSize: '13px',
                          fontWeight: 600,
                          color: '#cdd6f4',
                          marginTop: '8px',
                          lineHeight: '1.4',
                        }}
                      >
                        {card.one_sentence}
                      </div>

                      {/* 3-5 bullet points */}
                      <ul
                        style={{
                          margin: '8px 0',
                          paddingLeft: '16px',
                          fontSize: '12px',
                          color: '#a6adc8',
                          lineHeight: '1.5',
                        }}
                      >
                        {card.key_bullets.map((b, i) => (
                          <li key={i}>{b}</li>
                        ))}
                      </ul>

                      {/* Source basis location */}
                      <div style={{ fontSize: '10.5px', color: '#6c7086', marginTop: '4px' }}>
                        📍 选区位置: {card.source.file_path}:{card.source.line_start}-
                        {card.source.line_end}
                      </div>

                      {/* Expand Details */}
                      {isExpanded && (
                        <div
                          style={{
                            marginTop: '10px',
                            paddingTop: '10px',
                            borderTop: '1px solid #313244',
                            fontSize: '12px',
                            color: '#bac2de',
                            lineHeight: '1.6',
                          }}
                        >
                          <MarkdownRenderer content={card.detailed_body} fontSize={12} />
                        </div>
                      )}

                      {/* Card Footer Actions */}
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
                            color: '#89b4fa',
                            fontSize: '11px',
                            cursor: 'pointer',
                            fontWeight: 500,
                          }}
                        >
                          {isExpanded ? <ChevronUp size={13} /> : <ChevronDown size={13} />}
                          {isExpanded ? '收起详述' : '展开详细原理解析'}
                        </button>

                        <div style={{ display: 'flex', gap: '6px' }}>
                          <button
                            onClick={() => onMarkCardFeedback(card.id, 'helpful')}
                            title="标记有深度"
                            style={{
                              background: '#313244',
                              border: 'none',
                              borderRadius: '4px',
                              padding: '3px 6px',
                              color: '#a6e3a1',
                              fontSize: '10px',
                              cursor: 'pointer',
                            }}
                          >
                            👍 有用
                          </button>
                          <button
                            onClick={() => onMarkCardFeedback(card.id, 'too_generic')}
                            title="标记讲得太泛"
                            style={{
                              background: '#313244',
                              border: 'none',
                              borderRadius: '4px',
                              padding: '3px 6px',
                              color: '#f9e2af',
                              fontSize: '10px',
                              cursor: 'pointer',
                            }}
                          >
                            太泛
                          </button>
                          <button
                            onClick={() => onMarkCardFeedback(card.id, 'error')}
                            title="标记有源码依据偏差"
                            style={{
                              background: '#313244',
                              border: 'none',
                              borderRadius: '4px',
                              padding: '3px 6px',
                              color: '#f38ba8',
                              fontSize: '10px',
                              cursor: 'pointer',
                            }}
                          >
                            有误
                          </button>
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              <div
                style={{
                  textAlign: 'center',
                  padding: '32px 12px',
                  color: '#6c7086',
                  fontSize: '12.5px',
                }}
              >
                <Sparkles size={32} style={{ margin: '0 auto 10px', opacity: 0.4 }} />
                在左侧代码编辑器中选择任意代码片段，
                <br />
                点击浮动气泡中的「深度解读」，
                <br />
                即可生成四层结构化导读卡片！
              </div>
            )}
          </div>
        )}

        {activeTab === 'confusion' && (
          <div>
            {activeRewording && (
              <div
                style={{
                  background: 'rgba(243, 139, 168, 0.08)',
                  border: '1px solid rgba(243, 139, 168, 0.3)',
                  borderRadius: '8px',
                  padding: '12px',
                  marginBottom: '14px',
                }}
              >
                <div
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    alignItems: 'center',
                    marginBottom: '6px',
                  }}
                >
                  <div style={{ fontSize: '12px', fontWeight: 'bold', color: '#f38ba8' }}>
                    💡 针对「{activeRewording.original_category.toUpperCase()}」维度的换讲法
                  </div>
                  {activeRewording.is_template && (
                    <span
                      style={{
                        fontSize: '10px',
                        padding: '1px 6px',
                        borderRadius: '4px',
                        background: 'rgba(249, 226, 175, 0.15)',
                        color: '#f9e2af',
                      }}
                    >
                      模板占位，非 AI 解读
                    </span>
                  )}
                </div>

                <div style={{ fontSize: '11px', color: '#f9e2af', fontWeight: 600 }}>生活类比：</div>
                <div
                  style={{
                    fontSize: '12px',
                    color: '#cdd6f4',
                    marginBottom: '8px',
                    lineHeight: '1.4',
                  }}
                >
                  {activeRewording.analogy}
                </div>

                <div style={{ fontSize: '11px', color: '#a6e3a1', fontWeight: 600 }}>最小示例：</div>
                <pre
                  style={{
                    background: '#11111b',
                    padding: '8px',
                    borderRadius: '4px',
                    fontSize: '11px',
                    color: '#cdd6f4',
                    overflowX: 'auto',
                    margin: '4px 0 8px',
                  }}
                >
                  {activeRewording.minimal_example}
                </pre>

                <div style={{ fontSize: '11px', color: '#89b4fa', fontWeight: 600 }}>前置概念：</div>
                <div style={{ fontSize: '11.5px', color: '#a6adc8', marginBottom: '8px' }}>
                  {activeRewording.prerequisite_concept}
                </div>

                <div
                  style={{
                    fontSize: '11px',
                    color: '#cdd6f4',
                    background: '#313244',
                    padding: '6px 8px',
                    borderRadius: '4px',
                  }}
                >
                  🧭 建议：{activeRewording.guidance}
                </div>
              </div>
            )}

            <div
              style={{
                fontSize: '12px',
                fontWeight: 600,
                color: '#cdd6f4',
                marginBottom: '8px',
              }}
            >
              📚 卡点与「看不懂」学习档案 ({confusionMarks.length})
            </div>

            {confusionMarks.length > 0 ? (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                {confusionMarks.map((mark) => {
                  let parsedAi: RewordingResult | null = null;
                  if (mark.ai_explanation) {
                    try {
                      parsedAi = JSON.parse(mark.ai_explanation);
                    } catch {
                      parsedAi = null;
                    }
                  }
                  return (
                    <div
                      key={mark.id}
                      style={{
                        background: '#1e1e2e',
                        border: '1px solid #313244',
                        borderRadius: '6px',
                        padding: '10px',
                      }}
                    >
                      <div
                        style={{
                          display: 'flex',
                          justifyContent: 'space-between',
                          alignItems: 'center',
                        }}
                      >
                        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                          <span
                            style={{
                              fontSize: '10px',
                              padding: '2px 5px',
                              borderRadius: '3px',
                              background: '#f38ba8',
                              color: '#11111b',
                              fontWeight: 'bold',
                            }}
                          >
                            {mark.category}
                          </span>
                          {mark.model && (
                            <span style={{ fontSize: '10px', color: '#a6e3a1' }}>
                              AI ({mark.model})
                            </span>
                          )}
                        </div>
                        <span style={{ fontSize: '10px', color: '#6c7086' }}>
                          {mark.created_at.slice(0, 10)}
                        </span>
                      </div>

                      <div style={{ fontSize: '11px', color: '#89b4fa', marginTop: '4px' }}>
                        {mark.file_path}:{mark.line_start}-{mark.line_end}
                      </div>

                      <pre
                        style={{
                          background: '#11111b',
                          padding: '6px',
                          borderRadius: '4px',
                          fontSize: '10.5px',
                          color: '#a6adc8',
                          margin: '6px 0',
                          overflowX: 'auto',
                        }}
                      >
                        {mark.code_snippet}
                      </pre>

                      {parsedAi ? (
                        <div style={{ fontSize: '11px', color: '#cdd6f4', lineHeight: '1.45' }}>
                          <div>
                            <strong>类比：</strong>
                            {parsedAi.analogy}
                          </div>
                          <div style={{ color: '#a6adc8', marginTop: '3px' }}>
                            <strong>前置概念：</strong>
                            {parsedAi.prerequisite_concept}
                          </div>
                        </div>
                      ) : (
                        mark.explanation && (
                          <div style={{ fontSize: '11px', color: '#cdd6f4', lineHeight: '1.4' }}>
                            {mark.explanation}
                          </div>
                        )
                      )}
                    </div>
                  );
                })}
              </div>
            ) : (
              <div
                style={{
                  textAlign: 'center',
                  padding: '24px 8px',
                  color: '#6c7086',
                  fontSize: '12px',
                }}
              >
                目前没有未解决的代码卡点记录。
                <br />
                随时在代码中点击「看不懂」来记录和复盘。
              </div>
            )}
          </div>
        )}

        {activeTab === 'chat' && (
          <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
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
                    maxWidth: '88%',
                    background: m.role === 'user' ? '#89b4fa' : '#1e1e2e',
                    border: m.role === 'user' ? 'none' : '1px solid #313244',
                    color: m.role === 'user' ? '#11111b' : '#cdd6f4',
                    padding: '8px 12px',
                    borderRadius: '8px',
                    fontSize: '12.5px',
                    lineHeight: '1.5',
                  }}
                >
                  {m.role === 'user' ? (
                    <div style={{ whiteSpace: 'pre-wrap' }}>{m.content}</div>
                  ) : (
                    <MarkdownRenderer content={m.content} fontSize={12} />
                  )}
                </div>
              ))}
              {isSending && (
                <div
                  style={{
                    alignSelf: 'flex-start',
                    color: '#a6adc8',
                    fontSize: '11px',
                    display: 'flex',
                    alignItems: 'center',
                    gap: '5px',
                  }}
                >
                  <Loader2 size={13} style={{ animation: 'spin 1s linear infinite' }} />
                  <span>深度模型正在思考中...</span>
                </div>
              )}
            </div>

            {previewData && (
              <div
                style={{
                  background: '#11111b',
                  border: '1px solid #89b4fa',
                  borderRadius: '6px',
                  padding: '8px',
                  marginBottom: '8px',
                  fontSize: '11px',
                }}
              >
                <div
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    color: '#89b4fa',
                    fontWeight: 'bold',
                  }}
                >
                  <span>P0 外发安全预检 ({previewData.total_chars} 字符)</span>
                  <button
                    onClick={() => setPreviewData(null)}
                    style={{
                      background: 'transparent',
                      border: 'none',
                      color: '#6c7086',
                      cursor: 'pointer',
                    }}
                  >
                    ✕
                  </button>
                </div>
                <div style={{ color: '#a6e3a1', margin: '4px 0' }}>
                  已安全脱敏:{' '}
                  {previewData.contains_sanitized_tokens
                    ? '是 (已擦除敏感密钥/内网IP)'
                    : '未发现敏感字段'}
                </div>
                <div style={{ color: '#6c7086', fontSize: '10px' }}>
                  即将发往: {previewData.target_url}
                </div>
              </div>
            )}

            <div style={{ display: 'flex', gap: '6px' }}>
              <input
                type="text"
                value={chatInput}
                onChange={(e) => setChatInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.nativeEvent.isComposing && !e.shiftKey) {
                    e.preventDefault();
                    handleSend();
                  }
                }}
                placeholder="针对当前项目或卡片提问..."
                style={{
                  flex: 1,
                  padding: '8px 10px',
                  background: '#1e1e2e',
                  border: '1px solid #313244',
                  borderRadius: '6px',
                  color: '#cdd6f4',
                  fontSize: '12px',
                  outline: 'none',
                }}
              />
              <button
                onClick={handlePreview}
                title="P0 安全预检外发内容"
                style={{
                  padding: '0 8px',
                  background: '#313244',
                  border: 'none',
                  borderRadius: '6px',
                  color: '#a6adc8',
                  cursor: 'pointer',
                }}
              >
                <Eye size={15} />
              </button>
              <button
                onClick={handleSend}
                disabled={isSending}
                style={{
                  padding: '0 12px',
                  background: '#89b4fa',
                  border: 'none',
                  borderRadius: '6px',
                  color: '#11111b',
                  cursor: 'pointer',
                  fontWeight: 600,
                }}
              >
                <Send size={14} />
              </button>
            </div>
          </div>
        )}

        {activeTab === 'settings' && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
            <div style={{ fontSize: '13px', fontWeight: 600, color: '#cdd6f4' }}>
              模型服务与安全凭据管理 (Rust 安全存储)
            </div>

            <div
              style={{
                background: '#1e1e2e',
                border: '1px solid #313244',
                borderRadius: '8px',
                padding: '12px',
              }}
            >
              <label
                style={{
                  fontSize: '11px',
                  color: '#a6adc8',
                  display: 'block',
                  marginBottom: '4px',
                }}
              >
                模型提供方 (Provider)
              </label>
              <select
                value={llmConfig.provider}
                onChange={(e) => onUpdateLlmConfig({ ...llmConfig, provider: e.target.value })}
                style={{
                  width: '100%',
                  padding: '6px',
                  background: '#313244',
                  border: 'none',
                  borderRadius: '4px',
                  color: '#cdd6f4',
                  fontSize: '12px',
                  marginBottom: '10px',
                }}
              >
                <option value="deepseek">DeepSeek (官方兼容接口)</option>
                <option value="openai">OpenAI</option>
                <option value="ollama">Ollama (本地私有化模型)</option>
                <option value="custom">公司内网自定义兼容地址</option>
              </select>

              <label
                style={{
                  fontSize: '11px',
                  color: '#a6adc8',
                  display: 'block',
                  marginBottom: '4px',
                }}
              >
                接口基础地址 (API URL)
              </label>
              <input
                type="text"
                value={llmConfig.api_url}
                onChange={(e) => onUpdateLlmConfig({ ...llmConfig, api_url: e.target.value })}
                placeholder="https://api.deepseek.com"
                style={{
                  width: '100%',
                  boxSizing: 'border-box',
                  padding: '6px',
                  background: '#313244',
                  border: 'none',
                  borderRadius: '4px',
                  color: '#cdd6f4',
                  fontSize: '12px',
                  marginBottom: '10px',
                }}
              />

              <label
                style={{
                  fontSize: '11px',
                  color: '#a6adc8',
                  display: 'block',
                  marginBottom: '4px',
                }}
              >
                模型名称 (Model)
              </label>
              <input
                type="text"
                value={llmConfig.model}
                onChange={(e) => onUpdateLlmConfig({ ...llmConfig, model: e.target.value })}
                placeholder="deepseek-chat"
                style={{
                  width: '100%',
                  boxSizing: 'border-box',
                  padding: '6px',
                  background: '#313244',
                  border: 'none',
                  borderRadius: '4px',
                  color: '#cdd6f4',
                  fontSize: '12px',
                  marginBottom: '10px',
                }}
              />

              <label
                style={{
                  fontSize: '11px',
                  color: '#a6adc8',
                  display: 'block',
                  marginBottom: '4px',
                }}
              >
                API 密钥状态:{' '}
                <strong style={{ color: llmConfig.has_api_key ? '#a6e3a1' : '#f9e2af' }}>
                  {llmConfig.has_api_key
                    ? `已安全存储 (${llmConfig.api_key_masked})`
                    : '未配置'}
                </strong>
              </label>

              <div style={{ display: 'flex', gap: '6px', marginBottom: '8px' }}>
                <input
                  type="password"
                  value={pendingKeyInput}
                  onChange={(e) => setPendingKeyInput(e.target.value)}
                  placeholder={
                    llmConfig.has_api_key
                      ? '输入新密钥以覆盖现有凭据...'
                      : '输入 sk-... 密钥存入安全存储'
                  }
                  style={{
                    flex: 1,
                    padding: '6px',
                    background: '#313244',
                    border: 'none',
                    borderRadius: '4px',
                    color: '#cdd6f4',
                    fontSize: '12px',
                  }}
                />
                <button
                  onClick={handleSaveApiKey}
                  disabled={!pendingKeyInput.trim()}
                  style={{
                    padding: '6px 10px',
                    background: '#89b4fa',
                    color: '#11111b',
                    border: 'none',
                    borderRadius: '4px',
                    fontSize: '11px',
                    fontWeight: 600,
                    cursor: pendingKeyInput.trim() ? 'pointer' : 'default',
                  }}
                >
                  保存密钥
                </button>
              </div>

              <div style={{ display: 'flex', gap: '6px' }}>
                <button
                  onClick={handleTestConnection}
                  disabled={isTestingConn}
                  style={{
                    flex: 1,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    gap: '4px',
                    padding: '6px',
                    background: '#313244',
                    color: '#cdd6f4',
                    border: 'none',
                    borderRadius: '4px',
                    fontSize: '11px',
                    cursor: 'pointer',
                  }}
                >
                  <Wifi size={12} />
                  {isTestingConn ? '测试连接中...' : '测试连接'}
                </button>

                {llmConfig.has_api_key && onClearApiKey && (
                  <button
                    onClick={onClearApiKey}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '4px',
                      padding: '6px 10px',
                      background: 'rgba(243, 139, 168, 0.15)',
                      color: '#f38ba8',
                      border: '1px solid rgba(243, 139, 168, 0.35)',
                      borderRadius: '4px',
                      fontSize: '11px',
                      cursor: 'pointer',
                    }}
                  >
                    <Trash2 size={12} />
                    清除密钥
                  </button>
                )}
              </div>

              {connTestResult && (
                <div
                  style={{
                    marginTop: '8px',
                    padding: '6px 8px',
                    borderRadius: '4px',
                    fontSize: '11px',
                    background: connTestResult.ok
                      ? 'rgba(166, 227, 161, 0.12)'
                      : 'rgba(243, 139, 168, 0.12)',
                    color: connTestResult.ok ? '#a6e3a1' : '#f38ba8',
                  }}
                >
                  {connTestResult.message}
                </div>
              )}
            </div>

            <div
              style={{
                background: 'rgba(166, 227, 161, 0.08)',
                border: '1px solid rgba(166, 227, 161, 0.3)',
                borderRadius: '8px',
                padding: '12px',
              }}
            >
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '6px',
                  color: '#a6e3a1',
                  fontSize: '12px',
                  fontWeight: 'bold',
                  marginBottom: '6px',
                }}
              >
                <ShieldCheck size={16} />
                P0 级代码防外泄控制生效中
              </div>
              <ul
                style={{
                  margin: 0,
                  paddingLeft: '16px',
                  fontSize: '11px',
                  color: '#a6adc8',
                  lineHeight: '1.6',
                }}
              >
                <li>API 密钥由 Rust 后端独立加密/权限管控，绝不写入 localStorage</li>
                <li>严禁读取或外发 .env、.ssh、私钥证书等敏感文件</li>
                <li>自动正则脱敏密码、内网 IP、Bearer Token</li>
                <li>外发量限制：单次上限 100,000 字符</li>
              </ul>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
