<script lang="ts" generics="T extends string">
  /** A segmented control for 2–5 choices: native radios, so arrow keys move between them. */
  let {
    value,
    options,
    label,
    describedby,
    onchange,
  }: {
    value: T;
    options: { value: T; label: string }[];
    label: string;
    describedby?: string;
    onchange: (value: T) => void;
  } = $props();

  const name = `seg-${Math.random().toString(36).slice(2)}`;
</script>

<div class="seg" role="radiogroup" aria-label={label} aria-describedby={describedby}>
  {#each options as o (o.value)}
    <label class:on={o.value === value}>
      <input type="radio" {name} value={o.value} checked={o.value === value} onchange={() => onchange(o.value)} />
      <span>{o.label}</span>
    </label>
  {/each}
</div>

<style>
  .seg {
    display: inline-flex;
    flex: none;
    padding: 2px;
    gap: 2px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  label {
    position: relative;
    padding: 4px 10px;
    border-radius: 6px;
    font-weight: 600;
    font-size: 12.5px;
    color: var(--muted);
    cursor: pointer;
    white-space: nowrap;
    transition:
      background-color 160ms var(--ease-out),
      color 160ms var(--ease-out);
  }
  label.on {
    background: var(--accent);
    color: var(--on-accent);
  }
  input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }
  label:has(input:focus-visible) {
    outline: 2px solid var(--accent-light);
    outline-offset: 1px;
  }
  @media (hover: hover) and (pointer: fine) {
    label:not(.on):hover {
      color: var(--text);
    }
  }
  @media (forced-colors: active) {
    label.on {
      outline: 2px solid Highlight;
    }
  }
</style>
