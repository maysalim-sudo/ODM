<script lang="ts">
  import { onMount, tick } from "svelte";
  import { ACTION_LABELS, type RowAction } from "../lib/home";
  import Icon from "./Icon.svelte";

  let {
    actions,
    name,
    onchoose,
  }: { actions: RowAction[]; name: string; onchoose: (a: RowAction) => void } = $props();

  const ICONS: Partial<Record<RowAction, string>> = {
    details: "cards",
    "open-folder": "folder",
    "copy-link": "copy",
    "new-link": "refresh",
    cancel: "x",
    remove: "trash",
  };
  const MENU_W = 220;

  let open = $state(false);
  let more = $state<HTMLButtonElement>();
  let menu = $state<HTMLDivElement>();
  let pos = $state({ top: 0, left: 0 });

  const items = () => [...(menu?.querySelectorAll<HTMLButtonElement>("button") ?? [])];

  async function toggle(e: MouseEvent) {
    e.stopPropagation();
    if (open) return close();
    open = true;
    await tick();
    place();
    items()[0]?.focus();
  }

  // Fixed, so the list's scrolling can't clip it. It opens upward when it wouldn't fit below.
  function place() {
    if (!more || !menu) return;
    const b = more.getBoundingClientRect();
    const h = menu.offsetHeight;
    pos = {
      top: b.bottom + 4 + h > innerHeight ? Math.max(4, b.top - h - 4) : b.bottom + 4,
      left: Math.max(4, b.right - MENU_W),
    };
  }

  function close(refocus = true) {
    open = false;
    if (refocus) more?.focus();
  }

  function choose(e: MouseEvent, a: RowAction) {
    e.stopPropagation();
    close(false);
    onchoose(a);
  }

  function onkey(e: KeyboardEvent) {
    e.stopPropagation();
    const list = items();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    let next: number | null = null;
    if (e.key === "ArrowDown") next = (i + 1) % list.length;
    else if (e.key === "ArrowUp") next = (i - 1 + list.length) % list.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = list.length - 1;
    else if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      close();
    }
    if (next === null) return;
    e.preventDefault();
    list[next]?.focus();
  }

  onMount(() => {
    const away = (e: PointerEvent) => {
      if (open && !menu?.contains(e.target as Node) && !more?.contains(e.target as Node)) close(false);
    };
    window.addEventListener("pointerdown", away);
    return () => window.removeEventListener("pointerdown", away);
  });
</script>

<button
  bind:this={more}
  class="more"
  aria-haspopup="menu"
  aria-expanded={open}
  aria-label={`More actions for ${name}`}
  title="More actions"
  onclick={toggle}
>
  <Icon name="more" />
</button>
{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={menu}
    class="menu"
    role="menu"
    tabindex="-1"
    aria-label={`Actions for ${name}`}
    style:top={`${pos.top}px`}
    style:left={`${pos.left}px`}
    style:width={`${MENU_W}px`}
    onkeydown={onkey}
  >
    {#each actions as a (a)}
      <button role="menuitem" tabindex="-1" class:danger={a === "remove"} onclick={(e) => choose(e, a)}>
        <Icon name={ICONS[a] ?? "more"} />
        {ACTION_LABELS[a]}
      </button>
    {/each}
  </div>
{/if}

<style>
  .more {
    display: grid;
    place-items: center;
    flex: none;
    width: 32px;
    height: 32px;
    padding: 0;
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }
  .menu {
    position: fixed;
    z-index: 30;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-dialog);
    cursor: default;
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 10px;
    border-color: transparent;
    background: transparent;
    font-weight: 400;
    text-align: left;
    transition: none;
  }
  .menu button:active:not(:disabled) {
    transform: none;
  }
  .menu button:hover:not(:disabled),
  .menu button:focus-visible {
    border-color: transparent;
    background: var(--surface-2);
  }
  .danger {
    color: var(--error);
  }
</style>
