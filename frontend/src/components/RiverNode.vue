<script setup lang="ts">
import { computed } from 'vue'

import type { RiverNode } from '@/composables/useRiverLayout'
import { formatDotted } from '@/utils/format'
import { tagHue } from '@/utils/tagColor'

const props = defineProps<{
  node: RiverNode
  /** 所属那一段光河的左边界：卡片按段内坐标摆（段自己再整体平移） */
  origin?: number
}>()

const emit = defineEmits<{ open: [id: string] }>()

const moment = computed(() => props.node.moment)

const coverUrl = computed(
  () => moment.value.cover?.thumbUrl ?? moment.value.cover?.url ?? null,
)

const displayTitle = computed(
  () => moment.value.title.trim() || moment.value.excerpt.trim() || '未命名的这一刻',
)

const extraCount = computed(() => Math.max(0, moment.value.mediaCount - 1))
</script>

<template>
  <div
    class="river-node"
    :class="{ 'is-text': node.kind === 'text' }"
    :style="{
      left: `${node.x - (origin ?? 0)}px`,
      width: `${node.width}px`,
      '--lift': `${node.lift}px`,
      '--node-height': `${node.height}px`,
      '--tilt': `${node.tilt}deg`,
    }"
  >
    <button
      type="button"
      class="node-media"
      :data-moment-id="moment.id"
      :aria-label="`打开记忆：${displayTitle}`"
      @click="emit('open', moment.id)"
    >
      <span class="node-frame">
        <img
          v-if="coverUrl"
          :src="coverUrl"
          :alt="displayTitle"
          class="node-image"
          decoding="async"
          loading="lazy"
          draggable="false"
        />
        <span v-else class="node-text">
          <span class="node-text-title">{{ displayTitle }}</span>
          <span v-if="moment.excerpt && moment.title" class="node-text-body">
            {{ moment.excerpt }}
          </span>
        </span>
      </span>

      <span v-if="node.kind === 'video'" class="node-badge node-badge--play" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="currentColor" class="h-3 w-3">
          <path d="M8 5.14v13.72a1 1 0 0 0 1.54.84l10.29-6.86a1 1 0 0 0 0-1.68L9.54 4.3A1 1 0 0 0 8 5.14Z" />
        </svg>
      </span>

      <span v-if="moment.hasMusic" class="node-badge node-badge--music" aria-label="有背景音乐">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" class="h-3 w-3">
          <path d="M9 18V6l10-2v12" stroke-linecap="round" stroke-linejoin="round" />
          <circle cx="6.5" cy="18" r="2.5" />
          <circle cx="16.5" cy="16" r="2.5" />
        </svg>
      </span>

      <span v-if="extraCount > 0" class="node-badge node-badge--count" aria-hidden="true">
        +{{ extraCount }}
      </span>

      <span class="node-reveal" aria-hidden="true">{{ displayTitle }}</span>
    </button>

    <span class="node-stem" aria-hidden="true" />

    <div class="node-caption">
      <span class="node-date">{{ formatDotted(moment.happenedAt) }}</span>
      <span
        v-if="moment.tags.length"
        class="node-tags"
        :style="{ '--tag-hue': tagHue(moment.tags[0] ?? '') }"
      >
        {{ moment.tags.slice(0, 2).join(' · ') }}
      </span>
    </div>
  </div>
</template>

<style scoped>
.river-node {
  position: absolute;
  top: var(--river-y, 58%);
  height: 0;
  pointer-events: none;
}

/* 这里刻意不提层：相机位移由视口的滚动偏移承担（见 HomeView 的 paintCamera），
   内容不需要自成一个图层。逐张提层反而会随着镜头对焦切换不断新建纹理，
   既吃显存、也能被眼睛察觉出一下一下的顿。 */

