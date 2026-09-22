<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import { api } from '@/api/client'
import type { MomentSummary } from '@/api/types'
import { useMomentsStore } from '@/stores/moments'
import { formatDotted } from '@/utils/format'
import { tagHue } from '@/utils/tagColor'

type RangeKey = 'all' | 'year' | 'month' | 'week' | 'custom'

const route = useRoute()
const router = useRouter()
const store = useMomentsStore()

function parseTags(value: unknown): string[] {
  if (typeof value !== 'string') return []
  return value
    .split(',')
    .map((tag) => tag.trim())
    .filter(Boolean)
}

const query = ref(typeof route.query.q === 'string' ? route.query.q : '')
const selectedTags = ref<string[]>(parseTags(route.query.tags))
const range = ref<RangeKey>('all')
const customFrom = ref('')
const customTo = ref('')

const results = ref<MomentSummary[]>([])
const total = ref(0)
const loading = ref(false)
const searched = ref(false)

const input = ref<HTMLInputElement | null>(null)

const rangeOptions: { key: RangeKey; label: string }[] = [
  { key: 'all', label: '全部时间' },
  { key: 'year', label: '今年' },
  { key: 'month', label: '本月' },
  { key: 'week', label: '最近一周' },
  { key: 'custom', label: '自定义' },
]

const hasCondition = computed(
  () => Boolean(query.value.trim()) || selectedTags.value.length > 0 || range.value !== 'all',
)

function resolveRange(): { from?: string; to?: string } {
  const now = new Date()
  switch (range.value) {
    case 'year':
      return { from: new Date(now.getFullYear(), 0, 1).toISOString() }
    case 'month':
      return { from: new Date(now.getFullYear(), now.getMonth(), 1).toISOString() }
    case 'week':
      return { from: new Date(Date.now() - 7 * 86_400_000).toISOString() }
    case 'custom':
      return {
        from: customFrom.value ? new Date(`${customFrom.value}T00:00:00`).toISOString() : undefined,
        to: customTo.value ? new Date(`${customTo.value}T23:59:59`).toISOString() : undefined,
      }
    default:
      return {}
  }
}

async function runSearch() {
  if (!hasCondition.value) {
    results.value = []
    total.value = 0
    searched.value = false
    return
  }

  loading.value = true
  try {
    const { from, to } = resolveRange()
    const response = await api.listMoments({
      q: query.value.trim() || undefined,
      tags: selectedTags.value.join(',') || undefined,
      from,
      to,
      order: 'desc',
      limit: 200,
    })
    results.value = response.items
    total.value = response.total
    searched.value = true
  } catch {
    results.value = []
  } finally {
    loading.value = false
  }
}

let debounce = 0
watch(
  [query, selectedTags, range, customFrom, customTo],
  () => {
    window.clearTimeout(debounce)
    debounce = window.setTimeout(() => void runSearch(), 260)
  },
  { deep: true },
)

watch([query, selectedTags], () => {
  const next: Record<string, string> = {}
  if (query.value.trim()) next.q = query.value.trim()
  if (selectedTags.value.length) next.tags = selectedTags.value.join(',')
  void router.replace({ name: 'search', query: next })
})

function toggleTag(name: string) {
  const index = selectedTags.value.indexOf(name)
  if (index >= 0) selectedTags.value.splice(index, 1)
  else selectedTags.value.push(name)
}

function clearAll() {
  query.value = ''
  selectedTags.value = []
  range.value = 'all'
  customFrom.value = ''
  customTo.value = ''
}

function close() {
  const state = window.history.state as { back?: string | null } | null
  if (state?.back) router.back()
  else void router.push({ name: 'home' })
}

function open(id: string) {
  void router.push({ name: 'moment', params: { id } })
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') close()
}

onMounted(async () => {
  window.addEventListener('keydown', onKeydown)
  store.load()
  store.loadTags()
  input.value?.focus()
  if (hasCondition.value) void runSearch()
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
  window.clearTimeout(debounce)
})
</script>

