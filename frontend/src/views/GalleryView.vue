<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import { api } from '@/api/client'
import type { GalleryItem, GalleryTagStat, SimilarItem } from '@/api/types'
import MomentLightbox from '@/components/MomentLightbox.vue'
import { formatFullDate, formatClock } from '@/utils/format'

const router = useRouter()

defineOptions({ name: 'GalleryView' })

const items = ref<GalleryItem[]>([])
const total = ref(0)
const hasMore = ref(false)
const loading = ref(true)
const loadingMore = ref(false)
const error = ref<string | null>(null)

const keyword = ref('')
const activeTags = ref<string[]>([])
const tagStats = ref<GalleryTagStat[]>([])

const PAGE = 60
let offset = 0
/** 请求序号：快速切换筛选时丢弃过期的响应 —— 之前"偶现无图"就是旧结果盖掉了新结果 */
let loadSeq = 0

/** 按年月分组，顺着时间倒序往下排 */
const groups = computed(() => {
  const buckets = new Map<string, GalleryItem[]>()
  for (const item of items.value) {
    const date = new Date(item.happenedAt)
    const label = `${date.getFullYear()}年${date.getMonth() + 1}月`
    const bucket = buckets.get(label)
    if (bucket) bucket.push(item)
    else buckets.set(label, [item])
  }
  return [...buckets.entries()].map(([label, list]) => ({ label, list }))
})

/** 筛选条上展示的标签：命中数多的优先，最多铺两屏 */
const visibleStats = computed(() => tagStats.value.slice(0, 40))

/** 搜索框里的标签提示：正在输入时，从已认出的标签里匹配 */
const suggestions = computed(() => {
  const kw = keyword.value.trim()
  if (!kw) return []
  return tagStats.value
    .filter((stat) => stat.name.includes(kw))
    .slice(0, 8)
})

async function load(reset: boolean) {
  const seq = ++loadSeq
  if (reset) {
    offset = 0
    loading.value = true
  } else {
    if (!hasMore.value || loadingMore.value) return
    loadingMore.value = true
  }

  error.value = null
  try {
    const response = await api.listGallery({
      q: keyword.value.trim() || undefined,
      tags: activeTags.value.length ? activeTags.value.join(',') : undefined,
      limit: PAGE,
      offset,
    })

    // 期间用户又改了条件，这次的响应作废
    if (seq !== loadSeq) return

    items.value = reset ? response.items : [...items.value, ...response.items]
    total.value = response.total
    hasMore.value = response.hasMore
    offset += response.items.length
  } catch (err) {
    if (seq !== loadSeq) return
    error.value = err instanceof Error ? err.message : '图集没打开'
  } finally {
    if (seq === loadSeq) {
      loading.value = false
      loadingMore.value = false
    }
  }
}

/** 点一条提示：把关键词收进标签，keyword 清空后它的防抖会自动重新加载 */
function pickSuggestion(stat: GalleryTagStat) {
  if (!activeTags.value.includes(stat.key)) {
    activeTags.value.push(stat.key)
  }
  keyword.value = ''
}

// ---------------------------------------------------------------- 标签条滚动
const tagbarEl = ref<HTMLElement | null>(null)
const canLeft = ref(false)
const canRight = ref(false)

function updateFade() {
  const el = tagbarEl.value
  if (!el) return
  canLeft.value = el.scrollLeft > 4
  canRight.value = el.scrollLeft + el.clientWidth < el.scrollWidth - 4
}

/** 标签条上滚一下滚轮，让它横着走 —— 默认的垂直滚轮对横向条毫无办法 */
function onTagbarWheel(event: WheelEvent) {
  const el = event.currentTarget as HTMLElement
  if (Math.abs(event.deltaY) > Math.abs(event.deltaX)) {
    el.scrollLeft += event.deltaY
    event.preventDefault()
    updateFade()
  }
}

let debounceId = 0
watch(keyword, () => {
  window.clearTimeout(debounceId)
  debounceId = window.setTimeout(() => void load(true), 320)
})

function toggleTag(key: string) {
  const index = activeTags.value.indexOf(key)
  if (index >= 0) activeTags.value.splice(index, 1)
  else activeTags.value.push(key)
  void load(true)
}

function clearFilters() {
  keyword.value = ''
  activeTags.value = []
  void load(true)
}

const hasFilter = computed(() => activeTags.value.length > 0 || keyword.value.trim().length > 0)

// ---------------------------------------------------------------- 大图
const lightboxIndex = ref(-1)
const lightboxItem = computed(() => items.value[lightboxIndex.value] ?? null)

