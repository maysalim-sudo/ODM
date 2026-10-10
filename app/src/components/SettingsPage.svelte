<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { tick } from "svelte";
  import * as api from "../lib/api";
  import { integration } from "../lib/integration.svelte";
  import { CATEGORIES, CATEGORY_LABELS, integrationProblem, relativeTime } from "../lib/model";
  import {
    PAGE_MS,
    SECTIONS,
    checkFolderName,
    checkHost,
    checkInt,
    checkProxyUrl,
    checkSpeed,
    checkType,
    fromKbps,
    resetAll,
    restoreSection,
    sectionHasRestore,
    springEase,
    toKbps,
    visibleRows,
    type SectionId,
    type SpeedUnit,
  } from "../lib/settingsPage";
  import type { Settings } from "../lib/types";
  import NumberField from "./NumberField.svelte";
  import ScheduleEditor from "./ScheduleEditor.svelte";
  import Segmented from "./Segmented.svelte";
  import SettingRow from "./SettingRow.svelte";
  import Switch from "./Switch.svelte";
  import TagField from "./TagField.svelte";
  import ToolsCard from "./ToolsCard.svelte";

  let {
    open,
    onclose,
    onbrowser,
    oncheckupdates,
  }: {
    open: boolean;
    onclose: () => void;
    /** Opens the browser connection panel. */
    onbrowser: () => void;
    /** "Check for updates now" (Task 1). */
    oncheckupdates?: () => void;
  } = $props();

  const MB = 1024 * 1024;

  let s = $state<Settings | null>(null);
  let defaults: Settings | null = null;
  let loadError = $state<string | null>(null);
  let saveError = $state<string | null>(null);
  let version = $state("");
  let query = $state("");
  let active = $state<SectionId>("general");
  let speedUnit = $state<SpeedUnit>("kb");
  let lastKbps = 1024;
  let folderErrors = $state<Record<string, string>>({});
  let proxyDraft = $state("");
  let proxyError = $state<string | null>(null);
  let toast = $state<{ text: string; undo: Settings } | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let searchEl = $state<HTMLInputElement>();
  let returnFocus: HTMLElement | null = null;

  const searching = $derived(query.trim() !== "");
  const shown = $derived(new Set(visibleRows(searching ? null : active, query)));
  const show = (id: string) => shown.has(id);
  const any = (...ids: string[]) => ids.some(show);
  const sectionsShown = $derived(
    searching ? SECTIONS.filter((sec) => visibleRows(sec.id, query).length > 0) : SECTIONS.filter((sec) => sec.id === active),
  );

  const st = $derived(integration.status);
  const browserLine = $derived(
    st === null
      ? "Checking…"
      : (integrationProblem(st) ??
          (st.last_contact_ms !== null ? `Connected, ${relativeTime(st.last_contact_ms, integration.now)}` : "Not connected yet")),
  );

  $effect(() => {
    if (!open) return;
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    loadError = null;
    saveError = null;
    query = "";
    void integration.refresh();
    Promise.all([api.getSettings(), api.defaultSettings()])
      .then(([loaded, defs]) => {
        defaults = defs;
        adopt(loaded);
      })
      .catch((e) => (loadError = api.errorText(e)));
    getVersion()
      .then((v) => (version = v))
      .catch(() => (version = ""));
    void tick().then(() => searchEl?.focus());
    return () => {
      clearTimeout(toastTimer);
      toast = null;
      returnFocus?.focus();
    };
  });

  function adopt(next: Settings): void {
    s = next;
    speedUnit = fromKbps(next.speed_limit_kbps || lastKbps).unit;
    if (next.speed_limit_kbps > 0) lastKbps = next.speed_limit_kbps;
    proxyDraft = next.proxy_url;
    proxyError = null;
    folderErrors = {};
  }

  // Saves run one after another; a reply only lands if nothing newer was changed meanwhile.
  let chain: Promise<void> = Promise.resolve();
  let edits = 0;

  function commit(next: Settings): void {
    s = next;
    const snap = $state.snapshot(next) as Settings;
    const mine = ++edits;
    chain = chain.then(async () => {
      try {
        const saved = await api.saveSettings(snap);
        saveError = null;
        if (mine === edits) s = saved;
      } catch (e) {
        saveError = api.errorText(e);
        if (mine === edits) {
          const back = await api.getSettings().catch(() => null);
          if (back && mine === edits) adopt(back);
        }
      }
    });
  }

  function set<K extends keyof Settings>(key: K, value: Settings[K]): void {
    if (!s) return;
    commit({ ...($state.snapshot(s) as Settings), [key]: value });
  }

  function showToast(text: string, undo: Settings): void {
    clearTimeout(toastTimer);
    toast = { text, undo };
    toastTimer = setTimeout(() => (toast = null), 5000);
  }

  function undo(): void {
    if (!toast) return;
    clearTimeout(toastTimer);
    const prev = toast.undo;
    toast = null;
    commit(prev);
    adopt(prev);
  }

  function restore(id: SectionId, title: string): void {
    if (!s || !defaults) return;
    const before = $state.snapshot(s) as Settings;
    const next = restoreSection(before, defaults, id);
    commit(next);
    adopt(next);
    showToast(`${title} settings are back to their defaults.`, before);
  }

  function resetEverything(): void {
    if (!s || !defaults) return;
    const before = $state.snapshot(s) as Settings;
    const next = resetAll(before, defaults);
    commit(next);
    adopt(next);
    showToast("All settings are back to their defaults.", before);
  }

  async function chooseFolder(): Promise<void> {
    if (!s) return;
    const picked = await api.pickFolder(s.download_dir || null).catch(() => null);
    if (picked) set("download_dir", picked);
  }

  async function chooseProgram(key: "ffmpeg_path" | "ytdlp_path"): Promise<void> {
    if (!s) return;
    const picked = await api.pickProgram(s[key] || null).catch(() => null);
    if (picked) set(key, picked);
  }

  function setFolderName(cat: string, name: string): void {
    if (!s) return;
    const err = checkFolderName(name);
    const { [cat]: _, ...rest } = folderErrors;
    folderErrors = err ? { ...rest, [cat]: err } : rest;
    if (!err) set("category_folders", { ...s.category_folders, [cat]: name });
  }

  function setSpeedOn(on: boolean): void {
    if (!s) return;
    if (on) {
      speedUnit = fromKbps(lastKbps).unit;
      set("speed_limit_kbps", lastKbps);
    } else {
      if (s.speed_limit_kbps > 0) lastKbps = s.speed_limit_kbps;
      set("speed_limit_kbps", 0);
    }
  }

  const speedShown = (kbps: number, unit: SpeedUnit) => (unit === "mb" ? Math.round((kbps / 1024) * 100) / 100 : kbps);

  function applyProxyUrl(): void {
    proxyError = checkProxyUrl(proxyDraft);
    if (!proxyError && s && proxyDraft.trim() !== s.proxy_url) set("proxy_url", proxyDraft.trim());
  }

  async function openFolder(kind: "logs" | "data"): Promise<void> {
    try {
      await api.openAppFolder(kind);
    } catch (e) {
      saveError = api.errorText(e);
    }
  }

  function pickSection(id: SectionId): void {
    active = id;
    query = "";
  }

  function onkeydown(e: KeyboardEvent): void {
    if (!open || e.key !== "Escape") return;
    // A dialog on top (the browser panel) closes first.
    if (document.querySelector("dialog[open]")) return;
    e.preventDefault();
    if (searching && document.activeElement === searchEl) query = "";
    else onclose();
  }

  /** Slides in from the right edge, where the Settings button is, with a critically damped spring. Reduced motion: a cross-fade. */
  function slide(_node: Element) {
    const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
    return {
      duration: PAGE_MS,
      easing: springEase,
      css: (t: number) => (reduce ? `opacity: ${t}` : `transform: translateX(${(1 - t) * 100}%)`),
    };
  }

  function rise(_node: Element) {
    const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
    return {
      duration: 300,
      easing: springEase,
      css: (t: number) => (reduce ? `opacity: ${t}` : `opacity: ${t}; transform: translateY(${(1 - t) * 12}px)`),
    };
  }

  const HISTORY = [
    { value: "0", label: "Never" },
    { value: "7", label: "7 days" },
    { value: "30", label: "30 days" },
    { value: "90", label: "90 days" },
  ];
