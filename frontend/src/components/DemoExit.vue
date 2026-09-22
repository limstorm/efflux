<script setup lang="ts">
// 演示进行中的出口：右下角一枚叉号，正好落在演示按钮原来的那一格。
// 首页与详情页都挂这一枚，位置由自己决定（固定右下角），所以放在页面根上即可。
import { useDemo } from '@/composables/useDemo'

const demo = useDemo()
</script>

<template>
  <Transition name="demo-out">
    <div v-if="demo.running.value" class="demo-exit">
      <span class="demo-exit__flag">
        <span class="demo-exit__dot" aria-hidden="true" />
        {{ demo.mode.value === 'random' ? '随机演示中' : '顺序演示中' }}
      </span>

      <button
        type="button"
        class="demo-exit__btn tip"
        data-tip="退出演示（Esc）"
        aria-label="退出演示"
        @click="demo.stop()"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" class="h-4 w-4">
          <path d="m7.4 7.4 9.2 9.2M16.6 7.4l-9.2 9.2" stroke-linecap="round" />
        </svg>
      </button>
    </div>
  </Transition>
</template>

<style scoped>
/* 与页面底部让出的边距一致：叉号正好接在演示按钮那一格上 */
.demo-exit {
  position: fixed;
  right: max(clamp(1.1rem, 3.2vw, 2.6rem), calc(var(--safe-right) + 0.6rem));
  bottom: max(1.35rem, calc(var(--safe-bottom) + 0.85rem));
  z-index: 26;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.5rem;
  pointer-events: none;
}

/* 一点呼吸的琥珀色，说明此刻是程序在带着走 */
.demo-exit__flag {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.22rem 0.6rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-glow-500) 40%, transparent);
  background: color-mix(in oklab, var(--color-glow-700) 30%, transparent);
  font-size: 0.7rem;
  letter-spacing: 0.05em;
  white-space: nowrap;
  color: var(--color-glow-300);
}

.demo-exit__dot {
  width: 0.34rem;
  height: 0.34rem;
  border-radius: 999px;
  background: var(--color-glow-400);
  animation: demo-breath 2.8s var(--ease-out-soft) infinite;
}

/* 与页面上那排图标同一套：一样大、一样的一圈 */
.demo-exit__btn {
  pointer-events: auto;
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

.demo-exit__btn:hover,
.demo-exit__btn:focus-visible {
  opacity: 1;
  border-color: color-mix(in oklab, var(--color-glow-500) 66%, transparent);
  color: var(--color-glow-300);
  background: var(--color-ink-900);
}

.demo-exit__btn:active {
  transform: scale(0.94);
}

/* 按钮上方还立着「顺序演示中」那面小旗，提示得再高一层，别糊在旗子上 */
.demo-exit__btn.tip[data-tip]::after {
  bottom: calc(100% + 2.6rem);
}

.demo-out-enter-active {
  transition:
    opacity 0.24s var(--ease-out-soft),
    transform 0.3s var(--ease-out-soft);
}

.demo-out-leave-active {
  transition:
    opacity 0.16s ease-in,
    transform 0.16s ease-in;
}

.demo-out-enter-from,
.demo-out-leave-to {
  opacity: 0;
  transform: translateY(8px) scale(0.94);
}

@keyframes demo-breath {
  0%,
  100% {
    opacity: 1;
  }
  55% {
    opacity: 0.35;
  }
}

@media (prefers-reduced-motion: reduce) {
  .demo-exit__dot {
    animation: none;
  }
}
</style>
