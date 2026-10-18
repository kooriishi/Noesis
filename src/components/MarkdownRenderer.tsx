import React, { useState } from 'react';
import { Copy, Check } from 'lucide-react';

interface MarkdownRendererProps {
  content: string;
  fontSize?: number;
}

function renderInline(text: string): React.ReactNode[] {
  // Matches inline code `code`, bold **text**, and italic *text*
  const tokens: React.ReactNode[] = [];
  const regex = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*]+\*)/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      tokens.push(text.slice(lastIndex, match.index));
    }
    const token = match[0];
    if (token.startsWith('`') && token.endsWith('`')) {
      tokens.push(
        <code
          key={`code-${match.index}`}
          style={{
            background: '#11111b',
            color: '#f9e2af',
            padding: '1px 5px',
            borderRadius: '4px',
            fontSize: '0.9em',
            fontFamily: 'Consolas, Monaco, monospace',
            border: '1px solid #313244',
          }}
        >
          {token.slice(1, -1)}
        </code>
      );
    } else if (token.startsWith('**') && token.endsWith('**')) {
      tokens.push(
        <strong key={`bold-${match.index}`} style={{ color: '#cdd6f4', fontWeight: 600 }}>
          {token.slice(2, -2)}
        </strong>
      );
    } else if (token.startsWith('*') && token.endsWith('*')) {
      tokens.push(
        <em key={`italic-${match.index}`} style={{ color: '#bac2de' }}>
          {token.slice(1, -1)}
        </em>
      );
    }
    lastIndex = regex.lastIndex;
  }

  if (lastIndex < text.length) {
    tokens.push(text.slice(lastIndex));
  }

  return tokens.length > 0 ? tokens : [text];
}

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
        background: '#11111b',
        border: '1px solid #313244',
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
          background: '#181825',
          borderBottom: '1px solid #313244',
          fontSize: '11px',
          color: '#a6adc8',
        }}
      >
        <span>{language || 'code'}</span>
        <button
          onClick={handleCopy}
          style={{
            background: 'transparent',
            border: 'none',
            color: copied ? '#a6e3a1' : '#6c7086',
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
          color: '#cdd6f4',
          lineHeight: '1.45',
        }}
      >
        <code>{code}</code>
      </pre>
    </div>
  );
};

export const MarkdownRenderer: React.FC<MarkdownRendererProps> = ({ content, fontSize = 12 }) => {
  if (!content) return null;

  const lines = content.split('\n');
  const elements: React.ReactNode[] = [];

  let inCodeBlock = false;
  let codeBlockLang = '';
  let codeBlockLines: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // Check code fences
    if (line.trim().startsWith('```')) {
      if (inCodeBlock) {
        elements.push(
          <CodeBlock
            key={`cb-${i}`}
            language={codeBlockLang}
            code={codeBlockLines.join('\n')}
          />
        );
        inCodeBlock = false;
        codeBlockLines = [];
        codeBlockLang = '';
      } else {
        inCodeBlock = true;
        codeBlockLang = line.trim().slice(3).trim();
        codeBlockLines = [];
      }
      continue;
    }

    if (inCodeBlock) {
      codeBlockLines.push(line);
      continue;
    }

    const trimmed = line.trim();

    // Headers
    if (trimmed.startsWith('#### ')) {
      elements.push(
        <h4 key={`h4-${i}`} style={{ color: '#89b4fa', fontSize: `${fontSize + 1}px`, margin: '10px 0 4px', fontWeight: 600 }}>
          {renderInline(trimmed.slice(5))}
        </h4>
      );
    } else if (trimmed.startsWith('### ')) {
      elements.push(
        <h3 key={`h3-${i}`} style={{ color: '#89b4fa', fontSize: `${fontSize + 2}px`, margin: '12px 0 6px', fontWeight: 600 }}>
          {renderInline(trimmed.slice(4))}
        </h3>
      );
    } else if (trimmed.startsWith('## ')) {
      elements.push(
        <h2 key={`h2-${i}`} style={{ color: '#cba6f7', fontSize: `${fontSize + 3}px`, margin: '14px 0 6px', fontWeight: 600 }}>
          {renderInline(trimmed.slice(3))}
        </h2>
      );
    } else if (trimmed.startsWith('# ')) {
      elements.push(
        <h1 key={`h1-${i}`} style={{ color: '#fab387', fontSize: `${fontSize + 4}px`, margin: '16px 0 8px', fontWeight: 700 }}>
          {renderInline(trimmed.slice(2))}
        </h1>
      );
    } else if (trimmed === '---' || trimmed === '***') {
      elements.push(<hr key={`hr-${i}`} style={{ border: 'none', borderTop: '1px solid #313244', margin: '10px 0' }} />);
    } else if (trimmed.startsWith('- ') || trimmed.startsWith('* ')) {
      elements.push(
        <div key={`li-${i}`} style={{ display: 'flex', gap: '6px', margin: '3px 0', paddingLeft: '6px' }}>
          <span style={{ color: '#89b4fa', userSelect: 'none' }}>•</span>
          <span style={{ color: '#bac2de', lineHeight: '1.5' }}>{renderInline(trimmed.slice(2))}</span>
        </div>
      );
    } else if (trimmed.startsWith('> ')) {
      elements.push(
        <blockquote
          key={`quote-${i}`}
          style={{
            margin: '6px 0',
            paddingLeft: '10px',
            borderLeft: '3px solid #89b4fa',
            color: '#a6adc8',
            fontStyle: 'italic',
            lineHeight: '1.5',
          }}
        >
          {renderInline(trimmed.slice(2))}
        </blockquote>
      );
    } else if (trimmed.length > 0) {
      elements.push(
        <p key={`p-${i}`} style={{ margin: '4px 0', color: '#bac2de', lineHeight: '1.6' }}>
          {renderInline(line)}
        </p>
      );
    } else {
      // Empty line spacer
      elements.push(<div key={`sp-${i}`} style={{ height: '4px' }} />);
    }
  }

  if (inCodeBlock && codeBlockLines.length > 0) {
    elements.push(
      <CodeBlock
        key="cb-tail"
        language={codeBlockLang}
        code={codeBlockLines.join('\n')}
      />
    );
  }

  return <div style={{ fontSize: `${fontSize}px` }}>{elements}</div>;
};
