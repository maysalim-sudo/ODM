<script lang="ts">
  import { onMount } from "svelte";
  import AddDialog from "./components/AddDialog.svelte";
  import CancelDialog from "./components/CancelDialog.svelte";
  import BatchDialog from "./components/BatchDialog.svelte";
  import DetailPanel from "./components/DetailPanel.svelte";
  import DownloadTable from "./components/DownloadTable.svelte";
  import OpenProgramDialog from "./components/OpenProgramDialog.svelte";
  import RefreshUrlDialog from "./components/RefreshUrlDialog.svelte";
  import RemoveDialog from "./components/RemoveDialog.svelte";
  import ReuseDialog from "./components/ReuseDialog.svelte";
  import BrowserDialog from "./components/BrowserDialog.svelte";
  import WelcomeDialog from "./components/WelcomeDialog.svelte";
  import ConfirmDialog from "./components/ConfirmDialog.svelte";
  import ExistsDialog from "./components/ExistsDialog.svelte";
  import ShutdownDialog from "./components/ShutdownDialog.svelte";
  import SettingsPage from "./components/SettingsPage.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import Toolbar from "./components/Toolbar.svelte";
  import * as api from "./lib/api";
  import { downloads } from "./lib/downloads.svelte";
  import { integration } from "./lib/integration.svelte";

  let detailId = $state<string | null>(null);
  let addOpen = $state(false);
  let batchOpen = $state(false);
  let settingsOpen = $state(false);
  let removeOpen = $state(false);
  let cancelOpen = $state(false);
  let refreshOpen = $state(false);
  let programOpen = $state(false);
  let browserOpen = $state(false);
  let welcomeOpen = $state(false);
  const detailName = $derived(detailId ? (downloads.rows.get(detailId)?.filename ?? "") : "");

  onMount(() => {
    void downloads.init();
    // First start: the guided setup. A settings read that fails just skips it.
    api
      .getSettings()
      .then((s) => (welcomeOpen = !s.onboarding_done))
      .catch(() => {});
    return integration.startPolling();
  });

  function onkeydown(e: KeyboardEvent) {
    if (
      addOpen ||
      batchOpen ||
      settingsOpen ||
      removeOpen ||
      cancelOpen ||
      refreshOpen ||
      programOpen ||
      browserOpen ||
      welcomeOpen ||
      downloads.offers.length > 0 ||
      downloads.prompts.length > 0 ||
      downloads.shutdownAt !== null
    )
      return;
    if (e.ctrlKey && e.key === ",") {
      e.preventDefault();
      settingsOpen = true;
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
    if (e.ctrlKey && e.key.toLowerCase() === "n") {
      e.preventDefault();
      if (e.shiftKey) batchOpen = true;
      else addOpen = true;
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="app" inert={settingsOpen}>
  <Toolbar
    onadd={() => (addOpen = true)}
    onbatch={() => (batchOpen = true)}
    onsettings={() => (settingsOpen = true)}
    oncancel={() => (cancelOpen = true)}
    onremove={() => (removeOpen = true)}
  />
  {#if downloads.error}
    <div class="error" role="alert">
      <span>{downloads.error}</span>
      <button onclick={() => (downloads.error = null)}>Dismiss</button>
    </div>
  {/if}
  <div class="body">
    <Sidebar onbrowser={() => (browserOpen = true)} />
    <DownloadTable onopen={(id) => (detailId = id)} />
    <DetailPanel
      id={detailId}
      onclose={() => (detailId = null)}
      onrefresh={() => (refreshOpen = true)}
      onrisky={() => (programOpen = true)}
    />
  </div>
</div>

<AddDialog open={addOpen} onclose={() => (addOpen = false)} />
<BatchDialog open={batchOpen} onclose={() => (batchOpen = false)} />
<CancelDialog open={cancelOpen} onclose={() => (cancelOpen = false)} />
<RemoveDialog open={removeOpen} onclose={() => (removeOpen = false)} />
<RefreshUrlDialog open={refreshOpen} id={detailId} onclose={() => (refreshOpen = false)} />
<OpenProgramDialog open={programOpen} id={detailId} name={detailName} onclose={() => (programOpen = false)} />
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
<SettingsPage open={settingsOpen} onclose={() => (settingsOpen = false)} onbrowser={() => (browserOpen = true)} />
<ReuseDialog />
<ConfirmDialog />
<ExistsDialog />
<ShutdownDialog />

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
