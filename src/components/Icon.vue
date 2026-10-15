<script setup lang="ts">
withDefaults(defineProps<{ name: string; size?: number }>(), { size: 20 })
const paths: Record<string, string[]> = {
  translate: ['m5 8 6 0', 'M8 5v3c0 5-3 8-5 9', 'M4 12c1 2 3 4 6 5', 'm13 13 4-9 4 9', 'M14 10h6'],
  spark: ['m12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5L12 3Z', 'M20 2v4', 'M18 4h4'],
  settings: ['M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z', 'm9 3-1 3-3 1-2 3 2 2v3l3 2 1 4h6l1-4 3-2v-3l2-2-2-3-3-1-1-3H9Z'],
  clipboard: ['M9 4H6a2 2 0 0 0-2 2v14h16V6a2 2 0 0 0-2-2h-3', 'M9 2h6v5H9Z', 'M8 12h8', 'M8 16h5'],
  arrow: ['M4 12h16', 'm14 6 6 6-6 6'],
  copy: ['M9 9h11v11H9Z', 'M15 5V3H3v12h2'],
  check: ['m5 12 4 4L20 5'],
  close: ['m6 6 12 12', 'M6 18 18 6'],
  book: ['M4 4h6a3 3 0 0 1 3 3v14a4 4 0 0 0-4-2H4V4Z', 'M13 7a3 3 0 0 1 3-3h5v15h-4a4 4 0 0 0-4 2'],
  shield: ['m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6l8-3Z', 'm8 11 3 3 5-6'],
  keyboard: ['M3 5h18v14H3Z', 'M7 9h.01M11 9h.01M15 9h.01M18 9h.01M7 12h.01M11 12h.01M15 12h.01M18 12h.01', 'M7 16h10'],
  sun: ['M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z', 'M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5 19 19M5 19l1.5-1.5M17.5 6.5 19 5'],
  popup: ['M14 3h7v7', 'm21 3-9 9', 'M10 3H3v18h18v-7'],
  power: ['M12 2v10', 'M6 5a9 9 0 1 0 12 0'],
  info: ['M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Z', 'M12 11v6', 'M12 7h.01'],
}
</script>

<template>
  <svg :width="size" :height="size" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path v-for="(path, index) in paths[name] ?? paths.info" :key="index" :d="path" /></svg>
</template>
