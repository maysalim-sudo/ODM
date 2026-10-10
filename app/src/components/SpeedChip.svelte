<script lang="ts">
  import { onMount, tick } from "svelte";
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { formatSpeed } from "../lib/format";
  import { formatLimit, isActive, speedPresets, totalSpeed } from "../lib/model";
  import { checkSpeed, fromKbps, springEase, toKbps, type SpeedUnit } from "../lib/settingsPage";
  import { speedLimit } from "../lib/speedLimit.svelte";
  import Segmented from "./Segmented.svelte";

  const OPEN_MS = 320;

  let open = $state(false);
  let custom = $state(false);
  let chip = $state<HTMLButtonElement>();
  let pop = $state<HTMLDivElement>();
  let field = $state<HTMLInputElement>();
  let draft = $state<number | null>(null);
  let unit = $state<SpeedUnit>("mb");
  let error = $state<string | null>(null);

  const speed = $derived(totalSpeed(downloads.rows));
  const limited = $derived(speedLimit.kbps > 0);
  const items = $derived(speedPresets(speedLimit.kbps));
  const anyActive = $derived([...downloads.rows.values()].some((r) => isActive(r.state)));
  const allLabel = $derived(anyActive ? "Pause all" : "Resume all");
  const chipText = $derived(speed > 0 ? formatSpeed(speed) : "0 B/s");
  // The dot means a limit is set. With a schedule on, the chip cannot cheaply know whether the
  // window is open right now, so the tooltip says the limit is scheduled.
  const title = $derived(
    limited ? `Speed limit: ${formatLimit(speedLimit.kbps)}${speedLimit.scheduled ? " (scheduled)" : ""}` : "No speed limit",
  );

  onMount(() => {
    let off: (() => void) | undefined;
    let gone = false;
    void speedLimit.init().then((un) => (gone ? un() : (off = un)));
    return () => {
      gone = true;
      off?.();
    };
  });

  const buttons = () => [...(pop?.querySelectorAll<HTMLButtonElement>('[role^="menuitem"]') ?? [])];

  async function show(): Promise<void> {
    custom = false;
    error = null;
    open = true;
    void speedLimit.refresh();
    await tick();
    (buttons().find((b) => b.getAttribute("aria-checked") === "true") ?? buttons()[0])?.focus();
  }

  function close(refocus = true): void {
    open = false;
    custom = false;
    if (refocus) chip?.focus();
  }

  function toggle(): void {
    if (open) close();
    else void show();
  }

  async function choose(kbps: number): Promise<void> {
    close();
    await speedLimit.set(kbps);
    if (speedLimit.error) downloads.error = speedLimit.error;
  }

  async function openCustom(): Promise<void> {
    const f = fromKbps(speedLimit.kbps > 0 ? speedLimit.kbps : 1024);
    unit = f.unit;
    draft = f.value;
    error = null;
    custom = true;
    await tick();
    field?.focus();
    field?.select();
  }

  async function applyCustom(): Promise<void> {
    error = checkSpeed(draft, unit);
    if (error || draft === null) return;
    await choose(toKbps(draft, unit));
  }

  async function pauseOrResume(): Promise<void> {
    const pause = anyActive;
    close();
    await downloads.run(() => (pause ? api.pauseAll() : api.resumeAll()));
  }

  function onmenukey(e: KeyboardEvent): void {
    const list = buttons();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    let next: number | null = null;
    if (e.key === "ArrowDown") next = (i + 1) % list.length;
    else if (e.key === "ArrowUp") next = (i - 1 + list.length) % list.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = list.length - 1;
    if (next === null) return;
    e.preventDefault();
    list[next]?.focus();
  }

  function onwindowkey(e: KeyboardEvent): void {
    if (open && e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }

  function onpointerdown(e: PointerEvent): void {
    if (!open) return;
    const t = e.target as Node;
    if (!pop?.contains(t) && !chip?.contains(t)) close(false);
  }

  /** Grows from the chip's top-right corner with a critically damped spring. Reduced motion: a fade. */
  function grow(_node: Element) {
    const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
    return {
      duration: OPEN_MS,
      easing: springEase,
      css: (t: number) => (reduce ? `opacity: ${t}` : `opacity: ${t}; transform: scale(${0.9 + 0.1 * t})`),
    };
  }
</script>

<svelte:window onkeydown={onwindowkey} onpointerdown={onpointerdown} />

<span class="wrap">
  <button
    bind:this={chip}
    class="chip"
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label={`Speed ${chipText}. ${title}`}
    {title}
    onclick={toggle}
  >
    <span class="speed">{chipText}</span>
    {#if limited}<span class="dot" aria-hidden="true"></span>{/if}
  </button>
  {#if open}
    <div class="pop" bind:this={pop} transition:grow>
      {#if !custom}
        <div role="menu" aria-label="Speed limit" tabindex="-1" onkeydown={onmenukey}>
          {#each items as p (p.id)}
            <button
              role="menuitemradio"
              aria-checked={p.checked}
              tabindex="-1"
              class="item"
              onclick={() => (p.kbps === null ? void openCustom() : void choose(p.kbps))}
            >
              <span class="mark" aria-hidden="true">{p.checked ? "✓" : ""}</span>
              <span>{p.label}</span>
              {#if p.detail}<span class="detail">{p.detail}</span>{/if}
            </button>
          {/each}
          <hr />
          <button role="menuitem" tabindex="-1" class="item" onclick={pauseOrResume}>
            <span class="mark" aria-hidden="true"></span>
            <span>{allLabel}</span>
          </button>
        </div>
      {:else}
        <form
          class="custom"
          role="group"
          aria-label="Custom speed limit"
          onsubmit={(e) => {
            e.preventDefault();
            void applyCustom();
          }}
        >
          <label for="chip-custom">Limit all downloads to</label>
          <div class="row">
            <input
              id="chip-custom"
              bind:this={field}
              type="number"
              min="0"
              step={unit === "mb" ? 0.1 : 1}
              bind:value={draft}
              aria-invalid={error !== null}
              aria-describedby={error ? "chip-custom-err" : undefined}
              oninput={() => (error = null)}
            />
            <Segmented
              label="Unit"
              value={unit}
              options={[
                { value: "kb", label: "KB/s" },
                { value: "mb", label: "MB/s" },
              ]}
              onchange={(v) => (unit = v)}
            />
          </div>
          {#if error}<p class="field-error" id="chip-custom-err" role="alert">{error}</p>{/if}
          <div class="actions">
            <button type="button" onclick={() => close()}>Cancel</button>
            <button type="submit" class="primary">Apply</button>
          </div>
        </form>
      {/if}
    </div>
  {/if}
</span>

<style>
  .wrap {
    position: relative;
    display: inline-flex;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-variant-numeric: tabular-nums;
    min-width: 92px;
    justify-content: center;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }
  .pop {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 20;
    min-width: 200px;
    padding: 4px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-dialog);
    transform-origin: top right;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border-color: transparent;
    background: transparent;
    text-align: left;
  }
  .item:hover:not(:disabled),
  .item:focus-visible {
    background: var(--surface);
  }
  .mark {
    width: 14px;
    color: var(--accent);
    flex: none;
  }
  .detail {
    margin-left: auto;
    color: var(--muted);
  }
  hr {
    border: 0;
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }
  .custom {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px;
    min-width: 260px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  input {
    width: 90px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  input[aria-invalid="true"] {
    border-color: var(--error);
  }
  .field-error {
    margin: 0;
    color: var(--error);
    font-size: 12.5px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
</style>