</script>

<svelte:window {onkeydown} />

{#if open}
  <div class="page" role="dialog" aria-modal="true" aria-labelledby="settings-title" transition:slide>
    <header>
      <h1 id="settings-title">Settings</h1>
      <button class="primary" onclick={onclose}>Done</button>
    </header>
    <div class="cols">
      <nav aria-label="Settings sections">
        <input class="search" type="search" placeholder="Search settings" aria-label="Search settings" bind:value={query} bind:this={searchEl} />
        {#each SECTIONS as sec (sec.id)}
          {@const hits = searching ? visibleRows(sec.id, query).length : 0}
          {#if !searching || hits > 0}
            <button
              class:active={!searching && active === sec.id}
              aria-current={!searching && active === sec.id ? "page" : undefined}
              onclick={() => pickSection(sec.id)}
            >
              <span>{sec.title}</span>
              {#if searching}<span class="count">{hits}</span>{/if}
            </button>
          {/if}
        {/each}
      </nav>

      <main>
        <div class="content">
          {#if saveError}<p class="form-error" role="alert">{saveError}</p>{/if}
          {#if s}
            {#each sectionsShown as sec (sec.id)}
              <section aria-labelledby={`sec-${sec.id}`}>
                <h2 id={`sec-${sec.id}`}>{sec.title}</h2>

                {#if sec.id === "general"}
                  {#if any("start_with_windows", "start_minimized", "close_to_tray")}
                    <div class="card">
                      {#if show("start_with_windows")}
                        <SettingRow id="start_with_windows">
                          <Switch label="Start ODM with Windows" describedby="set-start_with_windows-desc" checked={s.start_with_windows} onchange={(v) => set("start_with_windows", v)} />
                        </SettingRow>
                      {/if}
                      {#if show("start_minimized")}
                        <SettingRow id="start_minimized">
                          <Switch label="Start minimized to the tray" describedby="set-start_minimized-desc" checked={s.start_minimized} onchange={(v) => set("start_minimized", v)} />
                        </SettingRow>
                      {/if}
                      {#if show("close_to_tray")}
                        <SettingRow id="close_to_tray">
                          <Switch label="Keep running in the tray when closed" describedby="set-close_to_tray-desc" checked={s.close_to_tray} onchange={(v) => set("close_to_tray", v)} />
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                  {#if any("notify_complete", "notify_failed", "prevent_sleep")}
                    <div class="card">
                      {#if show("notify_complete")}
                        <SettingRow id="notify_complete">
                          <Switch label="Notify when a download finishes" describedby="set-notify_complete-desc" checked={s.notify_complete} onchange={(v) => set("notify_complete", v)} />
                        </SettingRow>
                      {/if}
                      {#if show("notify_failed")}
                        <SettingRow id="notify_failed">
                          <Switch label="Notify when a download fails" describedby="set-notify_failed-desc" checked={s.notify_failed} onchange={(v) => set("notify_failed", v)} />
                        </SettingRow>
                      {/if}
                      {#if show("prevent_sleep")}
                        <SettingRow id="prevent_sleep">
                          <Switch label="Prevent sleep while downloading" describedby="set-prevent_sleep-desc" checked={s.prevent_sleep} onchange={(v) => set("prevent_sleep", v)} />
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                {:else if sec.id === "downloads"}
                  {#if any("download_dir", "sort_by_type")}
                    <div class="card">
                      {#if show("download_dir")}
                        <SettingRow id="download_dir">
                          <span class="path" title={s.download_dir}>{s.download_dir}</span>
                          <button onclick={chooseFolder}>Change…</button>
                        </SettingRow>
                      {/if}
                      {#if show("sort_by_type")}
                        <SettingRow id="sort_by_type">
                          <Switch label="Sort into folders by type" describedby="set-sort_by_type-desc" checked={s.sort_by_type} onchange={(v) => set("sort_by_type", v)} />
                          {#snippet below()}
                            {#if s?.sort_by_type}
                              <div class="folders">
                                {#each CATEGORIES as c (c)}
                                  <label class="folder">
                                    <span>{CATEGORY_LABELS[c]}</span>
                                    <input
                                      value={s.category_folders[c] ?? ""}
                                      placeholder="In the download folder"
                                      aria-invalid={folderErrors[c] !== undefined}
                                      oninput={(e) => {
                                        const err = checkFolderName(e.currentTarget.value);
                                        const { [c]: _, ...rest } = folderErrors;
                                        folderErrors = err ? { ...rest, [c]: err } : rest;
                                      }}
                                      onchange={(e) => setFolderName(c, e.currentTarget.value)}
                                    />
                                    {#if folderErrors[c]}<span class="field-error" role="alert">{folderErrors[c]}</span>{/if}
                                  </label>
                                {/each}
                              </div>
                            {/if}
                          {/snippet}
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                  {#if any("on_exists", "on_complete", "start_mode", "max_concurrent", "history_days")}
                    <div class="card">
                      {#if show("on_exists")}
                        <SettingRow id="on_exists">
                          <Segmented
                            label="When a file already exists"
                            value={s.on_exists}
                            options={[
                              { value: "rename", label: "Rename" },
                              { value: "overwrite", label: "Overwrite" },
                              { value: "ask", label: "Ask" },
                            ]}
                            onchange={(v) => set("on_exists", v)}
                          />
                        </SettingRow>
                      {/if}
                      {#if show("on_complete")}
                        <SettingRow id="on_complete">
                          <Segmented
                            label="When a download finishes"
                            value={s.on_complete}
                            options={[
                              { value: "nothing", label: "Nothing" },
                              { value: "open_folder", label: "Open folder" },
                              { value: "open_file", label: "Open file" },
                            ]}
                            onchange={(v) => set("on_complete", v)}
                          />
                        </SettingRow>
                      {/if}
                      {#if show("start_mode")}
                        <SettingRow id="start_mode">
                          <Segmented
                            label="Start new downloads"
                            value={s.start_mode}
                            options={[
                              { value: "now", label: "Right away" },
                              { value: "paused", label: "Paused" },
                            ]}
                            onchange={(v) => set("start_mode", v)}
                          />
                        </SettingRow>
                      {/if}
                      {#if show("max_concurrent")}
                        <SettingRow id="max_concurrent">
                          <NumberField label="Downloads at once" value={s.max_concurrent} check={(v) => checkInt(v, 1, 10)} onvalid={(v) => set("max_concurrent", v)} />
                        </SettingRow>
                      {/if}
                      {#if show("history_days")}
                        <SettingRow id="history_days">
                          <Segmented
                            label="Remove finished downloads from the list after"
                            value={String(s.history_days)}
                            options={HISTORY}
                            onchange={(v) => set("history_days", Number(v))}
                          />
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                {:else if sec.id === "speed"}
                  {#if any("speed_limit", "limit_schedule")}
                    <div class="card">
                      {#if show("speed_limit")}
                        <SettingRow id="speed_limit">
                          <Switch label="Speed limit" describedby="set-speed_limit-desc" checked={s.speed_limit_kbps > 0} onchange={setSpeedOn} />
                          {#snippet below()}
                            {#if s && s.speed_limit_kbps > 0}
                              <div class="inline">
                                <NumberField
                                  label="Speed limit"
                                  value={speedShown(s.speed_limit_kbps, speedUnit)}
                                  step={speedUnit === "mb" ? 0.1 : 1}
                                  check={(v) => checkSpeed(v, speedUnit)}
                                  onvalid={(v) => set("speed_limit_kbps", toKbps(v, speedUnit))}
                                />
                                <Segmented
                                  label="Unit"
                                  value={speedUnit}
                                  options={[
                                    { value: "kb", label: "KB/s" },
                                    { value: "mb", label: "MB/s" },
                                  ]}
                                  onchange={(v) => (speedUnit = v)}
                                />
                              </div>
                            {/if}
                          {/snippet}
                        </SettingRow>
                      {/if}
                      {#if show("limit_schedule")}
                        <SettingRow id="limit_schedule">
                          <Switch
                            label="Limit only at certain times"
                            describedby="set-limit_schedule-desc"
                            checked={s.limit_schedule.enabled}
                            onchange={(v) => s && set("limit_schedule", { ...s.limit_schedule, enabled: v })}
                          />
                          {#snippet below()}
                            {#if s?.limit_schedule.enabled}
                              <ScheduleEditor schedule={s.limit_schedule} fromLabel="From" toLabel="To" onchange={(v) => set("limit_schedule", v)} />
                            {/if}
                          {/snippet}
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                  {#if any("max_connections")}
                    <div class="card">
                      <SettingRow id="max_connections">
                        <NumberField label="Connections per download" value={s.max_connections} check={(v) => checkInt(v, 1, 32)} onvalid={(v) => set("max_connections", v)} />
                      </SettingRow>
                    </div>
                  {/if}
                  {#if any("queue_schedule", "after_queue")}
                    <div class="card">
                      {#if show("queue_schedule")}
                        <SettingRow id="queue_schedule">
                          <Switch
                            label="Run the queue at certain times"
                            describedby="set-queue_schedule-desc"
                            checked={s.queue_schedule.enabled}
                            onchange={(v) => s && set("queue_schedule", { ...s.queue_schedule, enabled: v })}
                          />
                          {#snippet below()}
                            {#if s?.queue_schedule.enabled}
                              <ScheduleEditor schedule={s.queue_schedule} fromLabel="Start at" toLabel="Stop at" onchange={(v) => set("queue_schedule", v)} />
                            {/if}
                          {/snippet}
                        </SettingRow>
                      {/if}
                      {#if show("after_queue")}
                        <SettingRow id="after_queue">
                          <Segmented
                            label="When the queue is done"
                            value={s.after_queue}
                            options={[
                              { value: "nothing", label: "Nothing" },
                              { value: "sleep", label: "Sleep" },
                              { value: "shutdown", label: "Shut down" },
                              { value: "quit", label: "Quit ODM" },
                            ]}
                            onchange={(v) => set("after_queue", v)}
                          />
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                {:else if sec.id === "browser"}
                  {#if any("capture_enabled", "confirm_browser_downloads", "min_capture")}
                    <div class="card">
                      {#if show("capture_enabled")}
                        <SettingRow id="capture_enabled">
                          <Switch label="Take over downloads from the browser" describedby="set-capture_enabled-desc" checked={s.capture_enabled} onchange={(v) => set("capture_enabled", v)} />
                        </SettingRow>
                      {/if}
                      {#if show("confirm_browser_downloads")}
                        <SettingRow id="confirm_browser_downloads">
                          <Switch label="Ask where to save each one" describedby="set-confirm_browser_downloads-desc" checked={s.confirm_browser_downloads} onchange={(v) => set("confirm_browser_downloads", v)} />
                        </SettingRow>
                      {/if}
                      {#if show("min_capture")}
                        <SettingRow id="min_capture">
                          <NumberField
                            label="Only take files larger than"
                            unit="MB"
                            step={0.1}
                            value={Math.round((s.min_capture_bytes / MB) * 10) / 10}
                            check={(v) => (v !== null && v >= 0 && v <= 100_000 ? null : "Enter a size from 0 to 100000 MB.")}
                            onvalid={(v) => set("min_capture_bytes", Math.round(v * MB))}
                          />
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                  {#if any("skip_types", "skip_hosts")}
                    <div class="card">
                      {#if show("skip_types")}
                        <SettingRow id="skip_types">
                          {#snippet below()}
                            <TagField
                              label="File types to leave to the browser"
                              describedby="set-skip_types-desc"
                              placeholder="pdf, jpg"
                              values={s?.skip_types ?? []}
                              check={checkType}
                              onchange={(v) => set("skip_types", v)}
                            />
                          {/snippet}
                        </SettingRow>
                      {/if}
                      {#if show("skip_hosts")}
                        <SettingRow id="skip_hosts">
                          {#snippet below()}
                            <TagField
                              label="Sites to leave to the browser"
                              describedby="set-skip_hosts-desc"
                              placeholder="example.com"
                              values={s?.skip_hosts ?? []}
                              check={checkHost}
                              onchange={(v) => set("skip_hosts", v)}
                            />
                          {/snippet}
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                  {#if any("browser_connection")}
                    <div class="card">
                      <SettingRow id="browser_connection">
                        <span class="status">{browserLine}</span>
                        <button onclick={onbrowser}>Open connection panel</button>
                      </SettingRow>
                    </div>
                  {/if}
                {:else if sec.id === "video"}
                  {#if show("video_tools")}
                    <div class="tools"><ToolsCard /></div>
                  {/if}
                  {#if any("video_quality", "video_container", "video_subtitles", "ytdlp_auto_update")}
                    <div class="card">
                      {#if show("video_quality")}
                        <SettingRow id="video_quality">
                          <Segmented
                            label="Preferred quality"
                            value={s.video_quality}
                            options={[
                              { value: "best", label: "Best" },
                              { value: "1080p", label: "1080p" },
                              { value: "720p", label: "720p" },
                              { value: "480p", label: "480p" },
                              { value: "audio", label: "Audio only" },
                            ]}
                            onchange={(v) => set("video_quality", v)}
                          />
                        </SettingRow>
                      {/if}
                      {#if show("video_container")}
                        <SettingRow id="video_container">
                          <Segmented
                            label="Format"
                            value={s.video_container}
                            options={[
                              { value: "mp4", label: "MP4" },
                              { value: "mkv", label: "MKV" },
                            ]}
                            onchange={(v) => set("video_container", v)}
                          />
                        </SettingRow>
                      {/if}
                      {#if show("video_subtitles")}
                        <SettingRow id="video_subtitles">
                          <Switch label="Download subtitles when available" describedby="set-video_subtitles-desc" checked={s.video_subtitles} onchange={(v) => set("video_subtitles", v)} />
                        </SettingRow>
                      {/if}
                      {#if show("ytdlp_auto_update")}
                        <SettingRow id="ytdlp_auto_update">
                          <Switch label="Keep yt-dlp up to date automatically" describedby="set-ytdlp_auto_update-desc" checked={s.ytdlp_auto_update} onchange={(v) => set("ytdlp_auto_update", v)} />
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                {:else if sec.id === "network"}
                  {#if any("proxy")}
                    <div class="card">
                      <SettingRow id="proxy">
                        <Segmented
                          label="Proxy"
                          value={s.proxy_mode}
                          options={[
                            { value: "system", label: "System" },
                            { value: "none", label: "None" },
                            { value: "manual", label: "Manual" },
                          ]}
                          onchange={(v) => set("proxy_mode", v)}
                        />
                        {#snippet below()}
                          {#if s?.proxy_mode === "manual"}
                            <input
                              class="wide"
                              aria-label="Proxy address"
                              placeholder="http://host:8080 or socks5://host:1080"
                              spellcheck="false"
                              aria-invalid={proxyError !== null}
                              bind:value={proxyDraft}
                              oninput={() => (proxyError = null)}
                              onchange={applyProxyUrl}
                            />
                            {#if proxyError}<p class="field-error" role="alert">{proxyError}</p>{/if}
                          {/if}
                        {/snippet}
                      </SettingRow>
                    </div>
                  {/if}
                  {#if any("connect_timeout_s", "read_timeout_s", "max_attempts")}
                    <div class="card">
                      {#if show("connect_timeout_s")}
                        <SettingRow id="connect_timeout_s">
                          <NumberField label="Connect timeout" unit="s" value={s.connect_timeout_s} check={(v) => checkInt(v, 5, 120)} onvalid={(v) => set("connect_timeout_s", v)} />
                        </SettingRow>
                      {/if}
                      {#if show("read_timeout_s")}
                        <SettingRow id="read_timeout_s">
                          <NumberField label="Read timeout" unit="s" value={s.read_timeout_s} check={(v) => checkInt(v, 10, 300)} onvalid={(v) => set("read_timeout_s", v)} />
                        </SettingRow>
                      {/if}
                      {#if show("max_attempts")}
                        <SettingRow id="max_attempts">
                          <NumberField label="Retries per part" value={s.max_attempts} check={(v) => checkInt(v, 1, 20)} onvalid={(v) => set("max_attempts", v)} />
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                {:else if sec.id === "updates"}
                  {#if any("update_check", "update_install", "check_now", "app_version")}
                    <div class="card">
                      {#if show("update_check")}
                        <SettingRow id="update_check">
                          <Switch label="Check for updates automatically" describedby="set-update_check-desc" checked={s.update_check} onchange={(v) => set("update_check", v)} />
                        </SettingRow>
                      {/if}
                      {#if show("update_install")}
                        <SettingRow id="update_install">
                          <Segmented
                            label="When an update is ready"
                            value={s.update_install}
                            options={[
                              { value: "ask", label: "Ask me" },
                              { value: "auto", label: "Install when I quit ODM" },
                            ]}
                            onchange={(v) => set("update_install", v)}
                          />
                        </SettingRow>
                      {/if}
                      {#if show("check_now")}
                        <SettingRow id="check_now">
                          <button onclick={() => oncheckupdates?.()}>Check now</button>
                        </SettingRow>
                      {/if}
                      {#if show("app_version")}
                        <SettingRow id="app_version">
                          <span class="status">{version ? `ODM ${version}` : "Unknown"}</span>
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                {:else if sec.id === "advanced"}
                  {#if any("user_agent", "ffmpeg_path", "ytdlp_path")}
                    <div class="card">
                      {#if show("user_agent")}
                        <SettingRow id="user_agent">
                          {#snippet below()}
                            <input
                              class="wide"
                              aria-label="Custom User-Agent"
                              aria-describedby="set-user_agent-desc"
                              placeholder="Leave empty for the default"
                              spellcheck="false"
                              value={s?.user_agent ?? ""}
                              onchange={(e) => set("user_agent", e.currentTarget.value.trim())}
                            />
                          {/snippet}
                        </SettingRow>
                      {/if}
                      {#if show("ffmpeg_path")}
                        <SettingRow id="ffmpeg_path">
                          <span class="path" title={s.ffmpeg_path}>{s.ffmpeg_path || "Automatic"}</span>
                          {#if s.ffmpeg_path}<button onclick={() => set("ffmpeg_path", "")}>Clear</button>{/if}
                          <button onclick={() => chooseProgram("ffmpeg_path")}>Choose…</button>
                        </SettingRow>
                      {/if}
                      {#if show("ytdlp_path")}
                        <SettingRow id="ytdlp_path">
                          <span class="path" title={s.ytdlp_path}>{s.ytdlp_path || "Automatic"}</span>
                          {#if s.ytdlp_path}<button onclick={() => set("ytdlp_path", "")}>Clear</button>{/if}
                          <button onclick={() => chooseProgram("ytdlp_path")}>Choose…</button>
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                  {#if any("app_folders", "reset_all")}
                    <div class="card">
                      {#if show("app_folders")}
                        <SettingRow id="app_folders">
                          <button onclick={() => openFolder("logs")}>Open logs folder</button>
                          <button onclick={() => openFolder("data")}>Open data folder</button>
                        </SettingRow>
                      {/if}
                      {#if show("reset_all")}
                        <SettingRow id="reset_all">
                          <button onclick={resetEverything}>Reset all settings</button>
                        </SettingRow>
                      {/if}
                    </div>
                  {/if}
                {/if}

                {#if !searching && sectionHasRestore(sec.id)}
                  <div class="restore">
                    <button class="link" onclick={() => restore(sec.id, sec.title)}>Restore defaults</button>
                  </div>
                {/if}
              </section>
            {:else}
              <p class="empty">No settings match “{query.trim()}”.</p>
            {/each}
          {:else if loadError}
            <p class="form-error" role="alert">{loadError}</p>
          {:else}
            <p class="hint">Loading…</p>
          {/if}
        </div>
      </main>
    </div>

    <div class="toast-slot" aria-live="polite">
      {#if toast}
        <div class="toast" transition:rise>
          <span>{toast.text}</span>
          <button onclick={undo}>Undo</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .page {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    box-shadow: var(--shadow-dialog);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px 8px 16px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  h1 {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
  }
  .cols {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  nav {
    width: 200px;
    flex: none;
    padding: 10px 8px;
    overflow-y: auto;
    background: var(--surface);
    border-right: 1px solid var(--border);
  }
  .search {
    width: 100%;
    margin-bottom: 10px;
  }
  nav button {
    display: flex;
    justify-content: space-between;
    width: 100%;
    margin-bottom: 2px;
    border-color: transparent;
    background: transparent;
    text-align: left;
    transition: none;
  }
  nav button:active:not(:disabled) {
    transform: none;
  }
  nav button.active {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
  }
  .count {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  main {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
  }
  .content {
    max-width: 680px;
    margin: 0 auto;
    padding: 20px 24px 80px;
  }
  section + section {
    margin-top: 28px;
  }
  h2 {
    margin: 0 0 12px;
    font-size: 20px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .card {
    margin-bottom: 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .card > :global(.srow + .srow) {
    border-top: 1px solid var(--border);
  }
  .tools {
    margin-bottom: 14px;
  }
  .restore {
    display: flex;
    justify-content: flex-end;
  }
  button.link {
    padding: 4px 6px;
    border-color: transparent;
    background: transparent;
    color: var(--accent);
    font-weight: 600;
  }
  .path {
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    text-align: left;
    user-select: text;
  }
  .status {
    color: var(--muted);
  }
  .folders {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 10px 12px;
  }
  .folder {
    display: grid;
    gap: 4px;
    color: var(--muted);
    font-size: 12.5px;
  }
  .folder input[aria-invalid="true"],
  .wide[aria-invalid="true"] {
    border-color: var(--error);
  }
  .inline {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
  }
  .wide {
    width: 100%;
  }
  .field-error {
    margin: 6px 0 0;
    color: var(--error);
    font-size: 12.5px;
  }
  .empty {
    color: var(--muted);
  }
  .toast-slot {
    position: absolute;
    left: 50%;
    bottom: 20px;
    transform: translateX(-50%);
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 8px 8px 14px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-dialog);
    white-space: nowrap;
    pointer-events: auto;
  }
</style>
