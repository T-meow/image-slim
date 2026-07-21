export type {
  AppCapabilities,
  AppError,
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

import type { AppError, InputItem, TaskStatus } from './generated/ipc';

export type ThemePreference = 'system' | 'light' | 'dark';
export type Language = 'zh' | 'en';

export interface TaskItem extends InputItem {
  status: TaskStatus;
  output_path?: string;
  output_size?: number;
  saved_bytes: number;
  error?: AppError;
}
