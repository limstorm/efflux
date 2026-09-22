<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import DemoExit from '@/components/DemoExit.vue'
import DemoMenu from '@/components/DemoMenu.vue'
import RiverNode from '@/components/RiverNode.vue'
import { type RiverNode as RiverNodeLayout, useRiverLayout } from '@/composables/useRiverLayout'
import { type DemoMode, useDemo } from '@/composables/useDemo'
import { useMomentsStore } from '@/stores/moments'
import { useToast } from '@/composables/useToast'
import { formatDotted, relativeTime } from '@/utils/format'

defineOptions({ name: 'HomeView' })

const router = useRouter()
const store = useMomentsStore()

// ---------------------------------------------------------------- 视口
// 光河的纵向位置交给 CSS 的 dvh（见样式里的 --river-y）：
// 它天然会随移动端地址栏收放重算，不必和 JS 的时序较劲。
const viewportWidth = ref(window.innerWidth)

const orderedMoments = computed(() => store.ordered)
const { nodes, totalWidth, markers, indexAt } = useRiverLayout(orderedMoments, viewportWidth)

// ---------------------------------------------------------------- 相机
/**
 * 相机位置。**刻意不用 ref**：它每帧都在动，一旦接进 Vue 的响应式，
 * 整条光河（几十上百个节点）就会跟着每帧重渲染一遍 —— 手机上掉帧多半出在这儿。
 * 这里直接写到 DOM 上（见 paintCamera）；只有「此刻对准哪一段」「是不是到头了」
 * 这类需要界面跟着变的信息，才回到响应式。
 */
let camX = 0
/** 手指按下那一刻的相机——离开光河时优先锁定这一份（见 onDeactivated） */
let pressedAt = 0
let pressedCamX: number | null = null
const trackEl = ref<HTMLElement | null>(null)
const autoFlow = ref(true)
const dragging = ref(false)
/** 光河是否在眼前：被详情页盖住时不该继续往前流 */
const onStage = ref(true)

const SPEED = 44 // px/s —— 记忆流过的速度

/** 光河分段：每段单独成层，宽度（含余量）控制在 GPU 单个图层上限以内 */
const CHUNK = 1200
const CHUNK_PAD = 420

/**
 * 把刻度、浮标、卡片按横坐标分到若干段里。
 * 卡片按**中心**归段，保证每张只属于一段；余量留得足够宽，所以不会有谁被裁掉。
 */
const chunks = computed(() => {
  const total = Math.max(1, totalWidth.value)
  const out: {
    key: number
    origin: number
    width: number
    markers: typeof markers.value
    pips: typeof nodes.value
    nodes: typeof nodes.value
  }[] = []
  for (let start = 0; start < total; start += CHUNK) {
    const edge = start + CHUNK
    out.push({
      key: start,
      // 盒子的左右各留一段余量：卡片宽，靠近边界的不能被裁掉
      origin: start - CHUNK_PAD,
      width: CHUNK + CHUNK_PAD * 2,
      // 归属则按**不重叠**的区间算，否则跨在余量里的会被两段各渲染一次
      markers: markers.value.filter((m) => m.x >= start && m.x < edge),
      pips: nodes.value.filter((n) => n.centerX >= start && n.centerX < edge),
      nodes: nodes.value.filter((n) => {
        const mid = n.x + n.width / 2
        return mid >= start && mid < edge
      }),
    })
  }
  return out
})
const maxCamX = computed(() => Math.max(0, totalWidth.value - viewportWidth.value))
/** 屏幕正中对准第几段。它一帧最多变一次，且多数帧并不变，所以留在响应式里 */
const focusedIndex = ref(0)
const focusedNode = computed(() => nodes.value[focusedIndex.value] ?? null)
/** 是不是已经流到「此刻」了 */
const atEnd = ref(false)

/** 要跟着相机走的元素：光河被切成几段，每段一个（见 chunks） */
let movables: HTMLElement[] = []

function collectMovables() {
  const track = trackEl.value
  movables = track ? Array.from(track.querySelectorAll<HTMLElement>('.river-chunk')) : []
}

/**
 * 把相机位置落到每一段光河上。
 *
 * 为什么分段，而不是别的方式——三条走过的弯路都记在这儿：
 * · 位移写成整条 track 的 transform：track 八千多像素宽，超过手机 GPU 单个
 *   图层的尺寸上限，成不了合成层，于是每帧整幅重绘（光栅 900ms/2.6s）。
 * · 让每张卡片各自成层：光栅省了，但提层集合随对焦切换不断增删，每次都要
 *   新建纹理——不掉帧也能被看出一下一下的顿，还吃显存。
 * · 用视口的 scrollLeft 承载位移：这是最便宜的路（2 秒光栅 3 次），但**滚动
 *   偏移会被取整**（实测写 8075.4 读回来是 8075）。自动流动只有 44px/s，
 *   60Hz 下每帧 0.73px、120Hz 下 0.37px，取整后就成了"停一帧、跳一像素"的
 *   台阶——手指拖动幅度大所以不明显，松手后匀速流动时就特别扎眼。
 *   （试过"整数走滚动、小数走 transform"来补：滚动在合成器上是异步应用的、
 *   transform 是同步的，两半对不齐，屏幕位置变成 1.27/1.27/回跳 0.73，更糟。）
 *
 * 分段之后：每段只有两千像素宽、稳定存在（不增删），单独成层很划算；
 * 位移由合成器应用，子像素不会被取整。
 *
 * 后来又补了一条：**快慢分路**（见下面的 setScrollDrive）。
 * 上面那个"两半对不齐"的坑，只在**同一帧里同时写滚动和 transform** 时才会踩到；
 * 一帧只由其中一条驱动就没事。所以：
 * · 慢速（手指拖动、自动流动、摇镜）走 transform，保住子像素；
 * · 甩出去那一程走视口原生滚动——滚动是浏览器唯一会"提前备图"的管线。
 *   实测（DPR 3，真触摸甩 1270px/s）：松手后 0.1 秒里堆了 80ms 栅格，
 *   整个 1 秒 93ms，而换到滚动之后这一坨就摊开了。
 * 两种模式的落点差不超过半像素。
 */
let scrollDriven = false
/** 切到 transform 模式时滚动停在哪；transform 只需补上剩下的零头 */
let scrollBase = 0

/** 换驱动方式。同一帧只由一条驱动，所以两半不会互相对齐不上 */
function setScrollDrive(on: boolean) {
  if (on === scrollDriven) return
  scrollDriven = on
  scrollBase = Math.round(camX)
  const el = viewportEl.value
  if (el) el.scrollLeft = scrollBase
  // 交出去的那一半必须归零：留着它位移就会被算两遍（滚动一份、transform 一份），
  // 相机直接跑到两倍远的地方，甩完再猛跳回来
  const shift = on ? 'translate3d(0, 0, 0)' : `translate3d(${(-(camX - scrollBase)).toFixed(2)}px, 0, 0)`
  for (const node of movables) node.style.transform = shift
}

/**
 * 把相机"就地"落到某个位置，并把两套驱动一起对齐。
 *
 * 回来时一律回到 transform 驱动：它同步、不取整、不依赖合成器的那一帧。
 * 惯性结束时本会由 advance() 调 stopFling() 切回来，可你要是**在惯性还没停时**
 * 就点进记忆，advance 已经因为 onStage=false 提前返回了——于是 scrollDriven 一直是 true：
 * 往后拖动都走滚动那条路（整数取整、没有子像素），手感就是"滑不动"；
 * 而且滚动值要等元素回到文档里才写得进去。
 */
