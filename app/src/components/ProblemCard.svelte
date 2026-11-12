<script lang="ts">
  import { downloads } from "../lib/downloads.svelte";
  import { FIX_LABELS, stoppedAt, type RowAction } from "../lib/home";
  import { problemInfo, type ProblemFix, type Row } from "../lib/model";
  import Icon from "./Icon.svelte";

  let {
    row,
    onaction,
    onselect,
  }: { row: Row; onaction: (a: RowAction) => void; onselect: (mode: "one" | "toggle" | "range") => void } = $props();

  const ICONS: Record<ProblemFix, string> = { "new-link": "refresh", tools: "video", retry: "alert" };

  const info = $derived(problemInfo(row));
  const where = $derived([row.filename, stoppedAt(row)].filter(Boolean).join(" · "));

  function act(e: MouseEvent, a: RowAction) {
    e.stopPropagation();
    onaction(a);
  }

  function click(e: MouseEvent) {
    if (e.shiftKey) onselect("range");
    else if (e.ctrlKey || e.metaKey) onselect("toggle");
    else onselect("one");
  }

  function key(e: KeyboardEvent) {
    if (e.target !== e.currentTarget) return;
    if (e.key === "Enter") onselect("one");
    if (e.key === " ") {
      e.preventDefault();
      onselect("toggle");
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<li
  class="card"
  data-row={row.id}
  class:selected={downloads.selected.has(row.id)}
  tabindex="0"
  aria-label={`${info.title}. ${row.filename}`}
  onclick={click}
  onkeydown={key}
>
  <span class="tile" aria-hidden="true"><Icon name={ICONS[info.action]} size={20} /></span>
  <div class="text">
    <div class="title">{info.title}</div>
    <div class="where" title={where}>{where}</div>
    <div class="body">{info.body}</div>
  </div>
  <button class="primary" aria-label={`${FIX_LABELS[info.action]} ${row.filename}`} onclick={(e) => act(e, "fix")}>{FIX_LABELS[info.action]}</button>
  <button class="plain" aria-label={`Remove ${row.filename}`} onclick={(e) => act(e, "remove")}>Remove</button>
</li>

<style>
  .card {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    padding: 14px 16px;
    border-radius: var(--radius-lg);
    background: var(--surface);
    cursor: pointer;
    list-style: none;
  }
  .card.selected {
    background: var(--selected);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  @media (forced-colors: active) {
    .card.selected {
      outline: 2px solid Highlight;
    }
  }
  .tile {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-lg);
    background: var(--error-tint);
    color: var(--error);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-size: 15px;
    font-weight: 600;
  }
  .where {
    overflow: hidden;
    color: var(--muted);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .body {
    margin-top: 4px;
    color: var(--muted);
  }
  button {
    flex: none;
  }
  .plain {
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }
</style>