<template>
  <div class="search-view">
    <header class="head">
      <button type="button" class="close" aria-label="返回光河" title="返回光河（Esc）" @click="close">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" class="h-4 w-4">
          <path d="M6 6l12 12M18 6 6 18" stroke-linecap="round" />
        </svg>
      </button>

      <div class="searchbox">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="searchbox__icon">
          <circle cx="11" cy="11" r="6.5" />
          <path d="m16 16 4.5 4.5" stroke-linecap="round" />
        </svg>
        <input
          ref="input"
          v-model="query"
          class="searchbox__input"
          type="search"
          placeholder="搜寻一段记忆…"
          aria-label="按主题、内容或地点搜索"
        />
        <button v-if="hasCondition" type="button" class="searchbox__clear" @click="clearAll">清空</button>
      </div>

      <span class="head__spacer" />
    </header>

    <section class="filters">
      <div class="chips">
        <button
          v-for="option in rangeOptions"
          :key="option.key"
          type="button"
          class="chip"
          :data-active="range === option.key"
          @click="range = option.key"
        >
          {{ option.label }}
        </button>
      </div>

      <div v-if="range === 'custom'" class="range-inputs">
        <label class="range-inputs__item">
          <span>从</span>
          <input v-model="customFrom" type="date" class="paper-input range-inputs__field" />
        </label>
        <label class="range-inputs__item">
          <span>到</span>
          <input v-model="customTo" type="date" class="paper-input range-inputs__field" />
        </label>
      </div>

      <div v-if="store.tags.length" class="tagcloud">
        <button
          v-for="tag in store.tags.slice(0, 24)"
          :key="tag.name"
          type="button"
          class="tag-pill"
          :style="{ '--tag-hue': tagHue(tag.name) }"
          :data-active="selectedTags.includes(tag.name)"
          @click="toggleTag(tag.name)"
        >
          {{ tag.name }}
          <span class="tagcloud__count">{{ tag.count }}</span>
        </button>
      </div>
    </section>

    <main class="results" aria-live="polite">
      <p v-if="loading && !results.length" class="state">正在翻找…</p>

      <p v-else-if="!searched" class="state">
        输入几个字，或者点一个标签，<br />就能回到那段时间。
      </p>

      <p v-else-if="!results.length" class="state">
        没有找到相关的记忆。<br />
        <button type="button" class="state__action" @click="clearAll">换个条件试试</button>
      </p>

      <template v-else>
        <p class="state state--count">
          找到 <span class="numeral numeral--clear">{{ total }}</span> 段记忆
        </p>
        <ul class="grid">
          <li v-for="item in results" :key="item.id">
            <button type="button" class="card" @click="open(item.id)">
              <span class="card__frame">
                <img
                  v-if="item.cover"
                  :src="item.cover.thumbUrl ?? item.cover.url"
                  :alt="item.title || '记忆'"
                  loading="lazy"
                  decoding="async"
                />
                <span v-else class="card__frame--blank">
                  {{ (item.title || item.excerpt || '这一刻').slice(0, 24) }}
                </span>
                <span v-if="item.hasMusic" class="card__music" aria-label="含背景音乐">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" class="h-3 w-3">
                    <path d="M9 18V6l10-2v12" stroke-linecap="round" stroke-linejoin="round" />
                    <circle cx="6.5" cy="18" r="2.5" />
                  </svg>
                </span>
              </span>
              <span class="card__body">
                <span class="card__title">{{ item.title || item.excerpt || '未命名的这一刻' }}</span>
                <span class="card__meta">
                  <span class="numeral">{{ formatDotted(item.happenedAt) }}</span>
                  <span v-if="item.tags.length" class="card__tags">
                    {{ item.tags.slice(0, 3).join(' · ') }}
                  </span>
                </span>
              </span>
            </button>
          </li>
        </ul>
      </template>
    </main>
  </div>
</template>

<style scoped>
.search-view {
  position: relative;
  min-height: 100vh;
  min-height: 100dvh;
  padding-bottom: 4rem;
}

/* ---------------- 顶部搜索 ---------------- */
.head {
  position: sticky;
  top: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 1rem;
  padding-top: max(1.2rem, calc(var(--safe-top) + 0.8rem));
  padding-bottom: 1.2rem;
  padding-left: max(clamp(1rem, 3vw, 2.4rem), calc(var(--safe-left) + 0.6rem));
  padding-right: max(clamp(1rem, 3vw, 2.4rem), calc(var(--safe-right) + 0.6rem));
  background: linear-gradient(
    to bottom,
    color-mix(in oklab, var(--color-ink-950) 94%, transparent) 58%,
    transparent
  );
  backdrop-filter: blur(10px);
}