function snapCamera(x: number) {
  camX = clamp(x, 0, maxCamX.value)
  scrollDriven = false
  scrollBase = Math.round(camX)
  const el = viewportEl.value
  if (el) el.scrollLeft = scrollBase
  collectMovables()
  paintCamera()
}

/**
 * 以元素**实际**的滚动值当基准，把位置全部交给 transform。
 *
 * 画面上内容的位置 = 布局位置 − 实际滚动 + transform，所以只要 transform 按"实际滚动"
 * 来算，基准是多少都不影响结果。之前是按"我们以为写进去的滚动值"算的——而元素刚回到
 * 文档里（滚动范围还没算出来）时那次写会被浏览器截成 0，于是画面整体偏掉：
 * 表现就是"退回光河停在第一个记忆"，手动滑一下、滚动值真的变了，画面才跳回当前记忆。
 * 这个函数顺带也接住了浏览器自己恢复历史滚动的情况。
 */
function rebaseToActualScroll() {
  const el = viewportEl.value
  if (!el) return
  const actual = el.scrollLeft
  if (actual === scrollBase) return
  scrollBase = actual
  paintCamera()
}

/** 视口自己滚了（不是我们写的那种）就以它为准 */
function onViewportScroll() {
  if (scrollDriven) return // 滚动驱动时是我们自己在写，不必插手
  rebaseToActualScroll()
}

function paintCamera() {
  // 被详情页盖住时，一个字节都不要往 DOM 上写。
  // 那会儿这块 DOM 已经从文档里挪走了：写 scrollLeft 会被浏览器截成 0（元素不在文档里，
  // 滚动范围也还没建立），于是下次回来就停在"河的开头"；而手动滑一下——元素已经回到文档里，
  // 同一行赋值立刻生效——就"立马跳到当前记忆"。用户的描述正好是这两句：
  // "一旦滑动过，点进记忆再退出只显示第一个记忆，再手动滑动会立马跳到当前记忆"。
  // 位置改由 onActivated 在元素回到文档之后再落一次。
  if (!onStage.value) return
  if (scrollDriven) {
    const el = viewportEl.value
    if (el) el.scrollLeft = camX
    return
  }
  const shift = `translate3d(${(-(camX - scrollBase)).toFixed(2)}px, 0, 0)`
  for (const el of movables) el.style.transform = shift
}

// 节点是 v-for 渲染的：数据到位、DOM 更新完了再收集
watch(
  nodes,
  () => {
    collectMovables()
  },
  { flush: 'post', immediate: true },
)

/** 光河整体的不透明度：一场里只在重放时淡出、淡入各一次 */
function setTrackOpacity(value: number) {
  const el = trackEl.value
  if (el) el.style.opacity = String(value)
}

/** 相机动过之后，把「对准哪一段」「到头了没」同步给界面（变了才写，避免多余渲染） */
function syncCamera() {
  const index = indexAt(camX + viewportWidth.value * 0.5)
  if (index !== focusedIndex.value) focusedIndex.value = index
  const end = maxCamX.value > 0 && camX >= maxCamX.value - 4
  if (end !== atEnd.value) atEnd.value = end
}

// ---------------------------------------------------------------- 字体预热
/**
 * 把记忆标题用到的衬线字形先取回来。
 *
 * 标题用的网络字体（Noto Serif SC）按 Unicode 拆成了一百多份、用到哪个字才取哪几份
 * （见 /assets/noto-serif-sc-*.woff2）。首页上几乎不出现衬线字，于是**点进一条记忆时**
 * 标题那几十个字对应的分片才开始下载：标题先拿系统宋体顶着，分片到了再换成网络字体。
 * 字形变了、字重也变了——看上去就是"从一种字体切到另一种字体，然后闪一下"。
 *
 * 实测（冷缓存 + 4G，DPR 3，直接打开记忆页，用 CDP 问标题实际用的字体）：
 *   2083ms  系统字体 Noto Serif CJK SC SemiBold
 *   3149ms  网络字体 Noto Serif SC        ← 三个分片约 200KB，每个约 930ms
 * 顺带一提：布局偏移量不到它——标题是整宽块级元素，换字体只改字形、不改盒子的位置，
 * 所以「看帧时间、看布局偏移」都发现不了，只有问"这行字是谁画的"才问得出来。
 *
 * 所以这里提前把眼前这几条记忆的标题喂给字体加载器：等真点进去时字形已在内存里，
 * 一次就画对，压根不存在"换"这个动作。只喂眼前这一小撮——全部标题加起来可能几千字，
 * 那会拖下好几 MB。取回的分片带内容指纹、浏览器长期缓存（/assets/ 是 immutable），
 * 所以这笔开销一辈子只付一次。
 */
const warmedChars = new Set<string>()
let warmPending = ''
let warmTimer = 0

/** 把要用的字记下来，稍后一起取（别和首屏抢带宽） */
function warmSerif(text: string) {
  if (!text || !document.fonts?.load) return
  for (const ch of text) {
    if (warmedChars.has(ch)) continue
    warmedChars.add(ch)
    warmPending += ch
  }
  if (!warmPending || warmTimer) return
  // 光河与缩略图先铺开，字体让一让——它不是第一眼要看的东西
  warmTimer = window.setTimeout(() => {
    warmTimer = 0
    const pending = warmPending
    warmPending = ''
    // 取不回来就退化成原来的行为（到了再换），不打扰用户
    document.fonts.load('600 1em "Noto Serif SC Variable"', pending).catch(() => {})
  }, 700)
}

// 对焦那条加上左右各一条。实测（390px 宽的视口）同一时刻只有一条卡片真的能点到，
// 所以不必多喂——多喂的就是白耗流量。
watch(
  focusedIndex,
  () => {
    const around = nodes.value.slice(Math.max(0, focusedIndex.value - 1), focusedIndex.value + 2)
    warmSerif(around.map((n) => n.moment.title || '未命名的这一刻').join(''))
  },
  { immediate: true },
)

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value))
}

function scrollBy(delta: number, pauseMs = 2600) {
  // 摇镜而不是瞬移，键盘翻页也该是柔的
  glideTo(clamp(camX + delta, 0, maxCamX.value), 520)
  pauseAuto(pauseMs)
}

/** 打开时先落在「最近的生活」附近，再向未来缓缓流过 */
function restingPosition(): number {
  const list = nodes.value
  const last = list[list.length - 1]
  if (!last) return 0
  return clamp(last.centerX - viewportWidth.value * 0.76, 0, maxCamX.value)
}

// ---------------------------------------------------------------- 自动流动
let rafId = 0
let lastTs = 0
let pauseUntil = 0
let resetting = false
let holdUntil = 0
/** 每次重新起步都从 35% 速度缓入，停顿之后不会像被推了一把 */
let flowing = false
let flowWakeTs = 0
const WAKE_MS = 760

function loop(ts: number) {
  rafId = requestAnimationFrame(loop)
  const elapsed = lastTs ? ts - lastTs : 16
  lastTs = ts
  advance(ts, Math.min(elapsed / 1000, 0.05))
  syncCamera()
  paintCamera()
}

