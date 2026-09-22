<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'

import type { MediaItem } from '@/api/types'

/**
 * 一段记忆的画面：图片拼成相册的一页，视频各自成行（相册在上，视频在下）。
 *
 * 抽成组件是因为它有两个使用者——站内的记忆页、以及别人打开的分享页。
 * 以前分享页自己写了一套：逐张竖排、用原图、没有懒加载，同一个链接在手机上
 * 打开，排版和站内对不上，流量还重得多。现在两边同源。
 */
const props = withDefaults(
  defineProps<{
    media: MediaItem[]
    title?: string
    /** 能不能点开看大图。分享页是只读的，不开灯箱 */
    interactive?: boolean
  }>(),
  { title: '', interactive: false },
)

const emit = defineEmits<{ select: [id: string] }>()

/** 相册最多铺这么多张，多出来的折进「+N」 */
const COLLAGE_MAX = 7

const images = computed(() => props.media.filter((item) => item.kind === 'image'))
const videos = computed(() => props.media.filter((item) => item.kind === 'video'))
const collage = computed(() => images.value.slice(0, COLLAGE_MAX))
const hiddenCount = computed(() => Math.max(0, images.value.length - COLLAGE_MAX))

function pick(id: string) {
  if (props.interactive) emit('select', id)
}

/** 哪些视频已经点开过了。没点开的只画封面，不碰原文件 */
const started = ref(new Set<string>())

function startVideo(id: string) {
  started.value.add(id)
}

/** 页面上挂着的视频元素：离开这一页时要把它们的下载断掉 */
const videoEls = new Map<string, HTMLVideoElement>()

function rememberVideo(id: string, el: unknown) {
  if (el instanceof HTMLVideoElement) videoEls.set(id, el)
  else videoEls.delete(id)
}

onBeforeUnmount(() => {
  // 只 pause() 不够：元素从 DOM 拿掉之后那条连接未必松开，清掉 src 再 load()
  // 浏览器才会真的收手——否则退出记忆页之后它还在后台下
  for (const el of videoEls.values()) {
    el.pause()
    el.removeAttribute('src')
    el.load()
  }
  videoEls.clear()
})
</script>

<template>
  <div v-if="media.length" class="sheet__media">
    <!-- 图片拼贴成一页相册 -->
    <div v-if="collage.length" class="collage" :data-count="collage.length">
      <component
        :is="interactive ? 'button' : 'div'"
        v-for="(item, index) in collage"
        :key="item.id"
        class="collage__item"
        :class="{ 'is-tappable': interactive }"
        :type="interactive ? 'button' : undefined"
        :aria-label="interactive ? `查看第 ${index + 1} 张画面` : undefined"
        @click="pick(item.id)"
      >
        <img
          :src="item.thumbUrl ?? item.url"
          :alt="title || '记忆中的画面'"
          loading="lazy"
          decoding="async"
        />
        <span v-if="index === collage.length - 1 && hiddenCount > 0" class="collage__more">
          +{{ hiddenCount }}
        </span>
      </component>
    </div>

    <!-- 视频各自成行。这里以前一上来就把原文件挂上（preload=metadata）——
         可 4K 原片动辄几百 MB，手机上边下边解会把整页拖住，看着就是"打不开"
         （实测第 3 条那个 429MB 的 4K 视频：请求挂在那里、readyState 一直是 0、
         连封面都没有，只剩一个黑框）。
         现在先只画封面——用上传时生成的首帧缩略图——点了才真正去取视频。 -->
    <div v-for="item in videos" :key="item.id" class="video-frame">
      <button
        v-if="!started.has(item.id)"
        type="button"
        class="video-cover"
        :aria-label="`播放视频：${title || '记忆中的画面'}`"
        @click="startVideo(item.id)"
      >
        <img v-if="item.thumbUrl" :src="item.thumbUrl" alt="" loading="lazy" decoding="async" />
        <span class="video-cover__play" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path
              d="M8 5.14v13.72a1 1 0 0 0 1.54.84l10.29-6.86a1 1 0 0 0 0-1.68L9.54 4.3A1 1 0 0 0 8 5.14Z"
            />
          </svg>
        </span>
      </button>
      <video
        v-else
        :ref="(el) => rememberVideo(item.id, el)"
        class="video-player"
        :src="item.url"
        controls
        autoplay
        playsinline
      />
    </div>
  </div>
