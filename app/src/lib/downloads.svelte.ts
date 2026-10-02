import * as api from "./api";
import { applyEvent, counts, replaceAll, visible, type Filter, type Row, type SortDir, type SortKey } from "./model";
import type { UiEvent } from "./types";

class DownloadsStore {
  rows = $state<Map<string, Row>>(new Map());
  filter = $state<Filter>("all");
  sortKey = $state<SortKey>("created_at");
  sortDir = $state<SortDir>("desc");
  selected = $state<Set<string>>(new Set());
  error = $state<string | null>(null);

  visible = $derived(visible(this.rows, this.filter, this.sortKey, this.sortDir));
  counts = $derived(counts(this.rows));
  selectedRows = $derived([...this.selected].map((id) => this.rows.get(id)).filter((r): r is Row => !!r));

  /** Load the list, then follow live events. Call once at start-up. */
  async init(): Promise<void> {
    await api.onEvent((ev) => this.handle(ev));
    await this.reload();
  }

  async reload(): Promise<void> {
    try {
      this.rows = replaceAll(await api.listDownloads());
      this.pruneSelection();
    } catch (e) {
      this.error = api.errorText(e);
    }
  }

  handle(ev: UiEvent): void {
    const r = applyEvent(this.rows, ev);
    this.rows = r.rows;
    if (r.resync) void this.reload();
    if (ev.type === "removed") this.pruneSelection();
  }

  private pruneSelection(): void {
    const kept = [...this.selected].filter((id) => this.rows.has(id));
    if (kept.length !== this.selected.size) this.selected = new Set(kept);
  }

  /** Plain click selects one; ctrl toggles; shift extends from the last selected row. */
  select(id: string, mode: "one" | "toggle" | "range"): void {
    if (mode === "one") {
      this.selected = new Set([id]);
      return;
    }
    const next = new Set(this.selected);
    if (mode === "toggle") {
      if (next.has(id)) next.delete(id);
      else next.add(id);
    } else {
      const ids = this.visible.map((r) => r.id);
      const anchor = [...this.selected].at(-1);
      const a = anchor ? ids.indexOf(anchor) : -1;
      const b = ids.indexOf(id);
      if (a === -1 || b === -1) next.add(id);
      else for (const x of ids.slice(Math.min(a, b), Math.max(a, b) + 1)) next.add(x);
    }
    this.selected = next;
  }

  toggleSort(key: SortKey): void {
    if (this.sortKey === key) this.sortDir = this.sortDir === "asc" ? "desc" : "asc";
    else {
      this.sortKey = key;
      this.sortDir = key === "created_at" ? "desc" : "asc";
    }
  }

  /** Run a command and surface its error message in the UI. */
  async run(action: () => Promise<unknown>): Promise<boolean> {
    try {
      await action();
      this.error = null;
      return true;
    } catch (e) {
      this.error = api.errorText(e);
      return false;
    }
  }
}

export const downloads = new DownloadsStore();
