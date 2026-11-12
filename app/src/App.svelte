<script lang="ts">
  import { onMount } from "svelte";
  import AddPanel from "./components/AddPanel.svelte";
  import CancelDialog from "./components/CancelDialog.svelte";
  import BatchDialog from "./components/BatchDialog.svelte";
  import BottomPanel from "./components/BottomPanel.svelte";
  import DownloadList from "./components/DownloadList.svelte";
  import DownloadTable from "./components/DownloadTable.svelte";
  import OpenProgramDialog from "./components/OpenProgramDialog.svelte";
  import RefreshUrlDialog from "./components/RefreshUrlDialog.svelte";
  import RemoveDialog from "./components/RemoveDialog.svelte";
  import ReuseDialog from "./components/ReuseDialog.svelte";
  import BrowserDialog from "./components/BrowserDialog.svelte";
  import WelcomeDialog from "./components/WelcomeDialog.svelte";
  import ConfirmDialog from "./components/ConfirmDialog.svelte";
  import ExistsDialog from "./components/ExistsDialog.svelte";
  import FinishedDialog from "./components/FinishedDialog.svelte";
  import AfterQueueDialog from "./components/AfterQueueDialog.svelte";
  import SettingsPage from "./components/SettingsPage.svelte";
  import Header from "./components/Header.svelte";
  import ActionBar from "./components/ActionBar.svelte";
  import PopMenu from "./components/PopMenu.svelte";
  import SpeedDialog from "./components/SpeedDialog.svelte";
  import StatusBar from "./components/StatusBar.svelte";
  import Tabs from "./components/Tabs.svelte";
  import * as api from "./lib/api";
  import { downloads } from "./lib/downloads.svelte";
  import { integration } from "./lib/integration.svelte";
  import { queueSchedule } from "./lib/queueSchedule.svelte";
  import { sectionFor, type SectionId } from "./lib/settingsPage";
  import type { RowAction } from "./lib/home";
  import { isRiskyFile, problemInfo, type Row } from "./lib/model";
  import { joinPath } from "./lib/format";
  import { queueOrder } from "./lib/home";
  import { LOW_KBPS, againArgs, contextMenu, moveManyInQueue, targets, type SelAction } from "./lib/selection";
  import type { ListView } from "./lib/types";
  import { speedHistory } from "./lib/speedHistory.svelte";
  import { updates } from "./lib/updates.svelte";

  let addPanel = $state<AddPanel>();
  let bottomPanel = $state<BottomPanel>();
  let batchOpen = $state(false);
  let settingsOpen = $state(false);
  let removeOpen = $state(false);
  let removeWithFiles = $state(false);
  let speedOpen = $state(false);
  /** The right-click menu: where it opened. */
  let context = $state<{ x: number; y: number } | null>(null);
  let cancelOpen = $state(false);
  let refreshOpen = $state(false);
  let programOpen = $state(false);
  let browserOpen = $state(false);
  let welcomeOpen = $state(false);
  let settingsSection = $state<SectionId | undefined>(undefined);
  let listView = $state<ListView>("cards");
  /** The row a dialog's action applies to. */
  let actionId = $state<string | null>(null);
  const targetId = $derived(actionId);
  const targetName = $derived(targetId ? (downloads.rows.get(targetId)?.filename ?? "") : "");

  onMount(() => {
    void downloads.init();
    void updates.init();
    // First start: the guided setup. A settings read that fails just skips it.
    api
      .getSettings()
      .then((s) => {
        welcomeOpen = !s.onboarding_done;
        listView = s.list_view;
      })
      .catch(() => {});
    // The tray can ask for the Settings page at a section.
    let off: (() => void) | undefined;
    let offQueue: (() => void) | undefined;
    let gone = false;
    void queueSchedule.init().then((un) => (gone ? un() : (offQueue = un)));
    void api
      .onOpenSettings((id) => {
        settingsSection = sectionFor(id);
        settingsOpen = true;
      })
      .then((un) => (gone ? un() : (off = un)));
    // A .torrent dropped on the window opens like one picked in the Add dialog.
    let offDrop: (() => void) | undefined;
    void api
      .onTorrentDrop((paths) => {
        for (const path of paths) void downloads.run(() => api.addTorrent({ kind: "file", path }));
      })
      .then((un) => (gone ? un() : (offDrop = un)));
    const stop = integration.startPolling();
    const stopSpeeds = speedHistory.start();
    return () => {
      gone = true;
      off?.();
      offQueue?.();
      offDrop?.();
      stop();
      stopSpeeds();
    };
  });

  async function setListView(v: ListView) {
    listView = v;
    await downloads.run(async () => api.saveSettings({ ...(await api.getSettings()), list_view: v }));
  }

  function rowAction(row: Row, a: RowAction) {
    const only = () => (downloads.selected = new Set([row.id]));
    switch (a) {
      case "details":
        only();
        bottomPanel?.showInfo();
        break;
      case "pause":
        void downloads.run(() => api.pause([row.id]));
        break;
      case "resume":
      case "start-now":
        void downloads.run(() => api.resume([row.id]));
        break;
      case "fix": {
        const fix = problemInfo(row).action;
        if (fix === "retry") void downloads.run(() => api.resume([row.id]));
        else rowAction(row, fix === "tools" ? "details" : "new-link");
        break;
      }
      case "choose-files":
        downloads.chooseFiles(row.id);
        break;
      case "open":
        if (isRiskyFile(row.filename)) {
          actionId = row.id;
          programOpen = true;
        } else void downloads.run(() => api.openFile(row.id));
        break;
      case "open-folder":
        void downloads.run(() => api.openFolder(row.id));
        break;
      case "copy-link":
        void downloads.run(() => navigator.clipboard.writeText(row.url));
        break;
      case "new-link":
        actionId = row.id;
        refreshOpen = true;
        break;
      case "cancel":
        only();
        cancelOpen = true;
        break;
      case "remove":
        only();
        removeOpen = true;
        break;
    }
  }

  const dialogOpen = $derived(
    batchOpen ||
      settingsOpen ||
      removeOpen ||
      speedOpen ||
      cancelOpen ||
      refreshOpen ||
      programOpen ||
      browserOpen ||
      welcomeOpen ||
      downloads.confirmRow !== undefined ||
      downloads.offers.length > 0 ||
      downloads.prompts.length > 0 ||
      downloads.afterQueue !== null,
  );

  /** The selected rows an action applies to, while the queue schedule is or isn't closed. */
  const selTargets = (a: SelAction) => targets(downloads.selectedRows, a, queueSchedule.wait !== null);

  /** Sets this limit (KB/s, null = none) on each row, then reloads so the list shows it. */
  async function limitAll(rows: Row[], kbps: number | null) {
    await downloads.run(() => Promise.all(rows.map((r) => api.setDownloadLimit(r.id, kbps))));
    void downloads.reload();
  }

  async function moveAll(rows: Row[], to: "top" | "up" | "down" | "bottom") {
    const queue = queueOrder([...downloads.rows.values()]).map((r) => r.id);
    await downloads.reorder(
      moveManyInQueue(
        queue,
        rows.map((r) => r.id),
        to,
      ),
    );
  }

  /** What the action bar and the right-click menu do with the selection. */
  function selAction(a: SelAction) {
    const rows = selTargets(a);
    const ids = rows.map((r) => r.id);
    const single = rows.length === 1 ? rows[0] : undefined;
    switch (a) {
      case "open":
      case "open-folder":
      case "new-link":
        if (single) rowAction(single, a);
        break;
      case "resume":
      case "start-now":
      case "retry":
        void downloads.run(() => api.resume(ids));
        break;
      case "pause":
        void downloads.run(() => api.pause(ids));
        break;
      case "stop":
        downloads.selected = new Set(ids);
        cancelOpen = true;
        break;
      case "move-top":
      case "move-up":
      case "move-down":
      case "move-bottom":
        void moveAll(rows, a === "move-bottom" ? "bottom" : (a.slice(5) as "top" | "up" | "down"));
        break;
      case "low":
        // Low is both: last in line, and slow.
        void moveAll(selTargets("move-bottom"), "bottom").then(() => limitAll(rows, LOW_KBPS));
        break;
      case "normal":
      case "speed-unlimited":
        void limitAll(rows, null);
        break;
      case "speed-low":
        void limitAll(rows, LOW_KBPS);
        break;
      case "speed-custom":
        speedOpen = true;
        break;
      case "copy-link":
        void downloads.run(() => navigator.clipboard.writeText(rows.map((r) => r.url).join("\n")));
        break;
      case "copy-name":
        void downloads.run(() => navigator.clipboard.writeText(rows.map((r) => r.filename).join("\n")));
        break;
      case "copy-path":
        void downloads.run(() => navigator.clipboard.writeText(rows.map((r) => joinPath(r.save_dir, r.filename)).join("\n")));
        break;
      case "again":
        for (const r of rows) {
          const args = againArgs(r);
          if (args) void downloads.run(() => api.addDownload(args));
        }
        break;
      case "select-all":
        downloads.selectAll();
        break;
      case "remove":
      case "delete":
        removeWithFiles = a === "delete";
        removeOpen = true;
        break;
    }
  }

  /** Right-click on a download: it joins the selection (or becomes it), and the menu opens there. */
  function oncontextmenu(e: MouseEvent) {
    const el = (e.target as Element | null)?.closest<HTMLElement>("[data-row]");
    const id = el?.dataset.row;
    if (!id || !downloads.rows.has(id)) return;
    e.preventDefault();
    if (!downloads.selected.has(id)) downloads.select(id, "one");
    context = { x: e.clientX, y: e.clientY };
  }

  function onkeydown(e: KeyboardEvent) {
    if (dialogOpen) return;
    if (e.ctrlKey && e.key === ",") {
      e.preventDefault();
      settingsOpen = true;
      return;
    }
    if (e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === "n") {
      e.preventDefault();
      addPanel?.focus();
      return;
    }
    const t = e.target;
    const typing =
      t instanceof HTMLInputElement ||
      t instanceof HTMLTextAreaElement ||
      t instanceof HTMLSelectElement ||
      (t instanceof HTMLElement && t.isContentEditable);
    if (typing) return;
    if (e.key === "Delete" && downloads.selected.size > 0) removeOpen = true;
    // Text fields keep their own Ctrl+A (returned above).
    if (e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === "a") {
      e.preventDefault();
      downloads.selectAll();
    }
    if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "n") {
      e.preventDefault();
      batchOpen = true;
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="app" inert={settingsOpen || welcomeOpen}>
  <Header
    onbatch={() => (batchOpen = true)}
    onsettings={() => {
      settingsSection = undefined;
      settingsOpen = true;
    }}
  >
    <AddPanel bind:this={addPanel} blocked={dialogOpen} />
  </Header>
  {#if downloads.error}
    <div class="error" role="alert">
      <span>{downloads.error}</span>
      <button onclick={() => (downloads.error = null)}>Dismiss</button>
    </div>
  {/if}
  <Tabs view={listView} onview={(v) => void setListView(v)} />
  <ActionBar onaction={selAction} />
  <div class="body">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="main" {oncontextmenu}>
      {#if listView === "cards"}
        <DownloadList onaction={rowAction} onremove={() => (removeOpen = true)} onbrowser={() => (browserOpen = true)} />
      {:else}
        <DownloadTable
          onopen={(id) => {
            // Double-click opens a finished file; other rows are already in the panel.
            const row = downloads.rows.get(id);
            if (row?.state === "completed") rowAction(row, "open");
          }}
        />
      {/if}
    </div>
    <BottomPanel
      bind:this={bottomPanel}
      onaction={(a) => {
        const id = downloads.selected.size === 1 ? [...downloads.selected][0] : undefined;
        const row = id ? downloads.rows.get(id) : undefined;
        if (row) rowAction(row, a);
      }}
    />
  </div>
  <StatusBar
    onbrowser={() => (browserOpen = true)}
    onnotes={() => {
      settingsSection = "advanced";
      settingsOpen = true;
    }}
  />
</div>

<BatchDialog open={batchOpen} onclose={() => (batchOpen = false)} />
<CancelDialog open={cancelOpen} onclose={() => (cancelOpen = false)} />
<RemoveDialog
  open={removeOpen}
  withFiles={removeWithFiles}
  onclose={() => {
    removeOpen = false;
    removeWithFiles = false;
  }}
/>
<SpeedDialog
  open={speedOpen}
  count={selTargets("speed-custom").length}
  onapply={(kbps) => void limitAll(selTargets("speed-custom"), kbps)}
  onclose={() => (speedOpen = false)}
/>
{#if context}
  <PopMenu
    items={contextMenu(downloads.selectedRows, queueSchedule.wait !== null)}
    enabled={() => true}
    x={context.x}
    y={context.y}
    label="Download actions"
    onchoose={selAction}
    onclose={() => (context = null)}
  />
{/if}
<RefreshUrlDialog open={refreshOpen} id={targetId} onclose={() => (refreshOpen = false)} />
<OpenProgramDialog open={programOpen} id={targetId} name={targetName} onclose={() => (programOpen = false)} />
<WelcomeDialog open={welcomeOpen} onclose={() => (welcomeOpen = false)} />
<BrowserDialog
  open={browserOpen}
  onsetup={() => {
    browserOpen = false;
    welcomeOpen = true;
  }}
  onclose={() => {
    browserOpen = false;
    void integration.refresh();
  }}
/>
<SettingsPage
  open={settingsOpen}
  section={settingsSection}
  onclose={() => (settingsOpen = false)}
  onbrowser={() => (browserOpen = true)}
  oncheckupdates={() => void updates.checkNow()}
/>
<ReuseDialog />
<ConfirmDialog />
<ExistsDialog />
<FinishedDialog />
<AfterQueueDialog />

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .body {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .main {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 10px;
    background: var(--error-tint);
    border-bottom: 1px solid var(--error);
    user-select: text;
  }
</style>
