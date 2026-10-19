import React, { useState, useEffect, useRef, useId } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import katex from 'katex';
import { Copy, Check, AlertCircle } from 'lucide-react';

export interface MarkdownRendererProps {
  content: string;
  fontSize?: number;
  currentFilePath?: string;
  onJumpToCitation?: (filePath: string, startLine: number, endLine: number) => void;
}

// -----------------------------------------------------------------------------
// Mermaid Async Loader & Dynamic Component
// -----------------------------------------------------------------------------

let mermaidPromise: Promise<any> | null = null;
let mermaidThemeId: string | null = null;

/** H4: 读取当前主题下真实的 CSS 变量值（mermaid 需要具体色值，不能直接吃 var()）。 */
function readThemeVar(name: string, fallback: string): string {
  if (typeof window === 'undefined') return fallback;
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

/**
 * H4: 按当前配色主题初始化 mermaid。主题切换后调用 `resetMermaidTheme()` 让它重新初始化。
 */
function getMermaid() {
  const themeId =
    (typeof document !== 'undefined' && document.documentElement.dataset.theme) || 'catppuccin';

  if (mermaidPromise && mermaidThemeId === themeId) return mermaidPromise;

  mermaidThemeId = themeId;
  const isLight = themeId === 'catppuccin-latte';
  mermaidPromise = import('mermaid').then((m) => {
    m.default.initialize({
      startOnLoad: false,
      theme: isLight ? 'default' : 'dark',
      securityLevel: 'strict',
      themeVariables: {
        darkMode: !isLight,
        background: readThemeVar('--noesis-bg-crust', '#11111b'),
        primaryColor: readThemeVar('--noesis-accent-blue', '#89b4fa'),
        primaryTextColor: readThemeVar('--noesis-text-primary', '#cdd6f4'),
        primaryBorderColor: readThemeVar('--noesis-bg-surface0', '#313244'),
        lineColor: readThemeVar('--noesis-accent-blue', '#89b4fa'),
        secondaryColor: readThemeVar('--noesis-bg-surface0', '#313244'),
        tertiaryColor: readThemeVar('--noesis-bg-mantle', '#181825'),
        fontFamily: readThemeVar('--noesis-font-mono', 'monospace'),
      },
    });
    return m.default;
  });
  return mermaidPromise;
}

/** H4: 主题切换后重置 mermaid 缓存，让下一张图用新配色重新渲染。 */
export function resetMermaidTheme() {
  mermaidPromise = null;
  mermaidThemeId = null;
}

const MermaidBlock: React.FC<{ code: string }> = ({ code }) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const [svg, setSvg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const uniqueId = useId().replace(/[^a-zA-Z0-9_-]/g, '_');

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);

    const renderGraph = async () => {
      const trimmed = code.trim();
      if (!trimmed) {
        setSvg(null);
        setLoading(false);
        return;
      }

      try {
        const mermaid = await getMermaid();
        if (cancelled) return;
        const renderId = `mermaid_${uniqueId}_${Date.now()}`;
        const { svg: renderedSvg } = await mermaid.render(renderId, trimmed);
        if (!cancelled) {
          setSvg(renderedSvg);
          setError(null);
          setLoading(false);
        }
      } catch (err: any) {
        if (!cancelled) {
          setError(err?.message || '图表语法有误');
          setLoading(false);
        }
      }
    };

    renderGraph();
    return () => {
      cancelled = true;
    };
  }, [code, uniqueId]);

  if (error) {
    return (
      <div
        style={{
          margin: '8px 0',
          background: 'var(--noesis-bg-mantle)',
          border: '1px solid rgba(243, 139, 168, 0.3)',
          borderRadius: '6px',
          padding: '8px 12px',
        }}
      >
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '6px',
            color: 'var(--noesis-accent-red)',
            fontSize: '11px',
            marginBottom: '6px',
          }}
        >
          <AlertCircle size={13} />
          <span>图表语法有误</span>
        </div>
        <pre
          style={{
            margin: 0,
            fontSize: '11px',
            color: 'var(--noesis-text-muted)',
            fontFamily: 'Consolas, Monaco, monospace',
            overflowX: 'auto',
          }}
        >
          <code>{code}</code>
        </pre>
      </div>
    );
  }

  if (loading && !svg) {
    return (
      <div
        style={{
          margin: '8px 0',
          padding: '16px',
          textAlign: 'center',
          background: 'var(--noesis-bg-mantle)',
          border: '1px solid var(--noesis-bg-surface0)',
          borderRadius: '6px',
          color: 'var(--noesis-text-subtle)',
          fontSize: '11px',
        }}
      >
        渲染图表中...
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      style={{
        margin: '10px 0',
        padding: '12px',
        background: 'var(--noesis-bg-mantle)',
        border: '1px solid var(--noesis-bg-surface0)',
        borderRadius: '6px',
        overflowX: 'auto',
        textAlign: 'center',
      }}
      dangerouslySetInnerHTML={{ __html: svg || '' }}
    />
  );
};

