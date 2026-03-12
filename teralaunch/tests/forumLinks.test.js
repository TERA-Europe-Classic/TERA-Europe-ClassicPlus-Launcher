import { describe, expect, it } from "vitest";

import {
  getForumLanguageParam,
  isLocaleAwareForumUrl,
  localizeForumUrl,
} from "../src/utils/forumLinks.js";

describe("forumLinks", () => {
  it("maps German launcher language to the German forum parameter", () => {
    expect(getForumLanguageParam("GER")).toBe("2");
  });

  it("falls back to the English forum parameter for non-German launcher languages", () => {
    expect(getForumLanguageParam("EUR")).toBe("1");
    expect(getForumLanguageParam("FRA")).toBe("1");
    expect(getForumLanguageParam("RUS")).toBe("1");
  });

  it("marks forum content URLs as locale-aware", () => {
    expect(
      isLocaleAwareForumUrl(
        new URL("https://forum.crazy-esports.com/forum/thread/1411-patch-notes-2-0-1/"),
      ),
    ).toBe(true);
    expect(
      isLocaleAwareForumUrl(
        new URL("https://forum.crazy-esports.com/forum/board/43-tera-classic-news/"),
      ),
    ).toBe(true);
  });

  it("leaves non-forum and non-localizable forum URLs alone", () => {
    expect(
      isLocaleAwareForumUrl(new URL("https://tera-europe-classic.com/register?locale=en")),
    ).toBe(false);
    expect(
      isLocaleAwareForumUrl(
        new URL("https://forum.crazy-esports.com/index.php?datenschutzerklaerung/"),
      ),
    ).toBe(false);
  });

  it("adds the English forum language parameter for English patch note links", () => {
    expect(
      localizeForumUrl(
        "https://forum.crazy-esports.com/forum/thread/1411-patch-notes-2-0-1/",
        "EUR",
      ),
    ).toBe(
      "https://forum.crazy-esports.com/forum/thread/1411-patch-notes-2-0-1/?l=1",
    );
  });

  it("replaces an existing forum language parameter when German is selected", () => {
    expect(
      localizeForumUrl(
        "https://forum.crazy-esports.com/forum/thread/1411-patch-notes-2-0-1/?l=1",
        "GER",
      ),
    ).toBe(
      "https://forum.crazy-esports.com/forum/thread/1411-patch-notes-2-0-1/?l=2",
    );
  });

  it("does not touch unrelated or malformed URLs", () => {
    expect(localizeForumUrl("https://discord.com/invite/crazyesports", "GER")).toBe(
      "https://discord.com/invite/crazyesports",
    );
    expect(localizeForumUrl("not-a-url", "GER")).toBe("not-a-url");
  });
});
