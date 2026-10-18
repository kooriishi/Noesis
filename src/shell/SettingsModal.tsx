import React, { useState, useEffect } from 'react';
import {
  Sliders,
  Type,
  ShieldCheck,
  Cpu,
  X,
  RotateCcw,
  Wifi,
  Trash2,
  Bot,
  Sparkles,
  Lock,
  Plus,
  Check,
  TrendingUp,
} from 'lucide-react';
import {
  AppSettings,
  LlmConfig,
  ConnectionTestResult,
  PromptPreset,
  EffectivePromptPreview,
  FeedbackStats,
} from '../types';
import { ApiService } from '../services/api';
import { useToast } from '../components/feedback';

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
  const toast = useToast();
  const [activeTab, setActiveTab] = useState<'general' | 'model' | 'agent'>('general');
  const [newKeyInput, setNewKeyInput] = useState('');
  const [testResult, setTestResult] = useState<ConnectionTestResult | null>(null);
  const [testing, setTesting] = useState(false);

  // F5 Prompt Preset State
  const [agentScope, setAgentScope] = useState<'explain' | 'ask'>('explain');
  const [presets, setPresets] = useState<PromptPreset[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<string>('');
  const [personaDraft, setPersonaDraft] = useState<string>('');
  const [isSavingPreset, setIsSavingPreset] = useState<boolean>(false);
  const [effectivePreview, setEffectivePreview] = useState<EffectivePromptPreview | null>(null);
  const [showLockedRules, setShowLockedRules] = useState<boolean>(false);
  const [feedbackStats, setFeedbackStats] = useState<FeedbackStats | null>(null);

  useEffect(() => {
    if (isOpen && activeTab === 'agent') {
      loadPresets(agentScope);
      loadFeedbackStats();
    }
  }, [isOpen, activeTab, agentScope]);

  const loadPresets = async (scope: string) => {
    try {
      const list = await ApiService.listPromptPresets(scope);
      setPresets(list);
      const active = list.find((p) => p.is_active) || list[0];
      if (active) {
        setSelectedPresetId(active.id);
        setPersonaDraft(active.persona_text);
      }
      const preview = await ApiService.previewEffectivePrompt(scope);
      setEffectivePreview(preview);
    } catch (err: any) {
      console.error('Failed to load presets:', err);
    }
  };

  const loadFeedbackStats = async () => {
    try {
      const stats = await ApiService.getFeedbackStats();
      setFeedbackStats(stats);
    } catch {
      // ignore
    }
  };

  if (!isOpen) return null;

  const handleSaveKey = () => {
    if (!newKeyInput.trim()) return;
    onUpdateLlmConfig({ ...llmConfig, api_key: newKeyInput.trim() });
    setNewKeyInput('');
    toast.success('API 密钥已更新');
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
      if (res.ok) {
        toast.success(`连接成功！响应延迟: ${res.latency_ms}ms`);
      } else {
        toast.error(`连接失败: ${res.message}`);
      }
    } catch (err: any) {
      const msg = err?.message || String(err);
      setTestResult({
        ok: false,
        latency_ms: 0,
        category: 'network',
        message: msg,
      });
      toast.error(`连接探测异常: ${msg}`);
    } finally {
      setTesting(false);
    }
  };

  const handleSelectPreset = (id: string) => {
    setSelectedPresetId(id);
    const target = presets.find((p) => p.id === id);
    if (target) {
      setPersonaDraft(target.persona_text);
    }
  };

  const handleSetActive = async () => {
    if (!selectedPresetId) return;
    try {
      await ApiService.setActivePromptPreset(selectedPresetId);
      toast.success('已切换为当前活动智能体预设');
      await loadPresets(agentScope);
    } catch (err: any) {
      toast.error(`激活预设失败: ${err?.message || err}`);
    }
  };

  const handleSaveDraft = async () => {
    const cur = presets.find((p) => p.id === selectedPresetId);
    if (!cur) return;
    setIsSavingPreset(true);
    try {
      await ApiService.savePromptPreset({
        ...cur,
        persona_text: personaDraft,
        updated_at: Date.now(),
      });
      toast.success('人设风格（L1）已保存并生效');
      await loadPresets(agentScope);
    } catch (err: any) {
      toast.error(`保存失败: ${err?.message || err}`);
    } finally {
      setIsSavingPreset(false);
    }
  };

  const handleCreateNewPreset = async () => {
    const name = window.prompt('请输入新智能体预设名称：', '自定义代码导读导师');
    if (!name || !name.trim()) return;
    try {
      const newPreset: PromptPreset = {
        id: `preset-${Date.now()}`,
        name: name.trim(),
        scope: agentScope,
        persona_text: personaDraft || '你是一位严谨的代码架构导读专家。',
        created_at: Date.now(),
        updated_at: Date.now(),
        is_active: true,
        is_builtin: false,
      };
      await ApiService.savePromptPreset(newPreset);
      await ApiService.setActivePromptPreset(newPreset.id);
      toast.success(`新预设「${name.trim()}」已创建并激活`);
      await loadPresets(agentScope);
    } catch (err: any) {
      toast.error(`创建预设失败: ${err?.message || err}`);
    }
  };

  const handleResetDefault = async () => {
    if (!window.confirm('确定将该场景的所有智能体提示词恢复为官方默认状态吗？')) return;
    try {
      await ApiService.resetPromptToDefault(agentScope);
      toast.success('已成功恢复内置默认提示词');
      await loadPresets(agentScope);
    } catch (err: any) {
      toast.error(`重置失败: ${err?.message || err}`);
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
          width: '680px',
          maxHeight: '88vh',
          backgroundColor: 'var(--noesis-bg-mantle, #181825)',
          border: '1px solid var(--noesis-bg-surface0, #313244)',
          borderRadius: 'var(--noesis-radius-lg, 10px)',
          boxShadow: 'var(--noesis-shadow-lg, 0 16px 40px rgba(0, 0, 0, 0.6))',
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
            padding: '12px 18px',
            borderBottom: '1px solid var(--noesis-bg-surface0, #313244)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            background: 'var(--noesis-bg-crust, #11111b)',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Sliders size={18} color="var(--noesis-accent-blue, #89b4fa)" />
            <span style={{ fontSize: '15px', fontWeight: 600, color: 'var(--noesis-text-primary, #cdd6f4)' }}>
              IDE 系统偏好设置
            </span>
          </div>

          <button
            onClick={onClose}
            style={{
              background: 'transparent',
              border: 'none',
              color: 'var(--noesis-text-subtle, #6c7086)',
              cursor: 'pointer',
              padding: '4px',
            }}
          >
            <X size={18} />
          </button>
        </div>

        {/* Navigation Tabs */}
        <div
          style={{
            display: 'flex',
            background: 'var(--noesis-bg-crust, #11111b)',
            borderBottom: '1px solid var(--noesis-bg-surface0, #313244)',
            padding: '0 12px',
            gap: '8px',
          }}
        >
          {[
            { id: 'general', label: '常规与外观', icon: Type },
            { id: 'model', label: '模型与接口', icon: Cpu },
            { id: 'agent', label: '智能体提示词 (F5)', icon: Bot },
          ].map((t) => {
            const Icon = t.icon;
            const isActive = activeTab === t.id;
            return (
              <button
                key={t.id}
                onClick={() => setActiveTab(t.id as any)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '6px',
                  padding: '9px 12px',
                  fontSize: '12px',
                  fontWeight: isActive ? 600 : 400,
                  color: isActive ? 'var(--noesis-accent-blue, #89b4fa)' : 'var(--noesis-text-muted, #a6adc8)',
                  background: 'transparent',
                  border: 'none',
                  borderBottom: isActive ? '2px solid var(--noesis-accent-blue, #89b4fa)' : '2px solid transparent',
                  cursor: 'pointer',
                }}
              >
                <Icon size={14} />
                {t.label}
              </button>
            );
          })}
        </div>

        {/* Body */}
        <div
          style={{
            flex: 1,
            overflowY: 'auto',
            padding: '18px',
            display: 'flex',
            flexDirection: 'column',
            gap: '16px',
          }}
        >
          {/* TAB 1: GENERAL */}
          {activeTab === 'general' && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
              <div
                style={{
                  background: 'var(--noesis-bg-base, #1e1e2e)',
                  borderRadius: 'var(--noesis-radius-md, 8px)',
                  padding: '14px',
                  border: '1px solid var(--noesis-bg-surface0, #313244)',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '12px',
                }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <div>
                    <div style={{ fontSize: '12.5px', color: 'var(--noesis-text-primary, #cdd6f4)', fontWeight: 500 }}>
                      代码字体大小 (Editor Font Size)
                    </div>
                    <div style={{ fontSize: '11px', color: 'var(--noesis-text-subtle, #6c7086)' }}>
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
                        color: 'var(--noesis-accent-blue, #89b4fa)',
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
                    <div style={{ fontSize: '12.5px', color: 'var(--noesis-text-primary, #cdd6f4)', fontWeight: 500 }}>
                      全局界面字号 (UI Font Scale)
                    </div>
                    <div style={{ fontSize: '11px', color: 'var(--noesis-text-subtle, #6c7086)' }}>
                      实时缩放侧边栏、状态栏和卡片标签等文字大小
                    </div>
                  </div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <input
                      type="range"
                      min="11"
                      max="16"
                      step="1"
                      value={settings.uiFontSize}
                      onChange={(e) =>
                        onUpdateSettings({ ...settings, uiFontSize: Number(e.target.value) })
                      }
                      style={{ width: '90px' }}
                    />
                    <span
                      style={{
                        fontSize: '12px',
                        color: 'var(--noesis-accent-blue, #89b4fa)',
                        fontWeight: 600,
                        width: '32px',
                        textAlign: 'right',
                      }}
                    >
                      {settings.uiFontSize}px
                    </span>
                  </div>
                </div>

                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <div>
                    <div style={{ fontSize: '12.5px', color: 'var(--noesis-text-primary, #cdd6f4)', fontWeight: 500 }}>
                      自动换行 (Word Wrap)
                    </div>
                    <div style={{ fontSize: '11px', color: 'var(--noesis-text-subtle, #6c7086)' }}>
                      长代码行自动折行显示
                    </div>
                  </div>
                  <input
                    type="checkbox"
                    checked={settings.wordWrap}
                    onChange={(e) => onUpdateSettings({ ...settings, wordWrap: e.target.checked })}
                    style={{ cursor: 'pointer' }}
                  />
                </div>
              </div>

              {/* Reset layout */}
              <div
                style={{
                  background: 'var(--noesis-bg-base, #1e1e2e)',
                  borderRadius: 'var(--noesis-radius-md, 8px)',
                  padding: '12px 14px',
                  border: '1px solid var(--noesis-bg-surface0, #313244)',
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                }}
              >
                <div>
                  <div style={{ fontSize: '12px', color: 'var(--noesis-text-primary, #cdd6f4)', fontWeight: 500 }}>
                    重置三栏窗格布局
                  </div>
                  <div style={{ fontSize: '11px', color: 'var(--noesis-text-subtle, #6c7086)' }}>
                    恢复左侧 260px、右侧 380px 的默认分割比例
                  </div>
                </div>
                <button
                  onClick={() => {
                    onResetLayout();
                    toast.success('三栏窗格宽度已重置为默认值');
                  }}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '5px',
                    padding: '5px 10px',
                    background: 'var(--noesis-bg-surface0, #313244)',
                    color: 'var(--noesis-text-primary, #cdd6f4)',
                    border: 'none',
                    borderRadius: 'var(--noesis-radius-sm, 4px)',
                    fontSize: '11.5px',
                    cursor: 'pointer',
                  }}
                >
                  <RotateCcw size={12} />
                  重置布局
                </button>
              </div>
            </div>
          )}

          {/* TAB 2: MODEL */}
          {activeTab === 'model' && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
              <div
                style={{
                  background: 'var(--noesis-bg-base, #1e1e2e)',
                  borderRadius: 'var(--noesis-radius-md, 8px)',
                  padding: '14px',
                  border: '1px solid var(--noesis-bg-surface0, #313244)',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '10px',
                }}
              >
                <div>
                  <label style={{ fontSize: '11px', color: 'var(--noesis-text-muted, #a6adc8)', display: 'block', marginBottom: '4px' }}>
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
                      background: 'var(--noesis-bg-surface0, #313244)',
                      border: 'none',
                      borderRadius: 'var(--noesis-radius-sm, 4px)',
                      color: 'var(--noesis-text-primary, #cdd6f4)',
                      fontSize: '12px',
                    }}
                  />
                </div>

                <div>
                  <label style={{ fontSize: '11px', color: 'var(--noesis-text-muted, #a6adc8)', display: 'block', marginBottom: '4px' }}>
                    模型密钥 (API Key) — 当前状态:{' '}
                    <strong style={{ color: llmConfig.has_api_key ? 'var(--noesis-accent-green, #a6e3a1)' : 'var(--noesis-accent-yellow, #f9e2af)' }}>
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
                        background: 'var(--noesis-bg-surface0, #313244)',
                        border: 'none',
                        borderRadius: 'var(--noesis-radius-sm, 4px)',
                        color: 'var(--noesis-text-primary, #cdd6f4)',
                        fontSize: '12px',
                      }}
                    />
                    <button
                      onClick={handleSaveKey}
                      disabled={!newKeyInput.trim()}
                      style={{
                        padding: '6px 12px',
                        background: 'var(--noesis-accent-blue, #89b4fa)',
                        color: 'var(--noesis-bg-crust, #11111b)',
                        border: 'none',
                        borderRadius: 'var(--noesis-radius-sm, 4px)',
                        fontSize: '11.5px',
                        fontWeight: 600,
                        cursor: newKeyInput.trim() ? 'pointer' : 'default',
                      }}
                    >
                      保存密钥
                    </button>
                    {llmConfig.has_api_key && onClearApiKey && (
                      <button
                        onClick={() => {
                          onClearApiKey();
                          toast.info('已清除已保存的模型密钥');
                        }}
                        title="清除已保存的密钥"
                        style={{
                          display: 'flex',
                          alignItems: 'center',
                          gap: '4px',
                          padding: '6px 10px',
                          background: 'rgba(243, 139, 168, 0.15)',
                          color: 'var(--noesis-accent-red, #f38ba8)',
                          border: '1px solid rgba(243, 139, 168, 0.3)',
                          borderRadius: 'var(--noesis-radius-sm, 4px)',
                          fontSize: '11px',
                          cursor: 'pointer',
                        }}
                      >
                        <Trash2 size={12} />
                        清除
                      </button>
                    )}
                  </div>
                </div>

                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
                  <div>
                    <label style={{ fontSize: '11px', color: 'var(--noesis-text-muted, #a6adc8)', display: 'block', marginBottom: '4px' }}>
                      模型名称 (Model)
                    </label>
                    <input
                      type="text"
                      value={llmConfig.model}
                      onChange={(e) => onUpdateLlmConfig({ ...llmConfig, model: e.target.value })}
                      style={{
                        width: '100%',
                        boxSizing: 'border-box',
                        padding: '6px 8px',
                        background: 'var(--noesis-bg-surface0, #313244)',
                        border: 'none',
                        borderRadius: 'var(--noesis-radius-sm, 4px)',
                        color: 'var(--noesis-text-primary, #cdd6f4)',
                        fontSize: '12px',
                      }}
                    />
                  </div>
                  <div>
                    <label style={{ fontSize: '11px', color: 'var(--noesis-text-muted, #a6adc8)', display: 'block', marginBottom: '4px' }}>
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
                        background: 'var(--noesis-bg-surface0, #313244)',
                        border: 'none',
                        borderRadius: 'var(--noesis-radius-sm, 4px)',
                        color: 'var(--noesis-text-primary, #cdd6f4)',
                        fontSize: '12px',
                      }}
                    />
                  </div>
                </div>

                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: '6px' }}>
                  <button
                    onClick={handleTestConn}
                    disabled={testing}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '6px',
                      padding: '6px 12px',
                      background: 'var(--noesis-bg-surface0, #313244)',
                      color: 'var(--noesis-text-primary, #cdd6f4)',
                      border: 'none',
                      borderRadius: 'var(--noesis-radius-sm, 4px)',
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
                        color: testResult.ok ? 'var(--noesis-accent-green, #a6e3a1)' : 'var(--noesis-accent-red, #f38ba8)',
                      }}
                    >
                      {testResult.message}
                    </span>
                  )}
                </div>

                <div
                  style={{
                    background: 'rgba(166, 227, 161, 0.08)',
                    borderRadius: 'var(--noesis-radius-sm, 6px)',
                    padding: '8px 10px',
                    display: 'flex',
                    alignItems: 'center',
                    gap: '6px',
                    color: 'var(--noesis-accent-green, #a6e3a1)',
                    fontSize: '11px',
                    marginTop: '4px',
                  }}
                >
                  <ShieldCheck size={14} />
                  <span>
                    P0 外发控制已自动脱敏代码内敏感 IP 与 Key，并限制 100,000 字符容量上限。
                  </span>
                </div>
              </div>
            </div>
          )}

          {/* TAB 3: AGENTS (F5) */}
          {activeTab === 'agent' && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
              {/* Scene Scope Switcher */}
              <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                <span style={{ fontSize: '12px', color: 'var(--noesis-text-muted, #a6adc8)' }}>适用场景:</span>
                <div style={{ display: 'flex', background: 'var(--noesis-bg-base, #1e1e2e)', padding: '2px', borderRadius: '6px', border: '1px solid var(--noesis-bg-surface0, #313244)' }}>
                  <button
                    onClick={() => setAgentScope('explain')}
                    style={{
                      padding: '4px 10px',
                      fontSize: '11.5px',
                      borderRadius: '4px',
                      border: 'none',
                      background: agentScope === 'explain' ? 'var(--noesis-accent-blue, #89b4fa)' : 'transparent',
                      color: agentScope === 'explain' ? 'var(--noesis-bg-crust, #11111b)' : 'var(--noesis-text-primary, #cdd6f4)',
                      fontWeight: 600,
                      cursor: 'pointer',
                    }}
                  >
                    四层解读 (explain)
                  </button>
                  <button
                    onClick={() => setAgentScope('ask')}
                    style={{
                      padding: '4px 10px',
                      fontSize: '11.5px',
                      borderRadius: '4px',
                      border: 'none',
                      background: agentScope === 'ask' ? 'var(--noesis-accent-blue, #89b4fa)' : 'transparent',
                      color: agentScope === 'ask' ? 'var(--noesis-bg-crust, #11111b)' : 'var(--noesis-text-primary, #cdd6f4)',
                      fontWeight: 600,
                      cursor: 'pointer',
                    }}
                  >
                    智能问答 (ask)
                  </button>
                </div>

                <div style={{ marginLeft: 'auto', display: 'flex', gap: '6px' }}>
                  <button
                    onClick={handleCreateNewPreset}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '4px',
                      padding: '4px 8px',
                      background: 'var(--noesis-bg-surface0, #313244)',
                      color: 'var(--noesis-accent-blue, #89b4fa)',
                      border: 'none',
                      borderRadius: '4px',
                      fontSize: '11px',
                      cursor: 'pointer',
                    }}
                  >
                    <Plus size={12} />
                    新建预设
                  </button>
                  <button
                    onClick={handleResetDefault}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '4px',
                      padding: '4px 8px',
                      background: 'transparent',
                      color: 'var(--noesis-text-subtle, #6c7086)',
                      border: '1px solid var(--noesis-bg-surface0, #313244)',
                      borderRadius: '4px',
                      fontSize: '11px',
                      cursor: 'pointer',
                    }}
                  >
                    <RotateCcw size={11} />
                    恢复默认
                  </button>
                </div>
              </div>

              {/* Preset Selector */}
              <div
                style={{
                  background: 'var(--noesis-bg-base, #1e1e2e)',
                  borderRadius: 'var(--noesis-radius-md, 8px)',
                  padding: '12px',
                  border: '1px solid var(--noesis-bg-surface0, #313244)',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px',
                }}
              >
                <label style={{ fontSize: '11.5px', color: 'var(--noesis-text-primary, #cdd6f4)', fontWeight: 500 }}>
                  当前选择预设:
                </label>
                <select
                  value={selectedPresetId}
                  onChange={(e) => handleSelectPreset(e.target.value)}
                  style={{
                    flex: 1,
                    padding: '5px 8px',
                    background: 'var(--noesis-bg-surface0, #313244)',
                    border: 'none',
                    borderRadius: '4px',
                    color: 'var(--noesis-text-primary, #cdd6f4)',
                    fontSize: '11.5px',
                  }}
                >
                  {presets.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name} {p.is_active ? '（当前生效中）' : ''} {p.is_builtin ? '· 内置' : ''}
                    </option>
                  ))}
                </select>

                <button
                  onClick={handleSetActive}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '4px',
                    padding: '5px 10px',
                    background: 'var(--noesis-accent-green, #a6e3a1)',
                    color: 'var(--noesis-bg-crust, #11111b)',
                    border: 'none',
                    borderRadius: '4px',
                    fontSize: '11.5px',
                    fontWeight: 600,
                    cursor: 'pointer',
                  }}
                >
                  <Check size={12} />
                  设为生效
                </button>
              </div>

              {/* L1 Persona Editor */}
              <div
                style={{
                  background: 'var(--noesis-bg-base, #1e1e2e)',
                  borderRadius: 'var(--noesis-radius-md, 8px)',
                  padding: '12px',
                  border: '1px solid var(--noesis-bg-surface0, #313244)',
                }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '8px' }}>
                  <div style={{ fontSize: '12px', fontWeight: 600, color: 'var(--noesis-accent-lavender, #b4befe)', display: 'flex', alignItems: 'center', gap: '5px' }}>
                    <Sparkles size={14} />
                    L1 · 人设风格设定（用户自由定制）
                  </div>
                  <button
                    onClick={handleSaveDraft}
                    disabled={isSavingPreset}
                    style={{
                      padding: '4px 10px',
                      background: 'var(--noesis-accent-blue, #89b4fa)',
                      color: 'var(--noesis-bg-crust, #11111b)',
                      border: 'none',
                      borderRadius: '4px',
                      fontSize: '11px',
                      fontWeight: 600,
                      cursor: 'pointer',
                    }}
                  >
                    {isSavingPreset ? '保存中...' : '保存人设'}
                  </button>
                </div>
                <textarea
                  value={personaDraft}
                  onChange={(e) => setPersonaDraft(e.target.value)}
                  rows={6}
                  placeholder="在此自由编写你希望 AI 扮演的专家角色风格、教学口吻、类比偏好与关注点..."
                  style={{
                    width: '100%',
                    boxSizing: 'border-box',
                    background: 'var(--noesis-bg-surface0, #313244)',
                    border: 'none',
                    borderRadius: '4px',
                    padding: '8px',
                    color: 'var(--noesis-text-primary, #cdd6f4)',
                    fontSize: '11.5px',
                    lineHeight: '1.5',
                    fontFamily: 'var(--noesis-font-mono, monospace)',
                    resize: 'vertical',
                  }}
                />
              </div>

              {/* L0 Locked Rules Accordion */}
              <div
                style={{
                  background: 'var(--noesis-bg-base, #1e1e2e)',
                  borderRadius: 'var(--noesis-radius-md, 8px)',
                  border: '1px solid var(--noesis-bg-surface0, #313244)',
                  overflow: 'hidden',
                }}
              >
                <button
                  onClick={() => setShowLockedRules(!showLockedRules)}
                  style={{
                    width: '100%',
                    padding: '10px 12px',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    background: 'transparent',
                    border: 'none',
                    color: 'var(--noesis-text-muted, #a6adc8)',
                    fontSize: '11.5px',
                    fontWeight: 600,
                    cursor: 'pointer',
                  }}
                >
                  <span style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                    <Lock size={13} color="var(--noesis-accent-peach, #fab387)" />
                    L0 · 系统级锁定铁律 (安全/Schema/P0 - 只读保护)
                  </span>
                  <span>{showLockedRules ? '收起 ▲' : '展开查看 ▼'}</span>
                </button>
                {showLockedRules && (
                  <div
                    style={{
                      padding: '10px 12px',
                      borderTop: '1px solid var(--noesis-bg-surface0, #313244)',
                      background: 'var(--noesis-bg-crust, #11111b)',
                      fontSize: '11px',
                      color: 'var(--noesis-text-subtle, #6c7086)',
                      lineHeight: '1.5',
                      maxHeight: '180px',
                      overflowY: 'auto',
                      fontFamily: 'var(--noesis-font-mono, monospace)',
                    }}
                  >
                    <pre style={{ margin: 0, whiteSpace: 'pre-wrap' }}>
                      {effectivePreview?.l0_locked || '加载锁定规则中...'}
                    </pre>
                  </div>
                )}
              </div>

              {/* Effective Full Prompt Preview & Token Stats */}
              {effectivePreview && (
                <div
                  style={{
                    background: 'var(--noesis-bg-base, #1e1e2e)',
                    borderRadius: 'var(--noesis-radius-md, 8px)',
                    padding: '10px 12px',
                    border: '1px solid var(--noesis-bg-surface0, #313244)',
                    display: 'flex',
                    justifyContent: 'space-between',
                    alignItems: 'center',
                    fontSize: '11px',
                    color: 'var(--noesis-text-muted, #a6adc8)',
                  }}
                >
                  <span>
                    完整组装提示词预期预算消耗:{' '}
                    <strong style={{ color: 'var(--noesis-accent-green, #a6e3a1)' }}>
                      ~{effectivePreview.token_estimate} tokens
                    </strong>{' '}
                    (L1: {effectivePreview.l1_persona.length} 字符 · L0: {effectivePreview.l0_locked.length} 字符)
                  </span>
                  <span style={{ color: 'var(--noesis-accent-blue, #89b4fa)', fontSize: '10.5px' }}>
                    已自动对接质量门禁
                  </span>
                </div>
              )}

              {/* Feedback Baseline Stats Summary (F7-a) */}
              {feedbackStats && (
                <div
                  style={{
                    background: 'var(--noesis-bg-base, #1e1e2e)',
                    borderRadius: 'var(--noesis-radius-md, 8px)',
                    padding: '10px 14px',
                    border: '1px solid var(--noesis-bg-surface0, #313244)',
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'center', gap: '5px', fontSize: '11.5px', fontWeight: 600, color: 'var(--noesis-accent-teal, #94e2d5)', marginBottom: '6px' }}>
                    <TrendingUp size={13} />
                    本地评测基线与用户反馈统计 (F7-a)
                  </div>
                  <div style={{ display: 'flex', gap: '16px', fontSize: '11px', color: 'var(--noesis-text-muted, #a6adc8)' }}>
                    <span>总收集反馈: <strong>{feedbackStats.total}</strong> 条</span>
                    <span>有用数: <strong style={{ color: 'var(--noesis-accent-green, #a6e3a1)' }}>{feedbackStats.helpful}</strong></span>
                    <span>太泛率: <strong style={{ color: feedbackStats.too_generic_rate > 0.3 ? 'var(--noesis-accent-peach, #fab387)' : 'var(--noesis-text-primary, #cdd6f4)' }}>{(feedbackStats.too_generic_rate * 100).toFixed(1)}%</strong></span>
                    <span>有误率: <strong style={{ color: feedbackStats.wrong_rate > 0.1 ? 'var(--noesis-accent-red, #f38ba8)' : 'var(--noesis-text-primary, #cdd6f4)' }}>{(feedbackStats.wrong_rate * 100).toFixed(1)}%</strong></span>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div
          style={{
            padding: '12px 18px',
            borderTop: '1px solid var(--noesis-bg-surface0, #313244)',
            display: 'flex',
            justifyContent: 'flex-end',
            background: 'var(--noesis-bg-crust, #11111b)',
          }}
        >
          <button
            onClick={onClose}
            style={{
              padding: '6px 16px',
              background: 'var(--noesis-accent-blue, #89b4fa)',
              color: 'var(--noesis-bg-crust, #11111b)',
              border: 'none',
              borderRadius: 'var(--noesis-radius-sm, 6px)',
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
