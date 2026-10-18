import React, { createContext, useContext, useState, useCallback } from 'react';
import { CheckCircle2, AlertCircle, Info, X } from 'lucide-react';

export type ToastType = 'success' | 'error' | 'info';

export interface ToastItem {
  id: string;
  type: ToastType;
  message: string;
  action?: {
    label: string;
    onClick: () => void;
  };
  durationMs?: number;
}

interface ToastContextType {
  toast: {
    success: (message: string, action?: ToastItem['action'], durationMs?: number) => void;
    error: (message: string, action?: ToastItem['action'], durationMs?: number) => void;
    info: (message: string, action?: ToastItem['action'], durationMs?: number) => void;
    dismiss: (id: string) => void;
  };
}

const ToastContext = createContext<ToastContextType | null>(null);

export const ToastProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [toasts, setToasts] = useState<ToastItem[]>([]);

  const dismiss = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const addToast = useCallback(
    (type: ToastType, message: string, action?: ToastItem['action'], durationMs: number = 3500) => {
      const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
      const item: ToastItem = { id, type, message, action, durationMs };
      setToasts((prev) => [...prev, item]);

      if (durationMs > 0) {
        setTimeout(() => {
          dismiss(id);
        }, durationMs);
      }
    },
    [dismiss]
  );

  const value = {
    toast: {
      success: (msg: string, action?: ToastItem['action'], duration?: number) =>
        addToast('success', msg, action, duration),
      error: (msg: string, action?: ToastItem['action'], duration?: number) =>
        addToast('error', msg, action, duration ?? 5000),
      info: (msg: string, action?: ToastItem['action'], duration?: number) =>
        addToast('info', msg, action, duration),
      dismiss,
    },
  };

  return (
    <ToastContext.Provider value={value}>
      {children}
      {/* Toast Overlay Container */}
      <div
        style={{
          position: 'fixed',
          bottom: '24px',
          right: '24px',
          display: 'flex',
          flexDirection: 'column',
          gap: 'var(--noesis-space-2, 8px)',
          zIndex: 9999,
          pointerEvents: 'none',
          maxWidth: '420px',
        }}
      >
        {toasts.map((t) => (
          <div
            key={t.id}
            role={t.type === 'error' ? 'alert' : 'status'}
            aria-live={t.type === 'error' ? 'assertive' : 'polite'}
            style={{
              pointerEvents: 'auto',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: 'var(--noesis-space-2, 8px)',
              padding: '10px 14px',
              borderRadius: 'var(--noesis-radius-md, 6px)',
              background: 'var(--noesis-bg-mantle, #181825)',
              border: `1px solid ${
                t.type === 'success'
                  ? 'var(--noesis-accent-green, #a6e3a1)'
                  : t.type === 'error'
                  ? 'var(--noesis-accent-red, #f38ba8)'
                  : 'var(--noesis-accent-blue, #89b4fa)'
              }`,
              boxShadow: 'var(--noesis-shadow-lg, 0 12px 36px rgba(0,0,0,0.6))',
              color: 'var(--noesis-text-primary, #cdd6f4)',
              fontSize: 'var(--noesis-font-size-md, 12.5px)',
              animation: 'fadeIn var(--noesis-transition-fast, 120ms) ease',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              {t.type === 'success' && (
                <CheckCircle2 size={16} color="var(--noesis-accent-green, #a6e3a1)" />
              )}
              {t.type === 'error' && (
                <AlertCircle size={16} color="var(--noesis-accent-red, #f38ba8)" />
              )}
              {t.type === 'info' && (
                <Info size={16} color="var(--noesis-accent-blue, #89b4fa)" />
              )}
              <span style={{ lineHeight: 1.4 }}>{t.message}</span>
            </div>

            <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginLeft: '12px' }}>
              {t.action && (
                <button
                  onClick={() => {
                    t.action?.onClick();
                    dismiss(t.id);
                  }}
                  style={{
                    background: 'var(--noesis-bg-surface0, #313244)',
                    color: 'var(--noesis-accent-blue, #89b4fa)',
                    border: 'none',
                    borderRadius: 'var(--noesis-radius-sm, 4px)',
                    padding: '2px 8px',
                    fontSize: 'var(--noesis-font-size-sm, 11.5px)',
                    fontWeight: 600,
                    cursor: 'pointer',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {t.action.label}
                </button>
              )}
              <button
                onClick={() => dismiss(t.id)}
                aria-label="关闭提示"
                style={{
                  background: 'transparent',
                  border: 'none',
                  color: 'var(--noesis-text-subtle, #6c7086)',
                  cursor: 'pointer',
                  padding: '2px',
                  display: 'flex',
                  alignItems: 'center',
                }}
              >
                <X size={14} />
              </button>
            </div>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
};

export const useToast = () => {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    // Fallback if rendered outside provider
    return {
      success: (m: string) => console.log('[Toast Success]', m),
      error: (m: string) => console.error('[Toast Error]', m),
      info: (m: string) => console.info('[Toast Info]', m),
      dismiss: () => {},
    };
  }
  return ctx.toast;
};
