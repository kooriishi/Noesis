import React, { useState } from 'react';
import { Play, Terminal, CheckCircle2, XCircle, Clock, BookOpen, AlertTriangle, FileCode, Sliders } from 'lucide-react';
import { RunRequest, RunResponse, ExerciseRecord } from '../types';

interface ScratchRunnerProps {
  initialCode: string;
  initialLanguage: string;
  onRunCode: (req: RunRequest) => Promise<RunResponse>;
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
  const [code, setCode] = useState(initialCode || '#include <iostream>\n\nint main() {\n    std::cout << "NOESIS Sandbox Runner ready!" << std::endl;\n    return 0;\n}');
  const [language, setLanguage] = useState(initialLanguage || 'cpp');
  const [cppStandard, setCppStandard] = useState<'c++14' | 'c++17' | 'c++20' | 'c11'>('c++17');
  const [extraArgs, setExtraArgs] = useState<string>('');
  const [stdin, setStdin] = useState<string>('');
  const [showStdin, setShowStdin] = useState<boolean>(false);
  const [showCompilerFlags, setShowCompilerFlags] = useState<boolean>(false);
  const [isRunning, setIsRunning] = useState(false);
  const [result, setResult] = useState<RunResponse | null>(null);

  const isCompiledLanguage = language === 'cpp' || language === 'c';

