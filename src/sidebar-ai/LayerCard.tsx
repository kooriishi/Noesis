import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  ChevronDown,
  ChevronRight,
  RefreshCw,
  Copy,
  Check,
  HelpCircle,
  AlertTriangle,
  Loader2,
  ShieldCheck,
} from 'lucide-react';
import { MarkdownRenderer } from '../components/MarkdownRenderer';
import type { CitationItem, LayerQualityReport, PerspectiveType } from '../types';

export type LayerStatus = 'idle' | 'loading' | 'streaming' | 'done' | 'error';

export interface LayerCardState {
  layer: PerspectiveType;
  status: LayerStatus;
  markdown: string;
  quality?: LayerQualityReport;
  citations?: CitationItem[];
  concepts?: string[];
  error?: string;
  cached?: boolean;
  explanationId?: string;
}

export interface LayerCardProps {
  state: LayerCardState;
  onRegenerate?: (layer: PerspectiveType) => void;
  onMarkConfusion?: (layer: PerspectiveType, markdown: string) => void;
  onFeedback?: (layer: PerspectiveType, type: 'helpful' | 'too_generic' | 'error') => void;
  onJumpToCitation?: (filePath: string, startLine: number, endLine: number) => void;
  currentFilePath?: string;
  fontSize?: number;
}

const LAYER_META: Record<
  PerspectiveType,
  { label: string; color: string; bg: string }
> = {
  overall: { label: '整体', color: 'var(--noesis-accent-blue)', bg: 'rgba(137, 180, 250, 0.18)' },
  logic: { label: '逻辑', color: 'var(--noesis-accent-green)', bg: 'rgba(166, 227, 161, 0.18)' },
  syntax: { label: '语法', color: 'var(--noesis-accent-yellow)', bg: 'rgba(249, 226, 175, 0.18)' },
  principle: { label: '原理', color: 'var(--noesis-accent-mauve)', bg: 'rgba(203, 166, 247, 0.18)' },
};

