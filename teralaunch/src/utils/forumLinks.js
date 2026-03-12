const FORUM_HOST = "forum.crazy-esports.com";
const DEFAULT_FORUM_LANGUAGE = "1";
const GERMAN_FORUM_LANGUAGE = "2";

function getForumLanguageParam(currentLanguage) {
  return currentLanguage === "GER" ? GERMAN_FORUM_LANGUAGE : DEFAULT_FORUM_LANGUAGE;
}

function isLocaleAwareForumUrl(parsedUrl) {
  if (parsedUrl.hostname !== FORUM_HOST) {
    return false;
  }

  return (
    parsedUrl.pathname.startsWith("/forum/") ||
    parsedUrl.pathname.startsWith("/startpage") ||
    parsedUrl.pathname.startsWith("/support")
  );
}

function localizeForumUrl(url, currentLanguage) {
  if (typeof url !== "string" || url.length === 0) {
    return url;
  }

  let parsedUrl;
  try {
    parsedUrl = new URL(url);
  } catch {
    return url;
  }

  if (!isLocaleAwareForumUrl(parsedUrl)) {
    return url;
  }

  parsedUrl.searchParams.set("l", getForumLanguageParam(currentLanguage));
  return parsedUrl.toString();
}

export { getForumLanguageParam, isLocaleAwareForumUrl, localizeForumUrl };
