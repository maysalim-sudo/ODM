export type OdmResponse<T = unknown> = {
  id: string | null;
  ok: boolean;
  payload?: T;
  error?: { code: string; message: string };
};

/** What a detected video is saved as. */
export type VideoFormat = "video" | "mp3";

/** A format from a page message; anything else means no choice. */
export function readFormat(value: unknown): VideoFormat | undefined {
  return value === "video" || value === "mp3" ? value : undefined;
}

export type AddDownloadPayload = {
  url: string;
  filename?: string;
  referrer?: string;
  headers: Record<string, string>;
  cookies?: string;
  filesize?: number;
  mime?: string;
  source: "capture" | "menu";
  expires_at_ms?: number;
  format?: VideoFormat;
};

/** A magnet link, or a `.torrent` URL the app fetches with these headers and cookies. */
export type AddTorrentPayload = {
  link: string;
  kind: "magnet" | "url";
  referrer?: string;
  headers: Record<string, string>;
  cookies?: string;
  source: "capture" | "menu";
};

export type AddStreamPayload = {
  page_url: string;
  page_title?: string;
  playlist_url: string;
  headers: Record<string, string>;
  cookies?: string;
  format?: VideoFormat;
};

export type AddPagePayload = {
  page_url: string;
  page_title?: string;
  headers: Record<string, string>;
  cookies?: string;
  format?: VideoFormat;
};

export type OdmRequest = { id: string; cmd: string; payload: object; ext?: string };

/** One message to ODM; `ext` is this extension's version, so ODM can tell when it is out of date. */
export function makeRequest(cmd: string, payload: object = {}, ext?: string): OdmRequest {
  return ext ? { id: crypto.randomUUID(), cmd, payload, ext } : { id: crypto.randomUUID(), cmd, payload };
}

/** Version `a` is older than `b`, compared number by number; anything that isn't numbers is not. */
export function olderThan(a: string, b: string): boolean {
  const parts = (v: string) => v.split(".").map((p) => (/^\d+$/.test(p) ? Number(p) : NaN));
  const x = parts(a);
  const y = parts(b);
  if ([...x, ...y].some(Number.isNaN)) return false;
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d < 0;
  }
  return false;
}

export function errorText(r: OdmResponse | null | undefined): string {
  if (!r) return "ODM did not answer.";
  if (r.ok) return "";
  return r.error?.message || "ODM reported an error.";
}

export type SendFailure = "missing_host" | "timeout" | "other";

/** Rejection value used when a native message gets no answer in time. */
export const TIMEOUT_SENTINEL = "timeout";

export function classifySendError(e: unknown): SendFailure {
  if (e === TIMEOUT_SENTINEL) return "timeout";
  const text = (e instanceof Error ? e.message : String(e)).toLowerCase();
  if (text.includes("native messaging host not found") || text.includes("specified native messaging host")) {
    return "missing_host";
  }
  return "other";
}