// -----------------------------------------------------------------------------
// LaTeX Block
// -----------------------------------------------------------------------------

const LatexBlock: React.FC<{ code: string }> = ({ code }) => {
  const [html, setHtml] = useState<string>('');

  useEffect(() => {
    try {
      const rendered = katex.renderToString(code.trim(), {
        displayMode: true,
        throwOnError: false,
      });
      setHtml(rendered);
    } catch {
      setHtml('');
    }
  }, [code]);

  if (!html) {
    return (
      <pre style={{ margin: '8px 0', padding: '8px', background: 'var(--noesis-bg-crust)', color: 'var(--noesis-text-primary)' }}>
        <code>{code}</code>
      </pre>
    );
  }

  return (
    <div
      style={{
        margin: '8px 0',
        padding: '8px 12px',
        background: 'var(--noesis-bg-mantle)',
        borderRadius: '6px',
        overflowX: 'auto',
      }}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
};

// -----------------------------------------------------------------------------
// Standard Code Block with Copy
// -----------------------------------------------------------------------------

const CodeBlock: React.FC<{ language: string; code: string }> = ({ language, code }) => {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div
      style={{
        margin: '8px 0',
        background: 'var(--noesis-bg-crust)',
        border: '1px solid var(--noesis-bg-surface0)',
        borderRadius: '6px',
        overflow: 'hidden',
      }}
    >
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          padding: '4px 8px',
          background: 'var(--noesis-bg-mantle)',
          borderBottom: '1px solid var(--noesis-bg-surface0)',
          fontSize: '11px',
          color: 'var(--noesis-text-muted)',
        }}
      >
        <span>{language || 'code'}</span>
        <button
          onClick={handleCopy}
          style={{
            background: 'transparent',
            border: 'none',
            color: copied ? 'var(--noesis-accent-green)' : 'var(--noesis-text-subtle)',
            cursor: 'pointer',
            display: 'flex',
            alignItems: 'center',
            gap: '3px',
            fontSize: '11px',
          }}
          title="复制代码"
        >
          {copied ? <Check size={12} /> : <Copy size={12} />}
          {copied ? '已复制' : '复制'}
        </button>
      </div>
      <pre
        style={{
          margin: 0,
          padding: '8px 10px',
          overflowX: 'auto',
          fontSize: '11.5px',
          fontFamily: 'Consolas, Monaco, monospace',
          color: 'var(--noesis-text-primary)',
          lineHeight: '1.45',
        }}
      >
        <code>{code}</code>
      </pre>
    </div>
  );
};

// -----------------------------------------------------------------------------
// Citation Badge Link Helper
// -----------------------------------------------------------------------------

const CITATION_REGEX = /\[(?:([a-zA-Z0-9_\-./\\]+):)?L(\d+)(?:-L?(\d+))?\]/g;