  const handleExecute = async () => {
    setIsRunning(true);
    try {
      const res = await onRunCode({
        language,
        code,
        stdin: stdin.trim() ? stdin : undefined,
        cpp_standard: isCompiledLanguage ? cppStandard : undefined,
        extra_args: isCompiledLanguage && extraArgs.trim() ? extraArgs.trim() : undefined,
        timeout_seconds: 30,
      });
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
        background: 'var(--noesis-bg-crust)',
        borderTop: '1px solid var(--noesis-bg-surface0)',
      }}
    >
      {/* Runner Bar */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          padding: '6px 14px',
          background: 'var(--noesis-bg-mantle)',
          borderBottom: '1px solid var(--noesis-bg-surface0)',
          flexWrap: 'wrap',
          gap: '6px',
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
              background: activeTab === 'scratch' ? 'var(--noesis-bg-surface0)' : 'transparent',
              color: activeTab === 'scratch' ? 'var(--noesis-accent-blue)' : 'var(--noesis-text-subtle)',
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
              background: activeTab === 'reviews' ? 'var(--noesis-bg-surface0)' : 'transparent',
              color: activeTab === 'reviews' ? 'var(--noesis-accent-yellow)' : 'var(--noesis-text-subtle)',
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

          <span
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              fontSize: '10.5px',
              color: 'var(--noesis-accent-yellow)',
              background: 'rgba(249, 226, 175, 0.1)',
              border: '1px solid rgba(249, 226, 175, 0.3)',
              borderRadius: '4px',
              padding: '2px 6px',
            }}
          >
            <AlertTriangle size={11} />
            本地临时沙盒运行
          </span>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          {activeTab === 'scratch' && (
            <>
              {/* Language Selector */}
              <select
                value={language}
                onChange={(e) => {
                  const newLang = e.target.value;
                  setLanguage(newLang);
                  if (newLang === 'cpp' && !code.includes('#include')) {
                    setCode('#include <iostream>\n\nint main() {\n    std::cout << "Hello C++ Sandbox!" << std::endl;\n    return 0;\n}');
                  } else if (newLang === 'c' && !code.includes('#include')) {
                    setCode('#include <stdio.h>\n\nint main() {\n    printf("Hello C Sandbox!\\n");\n    return 0;\n}');
                  } else if (newLang === 'python' && code.includes('#include')) {
                    setCode('print("Hello Python Sandbox!")');
                  } else if (newLang === 'javascript' && code.includes('#include')) {
                    setCode('console.log("Hello JS Sandbox!");');
                  }
                }}
                style={{
                  background: 'var(--noesis-bg-surface0)',
                  color: 'var(--noesis-text-primary)',
                  border: 'none',
                  borderRadius: '4px',
                  padding: '3px 8px',
                  fontSize: '11px',
                }}
              >
                <option value="cpp">C++ (g++/clang++/MSVC)</option>
                <option value="c">C (gcc/clang/MSVC)</option>
                <option value="python">Python 3 (python3/python/py)</option>
                <option value="javascript">JavaScript (Node)</option>
                <option value="typescript">TypeScript (Node.js)</option>
              </select>

              {/* C/C++ Standard Selector */}
              {isCompiledLanguage && (
                <select
                  value={cppStandard}
                  onChange={(e) => setCppStandard(e.target.value as any)}
                  style={{
                    background: 'var(--noesis-bg-surface0)',
                    color: 'var(--noesis-accent-blue)',
                    border: 'none',
                    borderRadius: '4px',
                    padding: '3px 6px',
                    fontSize: '11px',
                  }}
                  title="编译器标准"
                >
                  <option value="c++17">C++17 (推荐)</option>
                  <option value="c++20">C++20</option>
                  <option value="c++14">C++14</option>
                  <option value="c11">C11</option>
                </select>
              )}

              {/* Extra flags toggle */}
              {isCompiledLanguage && (
                <button
                  onClick={() => setShowCompilerFlags(!showCompilerFlags)}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '3px',
                    background: showCompilerFlags ? 'var(--noesis-bg-surface1)' : 'var(--noesis-bg-surface0)',
                    color: 'var(--noesis-text-primary)',
                    border: 'none',
                    borderRadius: '4px',
                    padding: '3px 7px',
                    fontSize: '11px',
                    cursor: 'pointer',
                  }}
                  title="编译附加参数"
                >
                  <Sliders size={12} />
                  参数
                </button>
              )}

              {/* Stdin toggle */}
              <button
                onClick={() => setShowStdin(!showStdin)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '3px',
                  background: showStdin ? 'var(--noesis-bg-surface1)' : 'var(--noesis-bg-surface0)',
                  color: stdin ? 'var(--noesis-accent-green)' : 'var(--noesis-text-primary)',
                  border: 'none',
                  borderRadius: '4px',
                  padding: '3px 7px',
                  fontSize: '11px',
                  cursor: 'pointer',
                }}
                title="配置标准输入 (stdin)"
              >
                <FileCode size={12} />
                标准输入 {stdin ? '•' : ''}
              </button>

              <button
                onClick={handleExecute}
                disabled={isRunning}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '4px',
                  background: 'var(--noesis-accent-green)',
                  color: 'var(--noesis-bg-crust)',
                  border: 'none',
                  borderRadius: '4px',
                  padding: '4px 10px',
                  fontSize: '11.5px',
                  fontWeight: 600,
                  cursor: isRunning ? 'default' : 'pointer',
                }}
              >
                <Play size={13} />
                {isRunning ? '执行中 (30s 超时保护)...' : '运行'}
              </button>
            </>
          )}

          <button
            onClick={onClose}
            style={{
              background: 'transparent',
              border: 'none',
              color: 'var(--noesis-text-subtle)',
              cursor: 'pointer',
              fontSize: '14px',
              padding: '0 4px',
            }}
          >
            ✕
          </button>
        </div>
      </div>

      {/* Compiler Flags Banner */}
      {isCompiledLanguage && showCompilerFlags && (
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            padding: '4px 14px',
            background: 'var(--noesis-bg-mantle)',
            borderBottom: '1px solid var(--noesis-bg-surface0)',
            fontSize: '11px',
          }}
        >
          <span style={{ color: 'var(--noesis-accent-blue)' }}>编译参数 (extra flags):</span>
          <input
            type="text"
            value={extraArgs}
            onChange={(e) => setExtraArgs(e.target.value)}
            placeholder="例如: -O2 -Wall -pthread"
            style={{
              flex: 1,
              background: 'var(--noesis-bg-crust)',
              color: 'var(--noesis-text-primary)',
              border: '1px solid var(--noesis-bg-surface0)',
              borderRadius: '4px',
              padding: '2px 8px',
              fontSize: '11px',
              fontFamily: 'Consolas, monospace',
            }}
          />
        </div>
      )}

      {/* Stdin Panel Banner */}
      {showStdin && (
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            padding: '4px 14px',
            background: 'var(--noesis-bg-mantle)',
            borderBottom: '1px solid var(--noesis-bg-surface0)',
            fontSize: '11px',
          }}
        >
          <span style={{ color: 'var(--noesis-accent-green)' }}>标准输入 (stdin):</span>
          <input
            type="text"
            value={stdin}
            onChange={(e) => setStdin(e.target.value)}
            placeholder="程序标准输入内容，以换行或空格分隔..."
            style={{
              flex: 1,
              background: 'var(--noesis-bg-crust)',
              color: 'var(--noesis-text-primary)',
              border: '1px solid var(--noesis-bg-surface0)',
              borderRadius: '4px',
              padding: '2px 8px',
              fontSize: '11px',
              fontFamily: 'Consolas, monospace',
            }}
          />
        </div>
      )}

      {/* Runner Body */}
      {activeTab === 'scratch' ? (
        <div style={{ flex: 1, display: 'grid', gridTemplateColumns: '1fr 1fr', overflow: 'hidden' }}>
          <div
            style={{
              display: 'flex',
              flexDirection: 'column',
              borderRight: '1px solid var(--noesis-bg-surface0)',
              height: '100%',
            }}
          >
            <textarea
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder="在此编写临时测试代码..."
              style={{
                flex: 1,
                padding: '10px',
                background: 'var(--noesis-bg-mantle)',
                color: 'var(--noesis-text-primary)',
                border: 'none',
                fontFamily: 'Consolas, "Fira Code", monospace',
                fontSize: '12px',
                resize: 'none',
                outline: 'none',
              }}
            />
          </div>

          <div
            style={{
              display: 'flex',
              flexDirection: 'column',
              height: '100%',
              background: 'var(--noesis-bg-crust)',
              padding: '10px',
              overflowY: 'auto',
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
              <span style={{ fontSize: '11px', color: 'var(--noesis-text-subtle)', fontWeight: 600 }}>
                {isCompiledLanguage ? 'C/C++ 编译与运行结果' : '子进程执行结果'} (30s 超时保护 · 1MB 上限)
              </span>
              {result && (
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '10.5px' }}>
                  {result.phase && (
                    <span
                      style={{
                        padding: '1px 5px',
                        borderRadius: '3px',
                        background: result.phase === 'compile' ? 'rgba(243, 139, 168, 0.2)' : 'rgba(166, 227, 161, 0.2)',
                        color: result.phase === 'compile' ? 'var(--noesis-accent-red)' : 'var(--noesis-accent-green)',
                      }}
                    >
                      {result.phase === 'compile' ? '编译阶段' : '运行阶段'}
                    </span>
                  )}
                  <span
                    style={{
                      color: result.success ? 'var(--noesis-accent-green)' : 'var(--noesis-accent-red)',
                      display: 'flex',
                      alignItems: 'center',
                      gap: '3px',
                    }}
                  >
                    {result.success ? <CheckCircle2 size={13} /> : <XCircle size={13} />}
                    退出码: {result.exit_code}
                  </span>
                  <span
                    style={{
                      color: 'var(--noesis-text-muted)',
                      display: 'flex',
                      alignItems: 'center',
                      gap: '3px',
                    }}
                  >
                    <Clock size={12} />
                    {result.duration_ms} ms
                  </span>
                </div>
              )}
            </div>

            {/* If compile errors exist */}
            {result?.compile_stderr && (
              <div style={{ marginBottom: '8px' }}>
                <div style={{ color: 'var(--noesis-accent-red)', fontSize: '11px', fontWeight: 600, marginBottom: '3px' }}>
                  【编译错误输出】
                </div>
                <pre
                  style={{
                    margin: 0,
                    padding: '8px',
                    background: 'var(--noesis-bg-mantle)',
                    border: '1px solid rgba(243, 139, 168, 0.3)',
                    borderRadius: '4px',
                    color: 'var(--noesis-accent-red)',
                    fontSize: '11.5px',
                    fontFamily: 'Consolas, monospace',
                    overflow: 'auto',
                    whiteSpace: 'pre-wrap',
                  }}
                >
                  {result.compile_stderr}
                </pre>
              </div>
            )}

            {/* Runtime stdout / stderr */}
            <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
              {result?.stdout && (
                <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0, marginBottom: result?.stderr ? '6px' : 0 }}>
                  <div style={{ color: 'var(--noesis-accent-green)', fontSize: '10.5px', fontWeight: 600, marginBottom: '2px' }}>
                    【标准输出 (stdout)】
                  </div>
                  <pre
                    style={{
                      flex: 1,
                      margin: 0,
                      padding: '8px',
                      background: 'var(--noesis-bg-mantle)',
                      borderRadius: '4px',
                      color: 'var(--noesis-accent-green)',
                      fontSize: '11.5px',
                      fontFamily: 'Consolas, monospace',
                      overflow: 'auto',
                      whiteSpace: 'pre-wrap',
                    }}
                  >
                    {result.stdout}
                  </pre>
                </div>
              )}

              {result?.stderr && (
                <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
                  <div style={{ color: 'var(--noesis-accent-peach)', fontSize: '10.5px', fontWeight: 600, marginBottom: '2px' }}>
                    【标准错误 (stderr)】
                  </div>
                  <pre
                    style={{
                      flex: 1,
                      margin: 0,
                      padding: '8px',
                      background: 'var(--noesis-bg-mantle)',
                      borderRadius: '4px',
                      color: 'var(--noesis-accent-peach)',
                      fontSize: '11.5px',
                      fontFamily: 'Consolas, monospace',
                      overflow: 'auto',
                      whiteSpace: 'pre-wrap',
                    }}
                  >
                    {result.stderr}
                  </pre>
                </div>
              )}

              {!result && (
                <pre
                  style={{
                    flex: 1,
                    margin: 0,
                    padding: '8px',
                    background: 'var(--noesis-bg-mantle)',
                    borderRadius: '4px',
                    color: 'var(--noesis-text-subtle)',
                    fontSize: '12px',
                    fontFamily: 'Consolas, monospace',
                    overflow: 'auto',
                    whiteSpace: 'pre-wrap',
                  }}
                >
                  点击右上角「运行」执行当前脚本...
                </pre>
              )}

              {result && !result.stdout && !result.stderr && !result.compile_stderr && (
                <pre
                  style={{
                    flex: 1,
                    margin: 0,
                    padding: '8px',
                    background: 'var(--noesis-bg-mantle)',
                    borderRadius: '4px',
                    color: 'var(--noesis-text-subtle)',
                    fontSize: '12px',
                    fontFamily: 'Consolas, monospace',
                    overflow: 'auto',
                    whiteSpace: 'pre-wrap',
                  }}
                >
                  (程序执行完毕，无输出)
                </pre>
              )}
            </div>
          </div>
        </div>
      ) : (
        <div style={{ flex: 1, padding: '12px', overflowY: 'auto' }}>
          <div style={{ fontSize: '12px', fontWeight: 600, color: 'var(--noesis-text-primary)', marginBottom: '8px' }}>
            📅 今日艾宾浩斯间隔复盘题目 ({dueReviews.length})
          </div>
          {dueReviews.length > 0 ? (
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
                gap: '10px',
              }}
            >
              {dueReviews.map((rev) => (
                <div
                  key={rev.id}
                  style={{
                    background: 'var(--noesis-bg-mantle)',
                    padding: '10px',
                    borderRadius: '6px',
                    border: '1px solid var(--noesis-bg-surface0)',
                  }}
                >
                  <div style={{ fontSize: '10px', color: 'var(--noesis-accent-yellow)', fontWeight: 'bold' }}>
                    {rev.exercise_type}
                  </div>
                  <div style={{ fontSize: '12px', color: 'var(--noesis-text-primary)', marginTop: '4px' }}>
                    {rev.question}
                  </div>
                  <pre
                    style={{
                      background: 'var(--noesis-bg-crust)',
                      padding: '6px',
                      borderRadius: '4px',
                      fontSize: '10.5px',
                      color: 'var(--noesis-accent-blue)',
                      margin: '6px 0',
                    }}
                  >
                    {rev.snippet}
                  </pre>
                  <div style={{ fontSize: '11px', color: 'var(--noesis-text-muted)' }}>
                    参考依据: {rev.file_path}
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div style={{ textAlign: 'center', padding: '24px', color: 'var(--noesis-text-subtle)', fontSize: '12px' }}>
              今日复习任务已全部达成！在日常阅读中标记看不懂，系统将自动安排次日复盘。
            </div>
          )}
        </div>
      )}
    </div>
  );
};