function openLightbox(index: number) {
  lightboxIndex.value = index
}

function closeLightbox() {
  lightboxIndex.value = -1
  clearSimilar()
}

function stepLightbox(delta: number) {
  const count = items.value.length
  if (count === 0) return
  lightboxIndex.value = (lightboxIndex.value + delta + count) % count
}

function openMoment(id: string | null) {
  if (!id) return
  closeLightbox()
  router.push({ name: 'moment', params: { id } })
}

// ---------------------------------------------------------------- 找相似
const similar = ref<SimilarItem[]>([])
const similarFor = ref('')
const loadingSimilar = ref(false)
const similarError = ref('')

function clearSimilar() {
  similar.value = []
  similarFor.value = ''
  similarError.value = ''
}

async function findSimilar(id: string) {
  if (loadingSimilar.value) return

  // 再点一次就收起来，别一直占着位置
  if (similarFor.value === id && similar.value.length) {
    clearSimilar()
    return
  }

  loadingSimilar.value = true
  similarError.value = ''
  try {
    similar.value = await api.similarPhotos(id)
    similarFor.value = id
    if (!similar.value.length) similarError.value = '库里还没有别的照片能和它比'
  } catch {
    clearSimilar()
    similarError.value = '没能比出结果，稍后再试'
  } finally {
    loadingSimilar.value = false
  }
}

/** 点相似的那张：把看大图的位置挪过去，而不是跳走 */
function jumpSimilar(item: SimilarItem) {
  const index = items.value.findIndex((one) => one.id === item.id)
  if (index >= 0) {
    lightboxIndex.value = index
    clearSimilar()
  } else {
    openMoment(item.momentId)
  }
}

/** 大图里图集是扁平列表，要换算回它在分组里的位置 */
function flatIndexOf(item: GalleryItem) {
  return items.value.indexOf(item)
}

function whenLabel(item: GalleryItem) {
  const date = new Date(item.happenedAt)
  return `${formatFullDate(date)} · ${formatClock(date)}`
}

// ---------------------------------------------------------------- 无限滚动
const sentinel = ref<HTMLElement | null>(null)
let observer: IntersectionObserver | null = null

function goBack() {
  const state = window.history.state as { back?: string | null } | null
  if (state?.back) router.back()
  else void router.push({ name: 'home' })
}

onMounted(async () => {
  await load(true)

  try {
    tagStats.value = await api.galleryTags()
    await nextTick()
    updateFade()
  } catch {
    /* 标签统计不是关键路径，取不到就不铺筛选条 */
  }

  observer = new IntersectionObserver(
    (entries) => {
      if (entries.some((entry) => entry.isIntersecting)) void load(false)
    },
    { rootMargin: '600px' },
  )
  if (sentinel.value) observer.observe(sentinel.value)
})

onBeforeUnmount(() => {
  window.clearTimeout(debounceId)
  observer?.disconnect()
})
</script>