/** 一帧的推演：摇镜、惯性、自动流动。中途返回只是这一帧不再往前，收尾交给 loop */
function advance(ts: number, dt: number) {
  if (document.hidden || resetting || !onStage.value) return

  if (glideSpan > 0) {
    const progress = Math.min(1, (ts - glideStart) / glideSpan)
    // ease-out-cubic：起步轻快，收尾几乎停住
    camX = glideFrom + (glideTarget - glideFrom) * (1 - Math.pow(1 - progress, 3))
    if (progress >= 1) glideSpan = 0
    flowing = false // 摇到位之后重新起步，也走一次缓入
    return
  }

  if (flinging) {
    camX += flingVelocity * dt
    // 摩擦：速度每秒衰减到 4%，一次快甩大约滑出三成距离
    flingVelocity *= Math.pow(FRICTION, dt)

    const edge = clamp(camX, 0, maxCamX.value)
    if (Math.abs(edge - camX) > 0.5) {
      camX = edge // 滑到头就贴住，别在边界外飘着
      stopFling()
    } else if (Math.abs(flingVelocity) < FLING_MIN) {
      stopFling()
    }
    return
  }

  if (ts < holdUntil) return

  const shouldFlow = autoFlow.value && !dragging.value && ts > pauseUntil
  if (!shouldFlow) {
    flowing = false
    return
  }

  if (!flowing) {
    flowing = true
    flowWakeTs = ts
  }

  if (camX >= maxCamX.value) {
    // 流到「此刻」，停一会儿，再从最初的记忆重新开始
    holdUntil = ts + 4200
    restartFlow()
    return
  }

  const wake = ts - flowWakeTs
  const ramp = wake < WAKE_MS ? 0.35 + 0.65 * (wake / WAKE_MS) : 1
  camX = Math.min(camX + SPEED * dt * ramp, maxCamX.value)
}

/** 回放的两段定时器（等待、跳回开头）。都要能取消，见 cancelFlowReset */
let flowWaitTimer = 0
let flowJumpTimer = 0

function restartFlow() {
  if (resetting) return
  resetting = true
  flowWaitTimer = window.setTimeout(() => {
    flowWaitTimer = 0
    // 已经在别处了（比如翻到了详情页）就别再折腾，回来时画面该是原样
    if (!onStage.value) {
      resetting = false
      return
    }
    setTrackOpacity(0)
    flowJumpTimer = window.setTimeout(() => {
      flowJumpTimer = 0
      // 这 0.72 秒里同样可能已经翻到别处去了。原来这里没有这道检查，
      // 于是"camX 归 0"会在详情页背后照常执行——退回来看到的就是开头那条记忆。
      if (!onStage.value) {
        resetting = false
        setTrackOpacity(1)
        return
      }
      camX = 0
      lastTs = 0
      setScrollDrive(false) // 回到起点：滚动与 transform 一起归零
      setTrackOpacity(1)
      resetting = false
    }, 720)
  }, 3200)
}

/**
 * 离开光河（翻到详情页）时，把还没执行完的回放整个取消。
 *
 * 之前只在外层那道 3.2 秒的定时器里查了 onStage，等待期间没事；可**淡出那 0.72 秒里
 * 没有任何检查**，而且离开页面时也不取消。你在"此刻"端浏览时回放是一轮轮排着队的，
 * 于是：翻进一条记忆、很快退回来——那笔待办的回放照样在背后把 camX 归 0，
 * 光河当场跳到开头（用户报的"显示在第一个记忆"）；正赶上淡出那一下就是整条河透明，
 * "一个也不显示"。
 *
 * 实测（手机尺寸，在回放前奏里点进去、1.5 秒后退回）：
 *   退回后 120~840ms   变换 -7987px，居中"测试音乐"      ← 正常
 *   退回后 960ms 起    变换 0px，居中"第一次在这里留下点什么"，此后一直停在那儿
 */
function cancelFlowReset() {
  if (flowWaitTimer) {
    window.clearTimeout(flowWaitTimer)
    flowWaitTimer = 0
  }
  if (flowJumpTimer) {
    window.clearTimeout(flowJumpTimer)
    flowJumpTimer = 0
  }
  if (resetting) {
    resetting = false
    setTrackOpacity(1) // 淡出到一半被打断的话，把不透明度还回来
  }
}

function pauseAuto(ms: number) {
  pauseUntil = performance.now() + ms
}

// 从某段记忆回来时，镜头缓缓摇过去，而不是瞬移
let glideFrom = 0
let glideTarget = 0
let glideStart = 0
let glideSpan = 0

function glideTo(target: number, duration: number) {
  glideFrom = camX
  glideTarget = clamp(target, 0, maxCamX.value)
  glideStart = performance.now()
  glideSpan = Math.max(1, duration)
  // 摇镜是慢速的，走 transform（子像素）
  setScrollDrive(false)
}

// ---------------------------------------------------------------- 交互
const viewportEl = ref<HTMLElement | null>(null)
const DRAG_THRESHOLD = 8
let dragStartX = 0
let dragStartCam = 0
let suppressClick = false
let pointerCaptured = false
/** 按下时命中的那段记忆（悬停会让照片上浮，所以要提前记住） */
let pressedId: string | null = null

// ---------------------------------------------------------------- 惯性
/** 松手时的余速（px/s，沿相机方向）：像推了一下水面上的纸船 */
let flingVelocity = 0
let flinging = false
/** 拖拽中的速度采样，指数平滑，最近的动作权重更大 */
let dragVelocity = 0
let sampleTs = 0
let sampleX = 0

const FLING_MIN = 90 // 低于这个速度就当没甩
const FLING_MAX = 3200
const FRICTION = 0.04 // 每秒衰减到 4%
const RUBBER = 0.34 // 拖出边界时只跟三成，像拉橡皮筋

/** 越过边界就变沉：拖得出去，但拽不远 */
function applyRubber(raw: number): number {
  const max = maxCamX.value
  if (raw < 0) return raw * RUBBER
  if (raw > max) return max + (raw - max) * RUBBER
  return raw
}

function stopFling() {
  flinging = false
  flingVelocity = 0
  // 惯性停了就换回 transform：接着的自动流动是慢速的，要用子像素
  setScrollDrive(false)
}

function pickMomentId(x: number, y: number): string | null {
  const hit = document.elementFromPoint(x, y)
  return hit?.closest<HTMLElement>('[data-moment-id]')?.dataset.momentId ?? null
}

function onPointerDown(event: PointerEvent) {
  if (event.button !== 0 && event.pointerType === 'mouse') return
  // 一碰光河，就把还没执行完的回放取消。
  // 那笔回放会在 3.2 秒后把 camX 归 0；你要是正好在这期间点进一条记忆，
  // 离开时锁存下来的位置**就已经是开头了**——退回来自然"显示在第一个记忆"。
  // 手指碰上了，光河就该停下来等你，而不是自己往回跑。
  cancelFlowReset()
  stopFling() // 一按下去就抓住，惯性立刻让位
  // 记住"手指按下的这一刻"的位置：你说的"点进记忆页的那个时候"就是这会儿。
  // 离开页面是在路由切完之后（懒加载的详情页要等一会儿才到位），那之间光河还会自己往前流，
  // 锁存的值就比你以为的位置差了那么一截。按下时先存一份，离开时优先用它。
  pressedAt = performance.now()
  pressedCamX = camX
  dragging.value = true
  suppressClick = false
  pointerCaptured = false
  dragStartX = event.clientX
  dragStartCam = camX
  pressedId = pickMomentId(event.clientX, event.clientY)
  dragVelocity = 0
  sampleTs = performance.now()
  sampleX = event.clientX
  // 刻意不在这里 setPointerCapture：按下就捕获指针的话，
  // 浏览器会把 click 交给视口而不是照片，记忆就点不开了。
  // 等确认是拖拽（位移超过阈值）再捕获。
}

