<script lang="ts">
  import { splitTags } from "../lib/settingsPage";

  /** A list of short entries shown as removable chips. Enter, comma or leaving the field adds what was typed. */
  let {
    values,
    label,
    placeholder = "",
    describedby,
    check,
    onchange,
  }: {
    values: string[];
    label: string;
    placeholder?: string;
    describedby?: string;
    /** A message for an entry that can't be added, or null. */
    check: (entry: string) => string | null;
    onchange: (values: string[]) => void;
  } = $props();

  let draft = $state("");
  let error = $state<string | null>(null);
  const errId = `tag-err-${Math.random().toString(36).slice(2)}`;

  function add(): void {
    const entries = splitTags(draft);
    if (entries.length === 0) {
      error = null;
      return;
    }
    const bad = entries.map(check).find((m) => m !== null) ?? null;
    if (bad) {
      error = bad;
      return;
    }
    error = null;
    draft = "";
    const next = [...values];
    for (const e of entries) if (!next.includes(e)) next.push(e);
    if (next.length !== values.length) onchange(next);
  }

  function keydown(e: KeyboardEvent): void {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      add();
    } else if (e.key === "Backspace" && draft === "" && values.length > 0) {
      onchange(values.slice(0, -1));
    }
  }
</script>

<div class="tags" class:invalid={error !== null}>
  {#each values as v (v)}
    <span class="tag">
      <span>{v}</span>
      <button type="button" aria-label={`Remove ${v}`} onclick={() => onchange(values.filter((x) => x !== v))}>×</button>
    </span>
  {/each}
  <input
    aria-label={label}
    aria-describedby={[describedby, error ? errId : null].filter(Boolean).join(" ") || undefined}
    aria-invalid={error !== null}
    spellcheck="false"
    {placeholder}
    bind:value={draft}
    onkeydown={keydown}
    onblur={add}
    oninput={() => (error = null)}
  />
</div>
{#if error}<p class="field-error" id={errId} role="alert">{error}</p>{/if}

<style>
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 6px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .tags:focus-within {
    outline: 2px solid var(--accent-light);
    outline-offset: 1px;
  }
  .tags.invalid {
    border-color: var(--error);
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 1px 2px 1px 8px;
    background: var(--surface-2);
    border-radius: 999px;
    font-size: 12.5px;
    user-select: text;
  }
  .tag button {
    padding: 0 6px;
    border: none;
    background: transparent;
    color: var(--muted);
    line-height: 1.2;
  }
  input {
    flex: 1;
    min-width: 120px;
    padding: 2px 4px;
    border: none;
    background: transparent;
  }
  input:focus {
    outline: none;
  }
  .field-error {
    margin: 6px 0 0;
    color: var(--error);
    font-size: 12.5px;
  }
</style>
