import * as api from "./api";
import type { IntegrationStatus } from "./types";

const POLL_MS = 30_000;

class IntegrationStore {
  status = $state<IntegrationStatus | null>(null);
  /** Epoch ms of the last refresh, so relative times move on even when nothing else changes. */
  now = $state(Date.now());

  async refresh(): Promise<void> {
    try {
      this.status = await api.integrationStatus();
    } catch {
      // The chip keeps its last value; the dialog shows its own errors.
    }
    this.now = Date.now();
  }

  /** Refresh now and every 30 s. Returns the function that stops it. */
  startPolling(): () => void {
    void this.refresh();
    const timer = setInterval(() => void this.refresh(), POLL_MS);
    return () => clearInterval(timer);
  }
}

export const integration = new IntegrationStore();
