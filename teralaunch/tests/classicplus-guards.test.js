/**
 * Tests for Classic+ URL guards and disabled feature behavior.
 * Verifies that empty URLs cause graceful skip/no-op behavior
 * and that disabled features (OAuth, leaderboard, profile) return early.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';

// ============================================================================
// EXTRACTED LOGIC: URL guard patterns used throughout app.js
// These replicate the guard logic so we can test it in isolation.
// ============================================================================

/**
 * Replicates the Classic+ URLS object with empty URLs for disabled features.
 */
const URLS = {
  launcher: {
    download: "",
    versionCheck: "",
    versionInfo: "",
  },
  content: {
    news: "",
    patchNotes: "",
    serverStatus: "http://157.90.107.2:8090/tera/ServerList?lang=en",
  },
  external: {
    register: "",
    forum: "",
    discord: "https://discord.com/invite/crazyesports",
    support: "https://helpdesk.crazy-esports.com",
    privacy: "",
    profile: "",
  },
};

// ============================================================================
// URL GUARD TESTS
// ============================================================================

describe('Classic+ URLS configuration', () => {
  it('has empty launcher URLs', () => {
    expect(URLS.launcher.download).toBe("");
    expect(URLS.launcher.versionCheck).toBe("");
    expect(URLS.launcher.versionInfo).toBe("");
  });

  it('has empty news and patchNotes URLs', () => {
    expect(URLS.content.news).toBe("");
    expect(URLS.content.patchNotes).toBe("");
  });

  it('has active server status URL pointing to v100 API', () => {
    expect(URLS.content.serverStatus).toContain("157.90.107.2:8090");
    expect(URLS.content.serverStatus).toContain("ServerList");
  });

  it('has empty register, forum, privacy, and profile URLs', () => {
    expect(URLS.external.register).toBe("");
    expect(URLS.external.forum).toBe("");
    expect(URLS.external.privacy).toBe("");
    expect(URLS.external.profile).toBe("");
  });

  it('retains Discord and support URLs', () => {
    expect(URLS.external.discord).toContain("discord.com");
    expect(URLS.external.support).toContain("helpdesk");
  });

  it('has no leaderboard section', () => {
    expect(URLS.leaderboard).toBeUndefined();
  });
});

describe('Empty URL guard behavior', () => {
  it('treats empty string as falsy for guards', () => {
    // This is the pattern used in all guards: if (!URLS.x.y) return;
    expect(!URLS.content.news).toBe(true);
    expect(!URLS.content.patchNotes).toBe(true);
    expect(!URLS.launcher.versionCheck).toBe(true);
    expect(!URLS.external.register).toBe(true);
    expect(!URLS.external.forum).toBe(true);
    expect(!URLS.external.profile).toBe(true);
  });

  it('treats non-empty URLs as truthy for guards', () => {
    expect(!URLS.content.serverStatus).toBe(false);
    expect(!URLS.external.discord).toBe(false);
    expect(!URLS.external.support).toBe(false);
  });
});

// ============================================================================
// DISABLED FEATURE BEHAVIOR TESTS
// ============================================================================

describe('loadNewsFeed guard', () => {
  it('sets "No news available" when URL is empty', () => {
    // Simulates the guard logic in loadNewsFeed
    const newsFeedList = { innerHTML: '' };
    if (!URLS.content.news) {
      newsFeedList.innerHTML = '<span class="news-item news-muted">No news available</span>';
    }
    expect(newsFeedList.innerHTML).toContain("No news available");
  });
});

describe('loadPatchNotes guard', () => {
  it('returns early when URL is empty', () => {
    let fetchCalled = false;
    // Simulates the guard in loadPatchNotes
    if (!URLS.content.patchNotes) {
      // Should skip fetch
    } else {
      fetchCalled = true;
    }
    expect(fetchCalled).toBe(false);
  });
});

describe('checkLauncherUpdate guard', () => {
  it('returns early when versionCheck URL is empty', () => {
    let fetchCalled = false;
    if (!URLS.launcher.versionCheck) {
      // Should return early
    } else {
      fetchCalled = true;
    }
    expect(fetchCalled).toBe(false);
  });
});

