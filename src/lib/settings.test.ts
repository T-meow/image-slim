import { afterEach, describe, expect, it, vi } from 'vitest';
import { initialMetadataPolicy, METADATA_KEY } from './settings';

function storage(initial: Record<string, string> = {}): Storage {
  const values = new Map(Object.entries(initial));
  return {
    get length() { return values.size; },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => { values.delete(key); },
    setItem: (key, value) => { values.set(key, value); }
  };
}

afterEach(() => vi.unstubAllGlobals());

describe('metadata setting migration', () => {
  it('migrates the legacy all value without changing the storage key', () => {
    const localStorage = storage({ [METADATA_KEY]: 'all' });
    vi.stubGlobal('localStorage', localStorage);

    expect(initialMetadataPolicy()).toBe('supported');
    expect(localStorage.getItem(METADATA_KEY)).toBe('supported');
  });

  it('keeps supported and defaults unknown values to essential', () => {
    const localStorage = storage({ [METADATA_KEY]: 'supported' });
    vi.stubGlobal('localStorage', localStorage);
    expect(initialMetadataPolicy()).toBe('supported');

    localStorage.setItem(METADATA_KEY, 'unknown');
    expect(initialMetadataPolicy()).toBe('essential');
  });
});
