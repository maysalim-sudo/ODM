import { ALT_GRACE_MS } from "./ids";

export class AltTracker {
  private held = false;
  private releasedAt = -Infinity;

  down(_now: number): void {
    this.held = true;
  }

  up(now: number): void {
    this.held = false;
    this.releasedAt = now;
  }

  active(now: number): boolean {
    return this.held || now - this.releasedAt < ALT_GRACE_MS;
  }
}
