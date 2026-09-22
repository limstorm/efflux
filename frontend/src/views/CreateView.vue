<script setup lang="ts">
import { computed, nextTick, onMounted, onBeforeUnmount, reactive, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import {
  AMBIENT_OPTIONS,
  isAmbientId,
  playAmbient,
  stopAmbient,
  type AmbientId,
} from '@/audio/ambient'
import MomentLightbox from '@/components/MomentLightbox.vue'
import { api, ApiError, uploadMedia } from '@/api/client'
import type { MediaItem, MomentInput } from '@/api/types'
import { useToast } from '@/composables/useToast'
import { useMomentsStore } from '@/stores/moments'
import { detectKind, prepareByKind } from '@/utils/thumbnail'
import { tagHue } from '@/utils/tagColor'
import { formatDuration } from '@/utils/format'

const router = useRouter()
const route = useRoute()
const toast = useToast()
const store = useMomentsStore()

type SlotStatus = 'preparing' | 'idle' | 'uploading' | 'done' | 'error'

interface MediaSlot {
  key: string
  kind: 'image' | 'video' | 'audio'
  name: string
  remote: MediaItem | null
  file: File | null
  previewUrl: string | null
  thumb: Blob | null
  width: number | null
  height: number | null
  durationMs: number | null
  status: SlotStatus
  progress: number
  error: string | null
}

const title = ref('')
const content = ref('')
const tagInput = ref('')
const tags = ref<string[]>([])
const slots = ref<MediaSlot[]>([])
const submitting = ref(false)
const uploading = ref(false)
const dragging = ref(false)
const ready = ref(false)

const useCustomTime = ref(false)
/** 时间拆成两格：日期与时刻各一个原生输入，手机上各自唤起干净的日历／时钟 */
const customDate = ref('')
const customClock = ref('')
/** 拼回 `YYYY-MM-DDTHH:mm`，提交时照旧 */
const customTime = computed(() =>
  customDate.value ? `${customDate.value}T${customClock.value || '00:00'}` : '',
)
const editId = computed(() => (typeof route.query.edit === 'string' ? route.query.edit : null))

const visualInput = ref<HTMLInputElement | null>(null)
const audioInput = ref<HTMLInputElement | null>(null)

const visualSlots = computed(() => slots.value.filter((slot) => slot.kind !== 'audio'))
const audioSlots = computed(() => slots.value.filter((slot) => slot.kind === 'audio'))

/**
 * 这一格显示哪张图：刚在本机选的走 blob 预览，改一条老记忆时没有本地文件，
 * 就看服务器上的缩略图。视频没有封面时返回 null——别把 mp4 塞给 <img>。
 */
function slotImage(slot: MediaSlot): string | null {
  if (slot.previewUrl) return slot.previewUrl
  const remote = slot.remote
  if (!remote) return null
  return remote.thumbUrl ?? (slot.kind === 'image' ? remote.url : null)
}
const musicSlot = computed(() => audioSlots.value[0] ?? null)

// 氛围音：默认安静。以前默认给一段（炉火），等于每段新记忆都自带声音——
// 想要安静的人还得先找到这个开关把它关掉。要氛围的点一下就行，四个都在原位。
const ambient = ref<AmbientId | null>(null)
/** 正在响的那一种（和"选中"是两回事：选中了也可以先不听） */
const ambientPreview = ref<AmbientId | null>(null)

const ambientHint = computed(
  () => AMBIENT_OPTIONS.find((option) => option.id === ambient.value)?.hint ?? '',
)

// 试听
const previewingKey = ref<string | null>(null)
const previewAudio = ref<HTMLAudioElement | null>(null)
const previewSource = computed(
  () => slots.value.find((slot) => slot.key === previewingKey.value)?.previewUrl ?? undefined,
)

// 拖动排序
const dragKey = ref<string | null>(null)
const dragOverKey = ref<string | null>(null)

const canSubmit = computed(() => {
  if (submitting.value) return false
  const hasText = title.value.trim().length > 0 || content.value.trim().length > 0
  const hasMedia = slots.value.length > 0
  return hasText || hasMedia
})

const submitLabel = computed(() => {
  if (uploading.value) {
    const total = slots.value.filter((slot) => !slot.remote && slot.file).length
    const done = slots.value.filter((slot) => slot.status === 'done' && !slot.remote).length
    return `正在保存 ${Math.min(done + 1, total)}/${total}…`
  }
  if (submitting.value) return '正在整理…'
  return editId.value ? '保存修改' : '把这一刻留住'
})

const suggestedTags = computed(() =>
  store.tags
    .map((tag) => tag.name)
    .filter((name) => !tags.value.includes(name))
    .slice(0, 10),
)

// ---------------------------------------------------------------- 媒体
function revoke(slot: MediaSlot) {
  if (slot.previewUrl) URL.revokeObjectURL(slot.previewUrl)
}

function addSlot(slot: MediaSlot) {
  slots.value.push(slot)
}

async function ingest(files: File[]) {
  const accepted = files
    .map((file) => ({ file, kind: detectKind(file) }))
    .filter((entry): entry is { file: File; kind: 'image' | 'video' | 'audio' } => Boolean(entry.kind))

  if (!accepted.length) {
    if (files.length) toast.error('这些文件暂时收不住，试试照片、视频或音乐')
    return
  }

  // 背景音乐只保留一首：拖进来的新曲子会换掉旧的，也会顶掉氛围音
  let audioTaken = false
  if (accepted.some((entry) => entry.kind === 'audio')) {
    slots.value.filter((slot) => slot.kind === 'audio').forEach((slot) => removeSlot(slot.key))
    stopPreview()
    ambient.value = null
    stopAmbientTest()
  }

  for (const { file, kind } of accepted) {
    if (kind === 'audio') {
      if (audioTaken) continue
      audioTaken = true
    }

    // 必须是响应式对象：缩略图与上传进度会在稍后异步写回
    const slot = reactive<MediaSlot>({
      key: `${Date.now()}-${Math.random().toString(36).slice(2)}`,
      kind,
      name: file.name,
      remote: null,
      file,
      previewUrl: kind === 'audio' ? null : URL.createObjectURL(file),
      thumb: null,
      width: null,
      height: null,
      durationMs: null,
      status: 'preparing',
      progress: 0,
      error: null,
    })
    addSlot(slot)

    void prepareByKind(file, kind).then((prepared) => {
      slot.thumb = prepared.thumb
      slot.width = prepared.width
      slot.height = prepared.height
      slot.durationMs = prepared.durationMs
      slot.status = 'idle'
    })
  }
}

function onPickVisual(event: Event) {
  const input = event.target as HTMLInputElement
  if (input.files?.length) void ingest(Array.from(input.files))
  input.value = ''
}

/** 背景音乐只留一首，新选的会替换掉原来的 */
function onPickAudio(event: Event) {
  const input = event.target as HTMLInputElement
  const files = Array.from(input.files ?? [])
  input.value = ''

  const audio = files.find((file) => detectKind(file) === 'audio')
  if (!audio) {
    if (files.length) toast.info('这里放的是一首歌（mp3、m4a、wav…）')
    return
  }

  slots.value.filter((slot) => slot.kind === 'audio').forEach((slot) => removeSlot(slot.key))
  stopPreview()
  void ingest([audio])
}

async function togglePreview(slot: MediaSlot) {
  const audio = previewAudio.value
  if (!audio || !slot.previewUrl) return

  if (previewingKey.value === slot.key) {
    stopPreview()
    return
  }

  previewingKey.value = slot.key
  await nextTick()
  try {
    audio.currentTime = 0
    await audio.play()
  } catch {
    previewingKey.value = null
  }
}

function stopPreview() {
  previewAudio.value?.pause()
  previewingKey.value = null
}

// ---------------------------------------------------------------- 氛围音

/** 点一下就选中并试听：这两个动作本来就是一回事；再点一次就停 */
async function pickAmbient(id: AmbientId) {
  ambient.value = id
  stopPreview()
  if (ambientPreview.value === id) {
    stopAmbientTest()
    return
  }
  ambientPreview.value = id
  const sounding = await playAmbient(id)
  if (!sounding) {
    ambientPreview.value = null
    toast.info('这台设备现在放不出声音，碰一下屏幕再试')
  }
}

/** 安静：这一段记忆什么都不放 */
function pickQuiet() {
  ambient.value = null
  stopAmbientTest()
}

function stopAmbientTest() {
  stopAmbient()
  ambientPreview.value = null
}

/**
 * 有没有"真鼠标"（含触控板）。只有它才用 HTML5 拖放：
 * 触摸设备上 dragstart/dragover/drop 根本不触发，而且长按 draggable 元素还会起
 * 浏览器自己的原生拖拽。触屏走下面的长按拖动。
 */
const canHover = window.matchMedia?.('(hover: hover) and (pointer: fine)').matches ?? false

/** 把某个格子挪到另一个格子的位置（两条拖动路径共用） */
function moveSlot(from: string, to: string) {
  if (!from || !to || from === to) return
  const list = slots.value
  const fromIndex = list.findIndex((item) => item.key === from)
  const toIndex = list.findIndex((item) => item.key === to)
  if (fromIndex < 0 || toIndex < 0) return
  const [item] = list.splice(fromIndex, 1)
  if (item) list.splice(toIndex, 0, item)
}

// ---------------------------------------------------------------- 排序（触屏）
/**
 * 手机浏览器上排不了序，是两件事同时挡着：
 * · HTML5 拖放（draggable + dragstart/dragover/drop）在触摸设备上压根不触发；
 * · 长按 <img> 会弹出浏览器的原生菜单（"查看图片 / 下载图片"），连拖动的机会都没有。
 *
 * 所以触屏改成指针 + 长按：按住 240ms 进入拖动（不到时间就移动 = 想滚动，直接让位），
 * 移动时用 elementFromPoint 找出指下的格子当落点，松手落位。
 * 原生菜单那头也关掉：CSS 上的 -webkit-touch-callout / user-select / user-drag，
 * 加上缩略图上的 @contextmenu.prevent。
 */
const LONG_PRESS_MS = 240
let pressTimer = 0
let pressFrom = { x: 0, y: 0 }
let pressEl: HTMLElement | null = null
let pressPointer = -1

/** 拖动期间拦住滚动：touch-action 在手势开始时已定、改不了，但长按这会儿手指还没划动，
 *  第一次 touchmove 里 preventDefault 仍然拦得住 */
function stopScrollWhileDragging(event: TouchEvent) {
  if (dragKey.value) event.preventDefault()
}

function onThumbPressStart(slot: MediaSlot, event: PointerEvent) {
  if (event.pointerType === 'mouse') return // 鼠标交给 HTML5 拖放
  pressFrom = { x: event.clientX, y: event.clientY }
  pressEl = (event.target as HTMLElement | null)?.closest('.thumb') as HTMLElement | null
  pressPointer = event.pointerId
  window.clearTimeout(pressTimer)
  pressTimer = window.setTimeout(() => {
    pressTimer = 0
    dragKey.value = slot.key
    dragOverKey.value = null
    // 捕获指针：手指移出这个格子之后，move/up 仍然发给它
    pressEl?.setPointerCapture?.(pressPointer)
    window.addEventListener('touchmove', stopScrollWhileDragging, { passive: false })
  }, LONG_PRESS_MS)
}

function onThumbPressMove(event: PointerEvent) {
  if (pressTimer) {
    // 还没到长按时间就明显动了：那是想滚动页面，别抢
    if (Math.hypot(event.clientX - pressFrom.x, event.clientY - pressFrom.y) > 10) {
      window.clearTimeout(pressTimer)
      pressTimer = 0
    }
    return
  }
  if (!dragKey.value) return
  const under = document.elementFromPoint(event.clientX, event.clientY)?.closest('.thumb')
  const key = under?.getAttribute('data-key') ?? ''
  if (key && key !== dragKey.value) dragOverKey.value = key
}

function onThumbPressEnd() {
  window.clearTimeout(pressTimer)
  pressTimer = 0
  window.removeEventListener('touchmove', stopScrollWhileDragging)
  const from = dragKey.value
  const to = dragOverKey.value
  dragKey.value = null
  dragOverKey.value = null
  if (!from || !to) return
  moveSlot(from, to)
  suppressSlotClick = true
  window.setTimeout(() => {
    suppressSlotClick = false
  }, 350)
}

// ---------------------------------------------------------------- 点一下看大图
/**
 * 点一下缩略图直接进大图，和记忆页共用同一个查看器（左右切图 / 视频、Esc 关闭、方向键翻页）。
 * 分工：单击 = 看大图，长按 240ms（触屏）或直接拖（鼠标）= 排序。
 * 大图里额外给一个"移除"（左上角回收站），位置和颜色都跟右上角的叉号分开。
 */
const previewIndex = ref(-1)
/** 刚拖完那一下会跟一个 click，别让它顺手把大图打开 */
let suppressSlotClick = false

/** 看大图看的是"本机这份"：预览地址优先，其次服务器上的 */
const viewerItems = computed(() =>
  visualSlots.value
    .map((slot) => ({
      id: slot.key,
      kind: slot.kind === 'video' ? ('video' as const) : ('image' as const),
      url: slot.previewUrl ?? slot.remote?.url ?? slotImage(slot) ?? '',
      thumbUrl: slot.remote?.thumbUrl ?? slot.previewUrl ?? '',
    }))
    .filter((item) => item.url),
)

function openPreview(slot: MediaSlot) {
  if (suppressSlotClick) return
  const index = viewerItems.value.findIndex((item) => item.id === slot.key)
  if (index >= 0) previewIndex.value = index
}

function stepPreview(delta: number) {
  const count = viewerItems.value.length
  if (!count) return
  previewIndex.value = (previewIndex.value + delta + count) % count
}

/** 大图里移除：删完接着看后面的；删光了就关掉 */
function removeFromPreview(id: string) {
  const index = viewerItems.value.findIndex((item) => item.id === id)
  removeSlot(id)
  toast.info('已移除这一段 · 保存后生效')
  const left = viewerItems.value.length
  if (!left) {
    previewIndex.value = -1
    return
  }
  if (index >= 0) previewIndex.value = Math.min(index, left - 1)
}

function onThumbDragStart(slot: MediaSlot, event: DragEvent) {
  dragKey.value = slot.key
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', slot.key)
  }
}

