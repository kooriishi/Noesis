import { useState, useRef, useCallback } from 'react';

export type TaskStatus = 'idle' | 'running' | 'success' | 'error';

export interface UseAsyncTaskOptions<T, P extends any[]> {
  taskFn: (...args: P) => Promise<T>;
  onSuccess?: (result: T) => void;
  onError?: (error: any) => void;
  stages?: string[];
}

export function useAsyncTask<T, P extends any[]>({
  taskFn,
  onSuccess,
  onError,
  stages = [],
}: UseAsyncTaskOptions<T, P>) {
  const [status, setStatus] = useState<TaskStatus>('idle');
  const [stageIndex, setStageIndex] = useState<number>(0);
  const [elapsedSeconds, setElapsedSeconds] = useState<number>(0);
  const [error, setError] = useState<string | null>(null);

  const timerRef = useRef<number | null>(null);
  const abortControllerRef = useRef<AbortController | null>(null);

  const run = useCallback(
    async (...args: P): Promise<T | undefined> => {
      setStatus('running');
      setStageIndex(0);
      setElapsedSeconds(0);
      setError(null);

      abortControllerRef.current = new AbortController();

      const startTime = Date.now();
      if (timerRef.current) clearInterval(timerRef.current);
      timerRef.current = window.setInterval(() => {
        const secs = Math.floor((Date.now() - startTime) / 1000);
        setElapsedSeconds(secs);
        if (stages.length > 0) {
          // Progress through stages as time elapses
          if (secs < 3) setStageIndex(0);
          else if (secs < 12) setStageIndex(1 % stages.length);
          else if (secs < 20) setStageIndex(2 % stages.length);
          else setStageIndex((stages.length - 1) % stages.length);
        }
      }, 1000);

      try {
        const res = await taskFn(...args);
        setStatus('success');
        onSuccess?.(res);
        return res;
      } catch (err: any) {
        setStatus('error');
        const msg = err?.message || String(err);
        setError(msg);
        onError?.(err);
        return undefined;
      } finally {
        if (timerRef.current) {
          clearInterval(timerRef.current);
          timerRef.current = null;
        }
      }
    },
    [taskFn, onSuccess, onError, stages]
  );

  const cancel = useCallback(() => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    abortControllerRef.current?.abort();
    setStatus('idle');
    setElapsedSeconds(0);
    setError(null);
  }, []);

  return {
    status,
    isRunning: status === 'running',
    isSuccess: status === 'success',
    isError: status === 'error',
    stageText: stages[stageIndex] || '',
    stageIndex,
    elapsedSeconds,
    error,
    run,
    cancel,
  };
}