function onPointerMove(event: PointerEvent) {
  if (!dragging.value) return
  const delta = event.clientX - dragStartX

  // 先跟随再判阈值：否则前 8px 纹丝不动、之后猛地跳一下，手感是断的
  camX = applyRubber(dragStartCam - delta)

  const now = performance.now()
  const dt = now - sampleTs
  if (dt > 0) {
    const instant = (-(event.clientX - sampleX) / dt) * 1000
    dragVelocity = dragVelocity * 0.62 + instant * 0.38
    sampleTs = now
    sampleX = event.clientX
  }

  if (pointerCaptured || Math.abs(delta) <= DRAG_THRESHOLD) return
  pointerCaptured = true
  suppressClick = true
  try {
    viewportEl.value?.setPointerCapture?.(event.pointerId)
  } catch {
    // 指针已经释放时捕获会失败，但事件仍会正常冒泡，拖动不受影响
  }
}

function onPointerUp(event: PointerEvent) {
  if (!dragging.value) return
  dragging.value = false
  if (pointerCaptured) {
    try {
      viewportEl.value?.releasePointerCapture?.(event.pointerId)
    } catch {
      /* 已释放则忽略 */
    }
    pointerCaptured = false
  }

  const target = pressedId
  pressedId = null
  finishDrag(target)
}

/**
 * 被浏览器抢走手势时（真机上双指、屏幕边缘手势都会这样）按「松手」收尾。
 * 之前这里只是把状态清掉：惯性不给、自动流动却立刻顶上来，看着就是一顿。
 */
function onPointerCancel(event: PointerEvent) {
  if (!dragging.value) return
  dragging.value = false
  if (pointerCaptured) {
    try {
      viewportEl.value?.releasePointerCapture?.(event.pointerId)
    } catch {
      /* 已释放则忽略 */
    }
    pointerCaptured = false
  }
  pressedId = null
  suppressClick = true // 被取消的这一下不该再去打开记忆
  finishDrag(null)
}

/** 松手与「被浏览器抢走手势」共用的收尾：接上惯性、把自动流动往后推 */
function finishDrag(openTarget: string | null) {
  // 停了一会儿才松手的不算甩，免得慢慢挪到位也会滑走
  const idle = performance.now() - sampleTs
  const velocity = idle > 90 ? 0 : dragVelocity
  dragVelocity = 0

  const edge = clamp(camX, 0, maxCamX.value)
  if (Math.abs(edge - camX) > 0.5) {
    glideTo(edge, 480) // 拖出边界了，先弹回边上
  } else if (Math.abs(velocity) > FLING_MIN) {
    flinging = true
    flingVelocity = clamp(velocity, -FLING_MAX, FLING_MAX)
    // 甩出去这一程交给视口原生滚动：唯有它会提前把新露出来的内容备好图
    setScrollDrive(true)
  }

  pauseAuto(3400)

  if (suppressClick) {
    suppressClick = false
    return
  }
  if (openTarget) openMoment(openTarget)
}

function onWheel(event: WheelEvent) {
  event.preventDefault()
  const delta = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY
  camX = clamp(camX + delta * 1.15, 0, maxCamX.value)
  pauseAuto(2800)
}

function onKeydown(event: KeyboardEvent) {
  // 演示里只认 Esc（退出）；别的键不接手，免得镜头被拨走
  if (demo.running.value) {
    if (event.key === 'Escape') stopDemo()
    return
  }

  if (event.key === 'ArrowLeft') {
    scrollBy(-viewportWidth.value * 0.42)
  } else if (event.key === 'ArrowRight') {
    scrollBy(viewportWidth.value * 0.42)
  } else if (event.key === 'Home') {
    camX = 0
    pauseAuto(4000)
  } else if (event.key === 'End') {
    camX = maxCamX.value
    pauseAuto(4000)
  } else if (event.key === ' ') {
    event.preventDefault()
    autoFlow.value = !autoFlow.value
  } else {
    return
  }
}

let openedAt = 0

function openMoment(id: string) {
  if (suppressClick) {
    suppressClick = false
    return
  }
  // 键盘回车 / 辅助技术仍走按钮的 click，这里去重，避免打开两次
  const now = performance.now()
  if (now - openedAt < 400) return
  openedAt = now
  router.push({ name: 'moment', params: { id } })
}

function jumpToNow() {
  camX = maxCamX.value
  pauseAuto(5200)
}

/** 那段记忆已经不在了（比如刚被删掉）：落回它原本占着的时间缝隙 */
function gapAt(list: RiverNodeLayout[], at: string): number | null {
  const time = new Date(at).getTime()
  let before: RiverNodeLayout | null = null
  let after: RiverNodeLayout | null = null

  for (const node of list) {
    if (new Date(node.moment.happenedAt).getTime() <= time) before = node
    else {
      after = node
      break
    }
  }

  if (before && after) return (before.x + before.width + after.x) / 2
  if (before) return before.x + before.width
  if (after) return after.x
  return null
}

/** 把镜头对准刚看过的那段记忆 */
function alignToAnchor(smooth: boolean) {
  const target = store.anchor
  const list = nodes.value
  if (!target || !list.length) return

  store.setAnchor(null)
  const node = list.find((item) => item.moment.id === target.id)
  const centerX = node ? node.centerX : gapAt(list, target.at)
  if (centerX === null) return

  const next = clamp(centerX - viewportWidth.value * 0.5, 0, maxCamX.value)

  if (!smooth) {
    camX = next
    pauseAuto(5200)
    return
  }

  // 隔得越远摇得越久，但不至于让人干等
  const distance = Math.abs(next - camX)
  glideTo(next, clamp(520 + distance * 0.32, 520, 1500))
  pauseAuto(5200)
}

function onResize() {
  const before = totalWidth.value
  viewportWidth.value = window.innerWidth
  // 被详情页盖住时只更新尺寸，**不许动相机**：这会儿相机是锁定的，
  // 而这里原来会把 camX 重新夹一遍、甚至（camX 恰好是 0 时）推到"此刻"去。
  // 尺寸的变化等回来放回相机时会重新夹一次（见 onActivated）。
  if (!onStage.value) return
  if (before > 0) {
    camX = clamp(camX, 0, Math.max(0, totalWidth.value - viewportWidth.value))
  }
  if (nodes.value.length && camX === 0) {
    camX = restingPosition()
  }
}

// ---------------------------------------------------------------- 演示（巡演）

const demo = useDemo()
const toast = useToast()

/** 顺序演示的进度：下一段演 nodes[demoStep] */
let demoStep = 0
/** 随机演示挑中的那一段 */
let demoPicked = -1
/** 等镜头把某一段带到正中的守望 */
let demoWatch = 0
let demoTimer = 0
/**
 * 起演后的宽限：这几秒里不动手翻开记忆，留给用户看清状态、随时停手。
 */
