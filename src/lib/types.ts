export type {
  AppCapabilities,
  AppError,
  AudioInfo,
  BatchRequest,
  BatchStartResult,
  BatchStartStatus,
  BatchSummary,
  CompressionPreset,
  ErrorCode,
  FormatCapability,
  ImageFormat,
  InputItem,
  InputLimits,
  ItemProgress,
  MetadataPolicy,
  OutputMode,
  PreviewRequest,
  PreviewResult,
  ScanEvent,
  ScanRequest,
  TaskStatus
} from './generated/ipc';

import type { AppError, BatchRequest, ImageFormat, InputItem, TaskStatus } from './generated/ipc';

export type ThemePreference = 'system' | 'light' | 'dark';
export type Language = 'zh' | 'en';

export type BatchSettings = Pick<BatchRequest, 'preset' | 'output_mode' | 'output_subfolder' | 'metadata_policy' | 'audio_bitrate_kbps'>;

export interface TaskItem extends InputItem {
  status: TaskStatus;
  output_path?: string;
  output_size?: number;
  saved_bytes: number;
  error?: AppError;
  attempt?: BatchSettings;
}

export function toInputItem(item: InputItem): InputItem {
  const { id, source_path, input_root, relative_path, name, format, width, height, audio, original_size, modified_ms } = item;
  return { id, source_path, input_root, relative_path, name, format, width, height, ...(audio ? { audio } : {}), original_size, modified_ms };
}

export function isAudioFormat(format: ImageFormat): boolean {
  return format === 'mp3' || format === 'wav' || format === 'flac' || format === 'm4a' || format === 'ogg';
}

export function sourceWasOverwritten(item: TaskItem): boolean {
  return !isAudioFormat(item.format) && item.status === 'completed' && item.saved_bytes > 0 && item.attempt?.output_mode === 'overwrite';
}
