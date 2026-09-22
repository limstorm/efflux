<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import { ApiError, api } from '@/api/client'

const router = useRouter()

const code = ref('')
const submitting = ref(false)
const error = ref<string | null>(null)
const input = ref<HTMLInputElement | null>(null)

async function submit() {
  const value = code.value.trim()
  if (!value || submitting.value) return

  submitting.value = true
  error.value = null
  try {
    await api.login(value)
    void router.replace({ name: 'home' })
  } catch (err) {
    error.value = err instanceof ApiError ? err.message : '没能进入，请再试一次'
    code.value = ''
    input.value?.focus()
  } finally {
    submitting.value = false
  }
}

onMounted(async () => {
  // 这个部署如果本来就不需要口令，就别把人拦在门外
  try {
    const status = await api.fetchStatus()
    if (!status.protected) {
      void router.replace({ name: 'home' })
      return
    }
  } catch {
    /* 探测失败就照常显示登录页 */
  }
  input.value?.focus()
})
</script>

<template>
  <div class="login-view">
    <!-- 一句呼应：光河从门外流过 -->
    <div class="login-river" aria-hidden="true" />

    <main class="login-card">
      <h1 class="login-card__title title-serif">流光</h1>
      <p class="login-card__sub">这里只对家人开放</p>

      <form class="login-card__form" @submit.prevent="submit">
        <label class="sr-only" for="access-code">访问口令</label>
        <input
          id="access-code"
          ref="input"
          v-model="code"
          class="paper-input"
          type="password"
          name="password"
          autocomplete="current-password"
          placeholder="访问口令"
          :disabled="submitting"
        />
        <button
          type="submit"
          class="glow-btn login-card__submit"
          :disabled="!code.trim() || submitting"
        >
          {{ submitting ? '正在进入…' : '进入' }}
        </button>
      </form>

      <p v-if="error" class="login-card__error" role="alert">{{ error }}</p>

      <p class="login-card__hint">口令由部署这个站点的人设置</p>
    </main>
  </div>
</template>

<style scoped>
.login-view {
  position: relative;
  display: grid;
  place-items: center;
  min-height: 100vh;
  min-height: 100dvh;
  padding: 1.5rem;
  overflow: hidden;
}

/* 门外的光河 */
.login-river {
  position: absolute;
  left: 0;
  right: 0;
  top: 60vh;
  height: 2px;
  background: linear-gradient(
    to right,
    transparent 0%,
    color-mix(in oklab, var(--color-glow-600) 55%, transparent) 20%,
    color-mix(in oklab, var(--color-glow-500) 92%, transparent) 50%,
    color-mix(in oklab, var(--color-glow-600) 55%, transparent) 80%,
    transparent 100%
  );
  box-shadow:
    0 1px 0 color-mix(in oklab, oklch(1 0 0) 55%, transparent),
    0 4px 18px -3px color-mix(in oklab, var(--color-glow-500) 62%, transparent);
  opacity: 0.75;
  pointer-events: none;
}

.login-card {
  position: relative;
  z-index: 1;
  width: min(23rem, 92vw);
  padding: 2.6rem 2rem 2.2rem;
  border-radius: var(--radius-xl);
  border: 1px solid color-mix(in oklab, oklch(1 0 0) 78%, var(--color-ink-700));
  background: color-mix(in oklab, var(--color-ink-900) 92%, transparent);
  backdrop-filter: blur(18px);
  box-shadow:
    0 2px 4px -2px color-mix(in oklab, var(--color-shade) 16%, transparent),
    0 30px 60px -34px color-mix(in oklab, var(--color-shade) 62%, transparent);
  text-align: center;
  animation: rise-in 0.8s var(--ease-out-soft) both;
}

.login-card__title {
  margin: 0;
  font-size: 2rem;
  letter-spacing: 0.3em;
  text-indent: 0.3em;
  color: var(--color-ink-50);
}

.login-card__sub {
  margin: 0.6rem 0 0;
  font-size: 0.82rem;
  letter-spacing: 0.1em;
  color: color-mix(in oklab, var(--color-ink-400) 94%, transparent);
}

.login-card__form {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  margin-top: 1.9rem;
}

.login-card__form .paper-input {
  text-align: center;
  letter-spacing: 0.16em;
}

.login-card__submit {
  padding: 0.8rem 1.5rem;
  font-size: 0.92rem;
}

.login-card__error {
  margin: 0.9rem 0 0;
  font-size: 0.8rem;
  color: var(--color-ember-400);
}

.login-card__hint {
  margin: 1.5rem 0 0;
  font-size: 0.72rem;
  color: color-mix(in oklab, var(--color-ink-500) 92%, transparent);
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}
</style>
