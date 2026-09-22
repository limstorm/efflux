<script setup lang="ts">
// 演示按钮 + 向上弹出的两种模式。摆在首页右下角的工具条里。
//
// 演示一旦跑起来，这枚按钮就退场（出口交给右下角的叉号，见 DemoExit），
// 所以这里不必再管「进行中」的样子。
import { onBeforeUnmount, onMounted, ref } from 'vue'

import { type DemoMode, useDemo } from '@/composables/useDemo'

const emit = defineEmits<{ start: [mode: DemoMode] }>()

const demo = useDemo()
const open = ref(false)
const root = ref<HTMLElement | null>(null)

const modes: { key: DemoMode; label: string }[] = [
  { key: 'sequence', label: '顺序演示' },
  { key: 'random', label: '随机演示' },
]

function toggle() {
  open.value = !open.value
}

function pick(mode: DemoMode) {
  open.value = false
  emit('start', mode)
}

// 点到别处、按 Esc 都收起来
function onDocumentPointer(event: PointerEvent) {
  if (open.value && !root.value?.contains(event.target as Node)) open.value = false
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && open.value) open.value = false
}

onMounted(() => {
  document.addEventListener('pointerdown', onDocumentPointer)
  window.addEventListener('keydown', onKeydown)
})

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onDocumentPointer)
  window.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <div ref="root" class="demo-menu">
    <Transition name="demo-pop">
      <div v-if="open" class="demo-pop" role="menu" aria-label="演示方式">
        <button
          v-for="item in modes"
          :key="item.key"
          type="button"
          class="demo-pop__item"
          role="menuitem"
          @click="pick(item.key)"
        >
          {{ item.label }}
        </button>
      </div>
    </Transition>

    <button
      v-if="!demo.running.value"
      type="button"
      class="demo-btn tip"
      aria-label="演示"
      :data-tip="open ? null : '演示：顺序或随机走一遍'"
      aria-haspopup="menu"
      :aria-expanded="open"
      @click="toggle"
    >
      <!-- 一块幕布、中间的播放、下面的支架：放映的样子，不是播放器 -->
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" class="h-4 w-4">
        <rect x="3" y="4.6" width="18" height="12.4" rx="1.8" />
        <path d="M10.6 8.2v5.2l4.5-2.6Z" fill="currentColor" stroke="none" />
        <path d="M12 17v2.8M8.6 21.4h6.8" stroke-linecap="round" />
      </svg>
    </button>
  </div>
</template>

<style scoped>
.demo-menu {
  position: relative;
  display: inline-flex;
  align-items: center;
  pointer-events: auto;
}

/* ---------------- 上弹的两种模式 ---------------- */
.demo-pop {
  position: absolute;
  right: -0.2rem;
  bottom: calc(100% + 0.7rem);
  z-index: 30;
  width: 10.5rem;
  padding: 0.35rem;
  border-radius: 14px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 55%, transparent);
  background: color-mix(in oklab, var(--color-ink-850) 97%, transparent);
  box-shadow: 0 24px 48px -32px color-mix(in oklab, var(--color-shade) 92%, transparent);
}

.demo-pop__item {
  display: block;
  width: 100%;
  padding: 0.46rem 0.7rem;
  border: none;
  border-radius: 10px;
  background: none;
  text-align: left;
  font-size: 0.84rem;
  letter-spacing: 0.06em;
  color: var(--color-ink-100);
  transition: background-color 0.25s var(--ease-out-soft);
}

.demo-pop__item:hover,
.demo-pop__item:focus-visible {
  background: color-mix(in oklab, var(--color-glow-700) 46%, transparent);
  color: var(--color-glow-300);
}

/* 进出：入场稍慢、退场更快（约四分之三） */
.demo-pop-enter-active {
  transition:
    opacity 0.18s var(--ease-out-soft),
    transform 0.24s var(--ease-out-soft);
}

.demo-pop-leave-active {
  transition:
    opacity 0.13s ease-in,
    transform 0.13s ease-in;
}

.demo-pop-enter-from,
.demo-pop-leave-to {
  opacity: 0;
  transform: translateY(6px) scale(0.985);
}

/* ---------------- 按钮 ----------------
   组件里的按钮拿不到父页面 scoped 的 .hud-icon（样式不出组件边界），
   所以自己带一圈：大小、描边、底色与页面上那排图标一致。 */
.demo-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.2rem;
  height: 2.2rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 55%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 72%, transparent);
  color: var(--color-ink-200);
  opacity: 0.62;
  backdrop-filter: blur(12px);
  box-shadow: 0 8px 22px -16px color-mix(in oklab, var(--color-shade) 90%, transparent);
  transition:
    opacity 0.5s var(--ease-out-soft),
    border-color 0.45s var(--ease-out-soft),
    color 0.45s var(--ease-out-soft),
    background-color 0.45s var(--ease-out-soft),
    transform 0.3s var(--ease-out-soft);
}

.demo-btn:hover,
.demo-btn:focus-visible {
  opacity: 1;
  border-color: color-mix(in oklab, var(--color-glow-500) 66%, transparent);
  color: var(--color-glow-300);
  background: var(--color-ink-900);
}

.demo-btn:active {
  transform: scale(0.94);
}

@media (max-width: 760px) {
  .demo-btn {
    width: 2.1rem;
    height: 2.1rem;
  }
}
</style>