/* ---------- 贴在光河上的照片 ---------- */
.node-media {
  position: absolute;
  left: 0;
  bottom: var(--lift);
  display: block;
  width: 100%;
  padding: 4px;
  border-radius: 12px;
  border: 1px solid color-mix(in oklab, oklch(1 0 0) 78%, var(--color-ink-700));
  background: oklch(1 0 0);
  /* 最外面那层原先模糊半径开到 56px。模糊的代价正比于半径×元素面积，
     而每换一次对焦、每滑过一张卡片都要重画一次——手机上就是那一两帧。
     把半径收小、用偏移把影子摊开，看着还是同一片影子，便宜得多。 */
  /* 三层收到两层：每多一层模糊，重画这张卡片时就要多算一遍。
     影子看着还是同一片，代价小一半。 */
  box-shadow:
    0 1px 2px color-mix(in oklab, var(--color-shade) 16%, transparent),
    0 16px 22px -14px color-mix(in oklab, var(--color-shade) 58%, transparent);
  /* 这里原有一层 -webkit-box-reflect（纸面上的水光）。它让每张卡片多渲染一遍，
     而 alpha 只有 0.05、17% 处就淡没了——留着不划算，去掉。 */
  /* 这里刻意不提层。整条光河是几个分段各自成层，卡片再提层就成了"合成层里套
     合成层"——合成器为了不重采样纹理，会把位移吸附到整像素，慢速的自动流动
     （0.73px/帧）就变成"停一帧、跳一像素"，整条河看着一帧一帧地挪。
     对焦点亮整个去掉了，滑到中间时不再有任何重画。 */
  pointer-events: auto;
  /* 一点点倾斜，像随手贴上去的照片；悬停时会自己摆正 */
  transform: rotate(var(--tilt, 0deg));
  /* 只过渡 transform（跑在合成层上，很便宜）。box-shadow 与 border-color 的
     过渡看着轻，其实每帧都要重画这张卡片——而镜头一直在缓缓流动、对焦每隔
     几秒就换一张，等于在持续重画（实测 fling 的 1.5 秒里 node-media 重绘
     55 次、node-caption 51 次、光栅 780ms）。高亮改成直接切换：静止时几乎
     看不出差别，动起来顺畅得多。 */
  transition: transform 0.6s var(--ease-out-soft);
}

/* 这里本来有「对焦点亮」：卡片滑到中间时描边发亮、影子变暖。
   它是整条河上最贵的一处——点亮意味着卡片那块区域要重画，而卡片底下压着模糊阴影，
   重画一次手机上就是一两帧。试过两种更便宜的做法（只换颜色、只做 transform + 提层）：
   前者仍然要重画，后者把运动吸附成了整像素、整条河一帧一帧地跳。
   2026-09-21 按用户要求整个去掉——滑到中间时不再有任何变化。 */

.node-media:hover {
  transform: rotate(0deg) translate3d(0, -9px, 0) scale(1.032);
  z-index: 3;
  border-color: color-mix(in oklab, var(--color-glow-500) 70%, oklch(1 0 0));
  box-shadow:
    0 1px 1px color-mix(in oklab, var(--color-shade) 10%, transparent),
    0 20px 36px -18px color-mix(in oklab, var(--color-shade) 48%, transparent),
    0 40px 72px -34px color-mix(in oklab, var(--color-glow-500) 62%, transparent);
}

.node-media:active {
  transform: rotate(0deg) translate3d(0, -4px, 0) scale(1.012);
}

.node-frame {
  position: relative;
  display: block;
  width: 100%;
  height: var(--node-height);
  overflow: hidden;
  border-radius: 9px;
  background: color-mix(in oklab, var(--color-ink-850) 90%, transparent);
}

.node-image {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  filter: saturate(1.02) contrast(1.01);
}

/* ---------- 纯文字卡片 ---------- */
.node-text {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding: 1.05rem 1.05rem 1.15rem;
  height: 100%;
  background:
    radial-gradient(
      120% 80% at 88% 0%,
      color-mix(in oklab, var(--color-glow-700) 70%, transparent) 0%,
      transparent 64%
    ),
    linear-gradient(168deg, oklch(1 0 0), color-mix(in oklab, var(--color-ink-850) 92%, transparent));
}

