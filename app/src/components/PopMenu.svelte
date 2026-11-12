<script lang="ts">
  import { onMount, tick } from "svelte";
  import { SEL_LABELS, type SelAction } from "../lib/selection";
  import Icon from "./Icon.svelte";

  /** A menu of selection actions at a point: the right-click menu, and the action bar's dropdowns. */
  let {
    items,
    enabled,
    x,
    y,
    label,
    anchor,
    onchoose,
    onclose,
  }: {
    items: (SelAction | "sep")[];
    enabled: (a: SelAction) => boolean;
    /** Where it opens, in window pixels; it moves to stay on screen. */
    x: number;
    y: number;
    label: string;
    /** The button that opened it: a press there is left to that button, which toggles the menu. */
    anchor?: HTMLElement;
    onchoose: (a: SelAction) => void;
    onclose: () => void;
  } = $props();

  const ICONS: Partial<Record<SelAction, string>> = {
    open: "cards",
    "open-folder": "folder",
    "move-bottom": "down",
    low: "gauge",
    normal: "gauge",
    "speed-unlimited": "gauge",
    "speed-low": "gauge",
    "speed-custom": "gauge",
    "select-all": "list",
    resume: "play",
    "start-now": "play",
    pause: "pause",
    retry: "refresh",
    "move-top": "top",
    "move-up": "up",
    "move-down": "down",
    "copy-link": "copy",
    "copy-name": "copy",
    "copy-path": "copy",
    "new-link": "refresh",
    again: "download",
    stop: "x",
    remove: "trash",
    delete: "trash",
  };
  const WIDTH = 230;

  let menu = $state<HTMLDivElement>();
  let pos = $state({ top: 0, left: 0 });
  const buttons = () => [...(menu?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];

  onMount(() => {
    void tick().then(() => {
      if (!menu) return;
      const h = menu.offsetHeight;
      pos = {
        top: y + h > innerHeight - 4 ? Math.max(4, y - h) : y,
        left: Math.min(Math.max(4, x), innerWidth - WIDTH - 4),
      };
      buttons()[0]?.focus();
    });
    const away = (e: PointerEvent) => {
      if (!menu?.contains(e.target as Node) && !anchor?.contains(e.target as Node)) onclose();
    };
    const gone = () => onclose();
    // Capture, so a click that opens another menu closes this one first.
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("blur", gone);
    window.addEventListener("resize", gone);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("blur", gone);
      window.removeEventListener("resize", gone);
    };
  });

  function onkey(e: KeyboardEvent) {
    e.stopPropagation();
    const list = buttons();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    let next: number | null = null;
    if (e.key === "ArrowDown") next = (i + 1) % list.length;
    else if (e.key === "ArrowUp") next = (i - 1 + list.length) % list.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = list.length - 1;
    else if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      onclose();
    }
    if (next === null) return;
    e.preventDefault();
    list[next]?.focus();
  }

  function choose(a: SelAction) {
    onclose();
    onchoose(a);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  bind:this={menu}
  class="menu"
  role="menu"
  tabindex="-1"
  aria-label={label}
  style:top={`${pos.top}px`}
  style:left={`${pos.left}px`}
  style:width={`${WIDTH}px`}
  onkeydown={onkey}
  oncontextmenu={(e) => e.preventDefault()}
>
  {#each items as a, i (`${a}-${i}`)}
    {#if a === "sep"}
      <hr />
    {:else}
      <button role="menuitem" tabindex="-1" class:danger={a === "delete" || a === "remove"} disabled={!enabled(a)} onclick={() => choose(a)}>
        <span class="icon"><Icon name={ICONS[a] ?? "more"} /></span>
        {SEL_LABELS[a]}
      </button>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 40;
    max-height: calc(100vh - 8px);
    overflow-y: auto;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-dialog);
    cursor: default;
  }
  button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 10px;
    border-color: transparent;
    background: transparent;
    font-weight: 400;
    text-align: left;
    transition: none;
  }
  button:active:not(:disabled) {
    transform: none;
  }
  button:hover:not(:disabled),
  button:focus-visible {
    border-color: transparent;
    background: var(--surface-2);
  }
  button:disabled {
    opacity: 0.4;
  }
  .icon {
    display: grid;
    width: 16px;
    color: var(--muted);
  }
  .danger,
  .danger .icon {
    color: var(--error);
  }
  hr {
    margin: 4px 6px;
    border: 0;
    border-top: 1px solid var(--border);
  }
</style>