function onThumbDragOver(slot: MediaSlot, event: DragEvent) {
  event.preventDefault()
  if (dragKey.value && dragKey.value !== slot.key) dragOverKey.value = slot.key
}

function onThumbDrop(slot: MediaSlot, event: DragEvent) {
  event.preventDefault()
  const from = dragKey.value
  dragKey.value = null
  dragOverKey.value = null
  if (from) moveSlot(from, slot.key)
}

function onThumbDragEnd() {
  dragKey.value = null
  dragOverKey.value = null
}

function onDrop(event: DragEvent) {
  dragging.value = false
  const files = event.dataTransfer?.files
  if (files?.length) void ingest(Array.from(files))
}

function removeSlot(key: string) {
  const index = slots.value.findIndex((slot) => slot.key === key)
  if (index < 0) return
  const slot = slots.value[index]!
  revoke(slot)
  // 编辑模式下移除已有媒体：提交时不再关联，服务器会在之后自动回收
  slots.value.splice(index, 1)
}

// ---------------------------------------------------------------- 标签
function addTag(raw: string) {
  const name = raw.trim().slice(0, 32)
  if (!name) return
  if (tags.value.length >= 12) {
    toast.info('标签最多 12 个，够用啦')
    return
  }
  if (!tags.value.some((tag) => tag.toLowerCase() === name.toLowerCase())) {
    tags.value.push(name)
  }
  tagInput.value = ''
}

function onTagKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter' || event.key === ',') {
    event.preventDefault()
    addTag(tagInput.value)
  } else if (event.key === 'Backspace' && !tagInput.value && tags.value.length) {
    tags.value.pop()
  }
}

function removeTag(name: string) {
  tags.value = tags.value.filter((tag) => tag !== name)
}

// ---------------------------------------------------------------- 地点
const placeCity = ref('')
const placeName = ref('')
const placeCoords = ref<{ latitude: number; longitude: number } | null>(null)
const locating = ref(false)

/** 坐标一拿到就去后端问一声「这是哪个市」，让城市当场出现在表单里 */
async function fillCityFromCoords(latitude: number, longitude: number) {
  try {
    const city = await api.cityAt(latitude, longitude)
    if (city && !placeCity.value.trim()) placeCity.value = city
  } catch {
    // 问不到也没关系，保存时后端还会再推断一次
  }
}

/** 取当前位置：浏览器只给坐标，具体地名还是留给人来写 */
function useCurrentLocation() {
  if (!('geolocation' in navigator)) {
    toast.info('这个浏览器不给定位')
    return
  }
  locating.value = true
  navigator.geolocation.getCurrentPosition(
    (position) => {
      const latitude = Number(position.coords.latitude.toFixed(6))
      const longitude = Number(position.coords.longitude.toFixed(6))
      placeCoords.value = { latitude, longitude }
      locating.value = false
      toast.success('位置已记下')
      void fillCityFromCoords(latitude, longitude)
    },
    () => {
      locating.value = false
      toast.info('没能拿到位置，可能是没给权限')
    },
    { enableHighAccuracy: false, timeout: 8000, maximumAge: 60_000 },
  )
}

function clearPlaceCoords() {
  placeCoords.value = null
}

// ---------------------------------------------------------------- 时间
function pad2(n: number): string {
  return String(n).padStart(2, '0')
}