<template>
  <div class="gallery-view">
    <header class="topbar">
      <button type="button" class="hud-icon" aria-label="返回" title="返回（Esc）" @click="goBack">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" class="h-5 w-5">
          <path d="M14.5 5.5 8 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>

      <div class="topbar__title">
        <h1 class="brand__name">图集</h1>
        <span v-if="total > 0" class="brand__meta numeral">{{ total }} 张画面</span>
      </div>

      <button
        v-if="hasFilter"
        type="button"
        class="hud-icon"
        aria-label="清除筛选"
        title="清除筛选"
        @click="clearFilters"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" class="h-4.5 w-4.5">
          <path d="M6 6l12 12M18 6 6 18" stroke-linecap="round" />
        </svg>
      </button>
      <span v-else class="topbar__spacer" />
    </header>

    <main class="stage">
      <div class="filters">
        <div class="search-wrap">
          <label class="search">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" class="h-4 w-4">
              <circle cx="11" cy="11" r="6.5" />
              <path d="m16 16 4.5 4.5" stroke-linecap="round" />
            </svg>
            <input
              v-model="keyword"
              class="search__input"
              type="search"
              placeholder="搜标签、标题或随手写下的话"
              aria-label="搜索图集"
            />
          </label>

          <!-- 输入时弹出的标签提示：点一下，关键词就变成按这个标签筛 -->
          <div v-if="suggestions.length" class="suggest" role="listbox" aria-label="匹配的标签">
            <button
              v-for="stat in suggestions"
              :key="stat.key"
              type="button"
              class="suggest__item"
              role="option"
              :aria-selected="activeTags.includes(stat.key)"
              @mousedown.prevent="pickSuggestion(stat)"
            >
              <span
                class="suggest__dot"
                :class="{ 'is-on': activeTags.includes(stat.key) }"
                aria-hidden="true"
              />
              {{ stat.name }}
              <span class="suggest__count numeral">{{ stat.count }}</span>
            </button>
          </div>
        </div>

        <div
          v-if="visibleStats.length"
          ref="tagbarEl"
          class="tagbar"
          :class="{ 'can-left': canLeft, 'can-right': canRight }"
          role="group"
          aria-label="按标签筛选"
          @wheel="onTagbarWheel"
          @scroll.passive="updateFade"
        >
          <button
            v-for="stat in visibleStats"
            :key="stat.key"
            type="button"
            class="tag-chip"
            :class="{ 'is-active': activeTags.includes(stat.key) }"
            :aria-pressed="activeTags.includes(stat.key)"
            @click="toggleTag(stat.key)"
          >
            {{ stat.name }}
            <span class="tag-chip__count numeral">{{ stat.count }}</span>
          </button>
        </div>
      </div>

      <p v-if="loading" class="notice">正在把画面一张张铺开…</p>

      <p v-else-if="error" class="notice">{{ error }}</p>

      <div v-else-if="!items.length" class="empty">
        <p class="empty__title title-serif">这里还是空的</p>
        <p class="empty__hint">
          {{
            hasFilter
              ? '换个词或去掉几个标签再试试。'
              : '等有了照片，它们会按时间自己排好队。'
          }}
        </p>
      </div>

      <template v-else>
        <section v-for="group in groups" :key="group.label" class="group">
          <h2 class="group__label">
            <span class="title-serif">{{ group.label }}</span>
            <span class="group__count numeral">{{ group.list.length }}</span>
          </h2>

          <div class="grid">
            <button
              v-for="item in group.list"
              :key="item.id"
              type="button"
              class="cell"
              :aria-label="item.title || '查看这张画面'"
              @click="openLightbox(flatIndexOf(item))"
            >
              <img
                :src="item.thumbUrl ?? item.url"
                :alt="item.title || '记忆中的画面'"
                loading="lazy"
                decoding="async"
              />
              <span v-if="item.kind === 'video'" class="cell__play" aria-hidden="true">
                <svg viewBox="0 0 24 24" fill="currentColor" class="h-3.5 w-3.5">
                  <path
                    d="M9 5.6v12.8c0 .8.9 1.3 1.6.9l10-6.4c.6-.4.6-1.4 0-1.8l-10-6.4c-.7-.4-1.6.1-1.6.9Z"
                  />
                </svg>
              </span>
              <span v-if="item.tags.length" class="cell__tag">{{ item.tags[0]?.name }}</span>
            </button>
          </div>
        </section>

        <div ref="sentinel" class="sentinel" aria-hidden="true" />
        <p v-if="loadingMore" class="notice notice--soft">再铺一些…</p>
        <p v-else-if="!hasMore" class="notice notice--soft">到这里就是最早的一张了</p>
      </template>
    </main>

    <!-- 看大图：和记忆页、分享页共用同一份（可以滑动、有缩略图垫底）。
         相册特有的那块——日期、标签、「回到这段记忆」「找相似」——走 #meta 插槽 -->
    <MomentLightbox
      class="gallery-lightbox"
      :items="items"
      :index="lightboxIndex"
      :title="lightboxItem?.title || ''"
      @close="closeLightbox"
      @step="stepLightbox"
    >
      <template #meta>
        <figure class="lightbox__figure">
          <figcaption class="lightbox__meta">
            <span class="lightbox__when numeral">{{ lightboxItem ? whenLabel(lightboxItem) : '' }}</span>
            <span v-if="lightboxItem?.title" class="lightbox__title title-serif">
              {{ lightboxItem.title }}
            </span>

            <span v-if="lightboxItem?.tags.length" class="lightbox__tags">
              <span v-for="tag in lightboxItem.tags" :key="tag.key" class="lightbox__tag">
                {{ tag.name }}
              </span>
            </span>

            <div v-if="lightboxItem" class="lightbox__actions">
              <button
                v-if="lightboxItem.momentId"
                type="button"
                class="ghost-btn lightbox__jump"
                @click="openMoment(lightboxItem.momentId)"
              >
                回到这段记忆
              </button>
              <button
                type="button"
                class="ghost-btn lightbox__jump"
                :disabled="loadingSimilar"
                @click="findSimilar(lightboxItem.id)"
              >
                {{ loadingSimilar ? '正在比…' : '找相似' }}
              </button>
            </div>

            <p v-if="similarError" class="lightbox__note">{{ similarError }}</p>

            <ul v-if="similar.length" class="lightbox__similar">
              <li v-for="item in similar" :key="item.id">
                <button
                  type="button"
                  class="similar-card"
                  :aria-label="`相似度 ${Math.round(item.score * 100)}%`"
                  @click="jumpSimilar(item)"
                >
                  <img
                    :src="item.thumbUrl ?? item.url"
                    :alt="item.title || '相似的一张'"
                    loading="lazy"
                  />
                  <span class="similar-card__score numeral">
                    {{ Math.round(item.score * 100) }}%
                  </span>
                </button>
              </li>
            </ul>
          </figcaption>
        </figure>
      </template>
    </MomentLightbox>
  </div>
