import * as api from "./api";
import type { UpdateInstall } from "./types";
import { applyEvent, fromStatus, initialView, type UpdateView } from "./updates";

/** The update banner and Settings → Updates share this. */
class UpdatesStore {
  view = $state<UpdateView>(initialView());
  /** "When an update is ready", followed live from the settings. */
  mode = $state<UpdateInstall>("ask");
  private started = false;
  /** Set by "Update", so the end of the download reads as installing. */
  private installing = false;

  /** Reads the state once, then follows ODM's events and the settings. Safe to call more than once. */
  async init(): Promise<void> {
    if (this.started) return;
    this.started = true;
    await api.onEvent((ev) => {
      const next = applyEvent(this.view, ev);
      this.view = this.installing && ev.type === "update_ready" ? { ...next, phase: "installing" } : next;
    });
    await api.onSettings((s) => (this.mode = s.update_install));
    try {
      this.mode = (await api.getSettings()).update_install;
      await this.refresh();
    } catch {
      // The banner just stays hidden.
    }
  }

  async refresh(): Promise<void> {
    this.view = fromStatus(await api.updateStatus(), this.view);
  }

  /** "Check now". The outcome shows in the status line; it never throws. */
  async checkNow(): Promise<void> {
    if (this.view.checking) return;
    this.view = { ...this.view, checking: true, error: null, upToDate: false };
    try {
      const s = await api.updateCheck();
      this.view = { ...fromStatus(s, this.view), checking: false, upToDate: s.available === null };
    } catch (e) {
      this.view = { ...this.view, checking: false, error: api.errorText(e) };
    }
  }

  /** "Update" / "Install now": downloads if needed, then ODM restarts into the new version. */
  async install(): Promise<void> {
    const back = this.view.phase;
    this.installing = true;
    this.view = { ...this.view, phase: this.view.phase === "ready" ? "installing" : "downloading", error: null };
    try {
      await api.updateInstall();
    } catch (e) {
      this.installing = false;
      this.view = { ...this.view, phase: back === "ready" ? "ready" : "idle", error: api.errorText(e) };
    }
  }

  /** "Later": hidden until the next version. */
  later(): void {
    const v = this.view.available?.version;
    if (!v) return;
    this.view = { ...this.view, dismissed: true };
    api.updateDismiss(v).catch(() => {});
  }
}

export const updates = new UpdatesStore();
