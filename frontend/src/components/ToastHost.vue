<script setup lang="ts">
import { useToast } from '@/composables/useToast'

const { items, dismiss } = useToast()
</script>

<template>
  <TransitionGroup
    tag="div"
    class="pointer-events-none fixed inset-x-0 bottom-[max(1.5rem,env(safe-area-inset-bottom))] z-70 flex flex-col items-center gap-2 px-4"
    name="toast"
  >
    <button
      v-for="item in items"
      :key="item.id"
      type="button"
      class="pointer-events-auto max-w-[min(30rem,92vw)] rounded-full border px-4 py-2.5 text-sm backdrop-blur-md transition"
      :class="{
        'border-ember-500/45 bg-ink-900/90 text-ember-400': item.tone === 'error',
        'border-glow-600/40 bg-ink-900/90 text-glow-300': item.tone === 'success',
        'border-ink-600/40 bg-ink-900/90 text-ink-200': item.tone === 'info',
      }"
      @click="dismiss(item.id)"
    >
      {{ item.message }}
    </button>
  </TransitionGroup>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition:
    opacity 0.45s var(--ease-out-soft),
    transform 0.45s var(--ease-out-soft);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translate3d(0, 12px, 0) scale(0.97);
}
</style>
