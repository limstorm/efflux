<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'

import type { MediaItem } from '@/api/types'

/**
 * 看大图。记忆页、分享页、相册三处共用这一份——
 * 相册要显示的日期、标签、「回到这段记忆」那些，走底下的 #meta 插槽。
 *
 * 手机上横着划过去看下一张：划的时候图跟着手指走，松手弹到位，同时旧的滑出、新的滑进。
 *
 * 手势这块踩过坑，写清楚免得改坏：
 * · 触摸走 touch 事件、鼠标走 pointer 事件，两套分开接。全都用 pointer 的话，
 *   真机上斜着划会被浏览器判成滚动、发 pointercancel，那一下就白划了——
 *   这也是"分享页划不动"的真正原因（同一份组件，两边一起修）。
 * · touchmove 里对"横向为主"的手势 preventDefault()，把这一下明确要过来；
 *   配 touch-action: pan-y，纵向仍是原生滚动（长图要能滚）。
 * · 方向一旦定成横向就不再改判，中途手指抖一下也不会忽然变成纵向。
 * · 落在视频上时不接管：那是播放器的进度条，别抢。
 */
/**
 * 大图只用到这几样。记忆页传的是完整的 MediaItem，相册传的是它自己那套
 * 带标题与标签的条目，结构上都满足这里——所以三处能共用同一份实现。
 */
type LightboxItem = Pick<MediaItem, 'id' | 'kind' | 'url' | 'thumbUrl'>

const props = withDefaults(
  defineProps<{
    items: LightboxItem[]
    /** 当前看的是第几张；-1 表示没打开 */
    index: number
    title?: string
    /** 能不能在这里把当前这张移除（创建/编辑记忆时要，记忆页与相册不要） */
    removable?: boolean
  }>(),
  { title: '', removable: false },
)

const emit = defineEmits<{ close: []; step: [delta: number]; remove: [id: string] }>()

const current = computed(() => props.items[props.index] ?? null)
const open = computed(() => props.index >= 0 && Boolean(current.value))

/**
 * 原图到哪一步了。缩略图垫在它下面：
 * 还没到（loading）先看缩略图、到不了（failed）就一直看缩略图，就是别给个黑框。
 */
const imgState = ref<'loading' | 'ready' | 'failed'>('loading')
/** 这次会话里完整显示过的图：再划回来时直接亮着，不必再淡入一遍 */
const shown = new Set<string>()

/** 滑动翻页的最小横向位移（px）。太灵敏会误翻，太钝了划不动 */
const SWIPE_MIN = 48
/** 跟手的最远距离：再往后拉就走阻尼，像有根皮筋 */
const FOLLOW_MAX = 150

/** 手指拽出来的位移（px）。松手归零，交给换页动画接手 */
const dragX = ref(0)
const dragging = ref(false)
/** 上一次翻页的方向：决定旧的从哪边出去、新的从哪边进来 */
const direction = ref<1 | -1>(1)

let from: { x: number; y: number } | null = null
let axis: 'x' | 'y' | null = null
/** 刚划完那一下会跟出一个 click，别顺手把大图关了 */
let swallowClick = false

function begin(x: number, y: number, target: EventTarget | null) {
  if (props.items.length < 2) return
  // 视频上那一条是进度条，别抢
  if (target instanceof Element && target.closest('video')) return
  from = { x, y }
  axis = null
  dragging.value = false
  dragX.value = 0
}

/** 返回 true 表示这一下归我们管（横向），调用方可以 preventDefault */
function move(x: number, y: number): boolean {
  if (!from) return false
  const dx = x - from.x
  const dy = y - from.y
  if (!axis) {
    // 先走够 8px 再判方向：手指刚落下的那几像素抖得厉害
    if (Math.abs(dx) < 8 && Math.abs(dy) < 8) return false
    axis = Math.abs(dx) > Math.abs(dy) * 1.2 ? 'x' : 'y'
  }
  if (axis !== 'x') return false
  dragging.value = true
  const far = Math.abs(dx)
  const eased = far > FOLLOW_MAX ? FOLLOW_MAX + (far - FOLLOW_MAX) / 3 : far
  dragX.value = Math.sign(dx) * eased
  return true
}

