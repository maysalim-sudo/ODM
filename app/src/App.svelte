<script lang="ts">
  import { onMount } from "svelte";
  import DownloadTable from "./components/DownloadTable.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import Toolbar from "./components/Toolbar.svelte";
  import { downloads } from "./lib/downloads.svelte";

  let detailId = $state<string | null>(null);
  let addOpen = $state(false);
  let batchOpen = $state(false);
  let settingsOpen = $state(false);
  let removeOpen = $state(false);

  // TEMPORARY (Task 11 only): the dialogs that read these flags arrive in Tasks 12-14; svelte-check
  // reports unread locals as errors here. Remove the temporary read in onMount in Task 14 at the latest.

  onMount(() => {
    void [detailId, addOpen, batchOpen, settingsOpen, removeOpen]; // TEMPORARY, see above
    void downloads.init();
  });

  function onkeydown(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
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
    <!-- Task 12: <DetailPanel id={detailId} onclose={() => (detailId = null)} /> -->
  </div>
</div>

<!-- Tasks 13 and 14 add: AddDialog, BatchDialog, RemoveDialog, SettingsDialog -->

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
