<script lang="ts">
  import { onMount } from "svelte";
  import AddDialog from "./components/AddDialog.svelte";
  import BatchDialog from "./components/BatchDialog.svelte";
  import DetailPanel from "./components/DetailPanel.svelte";
  import DownloadTable from "./components/DownloadTable.svelte";
  import RemoveDialog from "./components/RemoveDialog.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import Toolbar from "./components/Toolbar.svelte";
  import { downloads } from "./lib/downloads.svelte";

  let detailId = $state<string | null>(null);
  let addOpen = $state(false);
  let batchOpen = $state(false);
  let settingsOpen = $state(false);
  let removeOpen = $state(false);

  onMount(() => {
    // TEMPORARY: the dialogs that read these flags arrive in Tasks 13-14; svelte-check reports
    // unread locals as errors. Remove this read in Task 14 at the latest.
    void [settingsOpen];
    void downloads.init();
  });

  function onkeydown(e: KeyboardEvent) {
    if (addOpen || batchOpen || settingsOpen || removeOpen) return;
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

<div class="app">
  <Toolbar
    onadd={() => (addOpen = true)}
    onbatch={() => (batchOpen = true)}
    onsettings={() => (settingsOpen = true)}
    onremove={() => (removeOpen = true)}
  />
  {#if downloads.error}
    <div class="error" role="alert">
      <span>{downloads.error}</span>
      <button onclick={() => (downloads.error = null)}>Dismiss</button>
    </div>
  {/if}
  <div class="body">
    <Sidebar />
    <DownloadTable onopen={(id) => (detailId = id)} />
    <DetailPanel id={detailId} onclose={() => (detailId = null)} />
  </div>
</div>

<AddDialog open={addOpen} onclose={() => (addOpen = false)} />
<BatchDialog open={batchOpen} onclose={() => (batchOpen = false)} />
<RemoveDialog open={removeOpen} onclose={() => (removeOpen = false)} />
<!-- Task 14: SettingsDialog -->

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
    background: color-mix(in srgb, var(--error) 18%, var(--bg));
    border-bottom: 1px solid var(--error);
    user-select: text;
  }
</style>