function end() {
  const dx = dragX.value
  const horizontal = axis === 'x'
  from = null
  axis = null
  dragging.value = false
  dragX.value = 0
  if (!horizontal) return
  swallowClick = Math.abs(dx) > 4
  if (Math.abs(dx) < SWIPE_MIN) return
  direction.value = dx < 0 ? 1 : -1
  emit('step', dx < 0 ? 1 : -1)
}

/** 点空白处关闭；刚划完的那一下 click 不算 */
function onBackdrop() {
  if (swallowClick) {
    swallowClick = false
    return
  }
  emit('close')
}

// 触摸：真机上最可靠的一条
function onTouchStart(event: TouchEvent) {
  const touch = event.touches[0]
  if (touch) begin(touch.clientX, touch.clientY, event.target)
}

function onTouchMove(event: TouchEvent) {
  const touch = event.touches[0]
  if (!touch) return
  if (move(touch.clientX, touch.clientY)) event.preventDefault()
}

// 鼠标：桌面上也能拖着换页。触摸不从这里走，免得两套各算一次
function onPointerDown(event: PointerEvent) {
  if (event.pointerType !== 'mouse') return
  begin(event.clientX, event.clientY, event.target)
}

function onPointerMove(event: PointerEvent) {
  if (event.pointerType === 'mouse') move(event.clientX, event.clientY)
}

const videoRef = ref<HTMLVideoElement | null>(null)

/**
 * 视频不看了就把它的下载断掉。
 * 只 pause() 不够：元素从 DOM 里拿掉之后那条连接未必松开，得清掉 src 再 load()，
 * 浏览器才会真的收手——否则退出记忆页之后它还在后台下。
 */
function stopVideo(el: HTMLVideoElement | null) {
  if (!el) return
  el.pause()
  el.removeAttribute('src')
  el.load()
}

/**
 * 从视频划走的那一下：Transition 交给我们的是外面那一层容器，
 * 真正的 <video> 在它里面——所以要往里找一层。
 */
function onFrameLeave(el: Element) {
  const video = el instanceof HTMLVideoElement ? el : el.querySelector('video')
  stopVideo(video)
}

function onKeydown(event: KeyboardEvent) {
  if (!open.value) return
  if (event.key === 'Escape') emit('close')
  else if (event.key === 'ArrowLeft') emit('step', -1)
  else if (event.key === 'ArrowRight') emit('step', 1)
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
  // 离开这一页：预取与正在播的视频都别在后台继续下
  stopWarming()
  stopVideo(videoRef.value)
})

/** 翻页时把滚动位置收回顶部：留着上一张的滚动量，下一张会停在半截 */
watch(
  () => props.index,
  () => {
    const scroller = document.querySelector('.lightbox__scroller')
    if (scroller) scroller.scrollTop = 0
  },
)

watch(
  () => current.value?.id,
  (id) => {
    imgState.value = id && shown.has(id) ? 'ready' : 'loading'
  },
  { immediate: true },
)

function onLoaded() {
  const id = current.value?.id
  if (id) shown.add(id)
  imgState.value = 'ready'
}

/** 预取过的原图 id，同一个不重复取 */
const warmed = new Set<string>()
/** 还在路上的预取：收起或离开时要把它们叫停——已经开始的下载不会自己停 */
const warming = new Map<string, AbortController>()

/**
 * 悄悄把邻居的原图取回来：等划过去的时候，它多半已经在缓存里了。
 * 只往前看两张、往后看一张（人多数是往后翻），而且标成低优先级——别和
 * 正在看的那一张抢带宽。
 *
 * 用 fetch 而不是 new Image()：只有 fetch 能取消。new Image() 一旦开始，
 * 就算离开这一页它也会把整张下完——手机流量就是这么没的。
 * 读完整个 body 是为了让它进 HTTP 缓存（划过去时 <img> 直接命中），但读一块
 * 丢一块，不把图留在内存里。
 */
