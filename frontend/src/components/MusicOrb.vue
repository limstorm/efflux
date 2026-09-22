<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'

import { ambientName, isAmbientId, playAmbient, stopAmbient } from '@/audio/ambient'
import type { MediaItem } from '@/api/types'
import { useToast } from '@/composables/useToast'
import { formatDuration } from '@/utils/format'

/**
 * 右下角那枚播放圆：响着的时候音符自己在转，点一下停、再点一下接着转。
 *
 * 站内的记忆页与分享页共用，样式与站内图标（.hud-icon）同一套语言：
 * 平时是半透明的墨色玻璃，响起来才换成琥珀。
 *
 * 两种配乐都在这一个组件里：传上来的曲子走 <audio>，合成的氛围音走 Web Audio。
 * 曲子和氛围音只会有一个——创建页保证了这一点。
 */
const props = withDefaults(
  defineProps<{
    music: MediaItem | null
    ambient: string | null
    /** 打开页面就起播。分享页不开：陌生人点开链接不该被声音扑一脸 */
    autoplay?: boolean
    /** 演示进行中右下角那格让给了退出键，往上让一让 */
    raised?: boolean
  }>(),
  { autoplay: false, raised: false },
)

const toast = useToast()
const audioRef = ref<HTMLAudioElement | null>(null)
const playing = ref(false)
const currentTime = ref(0)
const duration = ref(0)

const ambientId = computed(() => (isAmbientId(props.ambient) ? props.ambient : null))

/** 有没有东西可放。空格键要先问一声，免得白吃一次 preventDefault */
const ready = computed(() => Boolean(props.music || ambientId.value))

const label = computed(() => {
  if (props.music) return '背景音乐'
  const name = ambientName(ambientId.value)
  return name ? `氛围音 · ${name}` : null
})

/** 悬停那一句：有曲子就顺带报一下放到哪了——圆上没有别的地方能显示进度 */
const tip = computed(() => {
  if (!label.value) return ''
  const action = playing.value ? '点击暂停' : '点击播放'
  if (props.music && duration.value > 0) {
    const now = formatDuration(currentTime.value * 1000)
    const total = formatDuration(duration.value * 1000)
    return `${label.value} ${now} / ${total} · ${action}`
  }
  return `${label.value} · ${action}`
})

async function toggle() {
  // 氛围音没有 <audio> 可以点，起停都交给合成器，界面跟着它的结果走
  const id = ambientId.value
  if (id) {
    if (playing.value) {
      stopAmbient()
      playing.value = false
    } else {
      playing.value = await playAmbient(id)
      if (!playing.value) toast.info('这台设备现在放不出声音，碰一下屏幕再试')
    }
    return
  }

  const audio = audioRef.value
  if (!audio) return
  if (audio.paused) {
    void audio.play().catch(() => toast.error('这段音乐暂时无法播放'))
  } else {
    audio.pause()
  }
}

function pause() {
  audioRef.value?.pause()
  stopAmbient()
  playing.value = false
}

async function start() {
  const id = ambientId.value
  if (id) {
    // 浏览器要用户手势才肯出声：没响就把按钮留成「播放」
    playing.value = await playAmbient(id)
    return
  }
  const audio = audioRef.value
  if (!audio) return
  duration.value = Number.isFinite(audio.duration) ? audio.duration : 0
  try {
    await audio.play()
    playing.value = true
  } catch {
    playing.value = false
  }
}

onMounted(() => {
  if (props.autoplay) void start()
})

// 换了一段记忆：上一段的先收掉；这一页负责自动播的，再把新的接上
watch(
  () => [props.music?.id ?? null, ambientId.value] as const,
  () => {
    pause()
    if (props.autoplay) void start()
  },
)

onBeforeUnmount(() => {
  pause()
  // 光停下播放不够：曲子还在下的话，把 src 清掉浏览器才会松开那条连接
  const audio = audioRef.value
  if (audio) {
    audio.removeAttribute('src')
    audio.load()
  }
})

defineExpose({ toggle, pause, ready })
</script>

<template>
  <button
    v-if="label"
    type="button"
    class="music-orb tip"
    :class="{ 'is-playing': playing, 'is-demo': raised }"
    :data-tip="tip"
    :aria-label="`${playing ? '暂停' : '播放'}${label}`"
    @click="toggle"
  >
    <!-- 实心符头的 ♫：圆只有 2.5rem，细线画的音符在这个尺寸里会糊成一团 -->
    <svg class="music-orb__note" viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M9.2 16.9V6.2l9.6-1.9v10.5"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <ellipse cx="6.5" cy="17.4" rx="2.9" ry="2.3" fill="currentColor" />
      <ellipse cx="16.1" cy="15.6" rx="2.9" ry="2.3" fill="currentColor" />
    </svg>
  </button>

  <!-- 放在按钮外面：<button> 里不该塞交互内容，何况它本来就不显示 -->
  <audio
    v-if="music"
    ref="audioRef"
    :src="music.url"
    preload="metadata"
    @play="playing = true"
    @pause="playing = false"
    @timeupdate="currentTime = ($event.target as HTMLAudioElement).currentTime"
    @loadedmetadata="duration = ($event.target as HTMLAudioElement).duration || 0"
    @ended="playing = false"
  />
</template>

<style scoped>
/* 右下角一枚圆，说的是页面自己的语言（见 .hud-icon）：
   平时是半透明的墨色玻璃、一层很轻的阴影，安静地待在角落；
   响起来才亮成琥珀色——全页只有这一处在亮，音符转着。 */
.music-orb {
  position: fixed;
  right: max(clamp(1rem, 3vw, 2.4rem), calc(var(--safe-right) + 0.6rem));
  bottom: max(1.4rem, calc(var(--safe-bottom) + 0.8rem));
  z-index: 20;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  /* 与全站的图标（2.5rem）一档：再大就压过顶栏那排按钮了 */
  width: 2.5rem;
  height: 2.5rem;
  padding: 0;
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
    background-color 0.4s var(--ease-out-soft),
    box-shadow 0.4s var(--ease-out-soft),
    transform 0.25s var(--ease-out-soft);
}

/* 响着的时候，和「记下一段流光」那枚加号一样是琥珀的 */
.music-orb.is-playing {
  opacity: 0.74;
  color: var(--color-glow-300);
  border-color: color-mix(in oklab, var(--color-glow-500) 52%, transparent);
  background: color-mix(in oklab, var(--color-glow-700) 46%, var(--color-ink-900));
}

.music-orb:hover {
  opacity: 1;
  color: var(--color-glow-300);
  border-color: color-mix(in oklab, var(--color-glow-500) 62%, transparent);
}

.music-orb.is-playing:hover {
  color: var(--color-glow-400);
  box-shadow: 0 10px 24px -14px color-mix(in oklab, var(--color-glow-500) 90%, transparent);
}

.music-orb:active {
  transform: scale(0.94);
}

/* 演示时右下角那一格让给了「退出演示」的叉号，这枚圆往上让一让 */
.music-orb.is-demo {
  bottom: max(7.4rem, calc(var(--safe-bottom) + 6.8rem));
}

.music-orb__note {
  /* 与顶栏图标同比例：1.25rem 的图标放在 2.5rem 的圆里 */
  width: 1.25rem;
  height: 1.25rem;
  transform-origin: 50% 50%;
  /* 一直挂着这条动画，暂停时冻在原地，不会"啪"一下弹回正位 */
  animation: orb-spin 6.5s linear infinite;
  animation-play-state: paused;
}

.music-orb.is-playing .music-orb__note {
  animation-play-state: running;
}

@keyframes orb-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
