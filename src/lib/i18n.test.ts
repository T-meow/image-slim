import { describe, expect, it } from 'vitest';
import { copy, errorText, interpolate } from './i18n';
import type { AppError } from './types';

function error(code: AppError['code']): AppError {
  return { code, params: {}, path: null, detail: null, retryable: false };
}

describe('localized application errors', () => {
  it('renders input-limit errors in both languages', () => {
    expect(errorText(error('file_too_large'), copy.zh)).toContain('512 MiB');
    expect(errorText(error('file_too_large'), copy.en)).toContain('512 MiB');
    expect(errorText(error('queue_limit_reached'), copy.zh)).toContain('10,000');
    expect(errorText(error('queue_limit_reached'), copy.en)).toContain('10,000');
  });

  it('interpolates dynamic capability hints', () => {
    expect(interpolate(copy.zh.addHint, { formats: 'PNG、JPEG' })).toContain('PNG、JPEG');
    expect(interpolate(copy.en.addHint, { formats: 'PNG / JPEG' })).toContain('PNG / JPEG');
  });
});
