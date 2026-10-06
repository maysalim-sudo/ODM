<script lang="ts">
  import * as api from "../lib/api";
  import { integration } from "../lib/integration.svelte";
  import { integrationProblem, relativeTime } from "../lib/model";
  import type { CheckStatus } from "../lib/types";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  type Note = { kind: "ok" | "error"; text: string };

  let testing = $state(false);
  let repairing = $state(false);
  let testNote = $state<Note | null>(null);
  let repairNote = $state<Note | null>(null);
  let folderError = $state<string | null>(null);

  const st = $derived(integration.status);
  const problem = $derived(st ? integrationProblem(st) : null);

  const PILL: Record<CheckStatus, string> = {
    ok: "Ready",
    missing: "Not registered",
    wrong_path: "Other copy",
    broken: "Damaged",
  };

  $effect(() => {
    if (!open) return;
    testNote = null;
    repairNote = null;
    folderError = null;
    void integration.refresh();
  });

  async function test() {
    if (testing) return;
    testing = true;
    testNote = null;
    try {
      const version = await api.integrationTest();
      testNote = { kind: "ok", text: `Working. ODM ${version} answered through the browser helper.` };
    } catch (e) {
      testNote = { kind: "error", text: api.errorText(e) };
    } finally {
      testing = false;
      void integration.refresh();
    }
  }

  async function repair() {
    if (repairing) return;
    repairing = true;
    repairNote = null;
    try {
      integration.status = await api.integrationRepair();
      repairNote = { kind: "ok", text: "Registration repaired." };
    } catch (e) {
      repairNote = { kind: "error", text: api.errorText(e) };
    } finally {
      repairing = false;
    }
  }

  async function openFolder(kind: "chromium" | "firefox") {
    folderError = null;
    try {
      await api.openExtensionFolder(kind);
    } catch (e) {
      folderError = api.errorText(e);
    }
  }
</script>

<Modal {open} title="Browser connection" {onclose} width={560}>
  {#if st}
    <p class="summary" class:warn={problem !== null}>
      {#if problem}Something needs attention. {problem}{:else}Everything is ready.{/if}
    </p>

    <ul class="browsers">
      {#each st.browsers as b (b.browser)}
        <li>
          <span class="name">{b.browser}</span>
          <span class="pill" class:ok={b.status === "ok"}>{PILL[b.status]}</span>
          <span class="detail">{b.detail}</span>
        </li>
      {/each}
    </ul>

    <p class="contact">
      {#if st.last_contact_ms !== null}
        Last message from a browser: {relativeTime(st.last_contact_ms, integration.now)}{st.last_command ? ` (${st.last_command})` : ""}
      {:else}
        No browser has contacted ODM since it started.
      {/if}
    </p>

    <div class="buttons">
      <button onclick={test} disabled={testing}>Test connection</button>
      <button onclick={repair} disabled={repairing}>Repair</button>
    </div>
    {#if testNote}
      <p class={testNote.kind === "ok" ? "note" : "form-error"} role={testNote.kind === "ok" ? "status" : "alert"}>{testNote.text}</p>
    {/if}
    {#if repairNote}
      <p class={repairNote.kind === "ok" ? "note" : "form-error"} role={repairNote.kind === "ok" ? "status" : "alert"}>{repairNote.text}</p>
    {/if}

    <h3>Add the extension</h3>
    <div class="ext">
      <h4>Chromium browsers</h4>
      <ol>
        <li>Open <code>chrome://extensions</code> (or your browser's extensions page).</li>
        <li>Turn on Developer mode.</li>
        <li>Choose Load unpacked.</li>
        <li>Pick the folder.</li>
      </ol>
      <button onclick={() => openFolder("chromium")}>Open Chromium extension folder</button>
    </div>
    <div class="ext">
      <h4>Firefox and Floorp</h4>
      <ol>
        <li>Open <code>about:debugging</code>.</li>
        <li>Choose This Firefox.</li>
        <li>Choose Load Temporary Add-on.</li>
        <li>Pick <code>manifest.json</code> in the folder.</li>
      </ol>
      <button onclick={() => openFolder("firefox")}>Open Firefox extension folder</button>
    </div>
    {#if folderError}<p class="form-error" role="alert">{folderError}</p>{/if}
  {:else}
    <p class="hint">Loading…</p>
  {/if}
  <div class="actions"><button onclick={onclose}>Close</button></div>
</Modal>

<style>
  .summary {
    margin: 0 0 10px;
    font-weight: 600;
    color: var(--success);
  }
  .summary.warn {
    color: var(--warn);
  }
  .browsers {
    margin: 0 0 10px;
    padding: 0;
    list-style: none;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .browsers li {
    display: grid;
    grid-template-columns: 140px 110px 1fr;
    gap: 8px;
    align-items: center;
    padding: 6px 10px;
  }
  .browsers li + li {
    border-top: 1px solid var(--border);
  }
  .pill {
    justify-self: start;
    padding: 1px 8px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--warn) 20%, var(--bg));
    color: var(--warn);
    font-size: 12px;
  }
  .pill.ok {
    background: color-mix(in srgb, var(--success) 18%, var(--bg));
    color: var(--success);
  }
  .detail,
  .contact {
    color: var(--muted);
  }
  .contact {
    margin: 0 0 12px;
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  .note {
    margin: 8px 0 0;
    color: var(--success);
    user-select: text;
  }
  h3 {
    margin: 18px 0 8px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  .ext {
    margin-bottom: 12px;
  }
  h4 {
    margin: 0 0 4px;
    font-size: 13px;
  }
  ol {
    margin: 0 0 8px;
    padding-left: 20px;
    color: var(--muted);
  }
  code {
    color: var(--text);
    user-select: text;
  }
</style>
