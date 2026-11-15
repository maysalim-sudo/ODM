<script lang="ts">
  /** ODM's logo, the sunset (the same drawing as app/app-icon.svg). */
  let { size = 32 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true">
  <rect width="1024" height="1024" rx="239" fill="#111111" />
  <path d="M199.7 734A341 341 0 1 1 824.3 734Z" fill="#ffa31a" />
  <g fill="#111111">
    <rect x="136" y="486" width="752" height="17" />
    <rect x="136" y="546" width="752" height="26" />
    <rect x="136" y="606" width="752" height="34" />
    <rect x="136" y="674" width="752" height="43" />
  </g>
  <path d="M136 751h752" stroke="#ffa31a" stroke-width="26" stroke-linecap="round" />
  <g fill="#ffa31a">
    <path d="M256 794h512l-60 42H316z" />
    <path d="M358 870h308l-51 35H410z" />
    <path d="M452 930h120l-60 43z" />
  </g>
</svg>

<style>
  svg {
    display: block;
    flex: none;
  }
</style>