function renderTextWithCitations(
  text: string,
  currentFilePath?: string,
  onJumpToCitation?: (filePath: string, startLine: number, endLine: number) => void
): React.ReactNode {
  if (!text || !text.includes('[L') && !text.includes(':L')) {
    return text;
  }

  const elements: React.ReactNode[] = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  CITATION_REGEX.lastIndex = 0;

  while ((match = CITATION_REGEX.exec(text)) !== null) {
    if (match.index > lastIndex) {
      elements.push(text.slice(lastIndex, match.index));
    }

    const file = match[1] || currentFilePath || '';
    const startLine = parseInt(match[2], 10) || 1;
    const endLine = match[3] ? parseInt(match[3], 10) : startLine;
    const badgeText = match[1]
      ? `${match[1].split('/').pop()}:L${startLine}${endLine !== startLine ? `-${endLine}` : ''}`
      : `L${startLine}${endLine !== startLine ? `-${endLine}` : ''}`;

    elements.push(
      <button
        key={`cit-${match.index}`}
        onClick={(e) => {
          e.stopPropagation();
          if (onJumpToCitation && file) {
            onJumpToCitation(file, startLine, endLine);
          }
        }}
        title={`跳转到 ${file}:${startLine}-${endLine}`}
        style={{
          display: 'inline-flex',
          alignItems: 'center',
          gap: '2px',
          padding: '1px 5px',
          margin: '0 2px',
          borderRadius: '4px',
          fontSize: '10.5px',
          fontFamily: 'Consolas, Monaco, monospace',
          background: 'rgba(137, 180, 250, 0.14)',
          border: '1px solid rgba(137, 180, 250, 0.35)',
          color: 'var(--noesis-accent-blue)',
          cursor: onJumpToCitation && file ? 'pointer' : 'default',
          verticalAlign: 'baseline',
          lineHeight: '1.2',
        }}
      >
        📍 {badgeText}
      </button>
    );

    lastIndex = CITATION_REGEX.lastIndex;
  }

  if (lastIndex < text.length) {
    elements.push(text.slice(lastIndex));
  }

  return elements.length > 0 ? elements : text;
}

function processChildren(
  children: React.ReactNode,
  currentFilePath?: string,
  onJumpToCitation?: (filePath: string, startLine: number, endLine: number) => void
): React.ReactNode {
  return React.Children.map(children, (child) => {
    if (typeof child === 'string') {
      return renderTextWithCitations(child, currentFilePath, onJumpToCitation);
    }
    return child;
  });
}

// -----------------------------------------------------------------------------
// Main MarkdownRenderer Component
// -----------------------------------------------------------------------------

