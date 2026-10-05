import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AddArgs, BatchResult, ConfirmArgs, Detail, Download, Offer, Settings, Suggestion, UiEvent } from "./types";

export const listDownloads = () => invoke<Download[]>("list_downloads");
export const getDetail = (id: string) => invoke<Detail | null>("get_detail", { id });
export const addDownload = (args: AddArgs) => invoke<string>("add_download", { args });
export const addBatch = (text: string, saveDir: string | null) =>
  invoke<BatchResult>("add_batch", { text, saveDir });
export const suggest = (url: string) => invoke<Suggestion>("suggest", { url });
export const pause = (ids: string[]) => invoke<void>("pause", { ids });
export const resume = (ids: string[]) => invoke<void>("resume", { ids });
export const cancel = (ids: string[], deleteFile: boolean) => invoke<void>("cancel", { ids, deleteFile });
export const remove = (ids: string[], deleteFile: boolean) => invoke<void>("remove", { ids, deleteFile });
export const pauseAll = () => invoke<void>("pause_all");
export const resumeAll = () => invoke<void>("resume_all");
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<Settings>("save_settings", { settings });
export const openFile = (id: string) => invoke<void>("open_file", { id });
export const openFolder = (id: string) => invoke<void>("open_folder", { id });
export const pickFolder = (start: string | null) => invoke<string | null>("pick_folder", { start });
export const listOffers = () => invoke<Offer[]>("list_offers");
export const resolveOffer = (offerId: string, reuse: boolean) => invoke<string>("resolve_offer", { offerId, reuse });
export const listPendingConfirms = () => invoke<string[]>("list_pending_confirms");
export const confirmDownload = (args: ConfirmArgs) => invoke<void>("confirm_download", { args });
export const refreshUrl = (id: string, url: string) => invoke<void>("refresh_url", { id, url });

export function onEvent(cb: (ev: UiEvent) => void): Promise<UnlistenFn> {
  return listen<UiEvent>("odm://event", (e) => cb(e.payload));
}

/** Commands reject with a plain string; anything else gets a generic message. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : "Something went wrong. Check the log for details.";
}
