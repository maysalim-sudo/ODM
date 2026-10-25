/** Which system ODM runs on, for wording and the few things that differ. Tests run as Windows. */
export const IS_LINUX = typeof navigator !== "undefined" && /Linux/.test(navigator.userAgent) && !/Android/.test(navigator.userAgent);

/** What the system is called in sentences: "Windows" or "Linux". */
export const SYSTEM_NAME = IS_LINUX ? "Linux" : "Windows";