async function warm(item: LightboxItem) {
  const controller = new AbortController()
  warming.set(item.id, controller)
  try {
    const response = await fetch(item.url, { signal: controller.signal, priority: 'low' })
    const reader = response.body?.getReader()
    if (!reader) return
    for (;;) {
      const { done } = await reader.read()
      if (done) break
    }
  } catch {
    /* 被取消、或网络不好：都不必出声，这只是顺手预取 */
  } finally {
    if (warming.get(item.id) === controller) warming.delete(item.id)
  }
}

function warmNeighbours() {
  const list = props.items
  const from = props.index
  if (from < 0 || list.length < 2) return
  for (const offset of [1, -1, 2]) {
    const item = list[(from + offset + list.length) % list.length]
    if (!item || item.kind !== 'image' || warmed.has(item.id)) continue
    warmed.add(item.id)
    void warm(item)
  }
}

/** 把还在飞的预取全叫停，并把"取过"的记录清掉（缓存里那份可能已经没了） */
function stopWarming() {
  for (const controller of warming.values()) controller.abort()
  warming.clear()
  warmed.clear()
}

// 刻意不 immediate：这一段只在灯箱真的打开之后才跑。
// 原先 immediate 会让它随着记忆页一挂载就启动——页面刚开就偷偷拉三张原图
// （一张好几 MB），恰好挤在进入动画最忙的那几百毫秒里。
watch([() => props.index, () => props.items.length, () => props.items[0]?.id, open], () => {
  if (open.value) warmNeighbours()
})

// 收起大图：在飞的预取叫停，正在放的视频也断掉——留着它们在后台下完，
// 等于用户没看也花流量。这里还没重渲染，所以 videoRef 仍指着那枚视频。
watch(open, (isOpen) => {
  if (isOpen) return
  stopWarming()
  stopVideo(videoRef.value)
})
</script>

<template>
  <Transition name="page">
    <div v-if="open && current" class="lightbox">
      <!-- 内容区自己滚；按钮和页码留在外面，上下滑的时候不跟着跑。
           touch-action 只放开纵向：横向留给翻页手势。 -->
      <div
        class="lightbox__scroller"
        @click.self="onBackdrop"
        @touchstart="onTouchStart"
        @touchmove="onTouchMove"
        @touchend="end"
        @touchcancel="end"
        @pointerdown="onPointerDown"
        @pointermove="onPointerMove"
        @pointerup="end"
        @pointercancel="end"
      >
        <!-- 划动时这一层跟着手指走；换页时旧的滑出、新的滑进 -->
        <div
          class="lightbox__slide"
          :class="{ 'is-dragging': dragging }"
          :style="{ transform: dragX ? `translate3d(${dragX}px, 0, 0)` : '' }"
        >
          <Transition
            :name="direction === 1 ? 'slide-next' : 'slide-prev'"
            @before-leave="onFrameLeave"
          >
            <div :key="current.id" class="lightbox__frame">
              <!-- 缩略图一直垫在底下（不只是"原图没到"的时候）。
                   原来它挂的是 v-if="… && imgState !== 'ready'"：原图一就绪，缩略图当场撤掉，
                   而原图此刻才从 opacity 0 开始淡入——中间那一两帧"缩略图没了、原图还没亮"，
                   就是滑动翻页时闪的那一下。现在两层一直在，原图淡入到不透明为止都有它托着，
                   看到的只是"变清楚了"。 -->
              <img
                v-if="current.kind === 'image' && current.thumbUrl"
                class="lightbox__thumb"
                :src="current.thumbUrl"
                alt=""
                aria-hidden="true"
                draggable="false"
              />

              <video
                v-if="current.kind === 'video'"
                ref="videoRef"
                class="lightbox__media"
                :src="current.url"
                :poster="current.thumbUrl ?? undefined"
                controls
                autoplay
                playsinline
              />
              <img
                v-else
                class="lightbox__media"
                :class="{ 'is-pending': imgState !== 'ready' }"
                :src="current.url"
                :alt="title || '记忆中的画面'"
                draggable="false"
                decoding="async"
                @load="onLoaded"
                @error="imgState = 'failed'"
              />

              <!-- 连缩略图都没有（老数据）时说一句，别让人对着空框猜 -->
              <span v-if="imgState === 'loading' && !current.thumbUrl" class="lightbox__hint">
                正在取这一张…
              </span>

              <!-- 调用方想在图下面放点东西（日期、标签、按钮）就放这儿：
                   它和图同属一层，于是划动时一起走、换页时一起滑 -->
              <slot name="meta" />
            </div>
          </Transition>
        </div>
      </div>

      <!-- 移除放在左上角、图标带一点赭红：关闭在右上角是叉号，
           两者离得远、颜色也不一样，手滑点错的可能性小很多 -->
      <button
        v-if="removable && current"
        type="button"
        class="lightbox__remove"
        aria-label="移除这一段"
        title="移除这一段"
        @click.stop="current && emit('remove', current.id)"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" class="h-5 w-5">
          <path d="M5 7.5h14M10 7.5V5.8a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v1.7" stroke-linecap="round" />
          <path
            d="M6.5 7.5 7.4 19a1.6 1.6 0 0 0 1.6 1.5h6a1.6 1.6 0 0 0 1.6-1.5l.9-11.5"
            stroke-linecap="round"
          />
        </svg>
      </button>

      <button
        type="button"
        class="lightbox__close"
        aria-label="关闭"
        title="关闭（Esc）"
        @click="emit('close')"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" class="h-5 w-5">
          <path d="M6 6l12 12M18 6 6 18" stroke-linecap="round" />
        </svg>
      </button>

      <button
        v-if="items.length > 1"
        type="button"
        class="lightbox__nav lightbox__nav--prev"
        aria-label="上一张"
        title="上一张（←）"
        @click.stop="emit('step', -1)"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" class="h-5 w-5">
          <path d="M14.5 5.5 8 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>

      <button
        v-if="items.length > 1"
        type="button"
        class="lightbox__nav lightbox__nav--next"
        aria-label="下一张"
        title="下一张（→）"
        @click.stop="emit('step', 1)"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" class="h-5 w-5">
          <path d="M9.5 5.5 16 12l-6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>

      <span v-if="items.length > 1" class="lightbox__counter numeral">
        {{ index + 1 }} / {{ items.length }}
      </span>
    </div>
  </Transition>
