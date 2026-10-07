<script lang="ts">
  import type { DownloadState } from "../lib/types";

  let { value, state }: { value: number | null; state: DownloadState } = $props();

  const running = $derived(state === "downloading" || state === "probing" || state === "merging");
</script>

<div
  class="bar"
  class:error={state === "failed"}
  class:done={state === "completed"}
  class:idle={state === "paused" || state === "cancelled"}
  class:queued={state === "queued"}
  class:indeterminate={value === null && running}
  role="progressbar"
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={value === null ? undefined : Math.round(value)}
>
  <div class="fill" style:width={value === null ? (state === "completed" ? "100%" : "0%") : `${value}%`}></div>
</div>

<style>
  .bar {
    height: 6px;
    background: var(--surface-2);
    border-radius: 3px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 200ms linear;
  }
  .done .fill {
    background: var(--success);
  }
  .idle .fill {
    background: var(--idle);
  }
  .queued .fill {
    background: var(--info);
  }
  .error .fill {
    background: var(--error);
  }
  .indeterminate .fill {
    width: 35% !important;
    animation: slide 1.2s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(290%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fill {
      transition: none;
    }
    .indeterminate .fill {
      animation: none;
      width: 100% !important;
      opacity: 0.4;
    }
  }
</style>
