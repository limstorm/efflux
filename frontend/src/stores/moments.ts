import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import { api } from '@/api/client'
import type { MomentSummary, StatsResponse, TagItem } from '@/api/types'

const HOME_LIMIT = 400

export const useMomentsStore = defineStore('moments', () => {
  const items = ref<MomentSummary[]>([])
  const total = ref(0)
  const stats = ref<StatsResponse | null>(null)
  const tags = ref<TagItem[]>([])
  const loading = ref(false)
  const loaded = ref(false)
  const error = ref<string | null>(null)

  /**
   * 从某段记忆回到光河时，希望镜头对准的那一段。
   * 只留在内存里，用过即清——下次回来不该再被上次的位置拽走。
   */
  const anchor = ref<{ id: string; at: string } | null>(null)

  /** 光河按时间正序排列：最旧的记忆在最左边，最新的在右边（"现在"） */
  const ordered = computed(() =>
    [...items.value].sort(
      (a, b) => new Date(a.happenedAt).getTime() - new Date(b.happenedAt).getTime(),
    ),
  )

  const newest = computed(() => ordered.value[ordered.value.length - 1] ?? null)

  async function load(force = false) {
    if (loading.value) return
    if (loaded.value && !force) return

    loading.value = true
    error.value = null
    try {
      const [list, statsResult] = await Promise.all([
        api.listMoments({ order: 'asc', limit: HOME_LIMIT }),
        api.stats().catch(() => null),
      ])
      items.value = list.items
      total.value = list.total
      if (statsResult) stats.value = statsResult
      loaded.value = true
    } catch (err) {
      error.value = err instanceof Error ? err.message : '加载失败'
    } finally {
      loading.value = false
    }
  }

  async function loadTags() {
    try {
      tags.value = await api.listTags()
    } catch {
      /* 标签不是关键路径，静默失败 */
    }
  }

  function setAnchor(next: { id: string; at: string } | null) {
    anchor.value = next
  }

  function removeLocal(id: string) {
    const index = items.value.findIndex((item) => item.id === id)
    if (index >= 0) {
      items.value.splice(index, 1)
      total.value = Math.max(0, total.value - 1)
    }
  }

  function upsertLocal(moment: MomentSummary) {
    const index = items.value.findIndex((item) => item.id === moment.id)
    if (index >= 0) {
      items.value[index] = moment
      return
    }
    items.value.push(moment)
    total.value += 1
  }

  return {
    items,
    total,
    stats,
    tags,
    loading,
    loaded,
    error,
    anchor,
    ordered,
    newest,
    load,
    loadTags,
    setAnchor,
    removeLocal,
    upsertLocal,
  }
})
