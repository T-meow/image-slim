import { invoke, isTauri } from '@tauri-apps/api/core';
import { ERROR_CODES } from './generated/ipc';
import type {
  AppCapabilities,
  AppError,
  BatchRequest,
  BatchStartResult,
  ErrorCode,
  PreviewRequest,
  PreviewResult,
  ScanRequest
} from './types';

export const inTauri = isTauri();

const ERROR_CODE_SET = new Set<ErrorCode>(ERROR_CODES);

export function startScan(request: ScanRequest): Promise<void> {
  return invoke('start_scan', { request });
}

export function cancelScan(scanId: string): Promise<boolean> {
  return invoke('cancel_scan', { scanId });
}

export function createPreview(request: PreviewRequest): Promise<PreviewResult> {
  return invoke('create_preview', { request });
}

export function cancelPreview(): Promise<boolean> {
  if (!inTauri) return Promise.resolve(false);
  return invoke('cancel_preview');
}

export function startBatch(request: BatchRequest): Promise<BatchStartResult> {
  return invoke('start_batch', { request });
}

export function cancelBatch(batchId: string): Promise<boolean> {
  return invoke('cancel_batch', { batchId });
}

export function getCapabilities(): Promise<AppCapabilities> {
  return invoke('get_capabilities');
}

export function revealPath(path: string): Promise<void> {
  return invoke('reveal_path', { path });
}

export function normalizeAppError(value: unknown): AppError {
  if (isAppError(value)) return value;
  return {
    code: 'internal',
    params: {},
    path: null,
    detail: value instanceof Error ? value.message : String(value),
    retryable: false
  };
}

function isAppError(value: unknown): value is AppError {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<AppError>;
  return typeof candidate.code === 'string'
    && isErrorCode(candidate.code)
    && typeof candidate.retryable === 'boolean'
    && candidate.params !== null
    && typeof candidate.params === 'object'
    && !Array.isArray(candidate.params);
}

export function isErrorCode(value: string): value is ErrorCode {
  return ERROR_CODE_SET.has(value as ErrorCode);
}