/** 从 Markdown 里按二级标题切分成若干节，用于「首屏只展开前两节」的折叠展示。 */
function splitSections(markdown: string): Array<{ title: string; body: string }> {
  const lines = markdown.split('\n');
  const sections: Array<{ title: string; body: string }> = [];
  let currentTitle = '';
  let buffer: string[] = [];

  const flush = () => {
    if (currentTitle || buffer.join('').trim()) {
      sections.push({ title: currentTitle, body: buffer.join('\n').trim() });
    }
  };

  for (const line of lines) {
    if (/^##\s+/.test(line)) {
      flush();
      currentTitle = line.replace(/^##\s+/, '').trim();
      buffer = [];
    } else {
      buffer.push(line);
    }
  }
  flush();
  return sections.filter((s) => s.title || s.body);
}

/** 解析语法层输出里的逐行注释 JSON 数组（```json 块）。 */
function extractAnnotatedLines(
  markdown: string
): Array<{ line_no: number; code: string; comment: string }> {
  const match = markdown.match(/```json\s*([\s\S]*?)```/);
  const candidate = match ? match[1] : markdown;
  const start = candidate.indexOf('[');
  const end = candidate.lastIndexOf(']');
  if (start < 0 || end <= start) return [];
  try {
    const parsed = JSON.parse(candidate.slice(start, end + 1));
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((x: any) => x && typeof x.line_no === 'number')
      .map((x: any) => ({
        line_no: Number(x.line_no),
        code: String(x.code ?? x.text ?? ''),
        comment: String(x.comment ?? x.explanation ?? ''),
      }));
  } catch {
    return [];
  }
}

export const LayerCard: React.FC<LayerCardProps> = ({
  state,
  onRegenerate,
  onMarkConfusion,
  onFeedback,
  onJumpToCitation,
  currentFilePath,
  fontSize = 12,
}) => {
  const meta = LAYER_META[state.layer] ?? LAYER_META.overall;
  const [copied, setCopied] = useState(false);
  const [expandedSections, setExpandedSections] = useState<Record<string, boolean>>({});
  const bodyRef = useRef<HTMLDivElement>(null);

  const sections = useMemo(() => splitSections(state.markdown || ''), [state.markdown]);
  const annotatedLines = useMemo(
    () => (state.layer === 'syntax' ? extractAnnotatedLines(state.markdown || '') : []),
    [state.layer, state.markdown]
  );

  // 原理层判定为「不适用」时折叠成一行灰字
  const isNotApplicable =
    state.layer === 'principle' &&
    state.status === 'done' &&
    /不适用/.test(state.markdown || '');

  useEffect(() => {
    // 生成过程中保持滚动到底部，让用户看到正在增长的内容
    if (state.status === 'streaming' && bodyRef.current) {
      const el = bodyRef.current;
      const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
      if (nearBottom) el.scrollTop = el.scrollHeight;
    }
  }, [state.markdown, state.status]);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(state.markdown || '');
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // ignore
    }
  };

  const statusBadge = () => {
    if (state.status === 'loading') {
      return (
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, fontSize: 10.5, color: 'var(--noesis-text-subtle)' }}>
          <Loader2 size={11} style={{ animation: 'spin 1s linear infinite' }} />
          准备上下文…
        </span>
      );
    }
    if (state.status === 'streaming') {
      return (
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, fontSize: 10.5, color: meta.color }}>
          <span className="noesis-caret" style={{ display: 'inline-block', width: 6, height: 11, background: meta.color }} />
          生成中 · {(state.markdown || '').length} 字
        </span>
      );
    }
    if (state.status === 'error') {
      return (
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, fontSize: 10.5, color: 'var(--noesis-accent-red)' }}>
          <AlertTriangle size={11} />
          {state.error || '生成失败'}
        </span>
      );
    }
    if (state.status === 'done' && state.quality) {
      return state.quality.passed ? (
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, fontSize: 10.5, color: 'var(--noesis-accent-green)' }}>
          <ShieldCheck size={11} />
          质检通过 · {state.quality.word_count} 字
        </span>
      ) : (
        <span
          style={{ display: 'inline-flex', alignItems: 'center', gap: 4, fontSize: 10.5, color: 'var(--noesis-accent-peach)' }}
          title={(state.quality.issues || []).join('；')}
        >
          <AlertTriangle size={11} />
          未通过：{(state.quality.issues || [])[0] || '内容不完整'}
        </span>
      );
    }
    return null;
  };

  if (isNotApplicable) {
    const reason = (state.markdown || '').replace(/[#*`]/g, '').replace(/\s+/g, ' ').trim().slice(0, 160);
    return (
      <div
        style={{
          padding: '8px 12px',
          borderRadius: 'var(--noesis-radius-md)',
          border: '1px dashed var(--noesis-bg-surface1)',
          color: 'var(--noesis-text-subtle)',
          fontSize: 11.5,
        }}
      >
        【原理】本层判定为不适用：{reason}
      </div>
    );
  }

  const shouldCollapse = sections.length > 2;
  const visibleSections = shouldCollapse
    ? sections.map((sec, idx) => ({
        ...sec,
        open: idx < 2 || expandedSections[`${idx}:${sec.title}`] === true,
      }))
    : sections.map((sec) => ({ ...sec, open: true }));

  return (
    <div
      style={{
        background: 'var(--noesis-bg-base)',
        border: '1px solid var(--noesis-bg-surface0)',
        borderRadius: 'var(--noesis-radius-md)',
        overflow: 'hidden',
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 8,
          padding: '8px 10px',
          borderBottom: '1px solid var(--noesis-bg-surface0)',
          background: 'var(--noesis-bg-mantle)',
          flexWrap: 'wrap',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span
            style={{
              fontSize: 10.5,
              fontWeight: 700,
              padding: '2px 7px',
              borderRadius: 4,
              background: meta.bg,
              color: meta.color,
            }}
          >
            {state.layer.toUpperCase()} · {meta.label}
          </span>
          {statusBadge()}
          {state.cached && (
            <span style={{ fontSize: 10, color: 'var(--noesis-text-subtle)' }}>缓存</span>
          )}
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          <button
            onClick={() => onRegenerate?.(state.layer)}
            title="重新生成本层"
            style={iconBtnStyle}
          >
            <RefreshCw size={12} />
          </button>
          <button onClick={handleCopy} title="复制本层 Markdown" style={iconBtnStyle}>
            {copied ? <Check size={12} color="var(--noesis-accent-green)" /> : <Copy size={12} />}
          </button>
        </div>
      </div>

      {/* Body */}
      <div ref={bodyRef} style={{ padding: '10px 12px', maxHeight: '70vh', overflowY: 'auto' }}>
        {state.status === 'error' ? (
          <div style={{ fontSize: 11.5, color: 'var(--noesis-accent-red)' }}>
            {state.error}
          </div>
        ) : !state.markdown ? (
          <div style={{ fontSize: 11.5, color: 'var(--noesis-text-subtle)' }}>
            {state.status === 'idle' ? '尚未生成' : '正在等待模型输出…'}
          </div>
        ) : (
          <>
            {visibleSections.map((sec, idx) => (
              <div key={`${idx}-${sec.title}`} style={{ marginBottom: 10 }}>
                {sec.title && (
                  <button
                    onClick={() =>
                      shouldCollapse &&
                      setExpandedSections((prev) => ({
                        ...prev,
                        [`${idx}:${sec.title}`]: !(prev[`${idx}:${sec.title}`] ?? idx < 2),
                      }))
                    }
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: 4,
                      background: 'transparent',
                      border: 'none',
                      padding: 0,
                      marginBottom: 4,
                      color: 'var(--noesis-text-primary)',
                      fontSize: 12,
                      fontWeight: 600,
                      cursor: shouldCollapse ? 'pointer' : 'default',
                    }}
                  >
                    {shouldCollapse &&
                      (sec.open ? <ChevronDown size={12} /> : <ChevronRight size={12} />)}
                    {sec.title}
                  </button>
                )}
                {sec.open && (
                  <MarkdownRenderer
                    content={sec.body}
                    fontSize={fontSize}
                    currentFilePath={currentFilePath}
                    onJumpToCitation={onJumpToCitation}
                  />
                )}
              </div>
            ))}

            {/* 语法层逐行注释表 */}
            {annotatedLines.length > 0 && (
              <div style={{ marginTop: 8 }}>
                <div style={{ fontSize: 11.5, fontWeight: 600, color: 'var(--noesis-accent-teal)', marginBottom: 4 }}>
                  逐行注释（{annotatedLines.length} 行）
                </div>
                <div
                  style={{
                    border: '1px solid var(--noesis-bg-surface0)',
                    borderRadius: 4,
                    overflow: 'hidden',
                  }}
                >
                  <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 11 }}>
                    <thead>
                      <tr style={{ background: 'var(--noesis-bg-crust)' }}>
                        <th style={thStyle}>行号</th>
                        <th style={thStyle}>代码</th>
                        <th style={thStyle}>注释</th>
                      </tr>
                    </thead>
                    <tbody>
                      {annotatedLines.map((ln) => (
                        <tr
                          key={ln.line_no}
                          onClick={() =>
                            currentFilePath &&
                            onJumpToCitation?.(currentFilePath, ln.line_no, ln.line_no)
                          }
                          style={{ cursor: currentFilePath ? 'pointer' : 'default' }}
                        >
                          <td style={{ ...tdStyle, color: 'var(--noesis-text-subtle)', textAlign: 'right', width: 44 }}>
                            {ln.line_no}
                          </td>
                          <td
                            style={{
                              ...tdStyle,
                              fontFamily: 'var(--noesis-font-mono)',
                              whiteSpace: 'pre-wrap',
                              wordBreak: 'break-all',
                            }}
                          >
                            {ln.code}
                          </td>
                          <td style={{ ...tdStyle, color: 'var(--noesis-accent-lavender)' }}>
                            {ln.comment}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {/* Footer */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 6,
          padding: '6px 10px',
          borderTop: '1px solid var(--noesis-bg-surface0)',
          background: 'var(--noesis-bg-mantle)',
          flexWrap: 'wrap',
        }}
      >
        <button
          onClick={() => onMarkConfusion?.(state.layer, state.markdown || '')}
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            gap: 4,
            background: 'transparent',
            border: 'none',
            color: 'var(--noesis-accent-peach)',
            fontSize: 10.5,
            cursor: 'pointer',
            padding: 0,
          }}
        >
          <HelpCircle size={11} />
          这层还是没懂
        </button>

        <div style={{ display: 'flex', gap: 4 }}>
          {(
            [
              ['helpful', '👍 有用', 'var(--noesis-accent-green)'],
              ['too_generic', '太泛', 'var(--noesis-accent-yellow)'],
              ['error', '有误', 'var(--noesis-accent-red)'],
            ] as const
          ).map(([key, label, color]) => (
            <button
              key={key}
              onClick={() => onFeedback?.(state.layer, key)}
              style={{
                background: 'var(--noesis-bg-surface0)',
                border: 'none',
                borderRadius: 4,
                padding: '2px 7px',
                color,
                fontSize: 10,
                cursor: 'pointer',
              }}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
};

const iconBtnStyle: React.CSSProperties = {
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  background: 'var(--noesis-bg-surface0)',
  border: 'none',
  borderRadius: 4,
  padding: 4,
  color: 'var(--noesis-text-muted)',
  cursor: 'pointer',
};

const thStyle: React.CSSProperties = {
  textAlign: 'left',
  padding: '4px 6px',
  color: 'var(--noesis-text-subtle)',
  fontWeight: 600,
  borderBottom: '1px solid var(--noesis-bg-surface0)',
};

const tdStyle: React.CSSProperties = {
  padding: '3px 6px',
  verticalAlign: 'top',
  borderBottom: '1px solid rgba(255,255,255,0.04)',
};