.head__spacer {
  width: 2.4rem;
}

.close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.4rem;
  height: 2.4rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 55%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 84%, transparent);
  color: var(--color-ink-200);
  opacity: 0.82;
  box-shadow: 0 8px 20px -16px color-mix(in oklab, var(--color-shade) 85%, transparent);
  transition: opacity 0.4s var(--ease-out-soft), color 0.4s var(--ease-out-soft);
}

.close:hover {
  opacity: 1;
  color: var(--color-glow-300);
}

.searchbox {
  position: relative;
  flex: 1;
  display: flex;
  align-items: center;
  gap: 0.7rem;
  max-width: 40rem;
  margin: 0 auto;
  padding: 0.35rem 1rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 62%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 90%, transparent);
  box-shadow: 0 10px 26px -22px color-mix(in oklab, var(--color-shade) 85%, transparent);
  transition: border-color 0.4s var(--ease-out-soft), box-shadow 0.4s var(--ease-out-soft);
}

.searchbox:focus-within {
  border-color: color-mix(in oklab, var(--color-glow-500) 72%, transparent);
  box-shadow: 0 12px 30px -20px color-mix(in oklab, var(--color-glow-500) 80%, transparent);
}

.searchbox__icon {
  width: 1.15rem;
  height: 1.15rem;
  color: color-mix(in oklab, var(--color-ink-400) 92%, transparent);
  flex: none;
}

.searchbox__input {
  flex: 1;
  border: none;
  background: none;
  outline: none;
  padding: 0.55rem 0;
  font-family: var(--font-serif);
  font-size: 1.02rem;
  letter-spacing: 0.06em;
  color: var(--color-ink-50);
}

.searchbox__input::-webkit-search-cancel-button {
  display: none;
}

.searchbox__clear {
  border: none;
  background: none;
  font-size: 0.76rem;
  color: color-mix(in oklab, var(--color-ink-400) 88%, transparent);
  padding: 0.2rem 0.3rem;
  transition: color 0.3s var(--ease-out-soft);
}

.searchbox__clear:hover {
  color: var(--color-glow-300);
}

/* ---------------- 筛选 ---------------- */
.filters {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  max-width: 60rem;
  margin: 0 auto;
  padding: 1.4rem clamp(1rem, 3vw, 2.4rem) 0;
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.chip {
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 55%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 72%, transparent);
  padding: 0.34rem 0.9rem;
  font-size: 0.78rem;
  color: color-mix(in oklab, var(--color-ink-300) 94%, transparent);
  transition:
    border-color 0.35s var(--ease-out-soft),
    color 0.35s var(--ease-out-soft),
    background-color 0.35s var(--ease-out-soft);
}

.chip:hover {
  border-color: color-mix(in oklab, var(--color-glow-500) 62%, transparent);
  color: var(--color-glow-300);
}

.chip[data-active='true'] {
  border-color: color-mix(in oklab, var(--color-glow-500) 72%, transparent);
  background: color-mix(in oklab, var(--color-glow-700) 68%, transparent);
  color: var(--color-glow-300);
}

.range-inputs {
  display: flex;
  flex-wrap: wrap;
  gap: 0.9rem;
}

.range-inputs__item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.78rem;
  color: color-mix(in oklab, var(--color-ink-400) 88%, transparent);
}

.range-inputs__field {
  padding: 0.4rem 0.7rem;
  font-size: 0.8rem;
}

.tagcloud {
  display: flex;
  flex-wrap: wrap;
  gap: 0.45rem;
}

.tagcloud__count {
  font-size: 0.68rem;
  opacity: 0.6;
  font-variant-numeric: tabular-nums;
}

/* ---------------- 结果 ---------------- */
.results {
  max-width: 68rem;
  margin: 0 auto;
  padding: 2rem clamp(1rem, 3vw, 2.4rem) 0;
}