const DEMO_GRACE = 4000
let demoGraceUntil = 0

/** 第 index 段正好落在屏幕正中时的机位 */
function spotAt(index: number): number {
  const node = nodes.value[index]
  if (!node) return 0
  return Math.max(0, node.centerX - viewportWidth.value * 0.5)
}

/** 摇镜时长随距离伸缩：跳得远就多给一点时间，别太生硬 */
function glideSpanFor(target: number): number {
  return clamp(700 + Math.abs(target - camX) * 0.22, 900, 2200)
}

/**
 * 等镜头把第 index 段带到屏幕正中——到了才翻开。
 * 用守望而不是算时间：中途有停顿、有回望，算出来的时刻都不作数，
 * 盯着 camX 反而准，落到中间的那一瞬就开。
 */
function awaitSpot(index: number) {
  window.clearInterval(demoWatch)
  const node = nodes.value[index]
  if (!node) return

  const spot = spotAt(index)
  demoWatch = window.setInterval(() => {
    if (!demo.running.value || demo.opened.value || !onStage.value) return
    if (performance.now() < demoGraceUntil) return
    if ((demo.mode.value === 'random' ? demoPicked : demoStep) !== index) return
    if (camX < spot - 3) return

    window.clearInterval(demoWatch)
    demoWatch = 0
    demo.markOpened(node.moment.id)
    openMoment(node.moment.id)
  }, 120)
}

/** 随机挑一段（不与上一段重复），镜头缓缓移过去 */
function jumpRandom() {
  const list = nodes.value
  if (!list.length) return
  const pool = list.map((_, index) => index).filter((index) => index !== demoPicked)
  demoPicked = pool.length ? pool[Math.floor(Math.random() * pool.length)] : 0

  const spot = spotAt(demoPicked)
  glideTo(spot, glideSpanFor(spot))
  pauseAuto(3600)
  awaitSpot(demoPicked)
}

/** 一段演完、回到光河之后：顺序往前走一段，随机再挑一段 */
function resumeDemo() {
  if (!demo.running.value) return

  if (demo.mode.value === 'random') {
    jumpRandom()
    return
  }

  const total = nodes.value.length
  if (!total) return
  demoStep += 1
  if (demoStep < total) {
    awaitSpot(demoStep)
    return
  }

  // 一轮演完：光河淡出、退回最初，再从第一段来
  demoStep = 0
  restartFlow()
  demoTimer = window.setTimeout(() => {
    demoTimer = 0
    if (demo.running.value) awaitSpot(0)
  }, 4300)
}

function startDemo(mode: DemoMode) {
  demo.start(mode)
  demoStep = 0
  demoPicked = -1
  autoFlow.value = true
  demoGraceUntil = performance.now() + DEMO_GRACE
  toast.info(mode === 'sequence' ? '顺序演示开始' : '随机演示开始')

  if (mode === 'sequence') {
    // 从头开始：镜头先退回光河最初，之后交给主页自己的流动
    camX = 0
    flowing = false
    pauseAuto(600)
    awaitSpot(0)
    return
  }
  jumpRandom()
}

function stopDemo() {
  demo.stop()
}

// 演示一停就把守望与排期收干净
watch(
  () => demo.running.value,
  (running) => {
    if (running) return
    window.clearInterval(demoWatch)
    window.clearTimeout(demoTimer)
    demoWatch = 0
    demoTimer = 0
    demoStep = 0
    demoPicked = -1
    toast.info('演示结束')
  },
)

// ---------------------------------------------------------------- 生命周期
/** 首次进场由 onMounted 收尾（那时数据才到位），之后每次回来才摇镜头 */
let firstActivation = true

onMounted(async () => {
  window.addEventListener('resize', onResize, { passive: true })
  window.addEventListener('keydown', onKeydown)
  rafId = requestAnimationFrame(loop)

  await store.load()
  store.loadTags()
  if (!store.loaded) return

  if (store.anchor) alignToAnchor(false)
  else camX = restingPosition()
})

onBeforeUnmount(() => {
  cancelAnimationFrame(rafId)
  window.clearTimeout(warmTimer)
  window.removeEventListener('resize', onResize)
  window.removeEventListener('keydown', onKeydown)
})

onActivated(() => {
  onStage.value = true
  lastTs = 0
  pauseAuto(900)

  // 解锁：原样放回离开时的位置。
  // 为什么是"放回"而不是"摇回刚看的那条"——用户的话最准：点进记忆那一刻，长河和记忆
  // 要锁定不动，退出来再解锁。摇镜是一次额外的位移，锁定就不该有位移。
  // 没有锁定值时（直接打开、第一次进场）才退回老办法：找刚看的那条对上去。
  if (!firstActivation) {
    if (frozenCamX === null) {
      alignToAnchor(demo.running.value)
    } else {
      const back = frozenCamX
      frozenCamX = null
      // 就地落位：位置、驱动基准、位移，一次对齐，别等到下一帧（那会露出"还在旧位置"的一瞬）
      snapCamera(back)
      // 再补两次：这一帧 DOM 才刚被放回文档，滚动范围要等布局算出来，而浏览器会把
      // 写不进去的滚动值截成 0——所以两次 rAF 之后按**实际**滚动值重算基准，
      // 画面就落在 camX 上，不再依赖那次写有没有成功。
      requestAnimationFrame(() => requestAnimationFrame(() => rebaseToActualScroll()))
    }
  }
  firstActivation = false

  // 从详情页回来：先让镜头摇回刚看过的那段，再接着演下一段
  if (demo.running.value) {
    demo.markClosed()
    window.clearInterval(demoWatch)
    window.clearTimeout(demoTimer)
    demoWatch = 0
    demoTimer = window.setTimeout(() => {
      demoTimer = 0
      resumeDemo()
    }, 2200)
  }
})

/**
 * 上锁：离开光河时的相机位置。
 *
 * 被详情页盖住期间，光河不只是"停住"——还有好几条路能让它在背后动：回放的定时器、
 * 详情页里滚动引起的视口重算（onResize 会重算 camX，甚至把相机推回"此刻"）、
 * 演示的排期…… 一条条堵住很脆：漏掉哪条，回来就会"显示在第一个记忆"或者空白，
 * 而且这种漏只在真机上、只在某些时序下才出现（本地怎么测都正常）。
 *
 * 所以按用户提的办法：离开时把相机位置整个记下来、把会自己动的东西全停掉，
 * 回来原样放回去。期间任何东西想动它都不作数。
 */
let frozenCamX: number | null = null

onDeactivated(() => {
  // 被详情页盖住时把光河停住：否则记忆会在背后悄悄流走，
  // 回来时那一段早已漂远，甚至触发"流到头"从最初的记忆重来。
  onStage.value = false

  // 已经排上队的"回放"也要一并取消，否则它会在背后把光河挪回开头（见 cancelFlowReset）
  cancelFlowReset()

  // 记下相机（优先用"手指按下那一刻"那一份），并把所有"会自己动"的东西停掉
  frozenCamX = pressedCamX !== null && performance.now() - pressedAt < 2500 ? pressedCamX : camX
  pressedCamX = null
  glideSpan = 0
  flinging = false
  flingVelocity = 0
  dragVelocity = 0
})

