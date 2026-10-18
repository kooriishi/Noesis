export type JsonExtractResult<T = Record<string, any>> =
  | { ok: true; data: T }
  | { ok: false; error: string };

/**
 * Extracts a JSON object from raw LLM output without using greedy regex `/\{[\s\S]*\}/`.
 * Strategy (A4):
 * 1. First inspect ```json ... ``` or ``` ... ``` code fences.
 * 2. Otherwise scan balanced `{ ... }` blocks (respecting string literals & escapes)
 *    until a valid JSON object is parsed.
 */
export function extractJsonObject<T = Record<string, any>>(raw: string): JsonExtractResult<T> {
  if (!raw || !raw.trim()) {
    return { ok: false, error: '模型返回内容为空' };
  }

  // 1. Try fenced code blocks first
  const fenceRegex = /```(?:json)?\s*([\s\S]*?)```/gi;
  let fenceMatch: RegExpExecArray | null;
  while ((fenceMatch = fenceRegex.exec(raw)) !== null) {
    const candidate = fenceMatch[1].trim();
    if (candidate.startsWith('{') && candidate.endsWith('}')) {
      try {
        const parsed = JSON.parse(candidate) as T;
        if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
          return { ok: true, data: parsed };
        }
      } catch {
        // Continue trying balanced braces inside fence or next fence
      }
    }
    const balancedInFence = scanBalancedJsonObjects<T>(candidate);
    if (balancedInFence.ok) {
      return balancedInFence;
    }
  }

  // 2. Scan balanced `{ ... }` objects across the raw text
  return scanBalancedJsonObjects<T>(raw);
}

function scanBalancedJsonObjects<T>(text: string): JsonExtractResult<T> {
  let lastParseError = '未找到合法的 JSON 对象结构';

  for (let i = 0; i < text.length; i++) {
    if (text[i] !== '{') continue;

    let depth = 0;
    let inString = false;
    let escaped = false;

    for (let j = i; j < text.length; j++) {
      const ch = text[j];

      if (inString) {
        if (escaped) {
          escaped = false;
        } else if (ch === '\\') {
          escaped = true;
        } else if (ch === '"') {
          inString = false;
        }
        continue;
      }

      if (ch === '"') {
        inString = true;
      } else if (ch === '{') {
        depth++;
      } else if (ch === '}') {
        depth--;
        if (depth === 0) {
          const candidate = text.slice(i, j + 1);
          try {
            const parsed = JSON.parse(candidate) as T;
            if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
              return { ok: true, data: parsed };
            }
          } catch (err: any) {
            lastParseError = err?.message || 'JSON 解析失败';
          }
          break;
        }
      }
    }
  }

  return { ok: false, error: lastParseError };
}