function toDateValue(date: Date): string {
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`
}

function toClockValue(date: Date): string {
  return `${pad2(date.getHours())}:${pad2(date.getMinutes())}`
}

/** 把一个时刻摊进「日期」与「几点」两格（都按本地时间算） */
function setTimeTo(iso: string) {
  const date = new Date(iso)
  customDate.value = toDateValue(date)
  customClock.value = toClockValue(date)
}

function onToggleCustomTime(next: boolean) {
  useCustomTime.value = next
  if (next && !customDate.value) setTimeTo(new Date().toISOString())
}

/** 快捷：「昨天」「前天」都按今天往回数，时刻沿用已填的那个 —— 点两下不会越退越远 */
function shiftDays(days: number) {
  const target = new Date()
  target.setDate(target.getDate() - days)
  customDate.value = toDateValue(target)
  if (!customClock.value) customClock.value = toClockValue(new Date())
}

function backToNow() {
  setTimeTo(new Date().toISOString())
}

// ---------------------------------------------------------------- 编辑模式
async function loadForEdit(id: string) {
  try {
    const detail = await api.getMoment(id)
    title.value = detail.title
    content.value = detail.content
    tags.value = [...detail.tags]
    const place = detail.place
    placeCity.value = place?.city ?? ''
    placeName.value = place?.name ?? ''
    placeCoords.value =
      place?.latitude != null && place.longitude != null
        ? { latitude: place.latitude, longitude: place.longitude }
        : null
    setTimeTo(detail.happenedAt)
    useCustomTime.value = Math.abs(
      new Date(detail.happenedAt).getTime() - new Date(detail.createdAt).getTime(),
    ) > 90_000

    // 有音乐文件就听文件，没有才轮到氛围音（两者只会有一个）
    ambient.value = isAmbientId(detail.ambient) ? detail.ambient : null

    const restored: MediaItem[] = [...detail.media]
    if (detail.music) restored.push(detail.music)

    slots.value = restored.map((media) =>
      reactive<MediaSlot>({
        key: media.id,
        kind: media.kind,
        name: media.kind === 'audio' ? '背景音乐' : '已有的画面',
        remote: media,
        file: null,
        previewUrl: null,
        thumb: null,
        width: media.width,
        height: media.height,
        durationMs: media.durationMs,
        status: 'done',
        progress: 1,
        error: null,
      }),
    )
  } catch (err) {
    toast.error(err instanceof ApiError ? err.message : '打不开这段记忆')
    void router.push({ name: 'home' })
  }
}

// ---------------------------------------------------------------- 提交
async function submit() {
  if (!canSubmit.value) {
    toast.info('写点什么，或者放一张照片吧')
    return
  }

  submitting.value = true
  try {
    // 1. 先上传还没上传的媒体
    const pending = slots.value.filter((slot) => !slot.remote && slot.file)
    if (pending.length) uploading.value = true

    for (const slot of pending) {
      slot.status = 'uploading'
      slot.progress = 0
      try {
        const handle = uploadMedia(
          slot.kind,
          {
            file: slot.file!,
            thumb: slot.thumb,
            width: slot.width,
            height: slot.height,
            durationMs: slot.durationMs,
          },
          (ratio) => (slot.progress = ratio),
        )
        const media = await handle.promise
        slot.remote = media
        slot.status = 'done'
        slot.progress = 1
      } catch (err) {
        slot.status = 'error'
        slot.error = err instanceof ApiError ? err.message : '上传失败'
        throw err
      }
    }
    uploading.value = false

    // 2. 组装并提交
    const mediaLinks = slots.value
      .filter((slot) => slot.remote)
      .map((slot, index) => ({ id: slot.remote!.id, position: index }))

    const happenedAt = useCustomTime.value && customTime.value
      ? new Date(customTime.value).toISOString()
      : undefined

    const input: MomentInput = {
      title: title.value,
      content: content.value,
      tags: tags.value,
      happenedAt,
      media: mediaLinks,
      // 空串是「不要了」：编辑一条老记忆时没选氛围，不会悄悄给它加上一段
      ambient: ambient.value ?? '',
      locationName: placeName.value.trim(),
      city: placeCity.value.trim(),
      // 坐标成对才有意义；没点过定位就传 null，后端按"没填"处理
      latitude: placeCoords.value?.latitude ?? null,
      longitude: placeCoords.value?.longitude ?? null,
    }

    const saved = editId.value
      ? await api.updateMoment(editId.value, input)
      : await api.createMoment(input)

    void store.load(true)
    toast.success(editId.value ? '已经改好了' : '这一刻，留住了')
    void router.replace({ name: 'moment', params: { id: saved.id } })
  } catch (err) {
    uploading.value = false
    const message = err instanceof ApiError ? err.message : '保存失败了，再试一次吧'
    toast.error(message)
  } finally {
    submitting.value = false
  }
}

function goBack() {
  const state = window.history.state as { back?: string | null } | null
  if (state?.back) router.back()
  else void router.push({ name: 'home' })
}

// ---------------------------------------------------------------- 生命周期
onMounted(async () => {
  store.loadTags()
  if (editId.value) {
    await loadForEdit(editId.value)
  }
  ready.value = true
})

onBeforeUnmount(() => {
  slots.value.forEach(revoke)
  stopAmbientTest()
})
</script>

<template>
  <div class="create-view" @dragover.prevent="dragging = true" @dragleave.prevent="dragging = false" @drop.prevent="onDrop">
    <header class="topbar">
      <button type="button" class="hud-icon" aria-label="返回" title="返回（Esc）" @click="goBack">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" class="h-5 w-5">
          <path d="M14.5 5.5 8 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <p class="topbar__hint">不用想太多，随手记下就好</p>
      <span class="topbar__spacer" />
    </header>

    <main class="sheet">
      <h1 class="sheet__heading title-serif">{{ editId ? '修改这段流光' : '记下一段流光' }}</h1>

      <!-- 画面 -->
      <section class="field">
        <div class="field__head">
          <span class="field__label">画面</span>
          <span class="field__note">照片、视频，可以一次选多个</span>
        </div>

        <button
          type="button"
          class="dropzone"
          :class="{ 'is-active': dragging }"
          @click="visualInput?.click()"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" class="h-6 w-6">
            <path d="M12 16V5m0 0-3.6 3.6M12 5l3.6 3.6" stroke-linecap="round" stroke-linejoin="round" />
            <path d="M4.5 15v3.2A1.8 1.8 0 0 0 6.3 20h11.4a1.8 1.8 0 0 0 1.8-1.8V15" stroke-linecap="round" />
          </svg>
          <span class="dropzone__text">点一下选择，或直接拖进来</span>
        </button>

        <input
          ref="visualInput"
          type="file"
          class="sr-only"
          multiple
          accept="image/*,video/*"
          @change="onPickVisual"
        />

        <template v-if="visualSlots.length">
          <p class="thumbs__hint">按住可以拖动排序，第一张会成为时间轴上的封面</p>
          <ul class="thumbs">
            <li
              v-for="slot in visualSlots"
              :key="slot.key"
              class="thumb"
              :class="{ 'is-dragging': dragKey === slot.key, 'is-over': dragOverKey === slot.key }"
              :data-key="slot.key"
              :draggable="canHover"
              @dragstart="onThumbDragStart(slot, $event)"
              @dragover="onThumbDragOver(slot, $event)"
              @drop="onThumbDrop(slot, $event)"
              @dragend="onThumbDragEnd"
              @pointerdown="onThumbPressStart(slot, $event)"
              @pointermove="onThumbPressMove"
              @pointerup="onThumbPressEnd"
              @pointercancel="onThumbPressEnd"
              @contextmenu.prevent
              @click="openPreview(slot)"
            >
              <!-- 本机刚选的看 blob 预览；改一条老记忆时没有本地文件，依次退到：
                   服务器缩略图 → 视频拿原文件取首帧 → 图片用原图。
                   总之别留下一个空格子让人猜哪个是哪个 -->
              <video
                v-if="slot.kind === 'video' && slot.previewUrl"
                :src="slot.previewUrl"
                muted
                playsinline
              />
              <!-- 预览一律走 CSS 背景，不用 <img>：
                   手机浏览器（微信 / UC / QQ 这类自带内核的尤其激进）会给 <img> 元素挂
                   "查看图片 / 下载图片"的原生菜单——它们的菜单是内核自己的 UI，JS 拦不住，
                   轻则抢走长按拖动，重则单击就弹。没有图片元素，就没有菜单可挂。
                   视频仍用 <video>（要能预览播放），但已经 pointer-events:none 排除在事件之外。 -->
              <span
                v-else-if="slotImage(slot)"
                class="thumb__pic"
                role="img"
                :aria-label="slot.name"
                :style="{ backgroundImage: `url(${slotImage(slot)})` }"
              />
              <!-- 没有缩略图的视频（早先传的、或首帧没生成出来）：拿原文件取首帧。
                   preload=metadata 只读文件头；#t=0.1 是让浏览器把那一帧画出来的老办法 -->
              <video
                v-else-if="slot.kind === 'video' && slot.remote"
                :src="`${slot.remote.url}#t=0.1`"
                preload="metadata"
                muted
                playsinline
              />
              <span class="thumb__kind">{{ slot.kind === 'video' ? '视频' : '照片' }}</span>

              <span v-if="slot.status === 'preparing'" class="thumb__mask">准备中…</span>
              <span v-else-if="slot.status === 'uploading'" class="thumb__mask">
                {{ Math.round(slot.progress * 100) }}%
              </span>
              <span v-else-if="slot.status === 'error'" class="thumb__mask thumb__mask--error">失败</span>

              <button
                type="button"
                class="thumb__remove"
                aria-label="移除"
                title="移除这一段"
                @click.stop="removeSlot(slot.key)"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="h-3 w-3">
                  <path d="M6 6l12 12M18 6 6 18" stroke-linecap="round" />
                </svg>
              </button>
            </li>
          </ul>
        </template>
      </section>

      <!-- 背景音乐 -->
      <section class="field">
        <div class="field__head">
          <span class="field__label">背景音乐</span>
          <span class="field__note">点一下就能听见 · 打开这段记忆时会自动播放</span>
        </div>

        <div v-if="musicSlot" class="music-card">
          <button
            type="button"
            class="music-card__play"
            :aria-label="previewingKey === musicSlot.key ? '暂停试听' : '试听'"
            @click="togglePreview(musicSlot)"
          >
            <svg
              v-if="previewingKey === musicSlot.key"
              viewBox="0 0 24 24"
              fill="currentColor"
              class="h-4 w-4"
            >
              <rect x="7" y="5.5" width="3.4" height="13" rx="1.2" />
              <rect x="13.6" y="5.5" width="3.4" height="13" rx="1.2" />
            </svg>
            <svg v-else viewBox="0 0 24 24" fill="currentColor" class="h-4 w-4">
              <path d="M8 5.2v13.6a.9.9 0 0 0 1.37.77l11.1-6.8a.9.9 0 0 0 0-1.54L9.37 4.43A.9.9 0 0 0 8 5.2Z" />
            </svg>
          </button>

          <div class="music-card__body">
            <span class="music-card__title">{{ musicSlot.name }}</span>
            <span class="music-card__meta">
              <template v-if="musicSlot.status === 'uploading'">
                正在上传 {{ Math.round(musicSlot.progress * 100) }}%
              </template>
              <template v-else-if="musicSlot.status === 'preparing'">正在读取…</template>
              <template v-else-if="musicSlot.status === 'error'">上传失败，可以重试</template>
              <template v-else-if="musicSlot.durationMs">
                {{ formatDuration(musicSlot.durationMs) }} · 点左边试听
              </template>
              <template v-else>已备好 · 点左边试听</template>
            </span>
          </div>

          <button
            type="button"
            class="music-card__remove"
            aria-label="移除背景音乐"
            @click="removeSlot(musicSlot.key)"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="h-3.5 w-3.5">
              <path d="M6 6l12 12M18 6 6 18" stroke-linecap="round" />
            </svg>
          </button>
        </div>

        <template v-else>
          <!-- 氛围音：现场合成，不用先去弄到一个音频文件 -->
          <div class="mood-pick">
            <button
              v-for="option in AMBIENT_OPTIONS"
              :key="option.id"
              type="button"
              class="mood-chip"
              :class="{
                'is-on': ambient === option.id,
                'is-sounding': ambientPreview === option.id,
              }"
              :aria-pressed="ambient === option.id"
              @click="pickAmbient(option.id)"
            >
              {{ option.name }}
            </button>
            <button
              type="button"
              class="mood-chip"
              :class="{ 'is-on': ambient === null }"
              :aria-pressed="ambient === null"
              @click="pickQuiet"
            >
              安静
            </button>
          </div>

          <p class="field__note mood-note">
            {{ ambientHint || '什么声音都不放。想找回一点现场感，也可以传一首进去。' }}
          </p>

          <button type="button" class="music-add" @click="audioInput?.click()">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="h-5 w-5">
              <path d="M9 18V6l10-2v12" stroke-linecap="round" stroke-linejoin="round" />
              <circle cx="6.5" cy="18" r="2.5" />
              <circle cx="16.5" cy="16" r="2.5" />
            </svg>
            <span>或者，从手机里选一首</span>
          </button>
        </template>

        <input
          ref="audioInput"
          type="file"
          class="sr-only"
          accept="audio/*"
          @change="onPickAudio"
        />

        <audio ref="previewAudio" :src="previewSource" @ended="previewingKey = null" />
      </section>

      <!-- 主题 -->
      <section class="field">
        <label class="field__label" for="moment-title">主题</label>
        <input
          id="moment-title"
          v-model="title"
          class="paper-input"
          type="text"
          maxlength="60"
          placeholder="给这一刻起个名字（可以留空）"
        />
      </section>

      <!-- 正文 -->
      <section class="field">
        <label class="field__label" for="moment-content">正文</label>
        <textarea
          id="moment-content"
          v-model="content"
          class="paper-input prose-input"
          rows="5"
          placeholder="此刻想记下什么？一句话也好。"
        />
      </section>

      <!-- 标签 -->
      <section class="field">
        <div class="field__head">
          <label class="field__label" for="moment-tag">标签</label>
          <span class="field__note">方便以后找回来</span>
        </div>
        <div class="tag-box">
          <span v-for="tag in tags" :key="tag" class="tag-pill" :style="{ '--tag-hue': tagHue(tag) }">
            {{ tag }}
            <button type="button" aria-label="移除标签" @click="removeTag(tag)">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" class="h-2.5 w-2.5">
                <path d="M6 6l12 12M18 6 6 18" stroke-linecap="round" />
              </svg>
            </button>
          </span>
          <input
            id="moment-tag"
            v-model="tagInput"
            class="tag-box__input"
            type="text"
            maxlength="32"
            placeholder="输入后按回车"
            @keydown="onTagKeydown"
            @blur="addTag(tagInput)"
          />
        </div>
        <div v-if="suggestedTags.length" class="suggestions">
          <button
            v-for="tag in suggestedTags"
            :key="tag"
            type="button"
            class="tag-pill"
            :style="{ '--tag-hue': tagHue(tag) }"
            @click="addTag(tag)"
          >
            {{ tag }}
          </button>
        </div>
      </section>

      <!-- 地点 -->
      <section class="field">
        <div class="field__head">
          <span class="field__label">发生在哪儿</span>
          <button type="button" class="field__toggle" @click="useCurrentLocation">
            {{ locating ? '定位中…' : placeCoords ? '重新定位' : '取当前位置' }}
          </button>
        </div>
        <div class="place-inputs">
          <input
            v-model="placeCity"
            class="paper-input place-input place-input--city"
            type="text"
            maxlength="24"
            placeholder="城市"
            aria-label="城市"
          />
          <input
            v-model="placeName"
            class="paper-input place-input"
            type="text"
            maxlength="60"
            placeholder="具体地点"
            aria-label="具体地点"
          />
        </div>
        <p class="field__value">
          <template v-if="placeCoords">
            已记下坐标 {{ placeCoords.latitude.toFixed(4) }}, {{ placeCoords.longitude.toFixed(4) }}
            ·
            <button type="button" class="field__toggle" @click="clearPlaceCoords">去掉</button>
          </template>
          <template v-else>留白也没关系。写个城市，以后能按地方翻找。</template>
        </p>
      </section>

      <!-- 时间 -->
      <section class="field">
        <div class="field__head">
          <span class="field__label">发生在什么时候</span>
          <button type="button" class="field__toggle" @click="onToggleCustomTime(!useCustomTime)">
            {{ useCustomTime ? '用「此刻」' : '改成别的时间' }}
          </button>
        </div>
        <template v-if="useCustomTime">
          <!-- 日期与时刻分开：合并成一个 datetime-local 时，有些手机浏览器
               只给一个难用的滚轮（甚至退化成要手打的文本框） -->
          <div class="time-inputs">
            <input
              v-model="customDate"
              class="paper-input paper-input--time"
              type="date"
              aria-label="哪一天"
            />
            <input
              v-model="customClock"
              class="paper-input paper-input--time"
              type="time"
              aria-label="几点"
            />
          </div>
          <div class="time-quick">
            <button type="button" class="time-quick__chip" @click="backToNow">此刻</button>
            <button type="button" class="time-quick__chip" @click="shiftDays(1)">昨天</button>
            <button type="button" class="time-quick__chip" @click="shiftDays(2)">前天</button>
          </div>
        </template>
        <p v-else class="field__value">此刻（现在）</p>
      </section>

      <!-- 提交 -->
      <div class="actions">
        <button type="button" class="glow-btn px-8 py-3 text-[0.95rem]" :disabled="!canSubmit" @click="submit">
          {{ submitLabel }}
        </button>
        <p v-if="!canSubmit && ready" class="actions__hint">写几个字，或加一张照片就能保存</p>
      </div>
    </main>

    <div v-if="dragging" class="drop-veil" aria-hidden="true">
      <span>松手，收进这段时光</span>
    </div>

    <!-- 点一下缩略图直接看大图：看的是本机这份（刚选的看 blob、旧记忆看服务器上的），
         与记忆页共用同一个查看器（左右切图 / 视频、Esc 关闭、方向键翻页）。
         这里额外开一个"移除"：左上角回收站，和右上角的叉号分得清楚。
         长按（触屏）或直接拖（鼠标）仍然是排序。 -->
    <MomentLightbox
      :items="viewerItems"
      :index="previewIndex"
      :title="title"
      removable
      @close="previewIndex = -1"
      @step="stepPreview"
      @remove="removeFromPreview"
    />
  </div>
