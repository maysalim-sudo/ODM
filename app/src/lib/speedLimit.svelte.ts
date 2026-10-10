import * as api from "./api";

/**
 * The speed limit as the chip shows it. It follows the saved settings (any change from the
 * Settings page or the tray arrives as an event) and saves through the same command.
 */
class SpeedLimitStore {
  /** Limit in KB/s; 0 = no limit. */
  kbps = $state(0);
  /** True when "Limit only at certain times" is on; the chip cannot tell if the window is open now. */
  scheduled = $state(false);
  error = $state<string | null>(null);

  private started = false;

  async init(): Promise<() => void> {
    if (this.started) return () => {};
    this.started = true;
    const un = await api.onSettings((s) => this.adopt(s));
    await this.refresh();
    return () => {
      this.started = false;
      un();
    };
  }

  private adopt(s: { speed_limit_kbps: number; limit_schedule: { enabled: boolean } }): void {
    this.kbps = s.speed_limit_kbps;
    this.scheduled = s.limit_schedule.enabled;
  }

  async refresh(): Promise<void> {
    try {
      this.adopt(await api.getSettings());
    } catch (e) {
      this.error = api.errorText(e);
    }
  }

  /** Saves a new limit. Resolves true when it was saved; the message is kept in `error` otherwise. */
  async set(kbps: number): Promise<boolean> {
    try {
      const s = await api.getSettings();
      this.adopt(await api.saveSettings({ ...s, speed_limit_kbps: kbps }));
      this.error = null;
      return true;
    } catch (e) {
      this.error = api.errorText(e);
      return false;
    }
  }
}

export const speedLimit = new SpeedLimitStore();
