<script lang="ts">
  import { downloads } from "../lib/downloads.svelte";
  import { formatSpeed } from "../lib/format";
  import { ACTION_LABELS, rowActions, rowSummary, type RowAction } from "../lib/home";
  import { isActive, progressPct, type Row } from "../lib/model";
  import { queueSchedule } from "../lib/queueSchedule.svelte";
  import Icon from "./Icon.svelte";
  import ProgressBar from "./ProgressBar.svelte";
  import RowMenu from "./RowMenu.svelte";

  let {
    row,
    note = null,
    onaction,
    onselect,
  }: {
    row: Row;
    /** Added to the summary, for example "2nd in line". */
    note?: string | null;
    onaction: (a: RowAction) => void;
    onselect: (mode: "toggle" | "range") => void;
  } = $props();

  const TILES: Record<string, string> = {
    video: "video",
    audio: "music",
    document: "doc",
    compressed: "archive",
    image: "image",
    program: "program",
    torrent: "magnet",
  };

  const waiting = $derived(downloads.waitsForChoice(row.id));
  const closed = $derived(queueSchedule.wait !== null);
  const actions = $derived(rowActions(row, closed, waiting));
  const failed = $derived(row.state === "failed");
  const running = $derived(isActive(row.state));
  const summary = $derived([rowSummary(row, waiting, queueSchedule.wait), note].filter(Boolean).join(" · "));
  const showBar = $derived(running || (row.bytes > 0 && !failed && row.state !== "completed" && row.state !== "cancelled"));
  const pct = $derived(row.segments || row.size ? progressPct(row) : null);
  const iconOnly = (a: RowAction) => a === "pause" || a === "open-folder";

  function click(e: MouseEvent) {
    if (e.shiftKey) onselect("range");
    else if (e.ctrlKey || e.metaKey) onselect("toggle");
    else onaction("details");
  }

  function key(e: KeyboardEvent) {
    if (e.target !== e.currentTarget) return;
    if (e.key === "Enter") onaction("details");
    if (e.key === " ") {
      e.preventDefault();
      onselect("toggle");
    }
  }

  function act(e: MouseEvent, a: RowAction) {
    e.stopPropagation();
    onaction(a);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<li
  class="card"
  class:selected={downloads.selected.has(row.id)}
  class:bad={failed}
  tabindex="0"
  aria-label={`${row.filename}. ${summary}`}
  onclick={click}
  onkeydown={key}
>
  <span class="tile" class:live={running} class:bad={failed} aria-hidden="true">
    <Icon name={failed ? "alert" : (TILES[row.category] ?? "doc")} size={20} />
  </span>
  <div class="text">
    <div class="name" title={row.filename}>{row.filename}</div>
    <div class="summary" class:bad={failed} title={summary}>{summary}</div>
    {#if showBar}<div class="progress"><ProgressBar value={pct} state={row.state} /></div>{/if}
  </div>
  {#if row.state === "downloading"}<span class="speed">{formatSpeed(row.speed_bps)}</span>{/if}
  <div class="actions">
    {#each [actions.primary, actions.secondary] as a (a)}
      {#if a}
        <button
          class:primary={a === "fix" || a === "choose-files"}
          class:icon={iconOnly(a)}
          title={ACTION_LABELS[a]}
          aria-label={`${ACTION_LABELS[a]} ${row.filename}`}
          onclick={(e) => act(e, a)}
        >
          {#if a === "pause"}<Icon name="pause" />{:else if a === "open-folder"}<Icon name="folder" />{:else}{ACTION_LABELS[a]}{/if}
        </button>
      {/if}
    {/each}
    <RowMenu actions={actions.menu} name={row.filename} onchoose={onaction} />
  </div>
</li>

<style>
  .card {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 68px;
    padding: 10px 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-lg);
    background: var(--surface);
    cursor: pointer;
    list-style: none;
  }
  .card.bad {
    border-color: var(--error);
  }
  .card.selected {
    background: var(--selected);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .tile {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-lg);
    background: var(--surface-2);
  }
  .tile.live {
    color: var(--accent);
  }
  .tile.bad {
    background: var(--error-tint);
    color: var(--error);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    font-size: 14px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .summary {
    overflow: hidden;
    color: var(--muted);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .summary.bad {
    color: var(--error);
  }
  .progress {
    margin-top: 6px;
    max-width: 90%;
  }
  .speed {
    flex: none;
    font-weight: 600;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    align-items: center;
    flex: none;
    gap: 6px;
  }
  .actions button.icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }
</style>
