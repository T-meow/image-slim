import { describe, expect, it } from 'vitest';
import { formatBytes, savingsPercent, validSubfolderName } from './format';

describe('format helpers', () => {
  it('formats byte sizes', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1024)).toBe('1 KB');
    expect(formatBytes(1536, 'en')).toBe('1.5 KB');
  });

  it('calculates savings without negative percentages', () => {
    expect(savingsPercent(1000, 600)).toBe(40);
    expect(savingsPercent(1000, 1200)).toBe(0);
  });

  it('checks Windows folder names', () => {
    expect(validSubfolderName('compressed')).toBe(true);
    expect(validSubfolderName('../out')).toBe(false);
    expect(validSubfolderName('CON')).toBe(false);
    expect(validSubfolderName('CON.results')).toBe(false);
    expect(validSubfolderName('results.')).toBe(false);
    expect(validSubfolderName(' results')).toBe(false);
  });
});