.state {
  margin: 3.4rem auto;
  max-width: 26rem;
  text-align: center;
  font-size: 0.86rem;
  line-height: 2;
  color: color-mix(in oklab, var(--color-ink-400) 88%, transparent);
}

.state--count {
  margin: 0 0 1.3rem;
  text-align: left;
  color: color-mix(in oklab, var(--color-ink-300) 88%, transparent);
  font-size: 0.78rem;
  letter-spacing: 0.08em;
}

.state__action {
  margin-top: 0.5rem;
  border: none;
  background: none;
  color: color-mix(in oklab, var(--color-glow-500) 92%, transparent);
  border-bottom: 1px dashed color-mix(in oklab, var(--color-glow-600) 48%, transparent);
  font-size: 0.84rem;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr));
  gap: 1.15rem;
  margin: 0;
  padding: 0;
  list-style: none;
}

.card {
  display: flex;
  flex-direction: column;
  width: 100%;
  text-align: left;
  border-radius: var(--radius-md);
  border: 1px solid color-mix(in oklab, oklch(1 0 0) 74%, var(--color-ink-700));
  background: oklch(1 0 0);
  padding: 4px;
  box-shadow:
    0 1px 2px color-mix(in oklab, var(--color-shade) 10%, transparent),
    0 14px 30px -22px color-mix(in oklab, var(--color-shade) 60%, transparent);
  transition:
    transform 0.45s var(--ease-out-soft),
    border-color 0.45s var(--ease-out-soft),
    box-shadow 0.45s var(--ease-out-soft);
}

.card:hover {
  transform: translate3d(0, -5px, 0);
  border-color: color-mix(in oklab, var(--color-glow-500) 62%, oklch(1 0 0));
  box-shadow:
    0 2px 4px color-mix(in oklab, var(--color-shade) 10%, transparent),
    0 24px 46px -24px color-mix(in oklab, var(--color-glow-500) 50%, transparent);
}

.card__frame {
  position: relative;
  display: block;
  aspect-ratio: 4 / 3;
  background: color-mix(in oklab, var(--color-ink-850) 92%, transparent);
  border-radius: calc(var(--radius-md) - 5px);
  overflow: hidden;
}

.card__frame img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  transition: transform 0.7s var(--ease-out-soft);
}

.card:hover .card__frame img {
  transform: scale(1.045);
}

.card__frame--blank {
  display: grid;
  place-items: center;
  height: 100%;
  padding: 1rem;
  font-family: var(--font-serif);
  font-size: 0.86rem;
  line-height: 1.7;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
  text-align: center;
  background:
    radial-gradient(
      120% 80% at 86% 0%,
      color-mix(in oklab, var(--color-glow-700) 68%, transparent) 0%,
      transparent 62%
    ),
    linear-gradient(165deg, oklch(1 0 0), color-mix(in oklab, var(--color-ink-850) 92%, transparent));
}

.card__music {
  position: absolute;
  right: 0.5rem;
  bottom: 0.5rem;
  display: inline-flex;
  padding: 0.28rem;
  border-radius: 999px;
  background: color-mix(in oklab, oklch(1 0 0) 84%, transparent);
  border: 1px solid color-mix(in oklab, var(--color-ink-700) 45%, transparent);
  color: var(--color-glow-300);
  backdrop-filter: blur(6px);
}

.card__body {
  display: flex;
  flex-direction: column;
  gap: 0.42rem;
  padding: 0.85rem 0.85rem 0.9rem;
}

.card__title {
  font-family: var(--font-serif);
  font-size: 0.92rem;
  line-height: 1.6;
  color: var(--color-ink-50);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.card__meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.6rem;
  font-size: 0.72rem;
  color: color-mix(in oklab, var(--color-ink-500) 96%, transparent);
}

.card__tags {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media (max-width: 760px) {
  .searchbox__input {
    font-size: 0.94rem;
  }
  .grid {
    grid-template-columns: repeat(auto-fill, minmax(9.4rem, 1fr));
    gap: 0.8rem;
  }
  .card__body {
    padding: 0.65rem 0.7rem 0.8rem;
  }
  .card__title {
    font-size: 0.84rem;
  }
}
</style>
