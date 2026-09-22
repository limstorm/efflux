<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { onBeforeRouteLeave, useRouter } from 'vue-router'

import { api, ApiError } from '@/api/client'
import type { MomentDetail } from '@/api/types'
import DemoExit from '@/components/DemoExit.vue'
import MomentGallery from '@/components/MomentGallery.vue'
import MomentLightbox from '@/components/MomentLightbox.vue'
import MusicOrb from '@/components/MusicOrb.vue'
import { useDemo } from '@/composables/useDemo'
import { useToast } from '@/composables/useToast'
import { useMomentsStore } from '@/stores/moments'
import { formatFullDate, formatClock, formatPlace } from '@/utils/format'
import { tagHue } from '@/utils/tagColor'

const props = defineProps<{ id: string }>()

const router = useRouter()
const store = useMomentsStore()
const toast = useToast()

/**
 * 进页面用的第一帧：从光河的 store 里摘一条摘要来铺。
 *
 * 之前 moment 从 null 起步、等 onMounted 里的 load() 回来才画，于是进来先是一屏
 * 骨架（很亮的纸面），数据到了才"啪"地出现内容。逐帧量整屏亮度（手机尺寸 + 4G）：
 *   96~373ms  239.7（骨架）→ 529ms 179.2（Δ-60）→ 729ms 157.8（Δ-21）→ 此后平稳。
 * 两次跳变共 80 个亮度单位——看到的就是"背景一变、然后闪一下"。
 *
 * store.items 里本来就有这条记忆的标题、封面缩略图与时间，够把首屏铺满；拿它当第一帧，
 * 等详情回来只是补上正文与更多媒体。直接从链接进来（store 里没有）时才走骨架。
 */
function seedFromStore(id: string): MomentDetail | null {
  const summary = store.items.find((item) => item.id === id)
  if (!summary) return null
  return {
    id: summary.id,
    title: summary.title,
    // 正文先用摘要：篇幅接近，免得整块先矮后高再跳一下
    content: summary.excerpt,
    happenedAt: summary.happenedAt,
    createdAt: summary.createdAt,
    updatedAt: summary.updatedAt,
    tags: summary.tags,
    // 封面先当唯一的媒体：首屏就有画面，详情回来再补其余几张
    media: summary.cover ? [summary.cover] : [],
    music: null,
    ambient: null,
    place: summary.place,
    shareToken: null,
  }
}

const moment = ref<MomentDetail | null>(seedFromStore(props.id))
const loading = ref(!moment.value)
const error = ref<string | null>(null)
const deleting = ref(false)
const deleteOpen = ref(false)
const deleteBtnRef = ref<HTMLButtonElement | null>(null)
const cancelBtnRef = ref<HTMLButtonElement | null>(null)

/** 右下角那枚播放圆：曲子与氛围音都由它自己管（空格键要能遥控它） */
const orbRef = ref<InstanceType<typeof MusicOrb> | null>(null)

// ---------------------------------------------------------------- 看大图
const lightboxIndex = ref(-1)
const lightboxItems = computed(() => moment.value?.media ?? [])
const lightboxCurrent = computed(() => lightboxItems.value[lightboxIndex.value] ?? null)
const lightboxOpen = computed(() => lightboxIndex.value >= 0 && Boolean(lightboxCurrent.value))

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

/**
 * 背景那层光晕用的图。
 *
 * 这里原来直接挂**原图**——一张 4096×3072、三五 MB 的照片，再套一层全屏
 * filter: blur(84px)。桌面扛得住，手机上要算几百毫秒：等它算完，整页的底色
 * 会"变一次"（用户的原话：进来之后背景好像重绘了一下，也会闪）。
 *
 * 两处都改便宜：用缩略图（几百 KB，而且拼贴那边多半已经把它读进缓存了），
 * 模糊半径也收一半——84px 模糊之后本来就只是一团色，看不出细节差别。
 * 另外等它真的就绪才淡入，不再"啪"地出现。
 */
