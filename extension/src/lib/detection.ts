/** Rules for turning video detection off per site or everywhere. Pure apart from the two small caches. */

export const STORAGE_KEY = "videoDetection";

export type DetectionSettings = { enabled: boolean; offSites: string[] };

export const DEFAULT_SETTINGS: DetectionSettings = { enabled: true, offSites: [] };

function cleanHost(host: string): string {
  return host.trim().toLowerCase().replace(/\.+$/, "").replace(/^www\./, "");
}

/** The tab's site: host lowercased, trailing dot and leading `www.` removed. Only http(s) pages have one. */
export function siteOf(url: string | undefined): string | null {
  if (!url) return null;
  try {
    const u = new URL(url);
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    const host = cleanHost(u.hostname);
    return host || null;
  } catch {
    return null;
  }
}

/** A list entry: a URL or a bare host; null when it is not a plausible host name. */
function entryOf(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  const text = raw.trim();
  if (text.includes("://")) return siteOf(text);
  const host = cleanHost(text);
  return /^[a-z0-9.-]+$/.test(host) && host.includes(".") ? host : null;
}

function sortedUnique(sites: string[]): string[] {
  return [...new Set(sites)].sort();
}

export function normalizeSettings(raw: unknown): DetectionSettings {
  if (typeof raw !== "object" || raw === null) return { ...DEFAULT_SETTINGS, offSites: [] };
  const r = raw as { enabled?: unknown; offSites?: unknown };
  const list = Array.isArray(r.offSites) ? r.offSites : [];
  const offSites = sortedUnique(list.map(entryOf).filter((s): s is string => s !== null));
  return { enabled: typeof r.enabled === "boolean" ? r.enabled : true, offSites };
}

function covers(entry: string, site: string): boolean {
  return site === entry || site.endsWith(`.${entry}`);
}

function siteListedOff(s: DetectionSettings, site: string | null): boolean {
  return site !== null && s.offSites.some((e) => covers(e, site));
}

export function detectionOn(s: DetectionSettings, pageUrl: string | undefined): boolean {
  if (!s.enabled) return false;
  return !siteListedOff(s, siteOf(pageUrl));
}

export function setSiteOff(s: DetectionSettings, site: string, off: boolean): DetectionSettings {
  const host = cleanHost(site);
  const offSites = off ? sortedUnique([...s.offSites, host]) : s.offSites.filter((e) => !covers(e, host));
  return { enabled: s.enabled, offSites };
}

export function changedTabs(
  before: DetectionSettings,
  after: DetectionSettings,
  tabs: { id: number; url?: string }[],
): { off: number[]; on: number[] } {
  const off: number[] = [];
  const on: number[] = [];
  for (const t of tabs) {
    const was = detectionOn(before, t.url);
    const now = detectionOn(after, t.url);
    if (was && !now) off.push(t.id);
    else if (!was && now) on.push(t.id);
  }
  return { off, on };
}

export type DetectionView = {
  site: string | null;
  siteOn: boolean;
  globalOn: boolean;
  offSites: string[];
  pageOff: boolean;
  offText: string;
};

export function detectionView(s: DetectionSettings, pageUrl: string | undefined): DetectionView {
  const site = siteOf(pageUrl);
  const pageOff = !detectionOn(s, pageUrl);
  return {
    site,
    siteOn: !siteListedOff(s, site),
    globalOn: s.enabled,
    offSites: s.offSites,
    pageOff,
    offText: !s.enabled ? "Video detection is off everywhere." : pageOff && site ? `Video detection is off on ${site}.` : "",
  };
}

/** Holds the settings in memory. The first question waits for the first load; a failed load means defaults. */
export class DetectionState {
  private settings: DetectionSettings | null = null;
  private loading: Promise<void> | null = null;

  constructor(private readonly load: () => Promise<unknown>) {}

  private ready(): Promise<void> {
    if (this.settings) return Promise.resolve();
    this.loading ??= this.load().then(
      (raw) => {
        this.settings ??= normalizeSettings(raw);
      },
      () => {
        this.settings ??= normalizeSettings(undefined);
      },
    );
    return this.loading;
  }

  async current(): Promise<DetectionSettings> {
    await this.ready();
    return this.settings ?? normalizeSettings(undefined);
  }

  async isOn(pageUrl: string | undefined): Promise<boolean> {
    return detectionOn(await this.current(), pageUrl);
  }

  apply(raw: unknown): { before: DetectionSettings; after: DetectionSettings } {
    const before = this.settings ?? normalizeSettings(undefined);
    const after = normalizeSettings(raw);
    this.settings = after;
    return { before, after };
  }
}

/** Remembers each tab's top-level address; asks the browser when it does not know. */
export class TabSites {
  private readonly urls = new Map<number, string>();

  constructor(private readonly get: (tabId: number) => Promise<string | undefined>) {}

  set(tabId: number, url: string): void {
    this.urls.set(tabId, url);
  }

  forget(tabId: number): void {
    this.urls.delete(tabId);
  }

  async url(tabId: number): Promise<string | undefined> {
    const known = this.urls.get(tabId);
    if (known !== undefined) return known;
    let found: string | undefined;
    try {
      found = await this.get(tabId);
    } catch {
      found = undefined;
    }
    if (found !== undefined) this.urls.set(tabId, found);
    return found;
  }
}
