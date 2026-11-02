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

export function makeRequest(cmd: string, payload: object = {}): { id: string; cmd: string; payload: object } {
  return { id: crypto.randomUUID(), cmd, payload };
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
