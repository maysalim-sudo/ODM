export type OdmResponse<T = unknown> = {
  id: string | null;
  ok: boolean;
  payload?: T;
  error?: { code: string; message: string };
};

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
};

export function makeRequest(cmd: string, payload: object = {}): { id: string; cmd: string; payload: object } {
  return { id: crypto.randomUUID(), cmd, payload };
}

export function errorText(r: OdmResponse | null | undefined): string {
  if (!r) return "ODM did not answer.";
  if (r.ok) return "";
  return r.error?.message || "ODM reported an error.";
}
