<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import { ApiError, api } from '@/api/client'
import type { BackupFile, RestoreReport } from '@/api/types'
import { useTheme } from '@/composables/useTheme'
import { useToast } from '@/composables/useToast'

const router = useRouter()
const toast = useToast()
const { themeId, themes, setTheme, syncMeta } = useTheme()

const loading = ref(true)
const protectedMode = ref(false)
const sessionDays = ref(30)

const currentCode = ref('')
const newCode = ref('')
const confirmCode = ref('')
const saving = ref(false)
const loggingOut = ref(false)

// ---------------------------------------------------------------- 备份
const backups = ref<BackupFile[]>([])
const exporting = ref(false)
const importing = ref(false)
const lastReport = ref<RestoreReport | null>(null)
const archiveInput = ref<HTMLInputElement | null>(null)

const backupNote = computed(() =>
  backups.value.length ? `已存 ${backups.value.length} 份` : '还没有备份',
)

const intervalDays = ref(7)
const savingInterval = ref(false)
const backupDir = ref('')
const defaultDir = ref('')
const savingDir = ref(false)

const dirIsDefault = computed(() => backupDir.value === defaultDir.value)

/** 几个够用的档位：太密没必要，太疏心里没底 */
const intervalOptions: { days: number; label: string }[] = [
  { days: 0, label: '关闭' },
  { days: 1, label: '每天' },
  { days: 3, label: '每 3 天' },
  { days: 7, label: '每周' },
  { days: 14, label: '每两周' },
  { days: 30, label: '每月' },
]

async function loadInterval() {
  try {
    const settings = await api.backupSettings()
    intervalDays.value = settings.intervalDays
    backupDir.value = settings.dir
    defaultDir.value = settings.defaultDir
  } catch {
    // 拿不到就用默认值，不耽误别的设置项
  }
}

/** 换目录。后端会把已有的包一起搬过去——不搬的话，恢复链就断在新地方了 */
async function saveDir() {
  if (savingDir.value) return
  savingDir.value = true
  try {
    const settings = await api.saveBackupSettings(intervalDays.value, backupDir.value)
    backupDir.value = settings.dir
    defaultDir.value = settings.defaultDir
    toast.success(
      settings.dir === settings.defaultDir ? '已经回到默认位置' : '备份目录换过去了',
    )
  } catch (err) {
    toast.error(err instanceof ApiError ? err.message : '没能改掉')
  } finally {
    savingDir.value = false
  }
}