watch(
  () => store.ordered.length,
  (length, previous) => {
    // 被详情页盖住时不动相机：位置锁定着，回来会原样放回
    if (length > 0 && previous === 0 && onStage.value) {
      camX = restingPosition()
    }
  },
)

const emptyState = computed(() => store.loaded && store.ordered.length === 0)
const firstLabel = computed(() => {
  const first = store.ordered[0]
  return first ? formatDotted(first.happenedAt) : ''
})
</script>

<template>
  <div class="home">
    <!-- 光河本体：始终横贯眼前，不随相机滚动（所以放在滚动视口外面） -->
    <div class="river-static" aria-hidden="true">
      <div class="river-line" />
      <div class="river-glow" />
    </div>

    <!-- 光河视口：内容比它宽，相机位移就是它的 scrollLeft -->
    <div
      ref="viewportEl"
      class="river-viewport"
      :class="{ 'is-dragging': dragging, 'is-demo': demo.running.value }"
      role="region"
      aria-label="记忆光河"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerCancel"
      @wheel="onWheel"
      @scroll.passive="onViewportScroll"
    >
      <div ref="trackEl" class="river-track" :style="{ '--track-width': `${totalWidth}px` }">
        <!-- 时间刻度 -->
        <!-- 光河分成几段：每段单独成层，位移由合成器承担（见 paintCamera） -->
        <div
          v-for="chunk in chunks"
          :key="chunk.key"
          class="river-chunk"
          :style="{ left: `${chunk.origin}px`, width: `${chunk.width}px` }"
        >
          <!-- 时间刻度 -->
          <div
            v-for="marker in chunk.markers"
            :key="marker.key"
            class="river-marker"
            :class="{ 'is-strong': marker.strong }"
            :style="{ left: `${marker.x - chunk.origin}px` }"
            aria-hidden="true"
          >
            <span class="river-marker__text">{{ marker.label }}</span>
          </div>

          <!-- 河面上的浮标：每段记忆在光河上的位置 -->
          <span
            v-for="node in chunk.pips"
            :key="`pip-${node.moment.id}`"
            class="river-pip"
            :style="{ left: `${node.centerX - chunk.origin}px` }"
            aria-hidden="true"
          />

          <!-- 记忆节点 -->
          <RiverNode
            v-for="node in chunk.nodes"
            :key="node.moment.id"
            :node="node"
            :origin="chunk.origin"
            @open="openMoment"
          />
        </div>
      </div>
    </div>

    <!-- 河面上缓缓漂过的光 -->
    <div class="sparks" aria-hidden="true">
      <span v-for="index in 9" :key="index" class="spark" :style="{ '--i': index }" />
    </div>

    <!-- 顶部：品牌与寻找 -->
    <header class="hud hud--top">
      <div class="brand">
        <h1 class="brand__name">流光</h1>
        <p class="brand__meta">
          <template v-if="store.total > 0">
            <span class="numeral numeral--clear">{{ store.total }}</span> 段记忆<template
              v-if="firstLabel"
            >
              · 始于 {{ firstLabel }}</template
            >
          </template>
          <template v-else> 把生活的点滴，交给时间慢慢流过 </template>
        </p>
      </div>

      <div class="hud-actions" :class="{ 'is-demo': demo.running.value }">
        <button
          type="button"
          class="hud-icon tip tip--down"
          aria-label="图集"
          data-tip="图集：所有画面按时间铺开，可按标签找"
          @click="router.push({ name: 'gallery' })"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" class="h-5 w-5">
            <rect x="3" y="5" width="18" height="14.5" rx="2.6" />
            <circle cx="8.8" cy="9.8" r="1.55" />
            <path
              d="M4.2 17.6l4.6-4.6 3.4 3.4 2.5-2.5 4.3 4.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </button>

        <button
          type="button"
          class="hud-icon tip tip--down"
          aria-label="寻找一段记忆"
          data-tip="寻找：按时间、主题、标签"
          @click="router.push({ name: 'search' })"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" class="h-5 w-5">
            <circle cx="11" cy="11" r="6.5" />
            <path d="m16 16 4.5 4.5" stroke-linecap="round" />
          </svg>
        </button>

        <button
          type="button"
          class="hud-icon tip tip--down"
          aria-label="点亮地图"
          data-tip="点亮地图：去过的地方，一盏灯一座城"
          @click="router.push({ name: 'map' })"
        >
          <!-- 地球经纬：折叠地图那张纸在一排细线图标里既宽又重，还容易被看成折纸盒 -->
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" class="h-5 w-5">
            <circle cx="12" cy="12" r="8.4" />
            <path d="M3.6 12h16.8" stroke-linecap="round" />
            <path
              d="M12 3.6c2.7 3.3 2.7 13.5 0 16.8M12 3.6c-2.7 3.3-2.7 13.5 0 16.8"
              stroke-linecap="round"
            />
          </svg>
        </button>

        <button
          type="button"
          class="hud-icon tip tip--down"
          aria-label="设置"
          data-tip="设置：访问口令"
          @click="router.push({ name: 'settings' })"
        >
          <!-- 齿轮的绘制范围天然比搜索、加号大一圈，按视觉尺寸收一点回来（内容约 14px） -->
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            class="h-[0.95rem] w-[0.95rem]"
          >
            <circle cx="12" cy="12" r="3.2" />
            <path
              d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.11a1.7 1.7 0 0 0-1.11-1.56 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.11A1.7 1.7 0 0 0 4.66 8.9a1.7 1.7 0 0 0-.34-1.87l-.06-.06A2 2 0 1 1 7.09 4.14l.06.06a1.7 1.7 0 0 0 1.87.34H9.1A1.7 1.7 0 0 0 10.14 3V3a2 2 0 1 1 4 0v.11a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v.09A1.7 1.7 0 0 0 21 10.1h.09a2 2 0 1 1 0 4h-.11A1.7 1.7 0 0 0 19.4 15Z"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </button>

        <button
          type="button"
          class="hud-icon hud-icon--accent tip tip--down"
          aria-label="记下一段流光"
          data-tip="记下一段流光"
          @click="router.push({ name: 'create' })"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" class="h-5 w-5">
            <path d="M12 5v14M5 12h14" stroke-linecap="round" />
          </svg>
        </button>

      </div>
    </header>

    <!-- 底部：时间刻度与操作 -->
    <footer class="hud hud--bottom">
      <div class="scrubber">
        <span class="scrubber__date numeral">
          {{ focusedNode ? formatDotted(focusedNode.moment.happenedAt) : '· · ·' }}
        </span>
        <span v-if="focusedNode" class="scrubber__hint">
          {{ focusedNode.moment.title || relativeTime(focusedNode.moment.happenedAt) }}
        </span>
      </div>

      <div class="controls" :class="{ 'is-demo': demo.running.value }">
        <button
          v-if="!atEnd && store.total > 3"
          type="button"
          class="hud-icon hud-icon--small tip"
          data-tip="回到此刻"
          aria-label="回到此刻"
          @click="jumpToNow"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" class="h-4 w-4">
            <path d="M5 12h13m0 0-4.5-4.5M18 12l-4.5 4.5" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>

        <button
          type="button"
          class="hud-icon hud-icon--small tip"
          :aria-label="autoFlow ? '暂停流动' : '继续流动'"
          :data-tip="autoFlow ? '让时间停一停' : '继续流淌'"
          @click="autoFlow = !autoFlow"
        >
          <svg v-if="autoFlow" viewBox="0 0 24 24" fill="currentColor" class="h-4 w-4">
            <rect x="7" y="5.5" width="3.4" height="13" rx="1.2" />
            <rect x="13.6" y="5.5" width="3.4" height="13" rx="1.2" />
          </svg>
          <svg v-else viewBox="0 0 24 24" fill="currentColor" class="h-4 w-4">
            <path d="M8 5.2v13.6a.9.9 0 0 0 1.37.77l11.1-6.8a.9.9 0 0 0 0-1.54L9.37 4.43A.9.9 0 0 0 8 5.2Z" />
          </svg>
        </button>

        <!-- 演示：右下角一枚放映图标，点开向上弹「顺序 / 随机」 -->
        <DemoMenu @start="startDemo" />
      </div>
    </footer>

    <!-- 演示进行中的出口：右下角一枚叉号，正好接在演示按钮那一格上 -->
    <DemoExit />

    <!-- 载入 / 空状态 -->
    <Transition name="page">
      <div v-if="store.loading && !store.loaded" class="veil">
        <p class="veil__text">正在打捞记忆…</p>
      </div>
    </Transition>

    <Transition name="page">
      <div v-if="emptyState" class="veil veil--empty">
        <div class="empty">
          <p class="empty__title title-serif">还没有任何记忆</p>
          <p class="empty__body">
            从今天开始，把值得留下的瞬间写下来。<br />
            一张照片、一句话、一段旋律，都可以。
          </p>
          <button type="button" class="glow-btn mt-6 px-6 py-2.5 text-sm" @click="router.push({ name: 'create' })">
            记下第一段流光
          </button>
        </div>
      </div>
    </Transition>

    <Transition name="page">
      <div v-if="atEnd && !emptyState" class="end-note">
        <span class="end-note__line" aria-hidden="true" />
        <span class="end-note__text">这是此刻的你</span>
        <span class="end-note__line" aria-hidden="true" />
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.home {
  position: fixed;
  inset: 0;
  overflow: hidden;
  /* 光河的纵向位置：先给不支持 dvh 的旧内核一个 vh 值，支持的浏览器再覆盖。
     dvh 会随移动端地址栏收放自动重算，所以不必用 JS 去追。 */
  --river-y: 58vh;
  --river-y: 60dvh;
}

