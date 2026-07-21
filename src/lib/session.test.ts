import { describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import { SessionController } from './session';

describe('SessionController', () => {
  it('counts each terminal item once within the active batch', () => {
    const session = new SessionController();
    session.beginBatch('batch', 2);

    session.itemFinished('previously-failed');
    session.itemFinished('previously-failed');
    expect(get(session).batchDone).toBe(1);

    session.itemFinished('second');
    expect(get(session).batchDone).toBe(2);
  });

  it('appends and clears structured issues', () => {
    const session = new SessionController();
    const issue = {
      code: 'invalid_image' as const,
      params: {},
      path: 'C:\\bad.png',
      detail: 'bad header',
      retryable: false
    };
    session.addIssues([issue]);
    session.addIssues([{ ...issue, path: 'C:\\bad-2.png' }]);
    expect(get(session).issues).toHaveLength(2);

    session.clearIssues();
    expect(get(session).issues).toEqual([]);
  });
});
