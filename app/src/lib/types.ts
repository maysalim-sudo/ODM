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
  | { type: "confirm_download"; id: string };

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
  notifications: boolean;
  confirm_browser_downloads: boolean;
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