</template>

<style scoped>
/* 城市与地点并排：像写一句地址，而不是填两个大格子 */
.place-inputs {
  display: flex;
  gap: 0.55rem;
  margin-top: 0.55rem;
}

.place-input {
  width: auto;
  min-width: 0;
  flex: 1;
  padding: 0.56rem 0.82rem;
  font-size: 0.88rem;
  letter-spacing: 0.04em;
}

.place-input--city {
  flex: 0 0 7rem;
}
.create-view {
  position: relative;
  min-height: 100vh;
  min-height: 100dvh;
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
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

.topbar__hint {
  margin: 0;
  font-size: 0.78rem;
  letter-spacing: 0.1em;
  color: color-mix(in oklab, var(--color-ink-400) 82%, transparent);
}

.topbar__spacer {
  width: 2.5rem;
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
  transition: opacity 0.4s var(--ease-out-soft), color 0.4s var(--ease-out-soft), transform 0.25s var(--ease-out-soft);
}

.hud-icon:hover {
  opacity: 1;
  color: var(--color-glow-300);
}

.hud-icon:active {
  transform: scale(0.94);
}

/* ---------------- 表单 ---------------- */
.sheet {
  position: relative;
  z-index: 1;
  max-width: 42rem;
  margin: 0 auto;
  padding: clamp(4.4rem, 9vh, 6rem) clamp(1.1rem, 4vw, 2rem) 1rem;
  display: flex;
  flex-direction: column;
  gap: 2.1rem;
}

.sheet__heading {
  margin: 0;
  font-size: clamp(1.5rem, 4vw, 2rem);
  letter-spacing: 0.06em;
  color: var(--color-ink-50);
}

.field {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}

.field__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 1rem;
}

