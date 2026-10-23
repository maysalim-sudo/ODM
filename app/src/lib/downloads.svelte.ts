import * as api from "./api";
import { ReloadGate, addOffer, addPrompt, applyEvent, counts, keepVisible, nextConfirm, visible, type Filter, type Row, type SortDir, type SortKey } from "./model";
import type { AfterQueue, ConfirmArgs, ExistsChoice, ExistsPrompt, Offer, UiEvent } from "./types";

class DownloadsStore {
  rows = $state<Map<string, Row>>(new Map());
  filter = $state<Filter>("all");
  sortKey = $state<SortKey>("created_at");
  sortDir = $state<SortDir>("desc");
  selected = $state<Set<string>>(new Set());
  error = $state<string | null>(null);
  /** True once the first list load has succeeded. */
  loaded = $state(false);
  /** Browser downloads that match an unfinished one; the first is shown to the user. */
  offers = $state<Offer[]>([]);

  /** Browser downloads added paused that wait for the "New download" dialog, oldest first. */
  confirms = $state<string[]>([]);

  /** Bumped per torrent when its file list arrives or changes, so open views reload it. */
  fileLists = $state<Record<string, number>>({});

  /** Finished downloads whose file name is taken, waiting for Replace, Keep both or Cancel. */
  prompts = $state<ExistsPrompt[]>([]);

  /** The "When the queue is done" countdown: what happens and when (epoch ms), or null. */
  afterQueue = $state<{ action: Exclude<AfterQueue, "nothing">; at: number } | null>(null);

  visible = $derived(visible(this.rows, this.filter, this.sortKey, this.sortDir));
  counts = $derived(counts(this.rows));
  /** The download the "New download" dialog is about, if any. */
  confirmRow = $derived(nextConfirm(this.confirms, [...this.rows.values()]));
  selectedRows = $derived([...this.selected].map((id) => this.rows.get(id)).filter((r): r is Row => !!r));

  private gate = new ReloadGate();
  private unlisten: (() => void) | null = null;

  /** Load the list, then follow live events. Safe to call more than once. */
  async init(): Promise<void> {
    if (this.unlisten) return;
    this.unlisten = await api.onEvent((ev) => this.handle(ev));
    try {
      for (const o of await api.listOffers()) this.offers = addOffer(this.offers, o);
      for (const id of await api.listPendingConfirms()) this.addConfirm(id);
    } catch (e) {
      this.error = api.errorText(e);
    }
    await this.reload();
  }

  /** Answer an offer. Throws the command's message on failure; the offer stays when it does. */
  async resolveOffer(offer: Offer, reuse: boolean): Promise<void> {
    await api.resolveOffer(offer.offer_id, reuse);
    this.offers = this.offers.filter((o) => o.offer_id !== offer.offer_id);
  }

  /** Send the answer, then drop the prompt. Throws the command's message on failure; the prompt stays. */
  async answerExists(p: ExistsPrompt, choice: ExistsChoice): Promise<void> {
    await api.answerExists(p.id, choice);
    this.dismissPrompt(p.id);
  }

  /** Hide a prompt without answering; the download keeps waiting and Resume asks again. */
  dismissPrompt(id: string): void {
    this.prompts = this.prompts.filter((p) => p.id !== id);
  }

  private addConfirm(id: string): void {
    if (!this.confirms.includes(id)) this.confirms = [...this.confirms, id];
  }

  /** Forget pending ids whose download is gone or no longer paused. Ids without a row yet are kept. */
  private pruneConfirms(): void {
    const kept = this.confirms.filter((id) => {
      const r = this.rows.get(id);
      return r === undefined || r.state === "paused";
    });
    if (kept.length !== this.confirms.length) this.confirms = kept;
    // A prompt goes once its download is no longer waiting (cancelled or removed from the list).
    const waiting = this.prompts.filter((p) => {
      const r = this.rows.get(p.id);
      return r === undefined || (r.state === "paused" && r.pause_reason === "exists");
    });
    if (waiting.length !== this.prompts.length) this.prompts = waiting;
  }

  /**
   * Send the user's choice, then drop the id so the next dialog can show. Throws the command's message
   * on failure; the id stays when it does.
   */
  async confirm(args: ConfirmArgs, dontAskAgain: boolean): Promise<void> {
    await api.confirmDownload(args);
    this.confirms = this.confirms.filter((id) => id !== args.id);
    if (dontAskAgain) {
      try {
        const s = await api.getSettings();
        await api.saveSettings({ ...s, confirm_browser_downloads: false });
      } catch (e) {
        this.error = api.errorText(e);
      }
    }
  }

  async reload(): Promise<void> {
    const ticket = this.gate.begin();
    try {
      const list = await api.listDownloads();
      const next = this.gate.finish(ticket, this.rows, list);
      if (next) {
        this.rows = next;
        this.pruneSelection();
        // After a full reload a pending id without a row has really gone.
        this.confirms = this.confirms.filter((id) => next.has(id) && next.get(id)?.state === "paused");
        this.loaded = true;
        this.error = null;
      }
    } catch (e) {
      this.error = api.errorText(e);
    }
  }

  handle(ev: UiEvent): void {
    if (ev.type === "reuse_offer") {
      this.offers = addOffer(this.offers, ev.offer);
      return;
    }
    if (ev.type === "confirm_download") {
      this.addConfirm(ev.id);
      return;
    }
    if (ev.type === "torrent_metadata") {
      this.fileLists = { ...this.fileLists, [ev.id]: (this.fileLists[ev.id] ?? 0) + 1 };
      return;
    }
    if (ev.type === "focus") {
      // A torrent already in the list was added again: show its row.
      if (this.rows.has(ev.id)) {
        this.setFilter("all");
        this.select(ev.id, "one");
      }
      return;
    }
    if (ev.type === "exists_prompt") {
      this.prompts = addPrompt(this.prompts, { id: ev.id, name: ev.name });
      return;
    }
    if (ev.type === "after_queue_pending") {
      this.afterQueue = { action: ev.action, at: Date.now() + ev.seconds * 1000 };
      return;
    }
    if (ev.type === "after_queue_cancelled" || ev.type === "after_queue_done") {
      this.afterQueue = null;
      return;
    }
    if (ev.type === "shutdown_failed") {
      this.afterQueue = null;
      this.error = ev.message;
      return;
    }
    this.gate.record(ev);
    const r = applyEvent(this.rows, ev);
    this.rows = r.rows;
    if (r.resync) void this.reload();
    this.pruneSelection();
    this.pruneConfirms();
  }

  /** Drop selected rows that are gone or hidden by the current filter. */
  private pruneSelection(): void {
    const kept = keepVisible(this.selected, this.visible);
    if (kept !== this.selected) this.selected = kept;
  }

  /** Change the filter; rows it hides drop out of the selection. */
  setFilter(f: Filter): void {
    this.filter = f;
    this.pruneSelection();
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
