import type { Cookie } from "./capture";
import { CAPTURE_TIMEOUT_MS, HOST_NAME } from "./ids";
import { classifySendError, makeRequest, TIMEOUT_SENTINEL, type OdmResponse, type SendFailure } from "./protocol";

export const ext: typeof chrome = (globalThis as any).browser ?? (globalThis as any).chrome;

/** Sends one native message and says why it failed, if it did. */
export async function sendDetailed<T>(
  cmd: string,
  payload?: object,
  timeoutMs?: number,
): Promise<{ response: OdmResponse<T> | null; failure?: SendFailure; detail?: string }> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(TIMEOUT_SENTINEL), timeoutMs ?? CAPTURE_TIMEOUT_MS);
  });
  try {
    const response = await Promise.race([
      ext.runtime.sendNativeMessage(HOST_NAME, makeRequest(cmd, payload)) as Promise<OdmResponse<T>>,
      timeout,
    ]);
    return { response };
  } catch (e) {
    const detail = e === TIMEOUT_SENTINEL ? "timed out" : e instanceof Error ? e.message : String(e);
    return { response: null, failure: classifySendError(e), detail };
  } finally {
    clearTimeout(timer);
  }
}

/** Sends one native message; null on timeout, a missing host, or any failure. */
export async function send<T>(
  cmd: string,
  payload?: object,
  timeoutMs?: number,
): Promise<OdmResponse<T> | null> {
  return (await sendDetailed<T>(cmd, payload, timeoutMs)).response;
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
