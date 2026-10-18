import React, { useState } from 'react';
import { Play, Terminal, CheckCircle2, XCircle, Clock, BookOpen } from 'lucide-react';
import { RunResponse, ExerciseRecord } from '../types';

interface ScratchRunnerProps {
  initialCode: string;
  initialLanguage: string;
  onRunCode: (language: string, code: string) => Promise<RunResponse>;
  dueReviews: ExerciseRecord[];
  onClose: () => void;
}

export const ScratchRunner: React.FC<ScratchRunnerProps> = ({
  initialCode,
  initialLanguage,
  onRunCode,
  dueReviews,
  onClose,
}) => {
  const [activeTab, setActiveTab] = useState<'scratch' | 'reviews'>('scratch');
  const [code, setCode] = useState(initialCode || 'console.log("NOESIS Sandbox Runner ready!");');
  const [language, setLanguage] = useState(initialLanguage || 'javascript');
  const [isRunning, setIsRunning] = useState(false);
  const [result, setResult] = useState<RunResponse | null>(null);

  const handleExecute = async () => {
    setIsRunning(true);
    try {
      const res = await onRunCode(language, code);
      setResult(res);
    } catch (err: any) {
      setResult({
        stdout: '',
        stderr: err?.message || String(err),
        exit_code: -1,
        duration_ms: 0,
        success: false,
      });
    } finally {
      setIsRunning(false);
    }
  };

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        background: '#11111b',
        borderTop: '1px solid #313244',
      }}
    >
      {/* Runner Bar */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          padding: '6px 14px',
          background: '#181825',
          borderBottom: '1px solid #313244',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <button
            onClick={() => setActiveTab('scratch')}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              padding: '4px 8px',
              background: activeTab === 'scratch' ? '#313244' : 'transparent',
              color: activeTab === 'scratch' ? '#89b4fa' : '#6c7086',
              border: 'none',
              borderRadius: '4px',
              fontSize: '11.5px',
              fontWeight: 600,
              cursor: 'pointer',
            }}
          >
            <Terminal size={14} />
            草稿沙盒运行器
          </button>

          <button
            onClick={() => setActiveTab('reviews')}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              padding: '4px 8px',
              background: activeTab === 'reviews' ? '#313244' : 'transparent',
              color: activeTab === 'reviews' ? '#f9e2af' : '#6c7086',
              border: 'none',
              borderRadius: '4px',
              fontSize: '11.5px',
              fontWeight: 600,
              cursor: 'pointer',
            }}
          >
            <BookOpen size={14} />
            艾宾浩斯复盘 ({dueReviews.length})
          </button>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          {activeTab === 'scratch' && (
            <>
              <select
                value={language}
                onChange={(e) => setLanguage(e.target.value)}
                style={{
                  background: '#313244',
                  color: '#cdd6f4',
                  border: 'none',
                  borderRadius: '4px',
                  padding: '3px 8px',
                  fontSize: '11px',
                }}
              >
                <option value="javascript">JavaScript (Node)</option>
                <option value="typescript">TypeScript (ts-node)</option>
                <option value="python">Python 3</option>
              </select>

              <button
                onClick={handleExecute}
                disabled={isRunning}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '4px',
                  background: '#a6e3a1',
                  color: '#11111b',
                  border: 'none',
                  borderRadius: '4px',
                  padding: '4px 10px',
                  fontSize: '11.5px',
                  fontWeight: 600,
                  cursor: isRunning ? 'default' : 'pointer',
                }}
              >
                <Play size={13} />
                {isRunning ? '执行中...' : '运行'}
              </button>
            </>
          )}

          <button
            onClick={onClose}
            style={{
              background: 'transparent',
              border: 'none',
              color: '#6c7086',
              cursor: 'pointer',
              fontSize: '14px',
              padding: '0 4px',
            }}
          >
            ✕
          </button>
        </div>
      </div>

      {/* Runner Body */}
      {activeTab === 'scratch' ? (
        <div style={{ flex: 1, display: 'grid', gridTemplateColumns: '1fr 1fr', overflow: 'hidden' }}>
          {/* Scratchpad editor */}
          <div style={{ display: 'flex', flexDirection: 'column', borderRight: '1px solid #313244', height: '100%' }}>
            <textarea
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder="在此编写临时测试代码..."
              style={{
                flex: 1,
                padding: '10px',
                background: '#181825',
                color: '#cdd6f4',
                border: 'none',
                fontFamily: 'Consolas, "Fira Code", monospace',
                fontSize: '12px',
                resize: 'none',
                outline: 'none',
              }}
            />
          </div>

          {/* Execution Output */}
          <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: '#11111b', padding: '10px', overflowY: 'auto' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '6px' }}>
              <span style={{ fontSize: '11px', color: '#6c7086', fontWeight: 600 }}>子进程执行结果</span>
              {result && (
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '10.5px' }}>
                  <span style={{ color: result.success ? '#a6e3a1' : '#f38ba8', display: 'flex', alignItems: 'center', gap: '3px' }}>
                    {result.success ? <CheckCircle2 size={13} /> : <XCircle size={13} />}
                    退出码: {result.exit_code}
                  </span>
                  <span style={{ color: '#a6adc8', display: 'flex', alignItems: 'center', gap: '3px' }}>
                    <Clock size={12} />
                    {result.duration_ms} ms
                  </span>
                </div>
              )}
            </div>

            <pre
              style={{
                flex: 1,
                margin: 0,
                padding: '8px',
                background: '#181825',
                borderRadius: '4px',
                color: result?.stderr ? '#f38ba8' : '#a6e3a1',
                fontSize: '12px',
                fontFamily: 'Consolas, monospace',
                overflow: 'auto',
                whiteSpace: 'pre-wrap',
              }}
            >
              {result ? (result.stdout || result.stderr || '(程序无输出)') : '点击右上角「运行」执行当前脚本...'}
            </pre>
          </div>
        </div>
      ) : (
        /* Reviews tab */
        <div style={{ flex: 1, padding: '12px', overflowY: 'auto' }}>
          <div style={{ fontSize: '12px', fontWeight: 600, color: '#cdd6f4', marginBottom: '8px' }}>
            📅 今日艾宾浩斯间隔复盘题目 ({dueReviews.length})
          </div>
          {dueReviews.length > 0 ? (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: '10px' }}>
              {dueReviews.map((rev) => (
                <div key={rev.id} style={{ background: '#181825', padding: '10px', borderRadius: '6px', border: '1px solid #313244' }}>
                  <div style={{ fontSize: '10px', color: '#f9e2af', fontWeight: 'bold' }}>{rev.exercise_type}</div>
                  <div style={{ fontSize: '12px', color: '#cdd6f4', marginTop: '4px' }}>{rev.question}</div>
                  <pre style={{ background: '#11111b', padding: '6px', borderRadius: '4px', fontSize: '10.5px', color: '#89b4fa', margin: '6px 0' }}>{rev.snippet}</pre>
                  <div style={{ fontSize: '11px', color: '#a6adc8' }}>参考依据: {rev.file_path}</div>
                </div>
              ))}
            </div>
          ) : (
            <div style={{ textAlign: 'center', padding: '24px', color: '#6c7086', fontSize: '12px' }}>
              今日复习任务已全部达成！在日常阅读中标记看不懂，系统将自动安排次日复盘。
            </div>
          )}
        </div>
      )}
    </div>
  );
};
