<script lang="ts">
  import { untrack } from "svelte";

  /**
   * A number with its unit. A valid value applies shortly after typing stops (or on leaving the
   * field); an invalid one shows its message underneath and is not applied.
   */
  let {
    value,
    label,
    unit = "",
    step = 1,
    describedby,
    check,
    onvalid,
  }: {
    value: number;
    label: string;
    unit?: string;
    step?: number;
    describedby?: string;
    check: (v: number | null) => string | null;
    onvalid: (v: number) => void;
  } = $props();

  let draft = $state<number | null>(null);
  let focused = $state(false);
  let error = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const errId = `num-err-${Math.random().toString(36).slice(2)}`;

  // Follow outside changes (restore defaults, a clamped save) while the field isn't being edited.
  $effect(() => {
    const v = value;
    if (!untrack(() => focused)) {
      draft = v;
      error = null;
    }
  });

  function apply(): void {
    clearTimeout(timer);
    timer = undefined;
    if (error === null && typeof draft === "number" && draft !== value) onvalid(draft);
  }

  function input(): void {
    error = check(typeof draft === "number" && Number.isFinite(draft) ? draft : null);
    clearTimeout(timer);
    if (error === null) timer = setTimeout(apply, 400);
  }

  function blur(): void {
    focused = false;
    apply();
  }
</script>

<span class="num">
  <input
    type="number"
    {step}
    aria-label={label}
    aria-describedby={[describedby, error ? errId : null].filter(Boolean).join(" ") || undefined}
    aria-invalid={error !== null}
    bind:value={draft}
    oninput={input}
    onfocus={() => (focused = true)}
    onblur={blur}
    onkeydown={(e) => e.key === "Enter" && apply()}
  />
  {#if unit}<span class="unit">{unit}</span>{/if}
</span>
{#if error}<p class="field-error" id={errId} role="alert">{error}</p>{/if}

<style>
  .num {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }
  input {
    width: 84px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  input[aria-invalid="true"] {
    border-color: var(--error);
  }
  .unit {
    color: var(--muted);
    min-width: 28px;
  }
  .field-error {
    flex-basis: 100%;
    margin: 6px 0 0;
    color: var(--error);
    font-size: 12.5px;
    text-align: right;
  }
</style>