const backdropSrc = computed(() => {
  const cover = moment.value?.media.find((item) => item.kind === 'image')
  return cover?.thumbUrl ?? cover?.url ?? null
})

const backdropReady = ref(false)

watch(
  backdropSrc,
  (src) => {
    backdropReady.value = false
    if (!src) return
    const img = new Image()
    img.onload = () => {
      backdropReady.value = true
    }
    img.src = src
    // 这张图多半刚从光河那边看到过、就在内存缓存里：那就别淡入了。
    // 淡入虽然柔和，但那也是一次"背景变化"（实测约 21 个亮度单位），
    // 而且首帧就带上 is-ready 时 CSS 不跑过渡，等于第一帧就画好。
    if (img.complete) backdropReady.value = true
  },
  { immediate: true },
)

const backdropStyle = computed(() => {
  const src = backdropSrc.value
  return src ? { backgroundImage: `url("${src}")` } : undefined
})

const whenLabel = computed(() => {
  if (!moment.value) return ''
  const date = new Date(moment.value.happenedAt)
  const created = new Date(moment.value.createdAt)
  const drifted = Math.abs(date.getTime() - created.getTime()) > 90_000
  return drifted ? `${formatFullDate(date)} · ${formatClock(date)}` : formatFullDate(date)
})

/** 地点：城市与具体地点拼一起，见 utils/format 的 formatPlace */
const placeLabel = computed(() => formatPlace(moment.value?.place))

/** 确认框里指出「删的是哪一条」 */
const deleteSubject = computed(() =>
  moment.value?.title ? `「${moment.value.title}」` : whenLabel.value,
)

async function load(id: string) {
  // 已经有东西可看（从 store 摘下来的那一帧）就别再切回骨架：切一次就是闪一下
  if (!moment.value) loading.value = true
  error.value = null
  try {
    moment.value = await api.getMoment(id)
  } catch (err) {
    error.value = err instanceof ApiError ? err.message : '打不开这段记忆'
  } finally {
    loading.value = false
  }
}



function goBack() {
  const state = window.history.state as { back?: string | null } | null
  if (state?.back) router.back()
  else void router.push({ name: 'home' })
}

// 离开这段记忆时留个锚点，回到光河好对准它。
// 用路由守卫而不是返回按钮：点按钮、浏览器后退、键盘 Esc 都算数。
// 目标是搜索页（或别处）就不打扰光河——它停在原处就好。
onBeforeRouteLeave((to) => {
  if (!moment.value || to.name !== 'home') return
  store.setAnchor({ id: moment.value.id, at: moment.value.happenedAt })
})

function edit() {
  if (!moment.value) return
  router.push({ name: 'create', query: { edit: moment.value.id } })
}

function askRemove() {
  if (!moment.value || deleting.value) return
  deleteOpen.value = true
}

function cancelRemove() {
  if (deleting.value) return
  deleteOpen.value = false
}

async function confirmRemove() {
  if (!moment.value || deleting.value) return
  deleting.value = true
  try {
    await api.deleteMoment(moment.value.id)
    store.removeLocal(moment.value.id)
    deleteOpen.value = false
    toast.success('这段记忆已经放下了')
    goBack()
  } catch (err) {
    toast.error(err instanceof ApiError ? err.message : '删除失败了')
  } finally {
    deleting.value = false
  }
}

function onKeydown(event: KeyboardEvent) {
  if (deleteOpen.value) {
    if (event.key === 'Escape') cancelRemove()
    return
  }

  if (lightboxOpen.value) {
    if (event.key === 'Escape') closeLightbox()
    else if (event.key === 'ArrowLeft') stepLightbox(-1)
    else if (event.key === 'ArrowRight') stepLightbox(1)
    return
  }

  // 演示进行中：Esc 只退出演示，不动这一页（滚到哪就停在哪）
  if (event.key === 'Escape' && demo.running.value) {
    demo.stop()
    return
  }

  if (event.key === 'Escape') goBack()
  if (event.key === ' ' && orbRef.value?.ready) {
    event.preventDefault()
    void orbRef.value.toggle()
  }
}