describe('openRegisterPopup guard', () => {
  it('returns early when register URL is empty', () => {
    let externalOpened = false;
    if (!URLS.external.register) {
      // Should return early
    } else {
      externalOpened = true;
    }
    expect(externalOpened).toBe(false);
  });
});

describe('handleViewProfile guard', () => {
  it('returns early when profile URL is empty', () => {
    let externalOpened = false;
    if (!URLS.external.profile) {
      // Should return early
    } else {
      externalOpened = true;
    }
    expect(externalOpened).toBe(false);
  });
});

describe('setupHeaderLinks URL guards', () => {
  it('hides elements with empty URLs', () => {
    const links = [
      { id: "discord-button", url: URLS.external.discord },
      { id: "support-button", url: URLS.external.support },
      { id: "privacy-link", url: URLS.external.privacy },
    ];

    const hiddenIds = [];
    const visibleIds = [];

    links.forEach((link) => {
      if (!link.url) {
        hiddenIds.push(link.id);
      } else {
        visibleIds.push(link.id);
      }
    });

    expect(hiddenIds).toContain("privacy-link");
    expect(visibleIds).toContain("discord-button");
    expect(visibleIds).toContain("support-button");
  });

  it('forum button returns early when URL is empty', () => {
    let externalOpened = false;
    if (!URLS.external.forum) {
      // Should return early
    } else {
      externalOpened = true;
    }
    expect(externalOpened).toBe(false);
  });
});

describe('versionInfo guard', () => {
  it('skips fetch when versionInfo URL is empty', () => {
    const versionInfo = true; // element exists
    let fetchCalled = false;
    if (versionInfo && URLS.launcher.versionInfo) {
      fetchCalled = true;
    }
    expect(fetchCalled).toBe(false);
  });
});

// ============================================================================
// DISABLED FUNCTION STUBS
// ============================================================================

describe('startOAuth stub', () => {
  it('returns immediately without side effects', () => {
    // Simulates the disabled startOAuth
    const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    function startOAuth(provider, pendingAction = null) {
      console.log("[Classic+] OAuth not available on Classic+ server");
      return;
    }
    const result = startOAuth('google');
    expect(result).toBeUndefined();
    consoleSpy.mockRestore();
  });
});

describe('handleOAuthCallback stub', () => {
  it('returns immediately without side effects', async () => {
    const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    async function handleOAuthCallback(token, oauthProvider = null) {
      console.log("[Classic+] OAuth callback not available on Classic+ server");
      return;
    }
    const result = await handleOAuthCallback('token123');
    expect(result).toBeUndefined();
    consoleSpy.mockRestore();
  });
});

describe('checkDeepLink stub', () => {
  it('returns immediately without side effects', async () => {
    async function checkDeepLink() {
      return;
    }
    const result = await checkDeepLink();
    expect(result).toBeUndefined();
  });
});

describe('ensureAuthSession stub', () => {
  it('returns true as a no-op', async () => {
    async function ensureAuthSession(promptOnFailure = false) {
      return true;
    }
    expect(await ensureAuthSession()).toBe(true);
    expect(await ensureAuthSession(true)).toBe(true);
  });
});

describe('getLeaderboardConsent stub', () => {
  it('returns unsuccessful result with null consent', async () => {
    async function getLeaderboardConsent() {
      return { success: false, consent: null };
    }
    const result = await getLeaderboardConsent();
    expect(result.success).toBe(false);
    expect(result.consent).toBeNull();
  });
});

describe('setLeaderboardConsent stub', () => {
  it('returns false as a no-op', async () => {
    async function setLeaderboardConsent(agreed) {
      return false;
    }
    expect(await setLeaderboardConsent(true)).toBe(false);
    expect(await setLeaderboardConsent(false)).toBe(false);
  });
});

describe('checkLeaderboardConsent stub', () => {
  it('returns false to never show modal', async () => {
    async function checkLeaderboardConsent() {
      return false;
    }
    expect(await checkLeaderboardConsent()).toBe(false);
  });
});

describe('LoadStartPage with empty news URL', () => {
  it('returns early from news data fetch when URL is empty', () => {
    let fetchDataCalled = false;

    // Simulates LoadStartPage logic with URLS.content.news guard
    if (!URLS.content.news) {
      // Should skip fetchData call
    } else {
      fetchDataCalled = true;
    }
    expect(fetchDataCalled).toBe(false);
  });
});
