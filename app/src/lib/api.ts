import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AddArgs,
  BatchResult,
  ConfirmArgs,
  Detail,
  Download,
  ExistsChoice,
  IntegrationStatus,
  Offer,
  Settings,
  StoreLinks,
  Suggestion,
  ToolsStatus,
  UiEvent,
} from "./types";

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
export const integrationStatus = () => invoke<IntegrationStatus>("integration_status");
/** Resolves with the ODM version that answered through the browser helper. */
export const integrationTest = () => invoke<string>("integration_test");
export const integrationRepair = () => invoke<IntegrationStatus>("integration_repair");
export const openExtensionFolder = (kind: "chromium" | "firefox") => invoke<void>("open_extension_folder", { kind });

/** Opens the browser on its extensions page (chrome://extensions and the like). */
export const openBrowserExtensionsPage = (browser: string) => invoke<void>("open_browser_extensions_page", { browser });
/** Opens a store listing in the chosen browser (only the Chrome, Edge and Firefox stores are accepted). */
export const openStorePage = (browser: string, url: string) => invoke<void>("open_store_page", { browser, url });
export const storeLinks = () => invoke<StoreLinks>("store_links");

export const toolsStatus = () => invoke<ToolsStatus>("tools_status");
/** Downloads FFmpeg, yt-dlp and Deno; progress arrives as `tools_progress` events. */
export const toolsInstall = () => invoke<ToolsStatus>("tools_install");

export function onEvent(cb: (ev: UiEvent) => void): Promise<UnlistenFn> {
  return listen<UiEvent>("odm://event", (e) => cb(e.payload));
}

/** Commands reject with a plain string; anything else gets a generic message. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : "Something went wrong. Check the log for details.";
}

/** Picks a program (.exe), starting next to `start` when given. */
export const pickProgram = (start: string | null) => invoke<string | null>("pick_program", { start });
/** The settings a new install starts with. */
export const defaultSettings = () => invoke<Settings>("default_settings");
export const openAppFolder = (kind: "logs" | "data") => invoke<void>("open_app_folder", { kind });
/** The answer to "<name> already exists". */
export const answerExists = (id: string, choice: ExistsChoice) => invoke<void>("answer_exists", { id, choice });
/** A download's own speed limit in KB/s; null removes it. */
export const setDownloadLimit = (id: string, kbps: number | null) => invoke<void>("set_download_limit", { id, kbps });
/** Cancels the shutdown that "When the queue is done" started. */
export const cancelShutdown = () => invoke<void>("cancel_shutdown");
