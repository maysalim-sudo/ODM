export type DownloadState =
  | "queued"
  | "probing"
  | "downloading"
  | "paused"
  | "merging"
  | "completed"
  | "failed"
  | "cancelled";

export type ChunkState = "pending" | "active" | "retrying" | "done" | "failed";

export interface Download {
  id: string;
  url: string;
  filename: string;
  save_dir: string;
  size: number | null;
  bytes: number;
  mime: string | null;
  category: string;
  state: DownloadState;
  error_code: string | null;
  error_message: string | null;
  max_connections: number;
  kind: string;
  page_url: string | null;
  page_title: string | null;
  created_at: number;
  started_at: number | null;
  finished_at: number | null;
  /** Why a paused download is paused: null (by the user), "schedule" or "exists". */
  pause_reason: string | null;
  /** This download's own speed limit; null = none. */
  speed_limit_kbps: number | null;
}

export interface ChunkView {
  idx: number;
  start: number;
  end: number | null;
  written: number;
  state: ChunkState;
}

export interface Progress {
  id: string;
  bytes: number;
  total: number | null;
  speed_bps: number;
  eta_s: number | null;
  chunks: ChunkView[];
  /** HLS only: finished and total segments. */
  segments?: SegmentProgress;
}

export interface SegmentProgress {
  done: number;
  total: number;
}

export interface Detail {
  download: Download;
  final_url: string | null;
  chunks: ChunkView[];
  header_names: string[];
}

export interface Offer {
  offer_id: string;
  existing_id: string;
  filename: string;
}

export type UiEvent =
  | { type: "upsert"; download: Download }
  | { type: "removed"; id: string }
  | { type: "progress_batch"; items: Progress[] }
  | { type: "resync" }
  | { type: "reuse_offer"; offer: Offer }
  | { type: "confirm_download"; id: string }
  | { type: "tools_progress"; stage: string; done: number; total: number }
  | { type: "exists_prompt"; id: string; name: string }
  | { type: "shutdown_pending"; seconds: number }
  | { type: "shutdown_cancelled" }
  | { type: "shutdown_failed"; message: string };

/** A finished download whose file name is taken, waiting for Replace, Keep both or Cancel. */
export interface ExistsPrompt {
  id: string;
  name: string;
}

export type ExistsChoice = "replace" | "keep_both" | "cancel";

/** The video tools ODM can use, found the way the engine finds them; null = not found. */
export interface ToolsStatus {
  ffmpeg: ToolInfo | null;
  ytdlp: ToolInfo | null;
  deno: ToolInfo | null;
}

/** A tool that was found: its version and where from ("Settings", "ODM", "PATH" or "winget"). */
export interface ToolInfo {
  version: string;
  source: string;
}

export interface Settings {
  download_dir: string;
  category_folders: Record<string, string>;
  max_concurrent: number;
  max_connections: number;
  max_attempts: number;
  speed_limit_kbps: number;
  connect_timeout_s: number;
  read_timeout_s: number;
  ffmpeg_path: string;
  ytdlp_path: string;
  capture_enabled: boolean;
  min_capture_bytes: number;
  skip_hosts: string[];
  close_to_tray: boolean;
  start_minimized: boolean;
  confirm_browser_downloads: boolean;
  /** The first-run welcome screen has been completed. */
  onboarding_done: boolean;
  start_with_windows: boolean;
  notify_complete: boolean;
  notify_failed: boolean;
  prevent_sleep: boolean;
  /** Off: every type is saved straight into download_dir. */
  sort_by_type: boolean;
  on_exists: OnExists;
  on_complete: OnComplete;
  start_mode: StartMode;
  /** Remove finished downloads from the list after this many days; 0 = never. */
  history_days: number;
  limit_schedule: Schedule;
  queue_schedule: Schedule;
  after_queue: AfterQueue;
  /** File extensions left to the browser, lower case without the dot. */
  skip_types: string[];
  video_quality: VideoQuality;
  video_container: VideoContainer;
  video_subtitles: boolean;
  ytdlp_auto_update: boolean;
  proxy_mode: ProxyMode;
  /** Used when proxy_mode is "manual": http://host:port or socks5://host:port. */
  proxy_url: string;
  /** Empty = ODM's default User-Agent. */
  user_agent: string;
  update_check: boolean;
  update_install: UpdateInstall;
}

export type OnExists = "rename" | "overwrite" | "ask";
export type OnComplete = "nothing" | "open_folder" | "open_file";
export type StartMode = "now" | "paused";
export type AfterQueue = "nothing" | "sleep" | "shutdown" | "quit";
export type VideoQuality = "best" | "1080p" | "720p" | "480p" | "audio";
export type VideoContainer = "mp4" | "mkv";
export type ProxyMode = "system" | "none" | "manual";
export type UpdateInstall = "ask" | "auto";

/** A weekly window. days: Monday (0) to Sunday (6). from/to: local "HH:MM"; to < from crosses midnight. */
export interface Schedule {
  enabled: boolean;
  days: boolean[];
  from: string;
  to: string;
}

export interface AddArgs {
  url: string;
  filename?: string | null;
  save_dir?: string | null;
  connections?: number | null;
  category?: string | null;
  checksum?: string | null;
  start_paused?: boolean;
  /** HLS: the chosen quality of a master playlist. */
  variant_url?: string | null;
  /** HLS: add `url` as a video stream. */
  stream?: boolean;
  /** Site page (YouTube, Facebook…): add `url` as a video downloaded with yt-dlp. */
  page?: boolean;
  /** This download's own speed limit in KB/s; null = none. Plain files only. */
  speed_limit_kbps?: number | null;
}

export interface ConfirmArgs {
  id: string;
  filename?: string | null;
  save_dir?: string | null;
  action: "start" | "later" | "cancel";
}

export interface Suggestion {
  filename: string;
  size: number | null;
  category: string;
  resumable: boolean;
  variants: Variant[];
  is_stream: boolean;
  /** `url` is a page of a video site; yt-dlp finds the media. */
  is_page: boolean;
}

export interface Variant {
  url: string;
  bandwidth: number;
  resolution: string | null;
  label: string;
}

export interface BatchResult {
  added: string[];
  rejected: string[];
}

export type CheckStatus = "ok" | "missing" | "wrong_path" | "broken";

export interface BrowserCheck {
  browser: string;
  status: CheckStatus;
  detail: string;
}

export interface IntegrationStatus {
  host_found: boolean;
  pipe_running: boolean;
  browsers: BrowserCheck[];
  /** Epoch ms of the last request from a browser. */
  last_contact_ms: number | null;
  last_command: string | null;
}

/** Store listings of the extension; null = not published yet. */
export interface StoreLinks {
  chrome_web_store_id: string | null;
  edge_addons_id: string | null;
  amo_url: string | null;
}
