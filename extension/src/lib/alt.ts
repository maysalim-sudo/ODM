import { ALT_GRACE_MS } from "./ids";

export class AltTracker {
  private held = false;
  private releasedAt = -Infinity;

  down(_now: number): void {
    this.held = true;
  }

  /** A release only counts after a press; a window blur alone says nothing about Alt. */
  up(now: number): void {
    if (!this.held) return;
    this.held = false;
    this.releasedAt = now;
  }

  active(now: number): boolean {
    return this.held || now - this.releasedAt < ALT_GRACE_MS;
  }
}