.field__label {
  font-size: 0.85rem;
  letter-spacing: 0.12em;
  color: color-mix(in oklab, var(--color-ink-200) 92%, transparent);
}

.field__note,
.field__value {
  margin: 0;
  font-size: 0.76rem;
  color: color-mix(in oklab, var(--color-ink-400) 86%, transparent);
}

.field__value {
  padding: 0.75rem 0;
  font-size: 0.9rem;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
}

.field__toggle {
  border: none;
  background: none;
  padding: 0;
  font-size: 0.76rem;
  color: color-mix(in oklab, var(--color-glow-500) 88%, transparent);
  border-bottom: 1px dashed color-mix(in oklab, var(--color-glow-600) 45%, transparent);
  transition: color 0.3s var(--ease-out-soft);
}

.field__toggle:hover {
  color: var(--color-glow-300);
}

/* ---------------- 时间：日期 + 时刻 ---------------- */
.time-inputs {
  display: flex;
  gap: 0.55rem;
}

.paper-input--time {
  min-width: 0;
  /* 各家浏览器里日期框的字高矮不一，定个高度让两格对齐 */
  height: 3.05rem;
  padding-top: 0;
  padding-bottom: 0;
  font-variant-numeric: tabular-nums;
}

.time-inputs .paper-input--time:first-child {
  flex: 1 1 auto;
}

