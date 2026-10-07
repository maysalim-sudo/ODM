<script lang="ts">
  import type { Snippet } from "svelte";
  import { shouldCloseOnBackdrop } from "../lib/model";

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

  let downOnBackdrop = false;

  function down(e: MouseEvent) {
    downOnBackdrop = e.target === el;
  }

  function backdrop(e: MouseEvent) {
    const close = shouldCloseOnBackdrop(downOnBackdrop, e.target === el);
    downOnBackdrop = false;
    if (close) onclose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={el} style:width={`${width}px`} aria-labelledby="modal-title" onclose={onclose} onmousedown={down} onclick={backdrop}>
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
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-dialog);
  }
  dialog::backdrop {
    background: var(--backdrop);
  }
  /* Entrance: an occasional element, so a short fade and settle; centred, not origin-aware. */
  dialog[open] {
    opacity: 1;
    transform: scale(1);
    transition:
      opacity 200ms var(--ease-out),
      transform 200ms var(--ease-out);
  }
  dialog[open]::backdrop {
    opacity: 1;
    transition: opacity 200ms var(--ease-out);
  }
  @starting-style {
    dialog[open] {
      opacity: 0;
      transform: scale(0.96);
    }
    dialog[open]::backdrop {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    dialog[open] {
      transform: none;
      transition: opacity 200ms var(--ease-out);
    }
    @starting-style {
      dialog[open] {
        transform: none;
      }
    }
  }
  .content {
    padding: 18px;
  }
  h2 {
    margin: 0 0 14px;
    font-size: 15px;
    font-weight: 700;
  }
</style>
