import { beforeEach, describe, expect, it, vi } from 'vitest';

function createStorageMock() {
  const store = new Map();
  return {
    getItem: vi.fn((key) => store.get(key) ?? null),
    setItem: vi.fn((key, value) => store.set(key, String(value))),
    removeItem: vi.fn((key) => store.delete(key)),
    clear: vi.fn(() => store.clear()),
  };
}

describe('AccountManager', () => {
  let AccountManager;

  beforeEach(async () => {
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: createStorageMock(),
    });
    Object.defineProperty(globalThis, 'sessionStorage', {
      configurable: true,
      value: createStorageMock(),
    });
    AccountManager = await import('../src/accountManager.js');
  });

  it('resolves the active account when login stores a numeric user number', () => {
    AccountManager.addAccount({
      userNo: 5,
      userName: 'imweak',
      credentials: btoa(JSON.stringify({ u: 'imweak', p: 'secret' })),
    });

    AccountManager.setActiveAccountId(5);

    expect(AccountManager.getActiveAccount()).toMatchObject({
      userNo: '5',
      userName: 'imweak',
    });
  });
});
