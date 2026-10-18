import React, { useEffect, useRef, useState } from 'react';
import { EditorState, Extension } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { foldGutter, indentOnInput, bracketMatching, syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language';
import { oneDark } from '@codemirror/theme-one-dark';
import { javascript } from '@codemirror/lang-javascript';
import { rust } from '@codemirror/lang-rust';
import { python } from '@codemirror/lang-python';
import { json } from '@codemirror/lang-json';
import { markdown } from '@codemirror/lang-markdown';
import { html } from '@codemirror/lang-html';
import { css } from '@codemirror/lang-css';
import { cpp } from '@codemirror/lang-cpp';
import { Sparkles, HelpCircle, Play, CheckCircle2 } from 'lucide-react';

interface CodeEditorProps {
  filePath: string;
  initialContent: string;
  language: string;
  onContentChange: (newContent: string) => void;
  onSave: () => void;
  onExplainSelection: (snippet: string, startLine: number, endLine: number) => void;
  onMarkConfusion: (snippet: string, startLine: number, endLine: number, category: string) => void;
  onSendToScratch: (snippet: string, language: string) => void;
}

export const CodeEditor: React.FC<CodeEditorProps> = ({
  filePath,
  initialContent,
  language,
  onContentChange,
  onSave,
  onExplainSelection,
  onMarkConfusion,
  onSendToScratch,
}) => {
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);

  const [selection, setSelection] = useState<{
    text: string;
    startLine: number;
    endLine: number;
    coords: { x: number; y: number } | null;
  } | null>(null);

  const [showConfusionModal, setShowConfusionModal] = useState(false);
  const [saveToast, setSaveToast] = useState(false);

  const getLanguageExtension = (lang: string): Extension => {
    switch (lang.toLowerCase()) {
      case 'rust':
        return rust();
      case 'typescript':
      case 'javascript':
        return javascript({ typescript: true, jsx: true });
      case 'python':
        return python();
      case 'json':
        return json();
      case 'markdown':
        return markdown();
      case 'html':
        return html();
      case 'css':
        return css();
      case 'c':
      case 'cpp':
        return cpp();
      default:
        return [];
    }
  };

  useEffect(() => {
    if (!editorRef.current) return;

    const saveKeyBinding = keymap.of([
      {
        key: 'Mod-s',
        run: () => {
          onSave();
          setSaveToast(true);
          setTimeout(() => setSaveToast(false), 2000);
          return true;
        },
      },
    ]);

    const updateListener = EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        onContentChange(update.state.doc.toString());
      }

      if (update.selectionSet || update.docChanged) {
        const ranges = update.state.selection.ranges;
        const main = ranges[0];
        if (main && !main.empty) {
          const selectedText = update.state.sliceDoc(main.from, main.to);
          const startLine = update.state.doc.lineAt(main.from).number;
          const endLine = update.state.doc.lineAt(main.to).number;

          const coords = viewRef.current?.coordsAtPos(main.to);
          setSelection({
            text: selectedText,
            startLine,
            endLine,
            coords: coords ? { x: coords.left, y: coords.bottom } : null,
          });
        } else {
          setSelection(null);
        }
      }
    });

    const startState = EditorState.create({
      doc: initialContent,
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        foldGutter(),
        history(),
        indentOnInput(),
        bracketMatching(),
        highlightActiveLine(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        oneDark,
        getLanguageExtension(language),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        saveKeyBinding,
        updateListener,
        EditorView.theme({
          '&': { height: '100%', fontSize: '13.5px', fontFamily: 'Consolas, "Fira Code", monospace' },
          '.cm-scroller': { overflow: 'auto' },
          '.cm-content': { padding: '12px 0' },
          '.cm-line': { padding: '0 16px', lineHeight: '1.6' },
        }),
      ],
    });

    const view = new EditorView({
      state: startState,
      parent: editorRef.current,
    });

    viewRef.current = view;

    return () => {
      view.destroy();
    };
  }, [filePath, language]);

  return (
    <div style={{ position: 'relative', width: '100%', height: '100%', background: '#1e1e2e' }}>
      <div ref={editorRef} style={{ width: '100%', height: '100%' }} />

      {/* Floating Selection Action Bubble */}
      {selection && !showConfusionModal && (
        <div
          style={{
            position: 'absolute',
            bottom: '24px',
            right: '32px',
            background: 'rgba(24, 24, 37, 0.95)',
            border: '1px solid rgba(137, 180, 250, 0.4)',
            backdropFilter: 'blur(8px)',
            borderRadius: '8px',
            boxShadow: '0 8px 24px rgba(0, 0, 0, 0.4)',
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            padding: '6px 10px',
            zIndex: 50,
            animation: 'fadeIn 0.2s ease',
          }}
        >
          <span style={{ fontSize: '11px', color: '#a6adc8', marginRight: '4px' }}>
            已选 {selection.endLine - selection.startLine + 1} 行
          </span>
          <button
            onClick={() => onExplainSelection(selection.text, selection.startLine, selection.endLine)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              padding: '5px 10px',
              background: '#89b4fa',
              color: '#11111b',
              border: 'none',
              borderRadius: '5px',
              fontSize: '12px',
              fontWeight: 600,
              cursor: 'pointer',
            }}
          >
            <Sparkles size={14} />
            深度解读
          </button>
          <button
            onClick={() => setShowConfusionModal(true)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              padding: '5px 10px',
              background: '#f38ba8',
              color: '#11111b',
              border: 'none',
              borderRadius: '5px',
              fontSize: '12px',
              fontWeight: 600,
              cursor: 'pointer',
            }}
          >
            <HelpCircle size={14} />
            看不懂
          </button>
          <button
            onClick={() => onSendToScratch(selection.text, language)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              padding: '5px 10px',
              background: '#a6e3a1',
              color: '#11111b',
              border: 'none',
              borderRadius: '5px',
              fontSize: '12px',
              fontWeight: 600,
              cursor: 'pointer',
            }}
          >
            <Play size={14} />
            沙盒运行
          </button>
        </div>
      )}

      {/* Confusion Category Picker Modal */}
      {showConfusionModal && selection && (
        <div
          style={{
            position: 'absolute',
            bottom: '24px',
            right: '32px',
            width: '320px',
            background: '#181825',
            border: '1px solid #f38ba8',
            borderRadius: '8px',
            padding: '12px',
            boxShadow: '0 10px 30px rgba(0,0,0,0.5)',
            zIndex: 60,
          }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '8px' }}>
            <span style={{ fontSize: '12px', fontWeight: 'bold', color: '#cdd6f4' }}>
              标记卡点类别 (换个讲法)
            </span>
            <button
              onClick={() => setShowConfusionModal(false)}
              style={{ background: 'transparent', border: 'none', color: '#6c7086', cursor: 'pointer' }}
            >
              ✕
            </button>
          </div>
          <p style={{ fontSize: '11px', color: '#a6adc8', marginBottom: '10px' }}>
            选择你感到疑惑的维度，系统将自动换用生活类比、最小示例与前置概念深入拆解：
          </p>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '8px' }}>
            {[
              { id: 'overall', name: '整体定位', desc: '搞不清在系统里的位置' },
              { id: 'logic', name: '控制逻辑', desc: '理不顺分支与状态跳转' },
              { id: 'syntax', name: '语言语法', desc: '语法特性或宏看不懂' },
              { id: 'principle', name: '底层原理', desc: '背后的算法或协议机制' },
            ].map((cat) => (
              <button
                key={cat.id}
                onClick={() => {
                  onMarkConfusion(selection.text, selection.startLine, selection.endLine, cat.id);
                  setShowConfusionModal(false);
                }}
                style={{
                  padding: '8px',
                  background: '#313244',
                  border: '1px solid rgba(255,255,255,0.05)',
                  borderRadius: '6px',
                  textAlign: 'left',
                  cursor: 'pointer',
                  color: '#cdd6f4',
                }}
              >
                <div style={{ fontSize: '12px', fontWeight: 600, color: '#f38ba8' }}>{cat.name}</div>
                <div style={{ fontSize: '10px', color: '#6c7086' }}>{cat.desc}</div>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Save Toast */}
      {saveToast && (
        <div
          style={{
            position: 'absolute',
            top: '16px',
            right: '24px',
            background: 'rgba(166, 227, 161, 0.95)',
            color: '#11111b',
            padding: '6px 12px',
            borderRadius: '6px',
            fontSize: '12px',
            fontWeight: 600,
            display: 'flex',
            alignItems: 'center',
            gap: '6px',
            zIndex: 100,
          }}
        >
          <CheckCircle2 size={15} />
          文件已保存并同步哈希
        </div>
      )}
    </div>
  );
};
