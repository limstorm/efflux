import { computed, ref } from 'vue'

/** 顺序：顺着时间从头看；随机：随便挑一段 */
export type DemoMode = 'sequence' | 'random'

/**
 * 演示（巡演）的共享状态。
 *
 * 两头各管一摊：主页那头摇镜头、把某段记忆打开；详情页那头看到演示在跑，
 * 就自己从上往下慢慢滚到底，再退回光河。跨路由，所以放在模块作用域。
 */
const mode = ref<DemoMode | null>(null)
/** 演示正打开的那一段（详情页据此决定要不要自动下滚） */
const opened = ref<string | null>(null)

export function useDemo() {
  const running = computed(() => mode.value !== null)

  function start(next: DemoMode) {
    mode.value = next
    opened.value = null
  }

  function stop() {
    mode.value = null
    opened.value = null
  }

  /** 这一段交给详情页了，它开始自己往下滚 */
  function markOpened(id: string) {
    opened.value = id
  }

  /** 详情页已经关掉（回到光河），下一段的门可以开了 */
  function markClosed() {
    opened.value = null
  }

  return { mode, running, opened, start, stop, markOpened, markClosed }
}