/* ---------------- 光河视口 ---------------- */
.river-viewport {
  position: absolute;
  inset: 0;
  z-index: 1;
  overflow: hidden;
  /* 单指拖动由 JS 自己接（所以挡掉浏览器的手势平移），
     但双指缩放要留给浏览器 —— 不然在手机上放大之后就缩不回去了 */
  touch-action: pinch-zoom;
  cursor: grab;
  user-select: none;
  /* 这里刻意不写 contain：实测 contain: strict（内含 paint）会让里面那些
     will-change 的分段**成不了合成层**——位移于是回到每帧整幅重画。
     fling 的 1.5 秒里，光河层重绘 112 次、光栅 780ms；去掉之后只剩 6 次。
     裁剪由 overflow: hidden 负责，足够了。 */
}

.river-viewport.is-dragging {
  cursor: grabbing;
}

/* 光河本体（线 + 光晕）：不随相机滚动，所以放在滚动视口的外面 */
.river-static {
  position: absolute;
  inset: 0;
  z-index: 0;
  pointer-events: none;
}

.river-track {
  position: absolute;
  top: 0;
  left: 0;
  width: var(--track-width, 100%);
  height: 100%;
  /* 不写 will-change：这条轨道八千多像素宽，离了分段成不了合成层。
     opacity 只在重放时淡出一次，用不着常驻提示。 */
  transition: opacity 0.7s var(--ease-out-soft);
}

/* 光河的一段：两千像素以内，单独成层，位移由合成器应用（子像素不会被取整）。
   段是稳定的——只在数据或视口变化时重建，不会随镜头对焦增删，所以没有
   "新建纹理"的顿挫，显存占用也只有可见的那一两段。 */
.river-chunk {
  position: absolute;
  top: 0;
  height: 100%;
  will-change: transform;
  /* 段与段是**互相重叠**的（给边界上的记忆留余量），后一段压在上一段上面。
     它自己不画任何东西，却默认会把那一整块的点击全部吃掉——于是落在重叠区里
     的记忆就"看得见、点不到"（用户报的"这条打不开"正是它，而且哪几条中招
     跟着视口宽度变：手机上第 3/6 条、电脑上最后一条）。
     这里不吃事件；卡片自己有 pointer-events: auto，照旧能点。 */
  pointer-events: none;
}

/* 光河：一条赭金色的线，像阳光落在纸面上（固定在眼前，不随相机移动） */
.river-line {
  position: absolute;
  top: var(--river-y);
  left: 0;
  width: 100%;
  height: 2px;
  z-index: 1;
  background: linear-gradient(
    to right,
    transparent 0%,
    color-mix(in oklab, var(--color-glow-600) 52%, transparent) 7%,
    color-mix(in oklab, var(--color-glow-500) 96%, transparent) 50%,
    color-mix(in oklab, var(--color-glow-600) 60%, transparent) 93%,
    transparent 100%
  );
  box-shadow:
    0 1px 0 color-mix(in oklab, oklch(1 0 0) 55%, transparent),
    0 4px 18px -3px color-mix(in oklab, var(--color-glow-500) 62%, transparent);
  pointer-events: none;
}

/* 河面上的暖晕 */
.river-glow {
  position: absolute;
  top: var(--river-y);
  left: 0;
  width: 100%;
  height: 64px;
  margin-top: -32px;
  background: linear-gradient(
    to bottom,
    transparent,
    color-mix(in oklab, var(--color-glow-600) 26%, transparent) 52%,
    transparent
  );
  filter: blur(18px);
  opacity: 0.9;
  pointer-events: none;
}

/* 光河上的浮标 */
.river-pip {
  position: absolute;
  top: var(--river-y);
  width: 6px;
  height: 6px;
  margin: -3px 0 0 -3px;
  border-radius: 50%;
  background: radial-gradient(circle, oklch(1 0 0) 22%, var(--color-glow-500) 70%);
  box-shadow:
    0 0 0 1.5px color-mix(in oklab, oklch(1 0 0) 70%, transparent),
    0 2px 10px color-mix(in oklab, var(--color-glow-500) 75%, transparent);
}

/* 顺着河面漂过去的光点 */
.sparks {
  position: fixed;
  inset: 0;
  z-index: 3;
  overflow: hidden;
  pointer-events: none;
}

.spark {
  position: absolute;
  top: var(--river-y);
  left: 0;
  width: 7px;
  height: 7px;
  margin: -3.5px 0 0 -3.5px;
  border-radius: 50%;
  background: radial-gradient(
    circle,
    oklch(1 0 0) 0%,
    var(--color-glow-400) 52%,
    transparent 74%
  );
  box-shadow: 0 0 16px color-mix(in oklab, var(--color-glow-500) 92%, transparent);
  opacity: 0;
  animation: spark-drift 24s linear infinite;
  animation-delay: calc(var(--i) * -2.6s);
  will-change: transform, opacity;
}

