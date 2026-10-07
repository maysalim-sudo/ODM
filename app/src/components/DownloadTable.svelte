<script lang="ts">
  import { downloads } from "../lib/downloads.svelte";
  import { formatBytes, formatDate, formatEta, formatSpeed, stateLabel } from "../lib/format";
  import { progressPct, type SortKey } from "../lib/model";
  import ProgressBar from "./ProgressBar.svelte";

  let { onopen }: { onopen: (id: string) => void } = $props();

  const columns: { key: SortKey | null; label: string; cls: string }[] = [
    { key: "filename", label: "Name", cls: "name" },
    { key: "size", label: "Size", cls: "num" },
    { key: "progress", label: "Progress", cls: "progress" },
    { key: "speed_bps", label: "Speed", cls: "num c-speed" },
    { key: null, label: "Time left", cls: "num c-eta" },
    { key: "state", label: "Status", cls: "status" },
    { key: "created_at", label: "Added", cls: "num added c-added" },
  ];

  function click(e: MouseEvent, id: string) {
    downloads.select(id, e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "one");
  }

  function key(e: KeyboardEvent, id: string) {
    if (e.key === "Enter") onopen(id);
    if (e.key === " ") {
      e.preventDefault();
      downloads.select(id, "toggle");
    }
  }
</script>

<div class="wrap">
  {#if downloads.visible.length === 0}
    <p class="empty">
      {downloads.rows.size === 0 ? "No downloads yet. Choose Add download to start one." : "Nothing matches this filter."}
    </p>
  {:else}
    <table role="grid">
      <thead>
        <tr>
          {#each columns as c (c.label)}
            <th class={c.cls} aria-sort={c.key === downloads.sortKey ? (downloads.sortDir === "asc" ? "ascending" : "descending") : undefined}>
              {#if c.key}
                {@const k = c.key}
                <button onclick={() => downloads.toggleSort(k)}>
                  {c.label}{downloads.sortKey === k ? (downloads.sortDir === "asc" ? " ↑" : " ↓") : ""}
                </button>
              {:else}
                {c.label}
              {/if}
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each downloads.visible as row (row.id)}
          {@const pct = row.segments || row.size ? progressPct(row) : null}
          <tr
            class:selected={downloads.selected.has(row.id)}
            tabindex="0"
            aria-selected={downloads.selected.has(row.id)}
            onclick={(e) => click(e, row.id)}
            ondblclick={() => onopen(row.id)}
            onkeydown={(e) => key(e, row.id)}
          >
            <td class="name" title={row.filename}>{row.filename}</td>
            <td class="num">{row.size === null ? "Unknown" : formatBytes(row.size)}</td>
            <td class="progress">
              <div class="progress-cell">
                <ProgressBar value={pct} state={row.state} />
                <span>{pct === null ? formatBytes(row.bytes) : `${pct.toFixed(pct < 100 ? 1 : 0)}%`}</span>
              </div>
            </td>
            <td class="num c-speed">{row.state === "downloading" ? formatSpeed(row.speed_bps) : "—"}</td>
            <td class="num c-eta">{row.state === "downloading" ? formatEta(row.eta_s) : "—"}</td>
            <td class="status" class:bad={row.state === "failed"} title={row.error_message ?? ""}>
              {stateLabel(row.state)}
            </td>
            <td class="num added c-added">{formatDate(row.created_at)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .wrap {
    flex: 1;
    min-width: 0;
    overflow: auto;
    container-type: inline-size;
  }
  .empty {
    margin: 80px auto;
    text-align: center;
    color: var(--muted);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
    min-width: 616px;
  }
  th {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 0;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-weight: 700;
    text-align: left;
  }
  th button {
    width: 100%;
    padding: 8px;
    border: none;
    border-radius: 0;
    background: none;
    text-align: inherit;
    color: inherit;
  }
  /* Sorting is a frequent click: no press motion. */
  th button {
    font-weight: inherit;
    transition: none;
  }
  th button:active:not(:disabled) {
    transform: none;
  }
  th:not(:has(button)) {
    padding: 8px;
  }
  td {
    padding: 7px 8px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  tr.selected td {
    background: var(--selected);
  }
  /* An edge marker, so selection does not rest on a tint that is darker than hover. */
  tr.selected td:first-child {
    box-shadow: inset 3px 0 0 var(--accent);
  }
  tbody tr:hover td {
    background: var(--surface-2);
  }
  tbody tr.selected:hover td {
    background: var(--selected-hover);
  }
  tbody tr:focus-visible {
    outline: 2px solid var(--accent-light);
    outline-offset: -2px;
  }
  .name {
    width: auto;
  }
  .num.added {
    width: 132px;
  }
  /*
   * Column widths and breakpoints, mirrored from COL / visibleColumns in lib/model.ts:
   * always shown 96 + 190 + 110 = 396; plus name 220 = 616 (table min-width);
   * speed +96 = 712, time left +96 = 808, added +132 = 940. The hide rules are max-width = threshold - 1.
   */
  @container (max-width: 939px) {
    .c-added {
      display: none;
    }
  }
  @container (max-width: 807px) {
    .c-eta {
      display: none;
    }
  }
  @container (max-width: 711px) {
    .c-speed {
      display: none;
    }
  }
  .num {
    width: 96px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .status {
    width: 110px;
  }
  .status.bad {
    color: var(--error);
  }
  .progress {
    width: 190px;
  }
  .progress-cell {
    display: grid;
    grid-template-columns: 1fr 52px;
    align-items: center;
    gap: 8px;
  }
  .progress-cell span {
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }
</style>