.time-inputs .paper-input--time:last-child {
  flex: 0 0 8.4rem;
}

.time-quick {
  display: flex;
  gap: 0.4rem;
  margin-top: 0.6rem;
}

.time-quick__chip {
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 52%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 74%, transparent);
  padding: 0.3rem 0.78rem;
  font-size: 0.78rem;
  letter-spacing: 0.04em;
  color: color-mix(in oklab, var(--color-ink-200) 94%, transparent);
  transition:
    border-color 0.3s var(--ease-out-soft),
    color 0.3s var(--ease-out-soft);
}

.time-quick__chip:hover,
.time-quick__chip:focus-visible {
  border-color: color-mix(in oklab, var(--color-glow-500) 62%, transparent);
  color: var(--color-glow-300);
}

.prose-input {
  resize: vertical;
  min-height: 8rem;
  line-height: 1.9;
}

/* ---------------- 拖放区 ---------------- */
.dropzone {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.6rem;
  padding: 2.2rem 1rem;
  border-radius: var(--radius-lg);
  border: 1px dashed color-mix(in oklab, var(--color-ink-500) 58%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 72%, transparent);
  color: color-mix(in oklab, var(--color-ink-300) 94%, transparent);
  transition:
    border-color 0.4s var(--ease-out-soft),
    background-color 0.4s var(--ease-out-soft),
    color 0.4s var(--ease-out-soft);
}

.dropzone:hover,
.dropzone.is-active {
  border-color: color-mix(in oklab, var(--color-glow-500) 72%, transparent);
  background: color-mix(in oklab, var(--color-glow-700) 46%, transparent);
  color: var(--color-glow-300);
}

.dropzone__text {
  font-size: 0.84rem;
}

/* ---------------- 缩略图 ---------------- */
.thumbs {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(7.2rem, 1fr));
  gap: 0.7rem;
  margin: 0.4rem 0 0;
  padding: 0;
  list-style: none;
}

.thumb {
  position: relative;
  aspect-ratio: 4 / 3;
  border-radius: var(--radius-md);
  overflow: hidden;
  padding: 3px;
  border: 1px solid color-mix(in oklab, oklch(1 0 0) 72%, var(--color-ink-700));
  background: oklch(1 0 0);
  box-shadow: 0 10px 22px -16px color-mix(in oklab, var(--color-shade) 70%, transparent);
  /* 触屏排序（长按拖动）：长按不许弹原生菜单、不许选中文字；
     竖着划仍然滚页面，长按起拖之后由 JS 拦住滚动 */
  -webkit-touch-callout: none;
  -webkit-user-select: none;
  user-select: none;
  touch-action: pan-y;
}

.thumb__pic,
.thumb video {
  width: 100%;
  height: 100%;
  display: block;
  border-radius: calc(var(--radius-md) - 4px);
}

/* 预览是 CSS 背景（不是 <img>），顺手把唯一的那个媒体元素也排除在事件之外：
   原生菜单与原生拖拽都以它为目标，指针事件关了，事件就落在格子上。 */
.thumb__pic {
  background-position: center;
  background-size: cover;
  background-repeat: no-repeat;
}

.thumb video {
  object-fit: cover;
  pointer-events: none;
  -webkit-user-drag: none;
}

.thumb__kind {
  position: absolute;
  left: 0.5rem;
  bottom: 0.5rem;
  font-size: 0.66rem;
  padding: 0.12rem 0.42rem;
  border-radius: 999px;
  background: color-mix(in oklab, oklch(1 0 0) 84%, transparent);
  border: 1px solid color-mix(in oklab, var(--color-ink-700) 45%, transparent);
  color: var(--color-ink-100);
  backdrop-filter: blur(6px);
}

.thumb__mask {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  font-size: 0.78rem;
  color: var(--color-ink-50);
  background: color-mix(in oklab, oklch(1 0 0) 78%, transparent);
  backdrop-filter: blur(3px);
}

.thumb__mask--error {
  color: var(--color-ember-400);
}

.thumb__remove {
  position: absolute;
  top: 0.45rem;
  right: 0.45rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.35rem;
  height: 1.35rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-700) 48%, transparent);
  background: color-mix(in oklab, oklch(1 0 0) 88%, transparent);
  color: var(--color-ink-100);
  backdrop-filter: blur(6px);
  transition: color 0.3s var(--ease-out-soft), background-color 0.3s var(--ease-out-soft);
}

.thumb__remove:hover {
  color: var(--color-ember-400);
  background: oklch(1 0 0);
}

.thumb__remove--inline {
  position: static;
}

.thumbs__hint {
  margin: 0;
  font-size: 0.72rem;
  color: color-mix(in oklab, var(--color-ink-500) 94%, transparent);
}

/* 拖动排序 */
.thumb {
  cursor: grab;
}

.thumb:active {
  cursor: grabbing;
}

.thumb.is-dragging {
  opacity: 0.35;
}

