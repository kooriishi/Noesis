import React, { useEffect, useRef, useState } from 'react';
import { EditorState, Extension, StateEffect, StateField } from '@codemirror/state';
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightActiveLineGutter,
  Decoration,
  DecorationSet,
} from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { searchKeymap, search } from '@codemirror/search';
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
import { Sparkles, HelpCircle, Play, CheckCircle2, AlertTriangle, MessageSquare } from 'lucide-react';

export const setHighlightEffect = StateEffect.define<{ fromLine: number; toLine: number } | null>();

export const lineHighlightField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(highlights, tr) {
    for (const e of tr.effects) {
      if (e.is(setHighlightEffect)) {
        if (!e.value) return Decoration.none;
        const decos = [];
        for (let l = e.value.fromLine; l <= e.value.toLine; l++) {
          if (l <= tr.state.doc.lines) {
            const line = tr.state.doc.line(l);
            decos.push(Decoration.line({ class: 'cm-jump-highlight' }).range(line.from));
          }
        }
        return Decoration.set(decos, true);
      }
    }
    return highlights.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

const LARGE_FILE_BYTE_THRESHOLD = 1_000_000; // 1 MB guard (Stage E1)

interface CachedEditorEntry {
  state: EditorState;
  scrollTop: number;
  scrollLeft: number;
  configKey: string;
}

const editorStateCache = new Map<string, CachedEditorEntry>();

export function evictEditorStateCache(filePath?: string) {
  if (filePath) {
    editorStateCache.delete(filePath);
  } else {
    editorStateCache.clear();
  }
}

interface CodeEditorProps {
  filePath: string;
  initialContent: string;
  language: string;
  fontSize?: number;
  fontFamily?: string;
  wordWrap?: boolean;
  jumpToRange?: { filePath: string; startLine: number; endLine: number; nonce: number } | null;
  onContentChange: (newContent: string) => void;
  onSave: (latestContent?: string) => void;
  onExplainSelection: (snippet: string, startLine: number, endLine: number) => void;
  onAskSelection?: (snippet: string, startLine: number, endLine: number) => void;
  onMarkConfusion: (snippet: string, startLine: number, endLine: number, category: string) => void;
  onSendToScratch: (snippet: string, language: string) => void;
}

export const CodeEditor: React.FC<CodeEditorProps> = ({
  filePath,
  initialContent,
  language,
  fontSize = 13.5,
  fontFamily = 'Consolas, "Fira Code", monospace',
  wordWrap = false,
  jumpToRange,
  onContentChange,
  onSave,
  onExplainSelection,
  onAskSelection,
  onMarkConfusion,
  onSendToScratch,
}) => {
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const activePathRef = useRef<string>(filePath);
  const onSaveRef = useRef(onSave);
  const onContentChangeRef = useRef(onContentChange);
  const toastTimerRef = useRef<number | null>(null);

  useEffect(() => {
    onSaveRef.current = onSave;
  }, [onSave]);

  useEffect(() => {
    onContentChangeRef.current = onContentChange;
  }, [onContentChange]);

  const [forceFullEditMap, setForceFullEditMap] = useState<Record<string, boolean>>({});
  const isLargeFile = initialContent.length > LARGE_FILE_BYTE_THRESHOLD;
  const isReadOnlyLargeMode = isLargeFile && !forceFullEditMap[filePath];

  const [selection, setSelection] = useState<{
    text: string;
    startLine: number;
    endLine: number;
    relTop: number;
    relLeft: number;
  } | null>(null);

  const [showConfusionModal, setShowConfusionModal] = useState(false);
  const [saveToast, setSaveToast] = useState(false);

  const getLanguageExtension = (lang: string, disableHeavySyntax: boolean): Extension => {
    if (disableHeavySyntax) return [];
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

  const buildExtensions = (
    lang: string,
    readOnlyLarge: boolean,
    fSize: number,
    fFamily: string,
    wrap: boolean
  ): Extension[] => {
    const saveKeyBinding = keymap.of([
      {
        key: 'Mod-s',
        run: (view) => {
          const currentText = view.state.doc.toString();
          onSaveRef.current(currentText);
          setSaveToast(true);
          if (toastTimerRef.current) window.clearTimeout(toastTimerRef.current);
          toastTimerRef.current = window.setTimeout(() => setSaveToast(false), 2000);
          return true;
        },
      },
    ]);

    const updateListener = EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        onContentChangeRef.current(update.state.doc.toString());
      }

      if (update.selectionSet || update.docChanged) {
        const ranges = update.state.selection.ranges;
        const main = ranges[0];
        if (main && !main.empty) {
          const selectedText = update.state.sliceDoc(main.from, main.to);
          const startLine = update.state.doc.lineAt(main.from).number;
          const endLine = update.state.doc.lineAt(main.to).number;

          const startCoords = viewRef.current?.coordsAtPos(main.from);
          const endCoords = viewRef.current?.coordsAtPos(main.to);
          const containerRect = editorRef.current?.getBoundingClientRect();

          let relTop = 20;
          let relLeft = 40;

          if (endCoords && containerRect) {
            const bubbleHeight = 44;
            const bubbleWidth = 330;
            const spaceBelow = containerRect.bottom - endCoords.bottom;
            if (spaceBelow < bubbleHeight + 16 && startCoords) {
              // Flip above selection when near bottom edge (Stage E1)
              relTop = Math.max(8, startCoords.top - containerRect.top - bubbleHeight - 8);
            } else {
              relTop = Math.max(
                8,
                Math.min(containerRect.height - bubbleHeight - 10, endCoords.bottom - containerRect.top + 8)
              );
            }
            relLeft = Math.max(
              12,
              Math.min(Math.max(12, containerRect.width - bubbleWidth - 12), endCoords.left - containerRect.left)
            );
          }

          setSelection({
            text: selectedText,
            startLine,
            endLine,
            relTop,
            relLeft,
          });
        } else {
          setSelection(null);
        }
      }
    });

    return [
      lineNumbers(),
      highlightActiveLineGutter(),
      foldGutter(),
      history(),
      indentOnInput(),
      bracketMatching(),
      highlightActiveLine(),
      readOnlyLarge ? EditorState.readOnly.of(true) : [],
      readOnlyLarge ? [] : syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      oneDark,
      search(),
      getLanguageExtension(lang, readOnlyLarge),
      wrap ? EditorView.lineWrapping : [],
      keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
      saveKeyBinding,
      updateListener,
      lineHighlightField,
      EditorView.theme({
        '&': { height: '100%', fontSize: `${fSize}px`, fontFamily: fFamily },
        '.cm-scroller': { overflow: 'auto' },
        '.cm-content': { padding: '12px 0' },
        '.cm-line': { padding: '0 16px', lineHeight: '1.6' },
      }),
    ];
  };

  // Mount single EditorView instance once and reuse across tabs via setState
  useEffect(() => {
    if (!editorRef.current) return;

    const configKey = `${language}:${isReadOnlyLargeMode}:${fontSize}:${fontFamily}:${wordWrap}`;
    const cached = editorStateCache.get(filePath);
    const initialState =
      cached && cached.configKey === configKey
        ? cached.state
        : EditorState.create({
            doc: initialContent,
            extensions: buildExtensions(language, isReadOnlyLargeMode, fontSize, fontFamily, wordWrap),
          });

    const view = new EditorView({
      state: initialState,
      parent: editorRef.current,
    });
    viewRef.current = view;
    activePathRef.current = filePath;

    if (cached) {
      requestAnimationFrame(() => {
        view.scrollDOM.scrollTop = cached.scrollTop;
        view.scrollDOM.scrollLeft = cached.scrollLeft;
      });
    }

    return () => {
      if (toastTimerRef.current) window.clearTimeout(toastTimerRef.current);
      if (viewRef.current) {
        editorStateCache.set(activePathRef.current, {
          state: viewRef.current.state,
          scrollTop: viewRef.current.scrollDOM.scrollTop,
          scrollLeft: viewRef.current.scrollDOM.scrollLeft,
          configKey,
        });
        viewRef.current.destroy();
        viewRef.current = null;
      }
    };
  }, []);

  // Swap EditorState without destroying EditorView when switching tabs or updating settings
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;

    const prevPath = activePathRef.current;
    const configKey = `${language}:${isReadOnlyLargeMode}:${fontSize}:${fontFamily}:${wordWrap}`;

    if (prevPath) {
      const prevCached = editorStateCache.get(prevPath);
      editorStateCache.set(prevPath, {
        state: view.state,
        scrollTop: view.scrollDOM.scrollTop,
        scrollLeft: view.scrollDOM.scrollLeft,
        configKey: prevCached?.configKey || configKey,
      });
    }

    activePathRef.current = filePath;
    setSelection(null);
    setShowConfusionModal(false);

    const existing = editorStateCache.get(filePath);
    if (existing && existing.configKey === configKey) {
      view.setState(existing.state);
      requestAnimationFrame(() => {
        if (viewRef.current) {
          viewRef.current.scrollDOM.scrollTop = existing.scrollTop;
          viewRef.current.scrollDOM.scrollLeft = existing.scrollLeft;
        }
      });
    } else {
      const docText = existing ? existing.state.doc.toString() : initialContent;
      const prevSelection = existing ? existing.state.selection : undefined;
      const nextState = EditorState.create({
        doc: docText,
        selection: prevSelection,
        extensions: buildExtensions(language, isReadOnlyLargeMode, fontSize, fontFamily, wordWrap),
      });
      view.setState(nextState);
      editorStateCache.set(filePath, {
        state: nextState,
        scrollTop: existing?.scrollTop || 0,
        scrollLeft: existing?.scrollLeft || 0,
        configKey,
      });
      if (existing) {
        requestAnimationFrame(() => {
          if (viewRef.current) {
            viewRef.current.scrollDOM.scrollTop = existing.scrollTop;
            viewRef.current.scrollDOM.scrollLeft = existing.scrollLeft;
          }
        });
      }
    }
  }, [filePath, language, isReadOnlyLargeMode, fontSize, fontFamily, wordWrap]);

  // Support jumping to and highlighting a citation line range with line pulse fade
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !jumpToRange || jumpToRange.filePath !== filePath) return;

    const doc = view.state.doc;
    const safeStart = Math.max(1, Math.min(doc.lines, jumpToRange.startLine));
    const safeEnd = Math.max(safeStart, Math.min(doc.lines, jumpToRange.endLine));
    const fromPos = doc.line(safeStart).from;
    const toPos = doc.line(safeEnd).to;

    view.dispatch({
      selection: { anchor: fromPos, head: toPos },
      effects: [
        EditorView.scrollIntoView(fromPos, { y: 'center' }),
        setHighlightEffect.of({ fromLine: safeStart, toLine: safeEnd }),
      ],
    });
    view.focus();

    // Fade out decoration automatically
    const timer = window.setTimeout(() => {
      if (viewRef.current) {
        viewRef.current.dispatch({ effects: setHighlightEffect.of(null) });
      }
    }, 1800);

    return () => window.clearTimeout(timer);
  }, [jumpToRange, filePath]);

  return (
    <div style={{ position: 'relative', width: '100%', height: '100%', display: 'flex', flexDirection: 'column', background: 'var(--noesis-bg-base, #1e1e2e)' }}>
      {/* Stage E1 Large File Guard Banner */}
      {isReadOnlyLargeMode && (
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: '8px',
            padding: '6px 14px',
            background: 'rgba(249, 226, 175, 0.14)',
            borderBottom: '1px solid rgba(249, 226, 175, 0.35)',
            color: 'var(--noesis-accent-yellow, #f9e2af)',
            fontSize: '11.5px',
            flexShrink: 0,
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
            <AlertTriangle size={14} />
            <span>
              文件较大 ({(initialContent.length / (1024 * 1024)).toFixed(2)} MB)，已自动以只读模式打开并关闭实时语法分析以保证流畅，仍可选择片段解读。
            </span>
          </div>
          <button
            onClick={() => setForceFullEditMap((prev) => ({ ...prev, [filePath]: true }))}
            style={{
              padding: '3px 10px',
              background: 'var(--noesis-accent-yellow, #f9e2af)',
              color: 'var(--noesis-bg-crust, #11111b)',
              border: 'none',
              borderRadius: 'var(--noesis-radius-sm, 4px)',
              fontSize: '11px',
              fontWeight: 600,
              cursor: 'pointer',
              whiteSpace: 'nowrap',
            }}
          >
            仍然完整打开
          </button>
        </div>
      )}

      <div style={{ position: 'relative', flex: 1, width: '100%', overflow: 'hidden' }}>
        <div ref={editorRef} style={{ width: '100%', height: '100%' }} />

        {/* Floating Selection Action Bubble - clamped to viewport bounds */}
        {selection && !showConfusionModal && (
          <div
            style={{
              position: 'absolute',
              top: `${selection.relTop}px`,
              left: `${selection.relLeft}px`,
              background: 'rgba(24, 24, 37, 0.95)',
              border: '1px solid rgba(137, 180, 250, 0.5)',
              backdropFilter: 'blur(8px)',
              borderRadius: 'var(--noesis-radius-lg, 8px)',
              boxShadow: 'var(--noesis-shadow-lg, 0 8px 24px rgba(0, 0, 0, 0.45))',
              display: 'flex',
              alignItems: 'center',
              gap: '6px',
              padding: '6px 10px',
              zIndex: 50,
              animation: 'fadeIn var(--noesis-transition-fast, 120ms) ease',
            }}
          >
            <span style={{ fontSize: '11px', color: 'var(--noesis-text-muted, #a6adc8)', marginRight: '4px', whiteSpace: 'nowrap' }}>
              已选 {selection.endLine - selection.startLine + 1} 行
            </span>
            <button
              onClick={() => onExplainSelection(selection.text, selection.startLine, selection.endLine)}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '4px',
                padding: '5px 10px',
                background: 'var(--noesis-accent-blue, #89b4fa)',
                color: 'var(--noesis-bg-crust, #11111b)',
                border: 'none',
                borderRadius: '5px',
                fontSize: '12px',
                fontWeight: 600,
                cursor: 'pointer',
                whiteSpace: 'nowrap',
              }}
            >
              <Sparkles size={14} />
              深度解读
            </button>
            {onAskSelection && (
              <button
                onClick={() => onAskSelection(selection.text, selection.startLine, selection.endLine)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '4px',
                  padding: '5px 10px',
                  background: 'var(--noesis-accent-mauve, #cba6f7)',
                  color: 'var(--noesis-bg-crust, #11111b)',
                  border: 'none',
                  borderRadius: '5px',
                  fontSize: '12px',
                  fontWeight: 600,
                  cursor: 'pointer',
                  whiteSpace: 'nowrap',
                }}
              >
                <MessageSquare size={14} />
                智能问答
              </button>
            )}
            <button
              onClick={() => setShowConfusionModal(true)}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '4px',
                padding: '5px 10px',
                background: 'var(--noesis-accent-red, #f38ba8)',
                color: 'var(--noesis-bg-crust, #11111b)',
                border: 'none',
                borderRadius: '5px',
                fontSize: '12px',
                fontWeight: 600,
                cursor: 'pointer',
                whiteSpace: 'nowrap',
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
                background: 'var(--noesis-accent-green, #a6e3a1)',
                color: 'var(--noesis-bg-crust, #11111b)',
                border: 'none',
                borderRadius: '5px',
                fontSize: '12px',
                fontWeight: 600,
                cursor: 'pointer',
                whiteSpace: 'nowrap',
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
              top: `${Math.min(selection.relTop, 260)}px`,
              left: `${selection.relLeft}px`,
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
                标记卡点类别 (针对选区降维重讲)
              </span>
              <button
                onClick={() => setShowConfusionModal(false)}
                style={{ background: 'transparent', border: 'none', color: '#6c7086', cursor: 'pointer' }}
              >
                ✕
              </button>
            </div>
            <p style={{ fontSize: '11px', color: '#a6adc8', marginBottom: '10px' }}>
              选择你感到困惑的具体维度，系统将根据选区真实代码换用生活类比、最小示例与前置概念深入拆解：
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
              boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
            }}
          >
            <CheckCircle2 size={15} />
            文件已保存并同步磁盘
          </div>
        )}
      </div>
    </div>
  );
};