</template>

<style scoped>
.gallery-view {
  position: relative;
  min-height: 100vh;
  min-height: 100dvh;
}

.topbar {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding-top: max(1.2rem, calc(var(--safe-top) + 0.8rem));
  padding-bottom: 1.2rem;
  padding-left: max(clamp(1rem, 3vw, 2.4rem), calc(var(--safe-left) + 0.6rem));
  padding-right: max(clamp(1rem, 3vw, 2.4rem), calc(var(--safe-right) + 0.6rem));
  pointer-events: none;
}

.topbar > * {
  pointer-events: auto;
}

.topbar__title {
  display: flex;
  align-items: baseline;
  gap: 0.6rem;
}

.topbar__spacer {
  width: 2.5rem;
}

.brand__name {
  margin: 0;
  font-family: var(--font-serif);
  font-size: 1.32rem;
  letter-spacing: 0.16em;
  color: var(--color-ink-50);
}

.brand__meta {
  font-size: 0.74rem;
  letter-spacing: 0.06em;
  color: color-mix(in oklab, var(--color-ink-400) 92%, transparent);
}

.hud-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.5rem;
  height: 2.5rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 55%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 84%, transparent);
  color: var(--color-ink-200);
  backdrop-filter: blur(12px);
  opacity: 0.82;
  box-shadow: 0 8px 22px -16px color-mix(in oklab, var(--color-shade) 90%, transparent);
  transition:
    opacity 0.4s var(--ease-out-soft),
    color 0.4s var(--ease-out-soft),
    transform 0.25s var(--ease-out-soft);
}

.hud-icon:hover {
  opacity: 1;
  color: var(--color-glow-300);
}

.hud-icon:active {
  transform: scale(0.94);
}

/* ---------------- 主体 ---------------- */
.stage {
  position: relative;
  z-index: 1;
  max-width: 62rem;
  margin: 0 auto;
  padding: clamp(5rem, 10vh, 6.6rem) clamp(1rem, 3.4vw, 2rem) 5rem;
}

.filters {
  position: sticky;
  top: 0;
  z-index: 10;
  display: flex;
  flex-direction: column;
  gap: 0.7rem;
  padding-bottom: 0.9rem;
  background: linear-gradient(
    to bottom,
    var(--color-ink-950) 62%,
    color-mix(in oklab, var(--color-ink-950) 0%, transparent) 100%
  );
  backdrop-filter: blur(2px);
}

.search {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.6rem 1rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-700) 82%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 86%, transparent);
  color: color-mix(in oklab, var(--color-ink-400) 92%, transparent);
  transition: border-color 0.35s var(--ease-out-soft);
}

.search:focus-within {
  border-color: color-mix(in oklab, var(--color-glow-500) 58%, transparent);
}

.search__input {
  flex: 1;
  min-width: 0;
  border: none;
  background: transparent;
  color: var(--color-ink-100);
  font: inherit;
  font-size: 0.9rem;
  outline: none;
}

.search__input::placeholder {
  color: color-mix(in oklab, var(--color-ink-500) 80%, transparent);
}

.tagbar {
  display: flex;
  gap: 0.4rem;
  overflow-x: auto;
  padding-bottom: 0.2rem;
  scrollbar-width: none;
}

.tagbar::-webkit-scrollbar {
  display: none;
}