// ---------------------------------------------------------------- 演示

const demo = useDemo()
let scrollRaf = 0
let demoTimer = 0

/**
 * 演示里这一段被打开后：从顶上慢慢滚到底，停一会儿，再自己退回光河。
 * 速度随篇幅伸缩——短的一页至少七秒，长的一页最多二十四秒；
 * 首尾各带一小段缓入缓出，走起来连绵，不像被推着走。
 */
function demoScroll() {
  window.scrollTo(0, 0)
  let y = 0
  let last = 0

  const step = (ts: number) => {
    if (!demo.running.value) {
      scrollRaf = 0
      return
    }
    const dt = last ? Math.min((ts - last) / 1000, 0.05) : 0.016
    last = ts

    const bottom = Math.max(0, document.documentElement.scrollHeight - window.innerHeight)
    const progress = bottom > 0 ? y / bottom : 0
    const ramp = Math.min(1, Math.min(progress, 1 - progress) / 0.12) * 0.35 + 0.65
    const speed = (bottom / Math.min(24, Math.max(7, bottom / 120))) * ramp
    y = Math.min(y + speed * dt, bottom)
    window.scrollTo(0, y)

    if (y >= bottom - 0.5) {
      scrollRaf = 0
      // 篇幅短到不用滚的，也在这儿多待一会儿——演示是给人看的，不是翻页
      const dwell = bottom > 2 ? 1800 : 6200
      demoTimer = window.setTimeout(() => {
        demoTimer = 0
        if (demo.running.value) goBack()
      }, dwell)
      return
    }
    scrollRaf = requestAnimationFrame(step)
  }

  scrollRaf = requestAnimationFrame(step)
}

function stopDemoScroll() {
  if (scrollRaf) cancelAnimationFrame(scrollRaf)
  scrollRaf = 0
  window.clearTimeout(demoTimer)
  demoTimer = 0
}

/** 这一段是演示打开的才开始滚：状态可能随挂载一起到，也可能晚一步 */
function startDemoScrollIfMine() {
  if (!demo.running.value || demo.opened.value !== props.id) return
  stopDemoScroll()
  demoScroll()
}

watch(
  () => (demo.running.value && demo.opened.value === props.id ? true : null),
  (should) => {
    if (should) startDemoScrollIfMine()
  },
)

// 演示被中途叫停（点了停止、按了 Esc），滚动立刻停下
watch(
  () => demo.running.value,
  (running) => {
    if (!running) stopDemoScroll()
  },
)

onMounted(async () => {
  window.addEventListener('keydown', onKeydown)
  await load(props.id)

  // 演示把这一段打开时，状态在挂载前就已就位——挂载即开始下滚
  startDemoScrollIfMine()
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
  stopDemoScroll()
})

watch(
  () => props.id,
  (id) => {
    // 换到另一条记忆：先把它的摘要铺上，再去取详情
    moment.value = seedFromStore(id)
    void load(id)
  },
)

// ---------------------------------------------------------------- 分享
const shareOpen = ref(false)
const sharing = ref(false)
const shareToken = ref<string | null>(null)

const shareLink = computed(() =>
  shareToken.value ? `${window.location.origin}/s/${shareToken.value}` : '',
)

function openShare() {
  // 卡片里已经带着凭证就直接用，省一次往返
  shareToken.value = moment.value?.shareToken ?? null
  shareOpen.value = true
}

async function createShare() {
  if (!moment.value || sharing.value) return
  sharing.value = true
  try {
    const { token } = await api.shareMoment(moment.value.id)
    shareToken.value = token
    moment.value.shareToken = token
    await copyLink()
  } catch {
    toast.error('没能生成分享链接')
  } finally {
    sharing.value = false
  }
}

async function copyLink() {
  if (!shareLink.value) return
  try {
    await navigator.clipboard.writeText(shareLink.value)
    toast.success('链接已复制，发给朋友吧')
  } catch {
    // 有些浏览器在非 HTTPS 下不给剪贴板权限，那就让人自己选中
    toast.info('复制不了，手动选中链接吧')
  }
}

