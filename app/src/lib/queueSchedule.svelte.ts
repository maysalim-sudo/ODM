import * as api from "./api";
import { queueWaitLabel } from "./format";
import type { Schedule } from "./types";

/**
 * The queue schedule as the list shows it: while it is closed, queued rows say when it
 * starts. It follows the saved settings and re-reads the clock every 30 s.
 */
class QueueScheduleStore {
  schedule = $state<Schedule | null>(null);
  now = $state(new Date());
  /** "Waiting for the schedule (starts …)", or null while queued downloads may run. */
  wait = $derived(this.schedule ? queueWaitLabel(this.schedule, this.now) : null);

  private started = false;

  async init(): Promise<() => void> {
    if (this.started) return () => {};
    this.started = true;
    const timer = setInterval(() => (this.now = new Date()), 30_000);
    const un = await api.onSettings((s) => (this.schedule = s.queue_schedule));
    try {
      this.schedule = (await api.getSettings()).queue_schedule;
    } catch {
      // Without settings the list just says "Queued".
    }
    return () => {
      this.started = false;
      clearInterval(timer);
      un();
    };
  }
}

export const queueSchedule = new QueueScheduleStore();
