/** H4: 可选配色主题（与 tokens.css 里的 `:root[data-theme=...]` 一一对应）。 */
export const THEME_OPTIONS: Array<{ id: string; label: string }> = [
  { id: 'catppuccin', label: 'Catppuccin Mocha' },
  { id: 'one-dark', label: 'One Dark Pro' },
  { id: 'dracula', label: 'Dracula' },
  { id: 'tokyo-night', label: 'Tokyo Night' },
  { id: 'catppuccin-latte', label: 'Catppuccin Latte（浅色）' },
];

/** H4: 常用等宽字体候选，设置面板下拉用。 */
export const MONO_FONT_OPTIONS: string[] = [
  '"JetBrains Mono", "Fira Code", Consolas, monospace',
  '"Fira Code", Consolas, monospace',
  '"Cascadia Code", Consolas, monospace',
  'Consolas, "Courier New", monospace',
  '"Source Code Pro", Consolas, monospace',
  'Menlo, Monaco, "Courier New", monospace',
];

/** H4: 常用界面字体候选。 */
export const UI_FONT_OPTIONS: string[] = [
  '-apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif',
  '"Inter", "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif',
  '"Noto Sans SC", "Microsoft YaHei", sans-serif',
  'system-ui, "Microsoft YaHei", sans-serif',
];
