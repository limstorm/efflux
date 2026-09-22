<script setup lang="ts">
// 分享页：只读。
//
// 这是整个站里唯一不要求口令的页面——凭证本身就是钥匙。所以这里只读不写：
// 没有编辑、没有删除，也不放任何指向站内的入口，只是安安静静地展示那一刻。
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'

import { api } from '@/api/client'
import type { MomentDetail } from '@/api/types'
import MomentGallery from '@/components/MomentGallery.vue'
import MomentLightbox from '@/components/MomentLightbox.vue'
import MusicOrb from '@/components/MusicOrb.vue'
import { formatClock, formatFullDate, formatPlace } from '@/utils/format'
import { tagHue } from '@/utils/tagColor'

const route = useRoute()
const token = computed(() => String(route.params.token ?? ''))

const moment = ref<MomentDetail | null>(null)
const loading = ref(true)
const failed = ref(false)

const whenLabel = computed(() => {
  if (!moment.value) return ''
  const date = new Date(moment.value.happenedAt)
  const created = new Date(moment.value.createdAt)
  const drifted = Math.abs(date.getTime() - created.getTime()) > 90_000
  return drifted ? `${formatFullDate(date)} · ${formatClock(date)}` : formatFullDate(date)
})

/** 城市与地点拼一起，见 utils/format 的 formatPlace */
const placeLabel = computed(() => formatPlace(moment.value?.place))

// ---------------------------------------------------------------- 看大图
// 与站内同一个组件。手机上左右滑着翻，桌面上用方向键。
const lightboxIndex = ref(-1)
const lightboxItems = computed(() => moment.value?.media ?? [])

function openLightbox(id: string) {
  const index = lightboxItems.value.findIndex((item) => item.id === id)
  if (index >= 0) lightboxIndex.value = index
}

function closeLightbox() {
  lightboxIndex.value = -1
}

function stepLightbox(delta: number) {
  const count = lightboxItems.value.length
  if (count === 0) return
  lightboxIndex.value = (lightboxIndex.value + delta + count) % count
}

onMounted(async () => {
  if (!token.value) {
    failed.value = true
    loading.value = false
    return
  }
  try {
    moment.value = await api.getShared(token.value)
  } catch {
    failed.value = true
  } finally {
    loading.value = false
  }
})


</script>

<template>
  <div class="shared-view">
    <main class="stage">
      <p v-if="loading" class="notice">正在打开…</p>

      <div v-else-if="failed" class="notice notice--failed">
        <span class="notice__line">这个链接打不开了</span>
        <span class="notice__hint">它可能已经被收回，或者从来就没有过。</span>
      </div>

      <article v-else-if="moment" class="sheet">
        <time class="sheet__when">{{ whenLabel }}</time>
        <p v-if="placeLabel" class="sheet__place">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" class="h-3.5 w-3.5">
            <path
              d="M12 21s6.5-5.6 6.5-10.3A6.5 6.5 0 0 0 5.5 10.7C5.5 15.4 12 21 12 21Z"
              stroke-linejoin="round"
            />
            <circle cx="12" cy="10.4" r="2.3" />
          </svg>
          {{ placeLabel }}
        </p>

        <h1 v-if="moment.title" class="sheet__title title-serif">{{ moment.title }}</h1>
        <h1 v-else class="sheet__title sheet__title--muted title-serif">未命名的这一刻</h1>

        <p v-if="moment.content" class="sheet__text">{{ moment.content }}</p>

        <ul v-if="moment.tags.length" class="sheet__tags">
          <li v-for="tag in moment.tags" :key="tag" class="tag-chip" :style="{ '--tag-hue': tagHue(tag) }">
            {{ tag }}
          </li>
        </ul>

        <!-- 画面走和站内同一份组件：拼贴、缩略图、懒加载、视频的 preload 都一样。
             能点开看大图，手机上左右滑着翻。 -->
        <MomentGallery
          :media="moment.media"
          :title="moment.title"
          interactive
          @select="openLightbox"
        />

        <p class="sheet__foot">
          <span class="sheet__foot-dot" aria-hidden="true" />
          有人把这一刻递给了你
        </p>
      </article>

      <!-- 配乐与氛围音：右下角那枚圆，和站内是同一个组件。
           不自动播——陌生人点开链接不该被声音扑一脸。 -->
      <MusicOrb :music="moment?.music ?? null" :ambient="moment?.ambient ?? null" />

      <MomentLightbox
        :items="lightboxItems"
        :index="lightboxIndex"
        :title="moment?.title || ''"
        @close="closeLightbox"
        @step="stepLightbox"
      />
    </main>
  </div>
</template>

<style scoped>
.shared-view {
  min-height: 100dvh;
}

/* 版心与站内的记忆页对齐：同样是 46rem、同样的左右留白，
   这样同一个链接在手机上打开，行宽和站内看是一致的 */
.stage {
  display: flex;
  justify-content: center;
  max-width: 46rem;
  margin: 0 auto;
  /* 底部留够：右下角那枚播放圆别压住最后一行字 */
  padding: clamp(2.2rem, 6vh, 3.6rem) clamp(1.2rem, 4vw, 2rem) 9rem;
}

.notice {
  align-self: center;
  text-align: center;
  font-size: 0.92rem;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
}

.notice--failed {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding-top: 22dvh;
}

.notice__line {
  font-family: var(--font-serif);
  font-size: 1.16rem;
  color: var(--color-ink-100);
}

.notice__hint {
  font-size: 0.84rem;
  color: color-mix(in oklab, var(--color-ink-400) 90%, transparent);
}

/* 不再套一张卡片：站内页就是直接铺在纸上的，两处得是同一版 */
.sheet {
  width: 100%;
}

.sheet__when {
  display: block;
  font-size: 0.82rem;
  letter-spacing: 0.14em;
  color: color-mix(in oklab, var(--color-glow-400) 88%, transparent);
}

.sheet__place {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  margin: 0.45rem 0 0;
  font-size: 0.82rem;
  letter-spacing: 0.06em;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
}

.sheet__title {
  margin: 0.7rem 0 0;
  font-size: clamp(1.6rem, 4.4vw, 2.35rem);
  line-height: 1.4;
  color: var(--color-ink-50);
}

.sheet__title--muted {
  color: color-mix(in oklab, var(--color-ink-400) 88%, transparent);
  font-size: clamp(1.3rem, 3.6vw, 1.8rem);
}

.sheet__text {
  margin: 1.1rem 0 0;
  font-size: 1rem;
  line-height: 2.05;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  color: color-mix(in oklab, var(--color-ink-100) 95%, transparent);
}

.sheet__tags {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
  margin: 1.2rem 0 0;
  padding: 0;
  list-style: none;
}

.tag-chip {
  padding: 0.2rem 0.66rem;
  border-radius: 999px;
  background: color-mix(in oklab, oklch(0.7 0.12 var(--tag-hue)) 12%, transparent);
  font-size: 0.76rem;
  color: oklch(0.9 0.06 var(--tag-hue));
}

.sheet__foot {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  margin: 2rem 0 0;
  font-size: 0.78rem;
  letter-spacing: 0.08em;
  color: color-mix(in oklab, var(--color-ink-400) 88%, transparent);
}

.sheet__foot-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--color-glow-400);
  box-shadow: 0 0 10px 2px color-mix(in oklab, var(--color-glow-500) 55%, transparent);
}
</style>
