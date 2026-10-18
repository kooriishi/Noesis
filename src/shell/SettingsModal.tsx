import React, { useState } from 'react';
import { Sliders, Type, Palette, ShieldCheck, Cpu, X, RotateCcw, Wifi, Trash2 } from 'lucide-react';
import { AppSettings, LlmConfig, ConnectionTestResult } from '../types';
import { ApiService } from '../services/api';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  settings: AppSettings;
  onUpdateSettings: (newSettings: AppSettings) => void;
  llmConfig: LlmConfig;
  onUpdateLlmConfig: (newConfig: LlmConfig) => void;
  onClearApiKey?: () => void;
  onResetLayout: () => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  settings,
  onUpdateSettings,
  llmConfig,
  onUpdateLlmConfig,
  onClearApiKey,
  onResetLayout,
}) => {
  const [newKeyInput, setNewKeyInput] = useState('');
  const [testResult, setTestResult] = useState<ConnectionTestResult | null>(null);
  const [testing, setTesting] = useState(false);

  if (!isOpen) return null;

  const handleSaveKey = () => {
    if (!newKeyInput.trim()) return;
    onUpdateLlmConfig({ ...llmConfig, api_key: newKeyInput.trim() });
    setNewKeyInput('');
  };

  const handleTestConn = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const cfg = newKeyInput.trim()
        ? { ...llmConfig, api_key: newKeyInput.trim() }
        : llmConfig;
      const res = await ApiService.testLlmConnection(cfg);
      setTestResult(res);
    } catch (err: any) {
      setTestResult({
        ok: false,
        latency_ms: 0,
        category: 'network',
        message: err?.message || String(err),
      });
    } finally {
      setTesting(false);
    }
  };

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        backgroundColor: 'rgba(0, 0, 0, 0.65)',
        backdropFilter: 'blur(4px)',
        zIndex: 9999,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
      }}
      onClick={onClose}
    >
      <div
        style={{
          width: '560px',
          maxHeight: '85vh',
          backgroundColor: '#181825',
          border: '1px solid #313244',
          borderRadius: '10px',
          boxShadow: '0 16px 40px rgba(0, 0, 0, 0.6)',
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
          animation: 'fadeIn 0.15s ease',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div
          style={{
            padding: '14px 18px',
            borderBottom: '1px solid #313244',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            background: '#11111b',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Sliders size={18} color="#89b4fa" />
            <span style={{ fontSize: '15px', fontWeight: 600, color: '#cdd6f4' }}>
              IDE 系统偏好设置
            </span>
          </div>

          <button
            onClick={onClose}
            style={{
              background: 'transparent',
              border: 'none',
              color: '#6c7086',
              cursor: 'pointer',
              padding: '4px',
            }}
          >
            <X size={18} />
          </button>
        </div>

        {/* Body */}
        <div
          style={{
            flex: 1,
            overflowY: 'auto',
            padding: '18px',
            display: 'flex',
            flexDirection: 'column',
            gap: '18px',
          }}
        >
          {/* Section: Editor Appearance & Font */}
          <div>
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                fontSize: '13px',
                fontWeight: 600,
                color: '#89b4fa',
                marginBottom: '10px',
              }}
            >
              <Type size={16} />
              编辑器与字体设置
            </div>

            <div
              style={{
                background: '#1e1e2e',
                borderRadius: '8px',
                padding: '12px',
                border: '1px solid #313244',
                display: 'flex',
                flexDirection: 'column',
                gap: '12px',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <div style={{ fontSize: '12.5px', color: '#cdd6f4', fontWeight: 500 }}>
                    代码字体大小 (Editor Font Size)
                  </div>
                  <div style={{ fontSize: '11px', color: '#6c7086' }}>
                    实时调整 CodeMirror 6 代码编辑区字号
                  </div>
                </div>
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <input
                    type="range"
                    min="11"
                    max="22"
                    step="1"
                    value={settings.editorFontSize}
                    onChange={(e) =>
                      onUpdateSettings({ ...settings, editorFontSize: Number(e.target.value) })
                    }
                    style={{ width: '90px' }}
                  />
                  <span
                    style={{
                      fontSize: '12px',
                      color: '#89b4fa',
                      fontWeight: 600,
                      width: '32px',
                      textAlign: 'right',
                    }}
                  >
                    {settings.editorFontSize}px
                  </span>
                </div>
              </div>

              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <div style={{ fontSize: '12.5px', color: '#cdd6f4', fontWeight: 500 }}>
                    全局界面字号 (UI Font Scale)
                  </div>
                  <div style={{ fontSize: '11px', color: '#6c7086' }}>
                    侧边栏、标签页及导读卡片的字号比例
                  </div>
                </div>
                <select
                  value={settings.uiFontSize}
                  onChange={(e) =>
                    onUpdateSettings({ ...settings, uiFontSize: Number(e.target.value) })
                  }
                  style={{
                    background: '#313244',
                    color: '#cdd6f4',
                    border: 'none',
                    borderRadius: '4px',
                    padding: '4px 8px',
                    fontSize: '12px',
                  }}
                >
                  <option value={12}>12px (精细)</option>
                  <option value={13}>13px (默认)</option>
                  <option value={14}>14px (适中)</option>
                  <option value={15}>15px (大号)</option>
                </select>
              </div>

              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <div style={{ fontSize: '12.5px', color: '#cdd6f4', fontWeight: 500 }}>
                    等宽字体族 (Font Family)
                  </div>
                  <div style={{ fontSize: '11px', color: '#6c7086' }}>代码展示优先采用的字体栈</div>
                </div>
                <select
                  value={settings.fontFamily}
                  onChange={(e) => onUpdateSettings({ ...settings, fontFamily: e.target.value })}
                  style={{
                    background: '#313244',
                    color: '#cdd6f4',
                    border: 'none',
                    borderRadius: '4px',
                    padding: '4px 8px',
                    fontSize: '12px',
                  }}
                >
                  <option value="Consolas, 'Fira Code', monospace">Consolas, Fira Code</option>
                  <option value="'JetBrains Mono', Consolas, monospace">JetBrains Mono</option>
                  <option value="'Cascadia Code', Consolas, monospace">Cascadia Code</option>
                  <option value="'Fira Code', monospace">Fira Code (Ligatures)</option>
                  <option value="monospace">系统默认等宽 (Monospace)</option>
                </select>
              </div>

              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <div style={{ fontSize: '12.5px', color: '#cdd6f4', fontWeight: 500 }}>
                    缩进与换行 (Indent & Wrap)
                  </div>
                  <div style={{ fontSize: '11px', color: '#6c7086' }}>自动软换行与 Tab 宽度</div>
                </div>
                <div style={{ display: 'flex', gap: '8px' }}>
                  <button
                    onClick={() => onUpdateSettings({ ...settings, wordWrap: !settings.wordWrap })}
                    style={{
                      padding: '4px 8px',
                      background: settings.wordWrap ? 'rgba(137, 180, 250, 0.2)' : '#313244',
                      color: settings.wordWrap ? '#89b4fa' : '#a6adc8',
                      border: '1px solid',
                      borderColor: settings.wordWrap ? '#89b4fa' : 'transparent',
                      borderRadius: '4px',
                      fontSize: '11px',
                      cursor: 'pointer',
                    }}
                  >
                    换行: {settings.wordWrap ? '开启' : '关闭'}
                  </button>

                  <select
                    value={settings.tabSize}
                    onChange={(e) =>
                      onUpdateSettings({ ...settings, tabSize: Number(e.target.value) })
                    }
                    style={{
                      background: '#313244',
                      color: '#cdd6f4',
                      border: 'none',
                      borderRadius: '4px',
                      padding: '4px 8px',
                      fontSize: '11px',
                    }}
                  >
                    <option value={2}>Tab: 2空格</option>
                    <option value={4}>Tab: 4空格</option>
                  </select>
                </div>
              </div>
            </div>
          </div>

          {/* Section: Themes & Layout */}
          <div>
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                fontSize: '13px',
                fontWeight: 600,
                color: '#a6e3a1',
                marginBottom: '10px',
              }}
            >
              <Palette size={16} />
              主题与三栏布局
            </div>

            <div
              style={{
                background: '#1e1e2e',
                borderRadius: '8px',
                padding: '12px',
                border: '1px solid #313244',
                display: 'flex',
                flexDirection: 'column',
                gap: '12px',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <div style={{ fontSize: '12.5px', color: '#cdd6f4', fontWeight: 500 }}>
                    IDE 配色主题 (Theme)
                  </div>
                  <div style={{ fontSize: '11px', color: '#6c7086' }}>
                    适配高对比度暗色系阅读体验
                  </div>
                </div>
                <select
                  value={settings.theme}
                  onChange={(e) => onUpdateSettings({ ...settings, theme: e.target.value })}
                  style={{
                    background: '#313244',
                    color: '#cdd6f4',
                    border: 'none',
                    borderRadius: '4px',
                    padding: '4px 8px',
                    fontSize: '12px',
                  }}
                >
                  <option value="one-dark">One Dark Pro (经典)</option>
                  <option value="catppuccin">Catppuccin Mocha</option>
                  <option value="dracula">Dracula Dark</option>
                  <option value="tokyo-night">Tokyo Night</option>
                </select>
              </div>

              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <div style={{ fontSize: '12.5px', color: '#cdd6f4', fontWeight: 500 }}>
                    重置三栏窗格宽度
                  </div>
                  <div style={{ fontSize: '11px', color: '#6c7086' }}>
                    将左侧树 (280px) 和右侧 AI 导读 (380px) 还原至默认尺寸
                  </div>
                </div>
                <button
                  onClick={onResetLayout}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '4px',
                    padding: '4px 10px',
                    background: '#313244',
                    color: '#cdd6f4',
                    border: 'none',
                    borderRadius: '4px',
                    fontSize: '11.5px',
                    cursor: 'pointer',
                  }}
                >
                  <RotateCcw size={13} />
                  重置布局
                </button>
              </div>
            </div>
          </div>

          {/* Section: LLM Model & Credential Store */}
          <div>
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                fontSize: '13px',
                fontWeight: 600,
                color: '#f9e2af',
                marginBottom: '10px',
              }}
            >
              <Cpu size={16} />
              AI 智能导读模型与安全凭据 (不经过 localStorage)
            </div>

            <div
              style={{
                background: '#1e1e2e',
                borderRadius: '8px',
                padding: '12px',
                border: '1px solid #313244',
                display: 'flex',
                flexDirection: 'column',
                gap: '10px',
              }}
            >
              <div>
                <label
                  style={{
                    fontSize: '11px',
                    color: '#a6adc8',
                    display: 'block',
                    marginBottom: '4px',
                  }}
                >
                  接口地址 (API Endpoint)
                </label>
                <input
                  type="text"
                  value={llmConfig.api_url}
                  onChange={(e) => onUpdateLlmConfig({ ...llmConfig, api_url: e.target.value })}
                  placeholder="https://api.deepseek.com"
                  style={{
                    width: '100%',
                    boxSizing: 'border-box',
                    padding: '6px 8px',
                    background: '#313244',
                    border: 'none',
                    borderRadius: '4px',
                    color: '#cdd6f4',
                    fontSize: '12px',
                  }}
                />
              </div>

              <div>
                <label
                  style={{
                    fontSize: '11px',
                    color: '#a6adc8',
                    display: 'block',
                    marginBottom: '4px',
                  }}
                >
                  模型密钥 (API Key) — 当前状态:{' '}
                  <strong style={{ color: llmConfig.has_api_key ? '#a6e3a1' : '#f9e2af' }}>
                    {llmConfig.has_api_key
                      ? `已安全存储 (${llmConfig.api_key_masked})`
                      : '未配置'}
                  </strong>
                </label>
                <div style={{ display: 'flex', gap: '8px' }}>
                  <input
                    type="password"
                    value={newKeyInput}
                    onChange={(e) => setNewKeyInput(e.target.value)}
                    placeholder={
                      llmConfig.has_api_key
                        ? '输入新密钥覆盖现有凭据...'
                        : '输入 sk-... 保存至系统安全凭据库'
                    }
                    style={{
                      flex: 1,
                      boxSizing: 'border-box',
                      padding: '6px 8px',
                      background: '#313244',
                      border: 'none',
                      borderRadius: '4px',
                      color: '#cdd6f4',
                      fontSize: '12px',
                    }}
                  />
                  <button
                    onClick={handleSaveKey}
                    disabled={!newKeyInput.trim()}
                    style={{
                      padding: '6px 12px',
                      background: '#89b4fa',
                      color: '#11111b',
                      border: 'none',
                      borderRadius: '4px',
                      fontSize: '11.5px',
                      fontWeight: 600,
                      cursor: newKeyInput.trim() ? 'pointer' : 'default',
                    }}
                  >
                    保存密钥
                  </button>
                  {llmConfig.has_api_key && onClearApiKey && (
                    <button
                      onClick={onClearApiKey}
                      title="清除已保存的密钥"
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: '4px',
                        padding: '6px 10px',
                        background: 'rgba(243, 139, 168, 0.15)',
                        color: '#f38ba8',
                        border: '1px solid rgba(243, 139, 168, 0.35)',
                        borderRadius: '4px',
                        fontSize: '11.5px',
                        cursor: 'pointer',
                      }}
                    >
                      <Trash2 size={13} />
                      清除
                    </button>
                  )}
                </div>
              </div>

              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
                <div>
                  <label
                    style={{
                      fontSize: '11px',
                      color: '#a6adc8',
                      display: 'block',
                      marginBottom: '4px',
                    }}
                  >
                    模型 (Model)
                  </label>
                  <input
                    type="text"
                    value={llmConfig.model}
                    onChange={(e) => onUpdateLlmConfig({ ...llmConfig, model: e.target.value })}
                    placeholder="deepseek-chat"
                    style={{
                      width: '100%',
                      boxSizing: 'border-box',
                      padding: '6px 8px',
                      background: '#313244',
                      border: 'none',
                      borderRadius: '4px',
                      color: '#cdd6f4',
                      fontSize: '12px',
                    }}
                  />
                </div>

                <div>
                  <label
                    style={{
                      fontSize: '11px',
                      color: '#a6adc8',
                      display: 'block',
                      marginBottom: '4px',
                    }}
                  >
                    采样温度 (Temperature)
                  </label>
                  <input
                    type="number"
                    min="0"
                    max="1"
                    step="0.1"
                    value={llmConfig.temperature}
                    onChange={(e) =>
                      onUpdateLlmConfig({
                        ...llmConfig,
                        temperature: parseFloat(e.target.value) || 0.3,
                      })
                    }
                    style={{
                      width: '100%',
                      boxSizing: 'border-box',
                      padding: '6px 8px',
                      background: '#313244',
                      border: 'none',
                      borderRadius: '4px',
                      color: '#cdd6f4',
                      fontSize: '12px',
                    }}
                  />
                </div>
              </div>

              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <button
                  onClick={handleTestConn}
                  disabled={testing}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '6px',
                    padding: '6px 12px',
                    background: '#313244',
                    color: '#cdd6f4',
                    border: 'none',
                    borderRadius: '4px',
                    fontSize: '11.5px',
                    cursor: 'pointer',
                  }}
                >
                  <Wifi size={13} />
                  {testing ? '正在探测连接...' : '测试连接'}
                </button>
                {testResult && (
                  <span
                    style={{
                      fontSize: '11px',
                      color: testResult.ok ? '#a6e3a1' : '#f38ba8',
                    }}
                  >
                    {testResult.message}
                  </span>
                )}
              </div>

              <div
                style={{
                  background: 'rgba(166, 227, 161, 0.08)',
                  borderRadius: '6px',
                  padding: '8px 10px',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '6px',
                  color: '#a6e3a1',
                  fontSize: '11px',
                }}
              >
                <ShieldCheck size={14} />
                <span>
                  P0 外发控制已自动脱敏代码内敏感 IP 与 Key，并限制 100,000 字符容量上限。
                </span>
              </div>
            </div>
          </div>
        </div>

        {/* Footer */}
        <div
          style={{
            padding: '12px 18px',
            borderTop: '1px solid #313244',
            display: 'flex',
            justifyContent: 'flex-end',
            background: '#11111b',
          }}
        >
          <button
            onClick={onClose}
            style={{
              padding: '6px 16px',
              background: '#89b4fa',
              color: '#11111b',
              border: 'none',
              borderRadius: '6px',
              fontSize: '12.5px',
              fontWeight: 600,
              cursor: 'pointer',
            }}
          >
            完成并关闭
          </button>
        </div>
      </div>
    </div>
  );
};