export const MarkdownRenderer: React.FC<MarkdownRendererProps> = ({
  content,
  fontSize = 12,
  currentFilePath,
  onJumpToCitation,
}) => {
  if (!content) return null;

  return (
    <div
      className="noesis-markdown"
      style={{
        fontSize: `${fontSize}px`,
        color: 'var(--noesis-text-primary)',
        lineHeight: 1.6,
      }}
    >
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={{
          code({ className, children, ...props }) {
            const match = /language-(\w+)/.exec(className || '');
            const lang = match ? match[1].toLowerCase() : '';
            const codeString = String(children).replace(/\n$/, '');

            if (lang === 'mermaid') {
              return <MermaidBlock code={codeString} />;
            }
            if (lang === 'latex' || lang === 'math') {
              return <LatexBlock code={codeString} />;
            }

            // Inline code
            const isInline = !className && !codeString.includes('\n');
            if (isInline) {
              return (
                <code
                  style={{
                    background: 'var(--noesis-bg-crust)',
                    color: 'var(--noesis-accent-yellow)',
                    padding: '1px 5px',
                    borderRadius: '4px',
                    fontSize: '0.9em',
                    fontFamily: 'Consolas, Monaco, monospace',
                    border: '1px solid var(--noesis-bg-surface0)',
                  }}
                  {...props}
                >
                  {children}
                </code>
              );
            }

            return <CodeBlock language={lang} code={codeString} />;
          },
          p({ children }) {
            return (
              <p style={{ margin: '4px 0', color: 'var(--noesis-text-secondary)' }}>
                {processChildren(children, currentFilePath, onJumpToCitation)}
              </p>
            );
          },
          li({ children }) {
            return (
              <li style={{ margin: '2px 0', color: 'var(--noesis-text-secondary)' }}>
                {processChildren(children, currentFilePath, onJumpToCitation)}
              </li>
            );
          },
          h1({ children }) {
            return (
              <h1
                style={{
                  color: 'var(--noesis-accent-peach)',
                  fontSize: `${fontSize + 4}px`,
                  margin: '16px 0 8px',
                  fontWeight: 700,
                  borderBottom: '1px solid var(--noesis-bg-surface0)',
                  paddingBottom: '4px',
                }}
              >
                {processChildren(children, currentFilePath, onJumpToCitation)}
              </h1>
            );
          },
          h2({ children }) {
            return (
              <h2
                style={{
                  color: 'var(--noesis-accent-mauve)',
                  fontSize: `${fontSize + 3}px`,
                  margin: '14px 0 6px',
                  fontWeight: 600,
                }}
              >
                {processChildren(children, currentFilePath, onJumpToCitation)}
              </h2>
            );
          },
          h3({ children }) {
            return (
              <h3
                style={{
                  color: 'var(--noesis-accent-blue)',
                  fontSize: `${fontSize + 2}px`,
                  margin: '12px 0 6px',
                  fontWeight: 600,
                }}
              >
                {processChildren(children, currentFilePath, onJumpToCitation)}
              </h3>
            );
          },
          h4({ children }) {
            return (
              <h4
                style={{
                  color: 'var(--noesis-accent-blue)',
                  fontSize: `${fontSize + 1}px`,
                  margin: '10px 0 4px',
                  fontWeight: 600,
                }}
              >
                {processChildren(children, currentFilePath, onJumpToCitation)}
              </h4>
            );
          },
          table({ children }) {
            return (
              <div style={{ overflowX: 'auto', margin: '8px 0' }}>
                <table
                  style={{
                    width: '100%',
                    borderCollapse: 'collapse',
                    fontSize: '0.9em',
                    border: '1px solid var(--noesis-bg-surface0)',
                  }}
                >
                  {children}
                </table>
              </div>
            );
          },
          thead({ children }) {
            return <thead style={{ background: 'var(--noesis-bg-mantle)' }}>{children}</thead>;
          },
          th({ children }) {
            return (
              <th
                style={{
                  border: '1px solid var(--noesis-bg-surface0)',
                  padding: '6px 10px',
                  textAlign: 'left',
                  color: 'var(--noesis-accent-blue)',
                  fontWeight: 600,
                }}
              >
                {children}
              </th>
            );
          },
          td({ children }) {
            return (
              <td
                style={{
                  border: '1px solid var(--noesis-bg-surface0)',
                  padding: '6px 10px',
                  color: 'var(--noesis-text-primary)',
                }}
              >
                {processChildren(children, currentFilePath, onJumpToCitation)}
              </td>
            );
          },
          blockquote({ children }) {
            return (
              <blockquote
                style={{
                  margin: '6px 0',
                  paddingLeft: '10px',
                  borderLeft: '3px solid var(--noesis-accent-blue)',
                  color: 'var(--noesis-text-muted)',
                  fontStyle: 'italic',
                }}
              >
                {children}
              </blockquote>
            );
          },
          hr() {
            return <hr style={{ border: 'none', borderTop: '1px solid var(--noesis-bg-surface0)', margin: '10px 0' }} />;
          },
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
};