/* 两端还能滚就露一点渐隐，暗示这排标签没到头 */
.tagbar.can-left {
  mask-image: linear-gradient(to right, transparent 0, black 20px, black 100%);
}
.tagbar.can-right {
  mask-image: linear-gradient(to right, black 0, black calc(100% - 20px), transparent 100%);
}
.tagbar.can-left.can-right {
  mask-image: linear-gradient(to right, transparent 0, black 20px, black calc(100% - 20px), transparent 100%);
}

/* ---------------- 搜索提示 ---------------- */
.search-wrap {
  position: relative;
}

.suggest {
  position: absolute;
  top: calc(100% + 0.45rem);
  left: 0;
  right: 0;
  z-index: 40;
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  padding: 0.3rem;
  border-radius: 14px;
  border: 1px solid color-mix(in oklab, var(--color-ink-700) 80%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 97%, transparent);
  backdrop-filter: blur(14px);
  box-shadow: 0 24px 48px -28px color-mix(in oklab, var(--color-shade) 85%, transparent);
  max-height: 16rem;
  overflow-y: auto;
}

.suggest__item {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0.52rem 0.7rem;
  border-radius: 9px;
  border: none;
  background: transparent;
  color: var(--color-ink-100);
  font-size: 0.85rem;
  text-align: left;
  transition:
    background-color 0.25s var(--ease-out-soft),
    color 0.25s var(--ease-out-soft);
}

.suggest__item:hover {
  background: color-mix(in oklab, var(--color-glow-600) 15%, transparent);
  color: var(--color-glow-300);
}

.suggest__dot {
  flex: none;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: color-mix(in oklab, var(--color-ink-500) 70%, transparent);
}

.suggest__dot.is-on {
  background: var(--color-glow-400);
  box-shadow: 0 0 8px color-mix(in oklab, var(--color-glow-400) 75%, transparent);
}

.suggest__count {
  margin-left: auto;
  font-size: 0.7rem;
  color: color-mix(in oklab, var(--color-ink-400) 88%, transparent);
}

.tag-chip {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 0.32rem;
  padding: 0.34rem 0.7rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-700) 76%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 62%, transparent);
  color: color-mix(in oklab, var(--color-ink-200) 94%, transparent);
  font-size: 0.78rem;
  white-space: nowrap;
  transition:
    border-color 0.3s var(--ease-out-soft),
    color 0.3s var(--ease-out-soft),
    background-color 0.3s var(--ease-out-soft);
}

.tag-chip:hover {
  border-color: color-mix(in oklab, var(--color-glow-500) 50%, transparent);
}

.tag-chip.is-active {
  border-color: color-mix(in oklab, var(--color-glow-500) 76%, transparent);
  background: color-mix(in oklab, var(--color-glow-600) 20%, transparent);
  color: var(--color-glow-300);
}

.tag-chip__count {
  font-size: 0.68rem;
  opacity: 0.62;
}

/* ---------------- 分组 ---------------- */
.group {
  margin-top: 1.7rem;
}

.group__label {
  position: sticky;
  top: 6.1rem;
  z-index: 5;
  display: flex;
  align-items: baseline;
  gap: 0.5rem;
  margin: 0 0 0.75rem;
  font-size: 0.98rem;
  letter-spacing: 0.12em;
  color: var(--color-ink-100);
}

.group__count {
  font-size: 0.72rem;
  color: color-mix(in oklab, var(--color-ink-400) 90%, transparent);
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(9.2rem, 1fr));
  gap: 0.5rem;
}

.cell {
  position: relative;
  display: block;
  padding: 0;
  border: 1px solid color-mix(in oklab, oklch(1 0 0) 72%, var(--color-ink-700));
  border-radius: 10px;
  overflow: hidden;
  aspect-ratio: 1 / 1;
  background: color-mix(in oklab, var(--color-ink-850) 92%, transparent);
  cursor: zoom-in;
  transition:
    transform 0.5s var(--ease-out-soft),
    box-shadow 0.5s var(--ease-out-soft),
    border-color 0.5s var(--ease-out-soft);
}

.cell:hover {
  transform: translateY(-3px);
  border-color: color-mix(in oklab, var(--color-glow-500) 62%, oklch(1 0 0));
  box-shadow: 0 18px 34px -24px color-mix(in oklab, var(--color-shade) 80%, transparent);
}

.cell img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.cell__tag {
  position: absolute;
  left: 0.4rem;
  bottom: 0.4rem;
  padding: 0.1rem 0.44rem;
  border-radius: 999px;
  background: color-mix(in oklab, var(--color-ink-950) 74%, transparent);
  backdrop-filter: blur(6px);
  font-size: 0.64rem;
  letter-spacing: 0.04em;
  color: color-mix(in oklab, var(--color-ink-100) 96%, transparent);
  opacity: 0;
  transition: opacity 0.4s var(--ease-out-soft);
}