.node-text-title {
  font-family: var(--font-serif);
  font-weight: 600;
  font-size: 0.98rem;
  line-height: 1.5;
  color: var(--color-ink-50);
  display: -webkit-box;
  -webkit-line-clamp: 3;
  line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.node-text-body {
  font-size: 0.78rem;
  line-height: 1.65;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
  display: -webkit-box;
  -webkit-line-clamp: 3;
  line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

/* ---------- 徽标 ---------- */
.node-badge {
  position: absolute;
  display: inline-flex;
  align-items: center;
  gap: 0.15rem;
  border-radius: 999px;
  font-size: 0.68rem;
  line-height: 1;
  color: var(--color-ink-100);
  background: color-mix(in oklab, oklch(1 0 0) 82%, transparent);
  border: 1px solid color-mix(in oklab, var(--color-ink-700) 45%, transparent);
  backdrop-filter: blur(8px);
  padding: 0.3rem;
  box-shadow: 0 2px 8px -4px color-mix(in oklab, var(--color-shade) 45%, transparent);
}

.node-badge--play {
  top: 0.75rem;
  left: 0.75rem;
  padding: 0.32rem 0.32rem 0.32rem 0.4rem;
}

.node-badge--music {
  bottom: 0.75rem;
  left: 0.75rem;
  color: var(--color-glow-300);
}

.node-badge--count {
  right: 0.75rem;
  bottom: 0.75rem;
  padding: 0.3rem 0.52rem;
  font-variant-numeric: tabular-nums;
}

/* ---------- 悬停时浮出的标题 ---------- */
.node-reveal {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  padding: 1.5rem 0.85rem 0.7rem;
  border-radius: 0 0 9px 9px;
  background: linear-gradient(
    to top,
    oklch(0.18 0.025 50 / 0.86),
    oklch(0.18 0.025 50 / 0.4) 55%,
    transparent
  );
  color: oklch(0.99 0.01 90);
  font-size: 0.8rem;
  line-height: 1.5;
  text-align: left;
  opacity: 0;
  transform: translate3d(0, 8px, 0);
  transition:
    opacity 0.45s var(--ease-out-soft),
    transform 0.45s var(--ease-out-soft);
  pointer-events: none;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.node-media:hover .node-reveal,
.node-media:focus-visible .node-reveal {
  opacity: 1;
  transform: none;
}

/* ---------- 光茎：把记忆挂到光河上 ---------- */
.node-stem {
  position: absolute;
  left: 50%;
  bottom: 0;
  width: 1px;
  height: var(--lift);
  transform: translateX(-0.5px);
  background: linear-gradient(
    to top,
    color-mix(in oklab, var(--color-glow-500) 78%, transparent),
    color-mix(in oklab, var(--color-glow-600) 20%, transparent)
  );
  opacity: 0.62;
  /* 不给它做过渡：对焦每隔几秒就换一张卡片，这条线会跟着淡入淡出——
     而它没有自己的合成层，每次都要重画（1.5 秒的 fling 里贡献了几十次）。
     直接切换：这是河边一条 2px 的细线，看不出差别。 */
}

/* 只留 hover：对焦不再改这条线的不透明度（同样是"滑到中间就重画"） */
.node-media:hover ~ .node-stem {
  opacity: 1;
}

/* ---------- 光河之下的时间刻度 ---------- */
.node-caption {
  position: absolute;
  left: 50%;
  top: 1.1rem;
  transform: translateX(-50%);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.15rem;
  white-space: nowrap;
  text-align: center;
}

.node-date {
  font-family: var(--font-display);
  font-size: 0.82rem;
  letter-spacing: 0.09em;
  color: color-mix(in oklab, var(--color-ink-400) 94%, transparent);
  /* 颜色没法交给合成器，做过渡就是每帧重画这行字——对焦一换就要重画半秒。
     直接切换。 */
}

.node-tags {
  font-size: 0.7rem;
  letter-spacing: 0.04em;
  color: oklch(0.5 0.1 var(--tag-hue, 58));
  max-width: 12rem;
  overflow: hidden;
  text-overflow: ellipsis;
}


@media (max-width: 760px) {
  .node-media {
    padding: 3px;
    border-radius: 10px;
    /* 小屏上倒影只留一点点，否则像贴在一块灰玻璃上 */
    -webkit-box-reflect: below 4px
      linear-gradient(to bottom, oklch(0 0 0 / 0.05) 0%, transparent 26%);
  }
  .node-text {
    padding: 0.85rem 0.85rem 0.95rem;
  }
  .node-text-title {
    font-size: 0.86rem;
  }
  .node-text-body {
    font-size: 0.72rem;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
  .node-date {
    font-size: 0.72rem;
  }
  .node-tags {
    display: none;
  }
}
</style>
