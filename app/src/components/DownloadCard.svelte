<script lang="ts">
  import { downloads } from "../lib/downloads.svelte";
  import { formatSpeed } from "../lib/format";
  import { ACTION_LABELS, FIX_LABELS, rowActions, rowSummary, type RowAction } from "../lib/home";
  import { isActive, problemInfo, progressPct, type Row } from "../lib/model";
  import { queueSchedule } from "../lib/queueSchedule.svelte";
  import Icon from "./Icon.svelte";
  import ProgressBar from "./ProgressBar.svelte";
  import RowMenu from "./RowMenu.svelte";

  let {
    row,
    line = null,
    moves = [],
    grip,
    dragging = false,
    dropEdge = null,
    onaction,
    onselect,
  }: {
    row: Row;
    /** Its place in the queue, such as "2nd in line"; only rows in Up next have one. */
    line?: string | null;
    /** Up next: the moves that apply to this row, for the menu and Alt+Up and Alt+Down. */
    moves?: RowAction[];
    /** Up next: pressing the handle starts a drag. */
    grip?: (e: PointerEvent) => void;
    dragging?: boolean;
    dropEdge?: "top" | "bottom" | null;
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
  const summary = $derived(rowSummary(row, waiting, queueSchedule.wait, line));
  const showBar = $derived(running || (row.bytes > 0 && !failed && row.state !== "completed" && row.state !== "cancelled"));
  const pct = $derived(row.segments || row.size ? progressPct(row) : null);
  const iconOnly = (a: RowAction) => a === "pause" || a === "open-folder";
  const label = (a: RowAction) => (a === "fix" ? FIX_LABELS[problemInfo(row).action] : ACTION_LABELS[a]);

  function click(e: MouseEvent) {
    if (e.shiftKey) onselect("range");
    else if (e.ctrlKey || e.metaKey) onselect("toggle");
    else onaction("details");
  }

  function key(e: KeyboardEvent) {
    if (e.target !== e.currentTarget) return;
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      const move = e.key === "ArrowUp" ? "move-up" : "move-down";
      if (moves.includes(move)) onaction(move);
      return;
    }
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
  class:dragging
  class:drop-top={dropEdge === "top"}
  class:drop-bottom={dropEdge === "bottom"}
  data-row={row.id}
  tabindex="0"
  aria-label={`${row.filename}. ${summary}`}
  onclick={click}
  onkeydown={key}
>
  {#if grip}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span class="grip" title="Drag to change the order" onpointerdown={grip} onclick={(e) => e.stopPropagation()}>
      <Icon name="grip" />
    </span>
  {/if}
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
          title={label(a)}
          aria-label={`${label(a)} ${row.filename}`}
          onclick={(e) => act(e, a)}
        >
          {#if a === "pause"}<Icon name="pause" />{:else if a === "open-folder"}<Icon name="folder" />{:else}{label(a)}{/if}
        </button>
      {/if}
    {/each}
    <RowMenu actions={[...moves, ...actions.menu]} name={row.filename} onchoose={onaction} />
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
  .card.dragging {
    opacity: 0.5;
  }
  .card.drop-top {
    box-shadow: 0 -2px 0 var(--accent);
  }
  .card.drop-bottom {
    box-shadow: 0 2px 0 var(--accent);
  }
  .grip {
    display: grid;
    place-items: center;
    flex: none;
    margin: -10px -8px -10px -6px;
    padding: 10px 2px;
    color: var(--muted);
    cursor: grab;
    touch-action: none;
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