</template>

<style scoped>
/* 这里刻意不写 backdrop-filter：底色已经是 92% 的深色，模糊只能透出 8%，
   看不出来；但它是全屏的一层，滑大图时每帧都要把背后重算一遍。
   小屏那份总开关本来也会把它关掉（见 main.css 的渲染降级），这里干脆一开始就不写。 */
.lightbox {
  position: fixed;
  inset: 0;
  z-index: 60;
  background: color-mix(in oklab, var(--color-scrim) 92%, transparent);
  overflow: hidden;
  scrollbar-width: none;
}

/* 内容层单独滚，按钮和页码留在弹层上不动 */
.lightbox__scroller {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  padding: clamp(1rem, 4vw, 2.6rem);
  overflow-y: auto;
  scrollbar-width: none;
  /* 纵向留给原生滚动（长图要能滚），横向留给翻页手势 */
  touch-action: pan-y;
}

.lightbox__scroller::-webkit-scrollbar {
  width: 0;
  height: 0;
}

/* 跟手那一层：划的时候不要过渡，松手才走过渡（弹回原位） */
.lightbox__slide {
  position: relative;
  display: grid;
  place-items: center;
  transition: transform 0.26s var(--ease-out-soft);
}

.lightbox__slide.is-dragging {
  transition: none;
}

/* 换页：旧的滑出去、新的从另一侧滑进来，方向由最后一次划动决定 */
.slide-next-enter-active,
.slide-next-leave-active,
.slide-prev-enter-active,
.slide-prev-leave-active {
  transition:
    transform 0.3s var(--ease-out-soft),
    opacity 0.3s var(--ease-out-soft);
}

/* 离场的那一张脱开文档流，否则两张会把内容区撑成两行。
   不用 inset: 0：那会把图片拉伸，交给 .lightbox__slide 的居中即可 */
.slide-next-leave-active,
.slide-prev-leave-active {
  position: absolute;
}

