<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    open,
    title,
    onclose,
    width = 520,
    children,
  }: { open: boolean; title: string; onclose: () => void; width?: number; children: Snippet } = $props();

  let el = $state<HTMLDialogElement>();

  $effect(() => {
    if (!el) return;
    if (open && !el.open) el.showModal();
    if (!open && el.open) el.close();
  });

  function backdrop(e: MouseEvent) {
    if (e.target === el) onclose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={el} style:width={`${width}px`} aria-labelledby="modal-title" onclose={onclose} onclick={backdrop}>
  {#if open}
    <div class="content">
      <h2 id="modal-title">{title}</h2>
      {@render children()}
    </div>
  {/if}
</dialog>

<style>
  dialog {
    max-width: calc(100vw - 40px);
    padding: 0;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.6);
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.55);
  }
  .content {
    padding: 18px;
  }
  h2 {
    margin: 0 0 14px;
    font-size: 15px;
  }
</style>