.thumb.is-over {
  border-color: color-mix(in oklab, var(--color-glow-500) 85%, oklch(1 0 0));
  box-shadow: 0 0 0 3px color-mix(in oklab, var(--color-glow-500) 32%, transparent);
}

/* ---------------- 背景音乐 ---------------- */
/* 氛围音：一排胶囊，点一下就选中并试听 */
.mood-pick {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.mood-chip {
  padding: 0.42rem 0.85rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 58%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 78%, transparent);
  color: color-mix(in oklab, var(--color-ink-300) 94%, transparent);
  font-size: 0.82rem;
  letter-spacing: 0.02em;
  transition:
    border-color 0.35s var(--ease-out-soft),
    background-color 0.35s var(--ease-out-soft),
    color 0.35s var(--ease-out-soft);
}

.mood-chip:hover {
  border-color: color-mix(in oklab, var(--color-glow-500) 62%, transparent);
  color: var(--color-glow-300);
}

.mood-chip.is-on {
  border-color: color-mix(in oklab, var(--color-glow-500) 78%, transparent);
  background: color-mix(in oklab, var(--color-glow-700) 52%, transparent);
  color: var(--color-glow-300);
}

/* 正在响的那一枚轻轻呼吸一下，好和"只是选中"分开 */
.mood-chip.is-sounding {
  animation: mood-breath 2.4s var(--ease-out-soft) infinite;
}

@keyframes mood-breath {
  0%,
  100% {
    box-shadow: 0 0 0 0 color-mix(in oklab, var(--color-glow-500) 34%, transparent);
  }
  50% {
    box-shadow: 0 0 0 0.34rem transparent;
  }
}

.mood-note {
  margin: 0.55rem 0 0.7rem;
}

.music-add {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.6rem;
  padding: 1.15rem;
  border-radius: var(--radius-md);
  border: 1px dashed color-mix(in oklab, var(--color-ink-500) 58%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 72%, transparent);
  color: color-mix(in oklab, var(--color-ink-300) 94%, transparent);
  font-size: 0.86rem;
  transition:
    border-color 0.4s var(--ease-out-soft),
    background-color 0.4s var(--ease-out-soft),
    color 0.4s var(--ease-out-soft);
}

.music-add:hover {
  border-color: color-mix(in oklab, var(--color-glow-500) 72%, transparent);
  background: color-mix(in oklab, var(--color-glow-700) 46%, transparent);
  color: var(--color-glow-300);
}

.music-card {
  display: flex;
  align-items: center;
  gap: 0.8rem;
  padding: 0.7rem 0.8rem;
  border-radius: var(--radius-md);
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 58%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 90%, transparent);
  box-shadow: 0 10px 26px -18px color-mix(in oklab, var(--color-shade) 78%, transparent);
}

.music-card__play {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.4rem;
  height: 2.4rem;
  flex: none;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-glow-500) 55%, transparent);
  background: color-mix(in oklab, var(--color-glow-700) 62%, transparent);
  color: var(--color-glow-300);
  transition:
    transform 0.25s var(--ease-out-soft),
    background-color 0.35s var(--ease-out-soft);
}

.music-card__play:hover {
  background: color-mix(in oklab, var(--color-glow-600) 52%, transparent);
}

.music-card__play:active {
  transform: scale(0.92);
}

.music-card__body {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
  min-width: 0;
  flex: 1;
}

.music-card__title {
  font-size: 0.88rem;
  color: var(--color-ink-100);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.music-card__meta {
  font-size: 0.74rem;
  color: color-mix(in oklab, var(--color-ink-400) 94%, transparent);
}

.music-card__remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.8rem;
  height: 1.8rem;
  flex: none;
  border-radius: 999px;
  border: 1px solid transparent;
  background: none;
  color: color-mix(in oklab, var(--color-ink-400) 90%, transparent);
  transition:
    color 0.3s var(--ease-out-soft),
    background-color 0.3s var(--ease-out-soft);
}

.music-card__remove:hover {
  color: var(--color-ember-400);
  background: color-mix(in oklab, var(--color-ember-500) 10%, transparent);
}

/* ---------------- 标签 ---------------- */
.tag-box {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.45rem;
  padding: 0.55rem 0.7rem;
  border-radius: var(--radius-md);
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 62%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 90%, transparent);
  transition: border-color 0.35s var(--ease-out-soft), box-shadow 0.35s var(--ease-out-soft);
}

.tag-box:focus-within {
  border-color: color-mix(in oklab, var(--color-glow-500) 72%, transparent);
  box-shadow: 0 0 0 4px color-mix(in oklab, var(--color-glow-600) 22%, transparent);
}

.tag-box .tag-pill button {
  display: inline-flex;
  border: none;
  background: none;
  padding: 0;
  color: inherit;
  opacity: 0.7;
}

.tag-box .tag-pill button:hover {
  opacity: 1;
}

.tag-box__input {
  flex: 1;
  min-width: 8rem;
  border: none;
  background: none;
  outline: none;
  padding: 0.3rem 0.2rem;
  font-size: 0.85rem;
}

.suggestions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}

/* ---------------- 提交 ---------------- */
/* 提交始终停在手边，不用往下找 */
.actions {
  position: sticky;
  bottom: 0;
  z-index: 5;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.7rem;
  margin-top: 0.4rem;
  padding: 1.3rem 0 max(1.1rem, calc(var(--safe-bottom) + 0.6rem));
  background: linear-gradient(
    to top,
    oklch(0.974 0.011 86) 0%,
    oklch(0.974 0.011 86 / 0.94) 58%,
    transparent 100%
  );
}

.actions__hint {
  margin: 0;
  font-size: 0.76rem;
  color: color-mix(in oklab, var(--color-ink-500) 88%, transparent);
}

.drop-veil {
  position: fixed;
  inset: 0;
  z-index: 40;
  display: grid;
  place-items: center;
  background: color-mix(in oklab, var(--color-ink-950) 82%, transparent);
  backdrop-filter: blur(6px);
  font-family: var(--font-serif);
  font-size: 1.1rem;
  letter-spacing: 0.24em;
  color: var(--color-glow-400);
  pointer-events: none;
}

@media (max-width: 760px) {
  .sheet {
    gap: 1.7rem;
  }
  .topbar__hint {
    display: none;
  }
  .thumbs {
    grid-template-columns: repeat(auto-fill, minmax(6rem, 1fr));
  }
}
</style>