async function revokeShare() {
  if (!moment.value) return
  try {
    await api.unshareMoment(moment.value.id)
    shareToken.value = null
    moment.value.shareToken = null
    shareOpen.value = false
    toast.success('链接已经作废')
  } catch {
    toast.error('收回失败了，再试一次')
  }
}

// 打开确认时把焦点交给「再想想」，习惯性敲回车也删不掉；关闭后交还给删除按钮
watch(deleteOpen, async (open) => {
  await nextTick()
  if (open) cancelBtnRef.value?.focus()
  else deleteBtnRef.value?.focus()
})
</script>

<template>
  <div class="moment-view">
    <div
      class="backdrop"
      :class="{ 'is-ready': backdropReady }"
      :style="backdropStyle"
      aria-hidden="true"
    />

    <header class="topbar" :class="{ 'is-demo': demo.running.value }">
      <button type="button" class="hud-icon" aria-label="返回" title="返回（Esc）" @click="goBack">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" class="h-5 w-5">
          <path d="M14.5 5.5 8 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>

      <div v-if="moment" class="topbar__actions">
        <button type="button" class="hud-icon" title="编辑" aria-label="编辑这段记忆" @click="edit">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" class="h-4.5 w-4.5">
            <path d="M4 20h4l10.5-10.5a2.1 2.1 0 0 0-3-3L5 17v3Z" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
        <button
          type="button"
          class="hud-icon"
          :class="{ 'is-shared': moment?.shareToken }"
          title="分享"
          aria-label="分享这段记忆"
          @click="openShare"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" class="h-4.5 w-4.5">
            <path d="M12 15.5V4m0 0L8.2 7.8M12 4l3.8 3.8" stroke-linecap="round" stroke-linejoin="round" />
            <path d="M5.5 13.5v5A1.5 1.5 0 0 0 7 20h10a1.5 1.5 0 0 0 1.5-1.5v-5" stroke-linecap="round" />
          </svg>
        </button>
        <button
          ref="deleteBtnRef"
          type="button"
          class="hud-icon hud-icon--danger"
          title="删除"
          aria-label="删除这段记忆"
          :disabled="deleting"
          @click="askRemove"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" class="h-4.5 w-4.5">
            <path d="M5 7h14M10 7V5.5A1.5 1.5 0 0 1 11.5 4h1A1.5 1.5 0 0 1 14 5.5V7m-7.5 0 .8 11.2A1.8 1.8 0 0 0 9.1 20h5.8a1.8 1.8 0 0 0 1.8-1.8L17.5 7" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
      </div>
    </header>

    <main v-if="loading" class="stage">
      <div class="skeleton-block" />
      <div class="skeleton-block skeleton-block--short" />
    </main>

    <main v-else-if="error" class="stage stage--center">
      <p class="notice">{{ error }}</p>
      <button type="button" class="ghost-btn px-5 py-2 text-sm" @click="goBack">回到光河</button>
    </main>

    <main v-else-if="moment" class="stage">
      <article class="sheet">
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

        <MomentGallery
          :media="moment.media"
          :title="moment.title"
          interactive
          @select="openLightbox"
        />

        <p v-if="moment.content" class="sheet__prose">{{ moment.content }}</p>

        <ul v-if="moment.tags.length" class="sheet__tags">
          <li v-for="tag in moment.tags" :key="tag">
            <button
              type="button"
              class="tag-pill"
              :style="{ '--tag-hue': tagHue(tag) }"
              @click="router.push({ name: 'search', query: { tags: tag } })"
            >
              {{ tag }}
            </button>
          </li>
        </ul>
      </article>
    </main>

    <!-- 看大图（分享页用的是同一个组件；手机上可以左右滑着翻） -->
    <MomentLightbox
      :items="lightboxItems"
      :index="lightboxIndex"
      :title="moment?.title || ''"
      @close="closeLightbox"
      @step="stepLightbox"
    />

    <!-- 分享：凭证就是钥匙，收回来旧链接立刻作废 -->
    <Transition name="page">
      <div
        v-if="shareOpen"
        class="confirm"
        role="dialog"
        aria-modal="true"
        aria-labelledby="share-title"
        @click.self="shareOpen = false"
      >
        <div class="confirm__card">
          <span class="confirm__ember" aria-hidden="true" />

          <h2 id="share-title" class="confirm__title title-serif">把这一刻递出去</h2>

          <template v-if="shareLink">
            <p class="confirm__desc">
              <span class="confirm__hint">拿到链接的人不用口令也能读，随时可以收回。</span>
            </p>
            <input
              class="paper-input share__link"
              :value="shareLink"
              readonly
              aria-label="分享链接"
              @focus="($event.target as HTMLInputElement).select()"
            />
            <div class="confirm__actions">
              <button type="button" class="confirm__btn confirm__btn--ghost" @click="copyLink">
                复制链接
              </button>
              <button type="button" class="confirm__btn confirm__btn--danger" @click="revokeShare">
                收回
              </button>
            </div>
          </template>

          <template v-else>
            <p class="confirm__desc">
              <span class="confirm__hint">
                会生成一条只有拿到链接才能看的地址，之后随时能收回。
              </span>
            </p>
            <div class="confirm__actions">
              <button
                type="button"
                class="confirm__btn confirm__btn--ghost"
                @click="shareOpen = false"
              >
                先不分享
              </button>
              <button
                type="button"
                class="confirm__btn confirm__btn--primary"
                :disabled="sharing"
                @click="createShare"
              >
                {{ sharing ? '正在生成…' : '生成链接' }}
              </button>
            </div>
          </template>
        </div>
      </div>
    </Transition>

    <!-- 删除确认：破坏性操作多一步，就少一点遗憾 -->
    <Transition name="page">
      <div
        v-if="deleteOpen"
        class="confirm"
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        @click.self="cancelRemove"
      >
        <div class="confirm__card">
          <span class="confirm__ember" aria-hidden="true" />

          <h2 id="confirm-title" class="confirm__title title-serif">要放下这段记忆吗？</h2>

          <p class="confirm__desc">
            <span class="confirm__subject">{{ deleteSubject }}</span>
            <span class="confirm__hint">将被永久删除，无法找回。</span>
          </p>

          <div class="confirm__actions">
            <button
              ref="cancelBtnRef"
              type="button"
              class="confirm__btn confirm__btn--ghost"
              @click="cancelRemove"
            >
              再想想
            </button>
            <button
              type="button"
              class="confirm__btn confirm__btn--danger"
              :disabled="deleting"
              @click="confirmRemove"
            >
              {{ deleting ? '正在删除…' : '删除' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <!-- 配乐与氛围音：右下角那枚圆（分享页用的是同一个组件） -->
    <MusicOrb
      ref="orbRef"
      :music="moment?.music ?? null"
      :ambient="moment?.ambient ?? null"
      :raised="demo.running.value"
      autoplay
    />

    <!-- 演示进行中的出口：固定右下角（头部那些按钮这时都收起来了） -->
    <DemoExit />
  </div>
</template>

<style scoped>
.moment-view {
  position: relative;
  min-height: 100vh;
  min-height: 100dvh;
}

.backdrop {
  position: fixed;
  top: 0;
  left: 0;
  z-index: 0;
  background-position: center;
  background-size: cover;
  /* 把记忆的光晕染在纸面上，而不是藏进黑暗里。
     模糊的代价正比于「半径 × 面积」，而这一层是全屏的：手机上 48px 要让几百万
     个像素各取样一遍，算一次就是几百毫秒——而且此后每次重画都要再算一遍，
     于是"进页面之后还会再闪一下"（连顶栏那些按钮都跟着闪，因为是整屏重画）。
     办法：只在一小块（四分之一边长）上做模糊，再用 transform 放大四倍铺满
     —— filter 先算、transform 后放大，视觉上还是 48px 的模糊，代价只有十六分之一。
     淡入也因此便宜了，不再需要 will-change 常驻一个全屏图层。 */
  width: 25%;
  height: 25%;
  transform-origin: 0 0;
  transform: scale(4);
  filter: blur(12px) saturate(1.1) brightness(1.22);
  opacity: 0;
  transition: opacity 0.45s var(--ease-out-soft);
}

.backdrop.is-ready {
  /* 0.4 是这层原本的浓度：它只是把记忆的光晕淡淡染在纸面上 */
  opacity: 0.4;
  pointer-events: none;
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
  padding-top: max(1.2rem, calc(var(--safe-top) + 0.8rem));
  padding-bottom: 1.2rem;
  padding-left: max(clamp(1rem, 3vw, 2.4rem), calc(var(--safe-left) + 0.6rem));
  padding-right: max(clamp(1rem, 3vw, 2.4rem), calc(var(--safe-right) + 0.6rem));
  pointer-events: none;
}

.topbar > * {
  pointer-events: auto;
}

.topbar__actions {
  display: flex;
  gap: 0.5rem;
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
  opacity: 0.8;
  box-shadow: 0 8px 22px -16px color-mix(in oklab, var(--color-shade) 90%, transparent);
  transition:
    opacity 0.4s var(--ease-out-soft),
    color 0.4s var(--ease-out-soft),
    border-color 0.4s var(--ease-out-soft),
    transform 0.25s var(--ease-out-soft);
}

.hud-icon:hover {
  opacity: 1;
  color: var(--color-glow-300);
  border-color: color-mix(in oklab, var(--color-glow-500) 62%, transparent);
}

.hud-icon:active {
  transform: scale(0.94);
}

.hud-icon--danger:hover {
  opacity: 1;
  color: var(--color-ember-500);
  border-color: color-mix(in oklab, var(--color-ember-500) 58%, transparent);
  background: color-mix(in oklab, var(--color-ember-500) 8%, var(--color-ink-900));
}

/* ---------------- 内容 ---------------- */
.stage {
  position: relative;
  z-index: 1;
  max-width: 46rem;
  margin: 0 auto;
  padding: clamp(4.6rem, 9vh, 6.4rem) clamp(1.2rem, 4vw, 2rem) 9rem;
}

.stage--center {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1.4rem;
  min-height: 70dvh;
  justify-content: center;
}

.sheet__when {
  display: block;
  font-size: 0.82rem;
  letter-spacing: 0.14em;
  color: color-mix(in oklab, var(--color-glow-400) 88%, transparent);
}

/* 地点：跟在日期底下，和日期一个字号，别抢标题 */
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

/* 画廊（相册拼贴 + 视频行）在 components/MomentGallery.vue —— 分享页用的是同一份，
   这样"我在站内看到的"和"别人打开链接看到的"排版一致。 */

/* 拼贴相关的样式都跟着画廊进组件了 */

.sheet__prose {
  margin: 2.1rem 0 0;
  font-size: 1rem;
  line-height: 2.05;
  letter-spacing: 0.01em;
  color: var(--color-ink-100);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.sheet__tags {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin: 2.3rem 0 0;
  padding: 0;
  list-style: none;
}

.notice {
  margin: 0;
  font-size: 0.95rem;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
}

.skeleton-block {
  width: min(46rem, 92vw);
  height: 44dvh;
  border-radius: var(--radius-lg);
  margin-bottom: 1.6rem;
}

.skeleton-block--short {
  height: 5rem;
}

/* ---------------- 删除确认 ---------------- */
.confirm {
  position: fixed;
  inset: 0;
  z-index: 70;
  display: grid;
  place-items: center;
  padding: 1.4rem;
  background: color-mix(in oklab, var(--color-scrim) 88%, transparent);
  backdrop-filter: blur(14px);
}

.confirm__card {
  width: min(23rem, 100%);
  padding: 1.9rem 1.7rem 1.5rem;
  border-radius: 20px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 62%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 94%, transparent);
  box-shadow: 0 40px 90px -44px color-mix(in oklab, var(--color-shade) 95%, transparent);
  text-align: center;
}

/* 一簇将要熄灭的光，和音乐里跳动的是同一颗 */
.confirm__ember {
  display: block;
  width: 7px;
  height: 7px;
  margin: 0 auto 1rem;
  border-radius: 50%;
  background: var(--color-ember-400);
  box-shadow: 0 0 16px 3px color-mix(in oklab, var(--color-ember-500) 66%, transparent);
}

.confirm__title {
  margin: 0;
  font-size: 1.22rem;
  line-height: 1.5;
  color: var(--color-ink-50);
}

.confirm__desc {
  margin: 0.8rem 0 0;
  font-size: 0.86rem;
  line-height: 1.9;
}

.confirm__subject {
  display: block;
  margin-bottom: 0.22rem;
  font-size: 0.96rem;
  color: var(--color-ink-100);
  overflow-wrap: anywhere;
}

.confirm__hint {
  display: block;
  color: color-mix(in oklab, var(--color-ink-400) 92%, transparent);
}

.confirm__actions {
  display: flex;
  gap: 0.6rem;
  margin-top: 1.7rem;
}

.confirm__btn {
  flex: 1;
  padding: 0.64rem 0;
  border-radius: 999px;
  background: transparent;
  font-size: 0.86rem;
  letter-spacing: 0.05em;
  transition:
    background-color 0.35s var(--ease-out-soft),
    border-color 0.35s var(--ease-out-soft),
    color 0.35s var(--ease-out-soft),
    transform 0.25s var(--ease-out-soft);
}

.confirm__btn:active {
  transform: scale(0.97);
}

.confirm__btn--ghost {
  border: 1px solid color-mix(in oklab, var(--color-ink-500) 55%, transparent);
  color: var(--color-ink-100);
}

.confirm__btn--ghost:hover {
  border-color: color-mix(in oklab, var(--color-glow-500) 58%, transparent);
  color: var(--color-glow-300);
}

.confirm__btn--danger {
  border: 1px solid color-mix(in oklab, var(--color-ember-500) 46%, transparent);
  background: color-mix(in oklab, var(--color-ember-500) 8%, var(--color-ink-900));
  color: var(--color-ember-500);
}

.confirm__btn--danger:hover {
  border-color: color-mix(in oklab, var(--color-ember-500) 68%, transparent);
  background: color-mix(in oklab, var(--color-ember-500) 15%, var(--color-ink-900));
}

.confirm__btn--danger:disabled {
  opacity: 0.6;
  cursor: default;
}

.confirm__btn--primary {
  border: 1px solid color-mix(in oklab, var(--color-glow-500) 55%, transparent);
  background: color-mix(in oklab, var(--color-glow-500) 12%, var(--color-ink-900));
  color: var(--color-glow-300);
}

.confirm__btn--primary:hover {
  border-color: color-mix(in oklab, var(--color-glow-500) 75%, transparent);
  background: color-mix(in oklab, var(--color-glow-500) 20%, var(--color-ink-900));
}

.confirm__btn--primary:disabled {
  opacity: 0.6;
  cursor: default;
}

.share__link {
  margin-top: 0.9rem;
  font-size: 0.8rem;
  text-align: center;
}

/* 已经分享过的记忆，按钮上留个记号 */
.hud-icon.is-shared {
  color: var(--color-glow-400);
}

@media (max-width: 760px) {
  .stage {
    /* 给小屏右下角那枚圆留出位置，别压住最后一段文字 */
    padding-bottom: 6rem;
  }
}

/* ---------------- 演示 ---------------- */
/* 演示时头部按钮都收起来（出口在右下角，见 DemoExit）；滚到底会自己退回光河 */
.topbar.is-demo .hud-icon,
.topbar.is-demo .topbar__actions {
  display: none;
}
</style>
