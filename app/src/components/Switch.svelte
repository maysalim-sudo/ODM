<script lang="ts">
  /** An on/off switch with the extension popup's look: orange track when on, muted knob when off. */
  let {
    checked,
    label,
    describedby,
    disabled = false,
    onchange,
  }: {
    checked: boolean;
    /** Read by screen readers; the visible label sits in the row. */
    label: string;
    describedby?: string;
    disabled?: boolean;
    onchange: (checked: boolean) => void;
  } = $props();
</script>

<label class="switch">
  <input
    type="checkbox"
    role="switch"
    aria-label={label}
    aria-describedby={describedby}
    {checked}
    {disabled}
    onchange={(e) => onchange(e.currentTarget.checked)}
  />
  <span class="track" aria-hidden="true"></span>
</label>

<style>
  .switch {
    position: relative;
    display: inline-flex;
    flex: none;
    cursor: pointer;
  }
  .switch:has(input:disabled) {
    cursor: default;
  }
  input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    opacity: 0;
    cursor: inherit;
  }
  .track {
    position: relative;
    width: 36px;
    height: 20px;
    border-radius: 999px;
    background: var(--surface-2);
    transition: background-color 160ms var(--ease-out);
  }
  .track::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--muted);
    transition: transform 160ms var(--ease-out);
  }
  input:checked + .track {
    background: var(--accent);
  }
  input:checked + .track::after {
    background: var(--on-accent);
    transform: translateX(16px);
  }
  input:focus-visible + .track {
    outline: 2px solid var(--accent-light);
    outline-offset: 2px;
  }
  input:disabled + .track {
    opacity: 0.5;
  }
  @media (prefers-reduced-motion: reduce) {
    .track::after {
      transition: none;
    }
  }
  @media (forced-colors: active) {
    .track {
      border: 1px solid ButtonText;
    }
  }
</style>
