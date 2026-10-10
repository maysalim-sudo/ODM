import { relativeTime } from "./model";
import type { UiEvent, UpdateInfo, UpdateInstall, UpdateStatus } from "./types";

/** The update banner and Settings → Updates, as plain data. */
export interface UpdateView {
  /** False in a build without the update key: no checks, no banner. */
  supported: boolean;
  current: string;
  lastChecked: number | null;
  available: UpdateInfo | null;
  /** "Later" hid the banner for `available`. */
  dismissed: boolean;
  phase: "idle" | "downloading" | "ready" | "installing";
  /** Download progress; null while the size is unknown. */
  percent: number | null;
  /** The last failure, in words for the user. */
  error: string | null;
  checking: boolean;
  /** The last "Check now" found nothing newer. */
  upToDate: boolean;
}

export const UNAVAILABLE = "Updates are not available in this build.";

export function initialView(): UpdateView {
  return {
    supported: false,
    current: "",
    lastChecked: null,
    available: null,
    dismissed: false,
    phase: "idle",
    percent: null,
    error: null,
    checking: false,
    upToDate: false,
  };
}

export function fromStatus(s: UpdateStatus, prev: UpdateView = initialView()): UpdateView {
  return {
    ...prev,
    supported: s.supported,
    current: s.current,
    lastChecked: s.last_checked_ms,
    available: s.available,
    dismissed: s.dismissed,
    phase: s.ready ? "ready" : s.downloading ? "downloading" : prev.phase === "installing" ? "installing" : "idle",
  };
}

/** The view after one event from ODM; other events leave it as it is. */
export function applyEvent(v: UpdateView, ev: UiEvent): UpdateView {
  switch (ev.type) {
    case "update_available": {
      const same = v.available?.version === ev.version;
      return {
        ...v,
        available: { version: ev.version, notes: ev.notes, date: ev.date },
        dismissed: same ? v.dismissed : false,
        phase: same ? v.phase : "idle",
        error: null,
        upToDate: false,
      };
    }
    case "update_progress":
      return { ...v, phase: "downloading", percent: ev.percent, error: null };
    case "update_ready":
      return { ...v, phase: "ready", percent: 100, error: null };
    case "update_failed":
      return { ...v, phase: "idle", percent: null, error: ev.message };
    default:
      return v;
  }
}

export function bannerShown(v: UpdateView): boolean {
  return v.supported && v.available !== null && !v.dismissed;
}

export function bannerText(v: UpdateView, mode: UpdateInstall): string {
  const version = v.available?.version ?? "";
  if (v.phase === "installing") return "Installing the update…";
  if (v.phase === "downloading") return v.percent === null ? "Downloading update…" : `Downloading update… ${v.percent}%`;
  if (v.error) return v.error;
  if (v.phase === "ready" && mode === "auto") return `ODM ${version} will be installed when you quit ODM.`;
  return `ODM ${version} is available.`;
}

/** The banner's main button; null while it is busy. */
export function primaryLabel(v: UpdateView, mode: UpdateInstall): string | null {
  if (v.phase === "downloading" || v.phase === "installing") return null;
  return v.phase === "ready" && mode === "auto" ? "Install now" : "Update";
}

/** The status line next to "Check now". */
export function checkLine(v: UpdateView, now: number): string {
  if (!v.supported) return UNAVAILABLE;
  if (v.checking) return "Checking…";
  if (v.error) return v.error;
  const last = v.lastChecked === null ? "Not checked yet" : `Last checked ${relativeTime(v.lastChecked, now)}`;
  return v.upToDate ? `ODM is up to date. ${last}` : last;
}
