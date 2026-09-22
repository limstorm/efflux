import type { Place } from '@/api/types'

const WEEKDAYS = ['日', '一', '二', '三', '四', '五', '六']

function toDate(value: string | Date): Date {
  return value instanceof Date ? value : new Date(value)
}

/** 2026年9月14日 星期一 */
export function formatFullDate(value: string | Date): string {
  const d = toDate(value)
  return `${d.getFullYear()}年${d.getMonth() + 1}月${d.getDate()}日 星期${WEEKDAYS[d.getDay()]}`
}

/** 9月14日 */
export function formatMonthDay(value: string | Date): string {
  const d = toDate(value)
  return `${d.getMonth() + 1}月${d.getDate()}日`
}

/** 2026.09.14 */
export function formatDotted(value: string | Date): string {
  const d = toDate(value)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}.${pad(d.getMonth() + 1)}.${pad(d.getDate())}`
}

/** 2026.09.14 21:30 */
export function formatDottedTime(value: string | Date): string {
  const d = toDate(value)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${formatDotted(d)} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 21:30 */
export function formatClock(value: string | Date): string {
  const d = toDate(value)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 刚刚 / 3 小时前 / 昨天 / 5 天前 / 2026年3月 */
export function relativeTime(value: string | Date): string {
  const d = toDate(value)
  const diff = Date.now() - d.getTime()
  const minute = 60_000
  const hour = 60 * minute
  const day = 24 * hour

  if (diff < minute) return '刚刚'
  if (diff < hour) return `${Math.floor(diff / minute)} 分钟前`
  if (diff < day) return `${Math.floor(diff / hour)} 小时前`
  if (diff < 2 * day) return '昨天'
  if (diff < 30 * day) return `${Math.floor(diff / day)} 天前`
  if (diff < 365 * day) return `${Math.floor(diff / (30 * day))} 个月前`
  return `${d.getFullYear()}年${d.getMonth() + 1}月`
}

/** 段落化的时间描述：今天 / 昨天 / 更早的某个时刻 */
export function describeWhen(value: string | Date): string {
  const d = toDate(value)
  const now = new Date()
  const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime()
  const days = Math.round((startOfDay(now) - startOfDay(d)) / 86_400_000)

  if (days === 0) return `今天 ${formatClock(d)}`
  if (days === 1) return `昨天 ${formatClock(d)}`
  if (days === 2) return `前天 ${formatClock(d)}`
  if (days > 2 && days < 30) return `${days} 天前`
  return formatDottedTime(d)
}

export function formatDuration(ms: number): string {
  const total = Math.round(ms / 1000)
  const minutes = Math.floor(total / 60)
  const seconds = total % 60
  if (minutes >= 60) {
    const hours = Math.floor(minutes / 60)
    return `${hours}:${String(minutes % 60).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`
  }
  return `${minutes}:${String(seconds).padStart(2, '0')}`
}

/**
 * 地点写成一行：「城市 · 具体地点」。
 *
 * 只记了坐标的那几条，退回坐标——保留两位就够看出大概在哪儿了，
 * 四位小数读起来像仪表读数，不像一句备注。
 */
export function formatPlace(place: Place | null | undefined): string {
  if (!place) return ''
  const city = place.city.trim()
  const spot = place.name.trim()
  if (city && spot) return `${city} · ${spot}`
  if (city || spot) return city || spot
  if (place.latitude != null && place.longitude != null) {
    return `${place.latitude.toFixed(2)}, ${place.longitude.toFixed(2)}`
  }
  return ''
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`
}
