import type { Language, ThemePreference } from './types';

export const THEME_KEY = 'image-slim-theme';
export const LANGUAGE_KEY = 'image-slim-language';
export const PRESET_KEY = 'image-slim-preset';
export const OUTPUT_MODE_KEY = 'image-slim-output-mode';
export const OUTPUT_FOLDER_KEY = 'image-slim-output-folder';
export const METADATA_KEY = 'image-slim-metadata';

export function initialMetadataPolicy(): 'essential' | 'supported' {
  const saved = localStorage.getItem(METADATA_KEY);
  if (saved === 'supported') return 'supported';
  if (saved === 'all') {
    localStorage.setItem(METADATA_KEY, 'supported');
    return 'supported';
  }
  return 'essential';
}

export function initialLanguage(): Language {
  const saved = localStorage.getItem(LANGUAGE_KEY);
  if (saved === 'zh' || saved === 'en') return saved;
  return navigator.language.toLowerCase().startsWith('zh') ? 'zh' : 'en';
}

export function initialTheme(): ThemePreference {
  const saved = localStorage.getItem(THEME_KEY);
  return saved === 'light' || saved === 'dark' || saved === 'system' ? saved : 'system';
}

export function applyTheme(preference: ThemePreference): void {
  const dark = preference === 'dark' || (preference === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
  document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  document.documentElement.dataset.themePreference = preference;
  document.documentElement.style.colorScheme = dark ? 'dark' : 'light';
}