/* 时间刻度 */
.river-marker {
  position: absolute;
  top: calc(var(--river-y) + 5.6rem);
  transform: translateX(-50%);
  pointer-events: none;
}

/* 从刻度牵一条细线到光河 */
.river-marker::before {
  content: '';
  position: absolute;
  left: 50%;
  bottom: calc(100% + 0.3rem);
  width: 1px;
  height: 4.1rem;
  background: linear-gradient(
    to top,
    color-mix(in oklab, var(--color-glow-500) 55%, transparent),
    transparent
  );
  opacity: 0.6;
}

.river-marker.is-strong::before {
  height: 4.3rem;
  opacity: 0.85;
}

.river-marker__text {
  font-family: var(--font-display);
  font-size: 0.88rem;
  letter-spacing: 0.18em;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
}

.river-marker.is-strong .river-marker__text {
  font-size: 1.16rem;
  letter-spacing: 0.26em;
  color: color-mix(in oklab, var(--color-ink-100) 88%, transparent);
  text-shadow: 0 0 26px color-mix(in oklab, var(--color-glow-600) 45%, transparent);
}

/* ---------------- HUD ---------------- */
.hud {
  position: fixed;
  left: 0;
  right: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  /* 让开刘海、灵动岛与底部横条，横屏时也避开圆角 */
  padding-top: max(1.35rem, calc(var(--safe-top) + 0.85rem));
  padding-bottom: max(1.35rem, calc(var(--safe-bottom) + 0.85rem));
  padding-left: max(clamp(1.1rem, 3.2vw, 2.6rem), calc(var(--safe-left) + 0.6rem));
  padding-right: max(clamp(1.1rem, 3.2vw, 2.6rem), calc(var(--safe-right) + 0.6rem));
  pointer-events: none;
}

.hud > * {
  pointer-events: auto;
}

.hud--top {
  top: 0;
  align-items: flex-start;
}

.hud--bottom {
  bottom: 0;
  align-items: flex-end;
}

.brand__name {
  font-family: var(--font-serif);
  font-weight: 600;
  font-size: 1.5rem;
  letter-spacing: 0.24em;
  color: var(--color-ink-50);
  margin: 0;
  text-shadow: 0 1px 0 color-mix(in oklab, oklch(1 0 0) 70%, transparent);
}

.brand__meta {
  margin: 0.35rem 0 0;
  font-size: 0.76rem;
  letter-spacing: 0.06em;
  color: color-mix(in oklab, var(--color-ink-400) 92%, transparent);
}

.hud-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.6rem;
  height: 2.6rem;
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

.hud-icon:hover,
.hud-icon:focus-visible {
  opacity: 1;
  border-color: color-mix(in oklab, var(--color-glow-500) 66%, transparent);
  color: var(--color-glow-300);
  background: var(--color-ink-900);
}

.hud-icon:active {
  transform: scale(0.94);
}

.hud-icon--small {
  width: 2.2rem;
  height: 2.2rem;
}

.hud-icon--accent {
  opacity: 0.74;
  color: var(--color-glow-300);
  border-color: color-mix(in oklab, var(--color-glow-500) 52%, transparent);
  background: color-mix(in oklab, var(--color-glow-700) 46%, var(--color-ink-900));
}

.hud-icon--accent:hover {
  opacity: 1;
  color: var(--color-glow-400);
  box-shadow: 0 10px 24px -14px color-mix(in oklab, var(--color-glow-500) 90%, transparent);
}

.hud-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.controls {
  display: flex;
  align-items: center;
  gap: 0.55rem;
}

/* ---------------- 演示 ---------------- */
/* 演示时把其它操作收起来：右下角那一格让给「退出演示」的叉号（见 DemoExit） */
.controls.is-demo > :not(.demo-menu),
.hud-actions.is-demo .hud-icon {
  display: none;
}

/* 演示时不再接鼠标：拖不动、也点不开，专心看 */
.river-viewport.is-demo {
  pointer-events: none;
  cursor: default;
}

.scrubber {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  min-width: 0;
  max-width: 46vw;
}

.scrubber__date {
  font-size: 0.98rem;
  letter-spacing: 0.16em;
  color: var(--color-ink-50);
}

.scrubber__hint {
  font-size: 0.76rem;
  color: color-mix(in oklab, var(--color-ink-400) 94%, transparent);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ---------------- 覆盖层 ---------------- */
.veil {
  position: fixed;
  inset: 0;
  z-index: 30;
  display: grid;
  place-items: center;
  background: color-mix(in oklab, var(--color-ink-950) 92%, transparent);
  backdrop-filter: blur(6px);
}

.veil--empty {
  background: transparent;
  backdrop-filter: none;
  pointer-events: none;
}

.veil--empty > * {
  pointer-events: auto;
}

.veil__text {
  font-family: var(--font-serif);
  font-size: 1rem;
  letter-spacing: 0.3em;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
  animation: pulse-soft 3.2s var(--ease-drift) infinite;
}

.empty {
  max-width: 30rem;
  padding: 2.6rem 2.4rem;
  text-align: center;
  border-radius: var(--radius-xl);
  border: 1px solid color-mix(in oklab, var(--color-ink-700) 85%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 92%, transparent);
  backdrop-filter: blur(18px);
  box-shadow: 0 40px 80px -46px color-mix(in oklab, var(--color-shade) 85%, transparent);
}

.empty__title {
  margin: 0;
  font-size: 1.3rem;
  letter-spacing: 0.16em;
  color: var(--color-ink-50);
}

.empty__body {
  margin: 0.9rem 0 0;
  font-size: 0.86rem;
  line-height: 1.9;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
}

.end-note {
  position: fixed;
  left: 50%;
  top: 46%;
  z-index: 18;
  transform: translate(-50%, -50%);
  display: flex;
  align-items: center;
  gap: 0.9rem;
  pointer-events: none;
}

.end-note__line {
  width: 3rem;
  height: 1px;
  background: linear-gradient(
    to right,
    transparent,
    color-mix(in oklab, var(--color-glow-500) 55%, transparent)
  );
}

.end-note__line:last-child {
  transform: scaleX(-1);
}

.end-note__text {
  font-family: var(--font-serif);
  font-size: 1.05rem;
  letter-spacing: 0.3em;
  color: color-mix(in oklab, var(--color-glow-300) 92%, transparent);
  text-shadow: 0 0 30px color-mix(in oklab, var(--color-glow-500) 45%, transparent);
  white-space: nowrap;
}

@keyframes spark-drift {
  0% {
    transform: translate3d(102vw, 0, 0) scale(0.5);
    opacity: 0;
  }
  14% {
    opacity: 0.95;
  }
  76% {
    opacity: 0.8;
  }
  100% {
    transform: translate3d(-8vw, 0, 0) scale(1.05);
    opacity: 0;
  }
}

@media (max-width: 760px) {
  .spark:nth-child(n + 6) {
    display: none;
  }

  .brand__name {
    font-size: 1.2rem;
    letter-spacing: 0.2em;
  }
  .brand__meta {
    font-size: 0.7rem;
    max-width: 62vw;
  }
  .scrubber {
    max-width: 40vw;
  }
  .scrubber__hint {
    display: none;
  }
  .hud-icon {
    width: 2.5rem;
    height: 2.5rem;
  }
  .river-glow {
    top: calc(var(--river-y) - 30px);
    height: 60px;
  }
}
</style>