.cell:hover .cell__tag,
.cell:focus-visible .cell__tag {
  opacity: 1;
}

/* 视频缩略图上盖一枚播放徽标，别让人以为点开是看图 */
.cell__play {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
}

.cell__play svg {
  width: 2.2rem;
  height: 2.2rem;
  padding: 0.58rem;
  box-sizing: content-box;
  border-radius: 50%;
  background: color-mix(in oklab, var(--color-ink-950) 52%, transparent);
  backdrop-filter: blur(5px);
  color: oklch(0.98 0.008 90);
  transition:
    transform 0.5s var(--ease-out-soft),
    background-color 0.5s var(--ease-out-soft);
}

.cell:hover .cell__play svg {
  transform: scale(1.1);
  background: color-mix(in oklab, var(--color-ink-950) 66%, transparent);
}

.sentinel {
  height: 1px;
}

/* ---------------- 状态 ---------------- */
.notice {
  margin: 2.4rem 0 0;
  text-align: center;
  font-size: 0.86rem;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
}

.notice--soft {
  margin-top: 1.6rem;
  font-size: 0.78rem;
  color: color-mix(in oklab, var(--color-ink-400) 86%, transparent);
}

.empty {
  margin-top: 5rem;
  text-align: center;
}

.empty__title {
  margin: 0;
  font-size: 1.2rem;
  color: var(--color-ink-100);
}

.empty__hint {
  margin: 0.6rem 0 0;
  font-size: 0.84rem;
  color: color-mix(in oklab, var(--color-ink-400) 92%, transparent);
}

/* ---------------- 大图 ---------------- */
/* 大图本体在 components/MomentLightbox.vue（记忆页、分享页、相册共用一份）。
   这里只留相册这侧特有的：图下面的日期、标签、按钮（走 #meta 插槽）。
   说明也想一眼看全，所以把图的高度上限收矮一点 */
.gallery-lightbox {
  --lightbox-media-max: 76dvh;
}

.lightbox__figure {
  display: flex;
  flex-direction: column;
  align-items: center;
  margin: 1.1rem 0 0;
  max-width: min(60rem, 100%);
}

.lightbox__meta {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.5rem;
  text-align: center;
}

.lightbox__when {
  font-size: 0.78rem;
  letter-spacing: 0.12em;
  color: oklch(0.88 0.02 90 / 0.86);
}

.lightbox__title {
  font-size: 1.1rem;
  color: oklch(0.97 0.01 90);
}

.lightbox__tags {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 0.35rem;
}

.lightbox__tag {
  padding: 0.16rem 0.6rem;
  border-radius: 999px;
  border: 1px solid oklch(1 0 0 / 0.2);
  background: oklch(1 0 0 / 0.08);
  font-size: 0.72rem;
  color: oklch(0.94 0.012 90);
}

.lightbox__jump {
  margin-top: 0.3rem;
  padding: 0.5rem 1.2rem;
  font-size: 0.82rem;
}

.lightbox__actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  align-items: center;
  gap: 0.4rem;
}

.lightbox__note {
  margin: 0.2rem 0 0;
  font-size: 0.78rem;
  color: oklch(0.88 0.02 90 / 0.7);
}

.lightbox__similar {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 0.5rem;
  margin: 0.5rem 0 0;
  padding: 0;
  list-style: none;
}

.similar-card {
  position: relative;
  display: block;
  padding: 0;
  border: none;
  border-radius: 8px;
  overflow: hidden;
  cursor: pointer;
  background: oklch(1 0 0 / 0.06);
  transition: transform 0.25s var(--ease-out-soft);
}

.similar-card:hover {
  transform: translateY(-2px);
}

.similar-card img {
  display: block;
  width: 5.2rem;
  height: 5.2rem;
  object-fit: cover;
}

.similar-card__score {
  position: absolute;
  right: 0.25rem;
  bottom: 0.25rem;
  padding: 0.05rem 0.35rem;
  border-radius: 999px;
  background: oklch(0 0 0 / 0.55);
  font-size: 0.66rem;
  color: oklch(0.97 0.01 90);
}

@media (max-width: 760px) {
  .grid {
    grid-template-columns: repeat(auto-fill, minmax(6.6rem, 1fr));
    gap: 0.34rem;
  }
  .group__label {
    top: 5.6rem;
  }
  .brand__meta {
    display: none;
  }
}
</style>