async function changeInterval(days: number) {
  if (savingInterval.value || days === intervalDays.value) return
  savingInterval.value = true
  try {
    const saved = await api.saveBackupSettings(days)
    intervalDays.value = saved.intervalDays
    toast.success(
      saved.intervalDays === 0 ? '自动备份已经关掉' : `改成每 ${saved.intervalDays} 天备份一次`,
    )
  } catch (err) {
    toast.error(err instanceof ApiError ? err.message : '没能改掉')
  } finally {
    savingInterval.value = false
  }
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`
}

async function loadBackups() {
  try {
    backups.value = await api.listBackups()
  } catch {
    // 列表拿不到不耽误别的设置项
  }
}

async function exportAll() {
  if (exporting.value) return
  exporting.value = true
  try {
    const file = await api.createBackup()
    toast.success(`打包好了，${formatSize(file.sizeBytes)}`)
    await loadBackups()
    // 打完顺手就让人拿到手
    window.location.href = `/api/backup/${file.name}`
  } catch (err) {
    toast.error(err instanceof ApiError ? err.message : '打包失败了')
  } finally {
    exporting.value = false
  }
}

function pickArchive() {
  archiveInput.value?.click()
}

async function onArchivePicked(event: Event) {
  const input = event.target as HTMLInputElement
  const files = [...(input.files ?? [])]
  input.value = ''
  if (!files.length) return

  importing.value = true
  try {
    const report = await api.restoreBackup(files)
    lastReport.value = report
    if (report.failed.length) {
      toast.info(`${report.failed.length} 个包没吃下，别的都好了`)
    } else {
      toast.success(`吃进来 ${report.momentsAdded} 段记忆`)
    }
    await loadBackups()
  } catch (err) {
    toast.error(err instanceof ApiError ? err.message : '导入失败了')
  } finally {
    importing.value = false
  }
}

void loadBackups()
void loadInterval()

const tooShort = computed(() => newCode.value.length > 0 && newCode.value.trim().length < 6)
const mismatched = computed(
  () => confirmCode.value.length > 0 && newCode.value !== confirmCode.value,
)
const canSave = computed(
  () =>
    !saving.value &&
    currentCode.value.trim().length > 0 &&
    newCode.value.trim().length >= 6 &&
    newCode.value === confirmCode.value,
)

async function load() {
  loading.value = true
  try {
    const session = await api.fetchSession()
    protectedMode.value = session.protected
    sessionDays.value = session.sessionDays
  } catch (err) {
    if (err instanceof ApiError && err.status === 401) {
      void router.replace({ name: 'login' })
    }
  } finally {
    loading.value = false
  }
}

async function save() {
  if (!canSave.value) return
  saving.value = true
  try {
    await api.changeCode(currentCode.value.trim(), newCode.value.trim())
    currentCode.value = ''
    newCode.value = ''
    confirmCode.value = ''
    toast.success('口令已更新，其他设备需要重新输一次')
  } catch (err) {
    toast.error(err instanceof ApiError ? err.message : '没能改成功，再试一次')
  } finally {
    saving.value = false
  }
}

async function logout() {
  if (loggingOut.value) return
  loggingOut.value = true
  try {
    await api.logout()
  } catch {
    /* 无论成功与否都回登录页 */
  }
  window.location.replace('/login')
}

function goBack() {
  const state = window.history.state as { back?: string | null } | null
  if (state?.back) router.back()
  else void router.push({ name: 'home' })
}

onMounted(() => {
  syncMeta()
  void load()
})
</script>

<template>
  <div class="settings-view">
    <header class="topbar">
      <button type="button" class="hud-icon" aria-label="返回" title="返回（Esc）" @click="goBack">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" class="h-5 w-5">
          <path d="M14.5 5.5 8 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <span class="topbar__spacer" />
    </header>

    <main class="sheet">
      <h1 class="sheet__heading title-serif">设置</h1>

      <section class="field">
        <div class="field__head">
          <span class="field__label">主题</span>
          <span class="field__note">换一种纸与光</span>
        </div>

        <p class="field__hint">只改这台设备的观感，不会影响别处看到的画面。</p>

        <div class="themes" role="radiogroup" aria-label="纸面主题">
          <button
            v-for="theme in themes"
            :key="theme.id"
            type="button"
            class="theme-card"
            :class="{ 'is-active': theme.id === themeId }"
            :data-preview="theme.id"
            role="radio"
            :aria-checked="theme.id === themeId"
            @click="setTheme(theme.id)"
          >
            <span class="theme-card__paper" aria-hidden="true">
              <span class="theme-card__glow" />
              <span class="theme-card__dot" />
            </span>
            <span class="theme-card__text">
              <span class="theme-card__name">
                {{ theme.name }}
                <span v-if="theme.id === themeId" class="theme-card__tick" aria-hidden="true" />
              </span>
              <span class="theme-card__note">{{ theme.note }}</span>
            </span>
          </button>
        </div>
      </section>

      <section class="field">
        <div class="field__head">
          <span class="field__label">访问口令</span>
          <span class="field__note">
            {{ protectedMode ? `登录状态保持 ${sessionDays} 天` : '当前未开启' }}
          </span>
        </div>

        <p v-if="!protectedMode" class="field__hint">
          这个部署没有设置访问口令，任何人知道网址都能查看和修改。
          设置一个口令后，未登录的访客（包括媒体文件）都会被挡在外面。
        </p>

        <template v-else>
          <p class="field__hint">
            改完立刻生效，<strong>其他设备上的登录会全部失效</strong>，需要重新输一次口令。
          </p>

          <label class="field__stack">
            <span class="field__sub">当前口令</span>
            <input
              v-model="currentCode"
              class="paper-input"
              type="password"
              autocomplete="current-password"
              placeholder="现在在用的口令"
            />
          </label>

          <label class="field__stack">
            <span class="field__sub">新口令</span>
            <input
              v-model="newCode"
              class="paper-input"
              type="password"
              autocomplete="new-password"
              placeholder="至少 6 位"
            />
          </label>

          <label class="field__stack">
            <span class="field__sub">再输一次</span>
            <input
              v-model="confirmCode"
              class="paper-input"
              type="password"
              autocomplete="new-password"
              placeholder="确认新口令"
            />
          </label>

          <p v-if="tooShort" class="field__error">新口令至少 6 位</p>
          <p v-else-if="mismatched" class="field__error">两次输入不一样</p>

          <button type="button" class="glow-btn field__submit" :disabled="!canSave" @click="save">
            {{ saving ? '正在保存…' : '更新口令' }}
          </button>
        </template>
      </section>

      <section class="field">
        <div class="field__head">
          <span class="field__label">备份</span>
          <span class="field__note">{{ backupNote }}</span>
        </div>

        <p class="field__hint">
          第一次导出是完整快照，之后只把新增的照片打进去（记忆仍走全量，它很小），
          改动和删除都会如实带上。恢复时把这条链上的包一次多选导入即可，顺序会按文件名排好。
        </p>

        <div class="backup-interval">
          <span class="backup-interval__label">自动备份</span>
          <div class="backup-interval__chips">
            <button
              v-for="option in intervalOptions"
              :key="option.days"
              type="button"
              class="interval-chip"
              :class="{ 'is-active': option.days === intervalDays }"
              :disabled="savingInterval"
              @click="changeInterval(option.days)"
            >
              {{ option.label }}
            </button>
          </div>
        </div>

        <div class="backup-dir">
          <span class="backup-interval__label">存放位置</span>
          <div class="backup-dir__row">
            <input
              v-model="backupDir"
              class="paper-input backup-dir__input"
              type="text"
              spellcheck="false"
              placeholder="/mnt/backup/efflux"
              aria-label="备份目录"
            />
            <button
              type="button"
              class="ghost-btn backup-btn backup-dir__save"
              :disabled="savingDir"
              @click="saveDir"
            >
              {{ savingDir ? '正在搬…' : '保存' }}
            </button>
          </div>
          <p class="backup-dir__note">
            <template v-if="dirIsDefault">现在用的就是默认位置</template>
            <template v-else>
              换目录时已有的包会一起搬过去
              <button type="button" class="backup-dir__reset" @click="backupDir = defaultDir">
                用默认
              </button>
            </template>
          </p>
        </div>

        <div class="backup-actions">
          <button
            type="button"
            class="glow-btn backup-btn"
            :disabled="exporting"
            @click="exportAll"
          >
            {{ exporting ? '正在打包…' : '立即备份' }}
          </button>
          <button
            type="button"
            class="ghost-btn backup-btn"
            :disabled="importing"
            @click="pickArchive"
          >
            {{ importing ? '正在导入…' : '导入备份' }}
          </button>
          <input
            ref="archiveInput"
            class="visually-hidden"
            type="file"
            accept=".zip,application/zip"
            multiple
            @change="onArchivePicked"
          />
        </div>

        <p v-if="lastReport" class="field__value">
          写入 {{ lastReport.momentsAdded }} 段记忆、{{ lastReport.filesWritten }} 个文件，跳过
          {{ lastReport.momentsSkipped }} 段一样的，重放删除
          {{ lastReport.deletionsApplied }} 次。
        </p>

        <ul v-if="backups.length" class="backup-list">
          <li v-for="file in backups" :key="file.name" class="backup-item">
            <span class="backup-item__kind" :class="`is-${file.kind}`">
              {{ file.kind === 'base' ? '完整' : '增量' }}
            </span>
            <a class="backup-item__name" :href="`/api/backup/${file.name}`" download>
              {{ file.name }}
            </a>
            <span class="backup-item__meta numeral">{{ formatSize(file.sizeBytes) }}</span>
          </li>
        </ul>
      </section>

      <section v-if="protectedMode" class="field">
        <div class="field__head">
          <span class="field__label">这次登录</span>
        </div>
        <p class="field__hint">在这台设备上忘掉登录状态，下次访问需要重新输口令。</p>
        <button
          type="button"
          class="ghost-btn field__submit field__submit--quiet"
          :disabled="loggingOut"
          @click="logout"
        >
          {{ loggingOut ? '正在退出…' : '退出登录' }}
        </button>
      </section>
    </main>
  </div>
</template>

<style scoped>
.backup-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.6rem;
  margin-top: 0.9rem;
}

/* 这是设置项里的小动作，不该和主按钮一个分量 */
.backup-btn {
  padding: 0.46rem 1.15rem;
  font-size: 0.82rem;
  letter-spacing: 0.04em;
}

.backup-list {
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
  margin: 1rem 0 0;
  padding: 0;
  list-style: none;
}

.backup-item {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.5rem 0.7rem;
  border-radius: 0.7rem;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 55%, transparent);
}

/* 整包还是增量，一眼分得出来 */
.backup-item__kind {
  flex: none;
  padding: 0.1rem 0.45rem;
  border-radius: 999px;
  font-size: 0.68rem;
  letter-spacing: 0.04em;
  border: 1px solid color-mix(in oklab, var(--color-ink-500) 55%, transparent);
  color: color-mix(in oklab, var(--color-ink-400) 92%, transparent);
}

.backup-item__kind.is-base {
  border-color: color-mix(in oklab, var(--color-glow-500) 55%, transparent);
  color: var(--color-glow-300);
}

.backup-item__name {
  flex: 1;
  min-width: 0;
  font-size: 0.86rem;
  color: color-mix(in oklab, var(--color-ink-200) 92%, transparent);
  text-decoration: none;
  word-break: break-all;
}

/* 自动备份的档位 */
.backup-interval {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-top: 0.9rem;
}

.backup-interval__label {
  font-size: 0.82rem;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
}

.backup-interval__chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem;
}

.interval-chip {
  padding: 0.24rem 0.7rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 60%, transparent);
  background: none;
  font-size: 0.76rem;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
  transition:
    border-color 0.25s var(--ease-out-soft),
    color 0.25s var(--ease-out-soft),
    background-color 0.25s var(--ease-out-soft);
}

.interval-chip:hover:not(:disabled) {
  border-color: color-mix(in oklab, var(--color-glow-500) 55%, transparent);
  color: var(--color-glow-300);
}

.interval-chip.is-active {
  border-color: color-mix(in oklab, var(--color-glow-500) 62%, transparent);
  background: color-mix(in oklab, var(--color-glow-500) 14%, transparent);
  color: var(--color-glow-300);
}

.interval-chip:disabled {
  opacity: 0.6;
  cursor: default;
}

/* 存放位置 */
.backup-dir {
  margin-top: 0.9rem;
}

.backup-dir__row {
  display: flex;
  align-items: stretch;
  gap: 0.45rem;
  margin-top: 0.4rem;
}

/* 路径用等宽字体，一眼看得出层级；高度定死，和按钮齐平 */
.backup-dir__input {
  flex: 1;
  width: auto;
  min-width: 0;
  height: 2.05rem;
  padding: 0 0.7rem;
  font-size: 0.78rem;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}

.backup-dir__save {
  flex: none;
  display: inline-flex;
  align-items: center;
  height: 2.05rem;
  padding: 0 0.95rem;
}

/* 提示行比正文小一号、颜色也退下去，不跟路径抢眼 */
.backup-dir__note {
  margin: 0.5rem 0 0;
  font-size: 0.74rem;
  line-height: 1.7;
  color: color-mix(in oklab, var(--color-ink-400) 88%, transparent);
}

.backup-dir__reset {
  border: none;
  background: none;
  padding: 0;
  margin-left: 0.3rem;
  font-size: inherit;
  color: color-mix(in oklab, var(--color-glow-500) 88%, transparent);
  border-bottom: 1px dashed color-mix(in oklab, var(--color-glow-600) 45%, transparent);
}

.backup-item__name:hover {
  color: var(--color-glow-400);
}

.backup-item__meta {
  flex: none;
  font-size: 0.76rem;
  color: color-mix(in oklab, var(--color-ink-400) 88%, transparent);
}

/* 藏起来但还能被读屏和 .click() 碰到 */
.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
}

.settings-view {
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
  padding-top: max(1.2rem, calc(var(--safe-top) + 0.8rem));
  padding-bottom: 1.2rem;
  padding-left: max(clamp(1rem, 3vw, 2.4rem), calc(var(--safe-left) + 0.6rem));
  padding-right: max(clamp(1rem, 3vw, 2.4rem), calc(var(--safe-right) + 0.6rem));
  pointer-events: none;
}

.topbar > * {
  pointer-events: auto;
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

.sheet {
  position: relative;
  z-index: 1;
  max-width: 34rem;
  margin: 0 auto;
  padding: clamp(4.4rem, 9vh, 6rem) clamp(1.1rem, 4vw, 2rem) 5rem;
  display: flex;
  flex-direction: column;
  gap: 2.4rem;
}

.sheet__heading {
  margin: 0;
  font-size: clamp(1.5rem, 4vw, 1.9rem);
  letter-spacing: 0.1em;
  color: var(--color-ink-50);
}

.field {
  display: flex;
  flex-direction: column;
  gap: 0.8rem;
  padding: 1.4rem 1.3rem 1.6rem;
  border-radius: var(--radius-lg);
  border: 1px solid color-mix(in oklab, var(--color-ink-700) 82%, transparent);
  background: color-mix(in oklab, var(--color-ink-900) 88%, transparent);
  box-shadow: 0 16px 40px -30px color-mix(in oklab, var(--color-shade) 80%, transparent);
}

/* ---------------- 主题 ---------------- */
.themes {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(9.4rem, 1fr));
  gap: 0.55rem;
}

.theme-card {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.55rem 0.6rem;
  border-radius: var(--radius-md);
  border: 1px solid color-mix(in oklab, var(--color-ink-700) 78%, transparent);
  background: color-mix(in oklab, var(--color-ink-950) 52%, transparent);
  text-align: left;
  transition:
    border-color 0.35s var(--ease-out-soft),
    background-color 0.35s var(--ease-out-soft),
    transform 0.25s var(--ease-out-soft);
}

.theme-card:hover {
  border-color: color-mix(in oklab, var(--color-glow-500) 52%, transparent);
}

.theme-card:active {
  transform: scale(0.98);
}

.theme-card.is-active {
  border-color: color-mix(in oklab, var(--color-glow-500) 78%, transparent);
  background: color-mix(in oklab, var(--color-glow-600) 14%, transparent);
}

/* 色卡画的就是那套主题本身：纸面、一道光河、一颗墨点 */
.theme-card__paper {
  position: relative;
  flex: none;
  width: 3.1rem;
  height: 2.15rem;
  border-radius: 7px;
  border: 1px solid var(--p-edge, var(--color-ink-700));
  background: linear-gradient(170deg, var(--p-top), var(--p-bottom));
  overflow: hidden;
  box-shadow: 0 3px 10px -6px color-mix(in oklab, var(--color-shade) 90%, transparent);
}

.theme-card__glow {
  position: absolute;
  left: 0.32rem;
  right: 0.32rem;
  top: 62%;
  height: 1.5px;
  border-radius: 2px;
  background: linear-gradient(
    to right,
    transparent,
    var(--p-glow) 20%,
    var(--p-glow) 80%,
    transparent
  );
  box-shadow: 0 0 8px -1px var(--p-glow);
}

.theme-card__dot {
  position: absolute;
  left: 50%;
  top: 62%;
  width: 5px;
  height: 5px;
  margin: -3.5px 0 0 -2.5px;
  border-radius: 50%;
  background: var(--p-ink);
  opacity: 0.7;
}

.theme-card__text {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  min-width: 0;
}

.theme-card__name {
  display: inline-flex;
  align-items: center;
  font-size: 0.86rem;
  color: var(--color-ink-100);
}

.theme-card.is-active .theme-card__name {
  color: var(--color-glow-300);
}

.theme-card__tick {
  width: 5px;
  height: 5px;
  margin-left: 0.32rem;
  border-radius: 50%;
  background: var(--color-glow-400);
  box-shadow: 0 0 8px color-mix(in oklab, var(--color-glow-400) 78%, transparent);
}

.theme-card__note {
  font-size: 0.68rem;
  color: color-mix(in oklab, var(--color-ink-400) 92%, transparent);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 每张色卡用自己那套颜色，不受当前主题影响 */
.theme-card[data-preview='xuan'] {
  --p-top: oklch(0.978 0.012 87);
  --p-bottom: oklch(0.952 0.019 82);
  --p-glow: oklch(0.702 0.152 63);
  --p-ink: oklch(0.3 0.03 50);
  --p-edge: oklch(0.876 0.022 80);
}

.theme-card[data-preview='mist'] {
  --p-top: oklch(0.972 0.008 236);
  --p-bottom: oklch(0.946 0.012 232);
  --p-glow: oklch(0.625 0.102 242);
  --p-ink: oklch(0.3 0.025 218);
  --p-edge: oklch(0.868 0.016 229);
}

.theme-card[data-preview='dusk'] {
  --p-top: oklch(0.968 0.02 62);
  --p-bottom: oklch(0.938 0.028 54);
  --p-glow: oklch(0.702 0.165 50);
  --p-ink: oklch(0.3 0.038 42);
  --p-edge: oklch(0.855 0.03 54);
}

.theme-card[data-preview='moss'] {
  --p-top: oklch(0.97 0.014 138);
  --p-bottom: oklch(0.94 0.02 132);
  --p-glow: oklch(0.622 0.108 152);
  --p-ink: oklch(0.3 0.029 118);
  --p-edge: oklch(0.858 0.022 130);
}

.theme-card[data-preview='night'] {
  --p-top: oklch(0.235 0.021 260);
  --p-bottom: oklch(0.19 0.017 256);
  --p-glow: oklch(0.828 0.078 228);
  --p-ink: oklch(0.878 0.016 238);
  --p-edge: oklch(0.362 0.025 250);
}

.theme-card[data-preview='relic'] {
  --p-top: oklch(0.958 0.028 86);
  --p-bottom: oklch(0.928 0.036 80);
  --p-glow: oklch(0.588 0.118 64);
  --p-ink: oklch(0.298 0.054 68);
  --p-edge: oklch(0.842 0.044 80);
}

.theme-card[data-preview='maple'] {
  --p-top: oklch(0.964 0.03 58);
  --p-bottom: oklch(0.936 0.038 50);
  --p-glow: oklch(0.618 0.19 30);
  --p-ink: oklch(0.298 0.046 40);
  --p-edge: oklch(0.852 0.04 52);
}

.theme-card[data-preview='sakura'] {
  --p-top: oklch(0.976 0.015 14);
  --p-bottom: oklch(0.95 0.022 8);
  --p-glow: oklch(0.658 0.138 12);
  --p-ink: oklch(0.304 0.032 354);
  --p-edge: oklch(0.862 0.026 6);
}

.theme-card[data-preview='ocean'] {
  --p-top: oklch(0.968 0.015 208);
  --p-bottom: oklch(0.94 0.022 202);
  --p-glow: oklch(0.628 0.118 226);
  --p-ink: oklch(0.3 0.033 190);
  --p-edge: oklch(0.856 0.026 202);
}

.theme-card[data-preview='starry'] {
  --p-top: oklch(0.228 0.038 288);
  --p-bottom: oklch(0.18 0.032 284);
  --p-glow: oklch(0.842 0.088 292);
  --p-ink: oklch(0.878 0.022 265);
  --p-edge: oklch(0.348 0.04 277);
}

.field__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 1rem;
}

.field__label {
  font-size: 0.92rem;
  letter-spacing: 0.1em;
  color: var(--color-ink-100);
}

.field__note {
  font-size: 0.74rem;
  color: color-mix(in oklab, var(--color-ink-400) 92%, transparent);
}

.field__hint {
  margin: 0;
  font-size: 0.8rem;
  line-height: 1.85;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
}

.field__hint strong {
  color: var(--color-glow-300);
  font-weight: 600;
}

.field__stack {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.field__sub {
  font-size: 0.76rem;
  letter-spacing: 0.08em;
  color: color-mix(in oklab, var(--color-ink-400) 94%, transparent);
}

.field__error {
  margin: 0;
  font-size: 0.78rem;
  color: var(--color-ember-400);
}

.field__submit {
  align-self: flex-start;
  margin-top: 0.4rem;
  padding: 0.72rem 1.6rem;
  font-size: 0.9rem;
}

.field__submit--quiet {
  padding: 0.62rem 1.4rem;
  font-size: 0.86rem;
}
</style>