</template>

<style scoped>
.sheet__media {
  display: flex;
  flex-direction: column;
  gap: 1.2rem;
  margin-top: 1.9rem;
}

/* 图片：拼贴成相册的一页 */
.collage {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px;
  padding: 6px;
  border-radius: 14px;
  border: 1px solid color-mix(in oklab, oklch(1 0 0) 80%, var(--color-ink-700));
  background: oklch(1 0 0);
  box-shadow:
    0 2px 4px -2px color-mix(in oklab, var(--color-shade) 16%, transparent),
    0 22px 44px -28px color-mix(in oklab, var(--color-shade) 50%, transparent);
}

.collage[data-count='1'] {
  grid-template-columns: minmax(0, 1fr);
}

/* 三张、五张、六张、七张时，第一张横跨整行 */
.collage[data-count='3'] > :first-child,
.collage[data-count='5'] > :first-child,
.collage[data-count='6'] > :first-child,
.collage[data-count='7'] > :first-child {
  grid-column: 1 / -1;
}

.collage__item {
  position: relative;
  display: block;
  padding: 0;
  border: none;
  border-radius: 9px;
  overflow: hidden;
  aspect-ratio: 1 / 1;
  background: color-mix(in oklab, var(--color-ink-850) 92%, transparent);
}

/* 只有能点开看大图的那一侧，指针才是放大镜 */
.collage__item.is-tappable {
  cursor: zoom-in;
}

.collage[data-count='1'] .collage__item {
  aspect-ratio: auto;
}

.collage__item img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  transition: transform 0.7s var(--ease-out-soft);
}

.collage[data-count='1'] .collage__item img {
  height: auto;
  max-height: 74dvh;
  object-fit: contain;
}

.collage__item.is-tappable:hover img {
  transform: scale(1.035);
}

.collage__more {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  font-family: var(--font-serif);
  font-size: 1.5rem;
  font-weight: 600;
  color: oklch(1 0 0);
  background: color-mix(in oklab, oklch(0.24 0.026 50) 54%, transparent);
  backdrop-filter: blur(2px);
}

/* 视频：整行播放 */
/* 视频：外层是那张白卡；里面要么是封面（点了才去取视频），要么是播放器 */
.video-frame {
  width: 100%;
  padding: 5px;
  border-radius: 14px;
  border: 1px solid color-mix(in oklab, oklch(1 0 0) 80%, var(--color-ink-700));
  background: oklch(1 0 0);
  box-shadow:
    0 2px 4px -2px color-mix(in oklab, var(--color-shade) 16%, transparent),
    0 22px 44px -28px color-mix(in oklab, var(--color-shade) 50%, transparent);
}

.video-player,
.video-cover {
  display: block;
  width: 100%;
  max-height: 76dvh;
  border-radius: 10px;
  object-fit: cover;
}

.video-cover {
  position: relative;
  padding: 0;
  border: 0;
  overflow: hidden;
  background: oklch(0.24 0.01 70deg);
  cursor: pointer;
}

.video-cover img {
  display: block;
  width: 100%;
  max-height: 76dvh;
  object-fit: cover;
}

.video-cover__play {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  color: oklch(0.98 0.01 90deg);
}

.video-cover__play svg {
  width: 3.4rem;
  height: 3.4rem;
  padding: 0.85rem;
  border-radius: 999px;
  background: oklch(0 0 0 / 0.42);
}
</style>
