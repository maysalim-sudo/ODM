import { recordSpeeds } from "./details";
import { downloads } from "./downloads.svelte";

/** Samples kept per download: one a second, so two minutes. */
export const SPEED_SAMPLES = 120;

/** Recent speed of every running download, for the bottom panel's graph. */
class SpeedHistory {
  byId = $state<Map<string, number[]>>(new Map());

  /** Starts sampling once a second; returns the stop function. */
  start(): () => void {
    const timer = setInterval(() => {
      this.byId = recordSpeeds(this.byId, downloads.rows.values(), SPEED_SAMPLES);
    }, 1000);
    return () => clearInterval(timer);
  }
}

export const speedHistory = new SpeedHistory();
