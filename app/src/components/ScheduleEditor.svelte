<script lang="ts">
  import { WEEKDAYS } from "../lib/settingsPage";
  import type { Schedule } from "../lib/types";

  /** Days of the week as chips, then the two times. The on/off switch lives in the row. */
  let {
    schedule,
    fromLabel,
    toLabel,
    onchange,
  }: {
    schedule: Schedule;
    fromLabel: string;
    toLabel: string;
    onchange: (s: Schedule) => void;
  } = $props();

  let error = $state<string | null>(null);

  function toggleDay(i: number): void {
    const days = schedule.days.map((d, j) => (j === i ? !d : d));
    if (!days.some(Boolean)) {
      error = "Keep at least one day.";
      return;
    }
    error = null;
    onchange({ ...schedule, days });
  }

  function setTime(which: "from" | "to", v: string): void {
    if (!/^\d\d:\d\d$/.test(v)) {
      error = "Enter a time like 22:00.";
      return;
    }
    error = null;
    onchange({ ...schedule, [which]: v });
  }
</script>

<div class="sched">
  <div class="days" role="group" aria-label="Days">
    {#each WEEKDAYS as d, i (d)}
      <button type="button" class="day" class:on={schedule.days[i]} aria-pressed={schedule.days[i]} onclick={() => toggleDay(i)}>{d}</button>
    {/each}
  </div>
  <div class="times">
    <label>
      <span>{fromLabel}</span>
      <input type="time" value={schedule.from} onchange={(e) => setTime("from", e.currentTarget.value)} />
    </label>
    <label>
      <span>{toLabel}</span>
      <input type="time" value={schedule.to} onchange={(e) => setTime("to", e.currentTarget.value)} />
    </label>
  </div>
  {#if error}<p class="field-error" role="alert">{error}</p>{/if}
</div>

<style>
  .sched {
    display: grid;
    gap: 10px;
  }
  .days {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .day {
    min-width: 44px;
    padding: 4px 8px;
    border-radius: 999px;
    font-size: 12.5px;
    color: var(--muted);
    background: var(--bg);
  }
  .day.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
  }
  .times {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }
  .times label {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }
  .field-error {
    margin: 0;
    color: var(--error);
    font-size: 12.5px;
  }
</style>