.slide-next-enter-from {
  transform: translate3d(64px, 0, 0);
  opacity: 0;
}

.slide-next-leave-to {
  transform: translate3d(-64px, 0, 0);
  opacity: 0;
}

.slide-prev-enter-from {
  transform: translate3d(-64px, 0, 0);
  opacity: 0;
}

.slide-prev-leave-to {
  transform: translate3d(64px, 0, 0);
  opacity: 0;
}

/* 一张图两层：缩略图垫底、原图在上。两层共用同一套尺寸约束，
   于是想怎么摆都重合——原图顶上来的那一刻不会有位移 */
.lightbox__frame {
  display: grid;
  place-items: center;
  /* 滑动换页时这一层每帧都在动。不给它图层的话，这一帧要连整块全屏的半透明
     幕布一起重画——手机上就是"切一张图、整页闪一下"。 */
  will-change: transform;
}

/* 缩略图与原图叠在同一格；插槽里的东西自然落到下一行 */
.lightbox__thumb,
.lightbox__media {
  grid-area: 1 / 1;
  max-width: 100%;
  /* 相册那侧把说明也摆在图下面，于是给自己留矮一点（见 GalleryView 的 --lightbox-media-max） */
  max-height: var(--lightbox-media-max, 88dvh);
  border-radius: 8px;
  /* 划动时别选到文字、也别触发图片的原生拖拽 */
  user-select: none;
  -webkit-user-drag: none;
}

.lightbox__media {
  box-shadow: 0 40px 90px -44px oklch(0 0 0 / 0.75);
  transition: opacity 0.32s var(--ease-out-soft);
}

/* 原图没到（或到不了）时先别露脸：露出来的会是一个空框 */
.lightbox__media.is-pending {
  opacity: 0;
}

/* 缩略图不加任何滤镜：它本身是低分辨率的小图，柔化看不出来，
   但 filter 会让这一层在划动时每帧重画一遍。两层只靠尺寸约束重合就够了。 */

/* 没有缩略图的老数据：给一句话，别让人对着空框猜 */
.lightbox__hint {
  padding: 0.5rem 0.95rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 55%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 82%, transparent);
  color: color-mix(in oklab, var(--color-ink-300) 90%, transparent);
  font-size: 0.78rem;
  letter-spacing: 0.04em;
}

.lightbox__close,
.lightbox__nav,
.lightbox__remove {
  position: absolute;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.6rem;
  height: 2.6rem;
  border-radius: 999px;
  border: 1px solid oklch(1 0 0 / 0.22);
  background: oklch(1 0 0 / 0.1);
  color: oklch(0.98 0.008 90);
  backdrop-filter: blur(8px);
  transition:
    background-color 0.3s var(--ease-out-soft),
    transform 0.25s var(--ease-out-soft);
}

.lightbox__close:hover,
.lightbox__nav:hover {
  background: oklch(1 0 0 / 0.22);
}

.lightbox__close:active,
.lightbox__nav:active,
.lightbox__remove:active {
  transform: scale(0.94);
}

.lightbox__close {
  top: 1.1rem;
  right: 1.1rem;
}

/* 移除：左上角，图标带一点赭红——和右上角那颗中性色的叉号分得清清楚楚 */
.lightbox__remove {
  top: 1.1rem;
  left: 1.1rem;
  color: oklch(0.84 0.1 32);
}

.lightbox__remove:hover {
  background: oklch(0.58 0.17 32 / 0.34);
  color: oklch(0.9 0.11 32);
}

.lightbox__nav {
  top: 50%;
  /* 用负外边距而不是 translateY：下面 :active 的缩放要占着 transform */
  margin-top: -1.3rem;
}

.lightbox__nav--prev {
  left: 1.1rem;
}

.lightbox__nav--next {
  right: 1.1rem;
}

.lightbox__counter {
  position: absolute;
  bottom: 1.4rem;
  left: 50%;
  transform: translateX(-50%);
  font-size: 0.82rem;
  letter-spacing: 0.14em;
  color: oklch(0.9 0.01 90 / 0.88);
}
</style>
