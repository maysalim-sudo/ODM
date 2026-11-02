<script lang="ts">
  import { onMount } from "svelte";
  import AddPanel from "./components/AddPanel.svelte";
  import CancelDialog from "./components/CancelDialog.svelte";
  import BatchDialog from "./components/BatchDialog.svelte";
  import DetailPanel from "./components/DetailPanel.svelte";
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
  import AfterQueueDialog from "./components/AfterQueueDialog.svelte";
  import SettingsPage from "./components/SettingsPage.svelte";
  import Header from "./components/Header.svelte";
  import SelectionBar from "./components/SelectionBar.svelte";
  import StatusBar from "./components/StatusBar.svelte";
  import Tabs from "./components/Tabs.svelte";
  import * as api from "./lib/api";
  import { downloads } from "./lib/downloads.svelte";
  import { integration } from "./lib/integration.svelte";
  import { queueSchedule } from "./lib/queueSchedule.svelte";
  import { sectionFor, type SectionId } from "./lib/settingsPage";
  import type { RowAction } from "./lib/home";
  import { isRiskyFile, problemInfo, type Row } from "./lib/model";
  import type { ListView } from "./lib/types";
  import { updates } from "./lib/updates.svelte";

  let detailId = $state<string | null>(null);
  let addPanel = $state<AddPanel>();
  let batchOpen = $state(false);
  let settingsOpen = $state(false);
  let removeOpen = $state(false);
  let cancelOpen = $state(false);
  let refreshOpen = $state(false);
  let programOpen = $state(false);
  let browserOpen = $state(false);
  let welcomeOpen = $state(false);
  let settingsSection = $state<SectionId | undefined>(undefined);
  let listView = $state<ListView>("cards");
  /** The row a card's action applies to; the details panel's row otherwise. */
  let actionId = $state<string | null>(null);
  const targetId = $derived(actionId ?? detailId);
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
    return () => {
      gone = true;
      off?.();
      offQueue?.();
      offDrop?.();
      stop();
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
        detailId = row.id;
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
    if (e.key === "Escape") detailId = null;
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
  <div class="body">
    <div class="main">
      {#if listView === "cards"}
        <DownloadList onaction={rowAction} onremove={() => (removeOpen = true)} onbrowser={() => (browserOpen = true)} />
      {:else}
        <DownloadTable onopen={(id) => (detailId = id)} />
      {/if}
      <SelectionBar oncancel={() => (cancelOpen = true)} onremove={() => (removeOpen = true)} />
    </div>
    <DetailPanel
      id={detailId}
      onclose={() => (detailId = null)}
      onaction={(a) => {
        const row = detailId ? downloads.rows.get(detailId) : undefined;
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
<RemoveDialog open={removeOpen} onclose={() => (removeOpen = false)} />
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
    min-height: 0;
  }
  .main {
    position: relative;
    flex: 1;
    min-width: 0;
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
