import { writable } from 'svelte/store';
import type { AppError, BatchSummary } from './types';

export interface SessionState {
  scanning: boolean;
  running: boolean;
  stopping: boolean;
  activeScanId: string;
  activeBatchId: string;
  scanVisited: number;
  scanAccepted: number;
  scanPath: string;
  batchTotal: number;
  batchDone: number;
  lastSummary?: BatchSummary;
  issues: AppError[];
  notice: string;
  noticeIsError: boolean;
}

const INITIAL_STATE: SessionState = {
  scanning: false,
  running: false,
  stopping: false,
  activeScanId: '',
  activeBatchId: '',
  scanVisited: 0,
  scanAccepted: 0,
  scanPath: '',
  batchTotal: 0,
  batchDone: 0,
  issues: [],
  notice: '',
  noticeIsError: false
};

export class SessionController {
  private readonly store = writable<SessionState>({ ...INITIAL_STATE });
  private readonly finishedItemIds = new Set<string>();
  private issueItems: AppError[] = [];
  readonly subscribe = this.store.subscribe;

  beginScan(scanId: string): void {
    this.store.update((state) => ({
      ...state,
      scanning: true,
      stopping: false,
      activeScanId: scanId,
      scanVisited: 0,
      scanAccepted: 0,
      scanPath: '',
      notice: '',
      noticeIsError: false
    }));
  }

  updateScan(visited: number, accepted: number, currentPath: string): void {
    this.store.update((state) => ({
      ...state,
      scanVisited: visited,
      scanAccepted: accepted,
      scanPath: currentPath
    }));
  }

  finishScan(scanId: string): void {
    this.store.update((state) => state.activeScanId === scanId ? {
      ...state,
      scanning: false,
      stopping: false,
      activeScanId: '',
      scanPath: ''
    } : state);
  }

  beginBatch(batchId: string, total: number): void {
    this.finishedItemIds.clear();
    this.store.update((state) => ({
      ...state,
      running: true,
      stopping: false,
      activeBatchId: batchId,
      batchTotal: total,
      batchDone: 0,
      lastSummary: undefined,
      notice: '',
      noticeIsError: false
    }));
  }

  itemFinished(itemId: string): void {
    if (this.finishedItemIds.has(itemId)) return;
    this.finishedItemIds.add(itemId);
    this.store.update((state) => ({
      ...state,
      batchDone: Math.min(state.batchTotal, state.batchDone + 1)
    }));
  }

  finishBatch(summary: BatchSummary): void {
    this.finishedItemIds.clear();
    this.store.update((state) => ({
      ...state,
      running: false,
      stopping: false,
      activeBatchId: '',
      batchDone: state.batchTotal,
      lastSummary: summary
    }));
  }

  requestStop(): void {
    this.store.update((state) => ({ ...state, stopping: true }));
  }

  clearStopping(): void {
    this.store.update((state) => ({ ...state, stopping: false }));
  }

  addIssues(issues: AppError[]): void {
    if (!issues.length) return;
    this.issueItems.push(...issues);
    this.store.update((state) => ({ ...state, issues: this.issueItems }));
  }

  clearIssues(): void {
    this.issueItems = [];
    this.store.update((state) => ({ ...state, issues: this.issueItems }));
  }

  setNotice(notice: string, noticeIsError = false): void {
    this.store.update((state) => ({ ...state, notice, noticeIsError }));
  }

  resetSummary(): void {
    this.store.update((state) => ({ ...state, lastSummary: undefined }));
  }
}
