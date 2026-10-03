import type { Cookie } from "./capture";
import { CAPTURE_TIMEOUT_MS, HOST_NAME } from "./ids";
import { makeRequest, type OdmResponse } from "./protocol";
import { withTimeout } from "./timeout";

export const ext: typeof chrome = (globalThis as any).browser ?? (globalThis as any).chrome;

/** Sends one native message; null on timeout, a missing host, or any failure. */
export function send<T>(
  cmd: string,
  payload?: object,
  timeoutMs?: number,
): Promise<OdmResponse<T> | null> {
  return withTimeout(
    ext.runtime.sendNativeMessage(HOST_NAME, makeRequest(cmd, payload)) as Promise<OdmResponse<T>>,
    timeoutMs ?? CAPTURE_TIMEOUT_MS,
  );
}

export async function cookiesFor(url: string, storeId?: string): Promise<Cookie[]> {
  try {
    return await ext.cookies.getAll({ url, storeId });
  } catch (e) {
    if (String(e).includes("firstPartyDomain")) {
      try {
        // Firefox with first-party isolation requires this key; null means every domain.
        const query = { url, storeId, firstPartyDomain: null } as unknown as chrome.cookies.GetAllDetails;
        return await ext.cookies.getAll(query);
      } catch {
        return [];
      }
    }
    return [];
  }
}
