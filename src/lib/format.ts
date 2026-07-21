export function formatBytes(value: number, language: 'zh' | 'en' = 'zh'): string {
  if (!Number.isFinite(value) || value <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const unitIndex = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  const amount = value / 1024 ** unitIndex;
  const digits = unitIndex === 0 || amount >= 100 ? 0 : amount >= 10 ? 1 : 2;
  return `${amount.toLocaleString(language === 'zh' ? 'zh-CN' : 'en-US', {
    minimumFractionDigits: 0,
    maximumFractionDigits: digits
  })} ${units[unitIndex]}`;
}

export function savingsPercent(original: number, output?: number): number {
  if (!output || original <= 0 || output >= original) return 0;
  return Math.round(((original - output) / original) * 100);
}

export function validSubfolderName(value: string): boolean {
  const trimmed = value.trim();
  if (
    !trimmed ||
    trimmed !== value ||
    trimmed === '.' ||
    trimmed === '..' ||
    trimmed.endsWith('.') ||
    [...trimmed].some((character) => character.charCodeAt(0) < 32) ||
    /[<>:"/\\|?*]/.test(trimmed) ||
    [...trimmed].reduce((length, character) => length + (character.codePointAt(0)! > 0xffff ? 2 : 1), 0) > 255
  ) return false;
  const stem = trimmed.split('.')[0].toUpperCase();
  return !/^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/.test(stem);
}
