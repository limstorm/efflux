import { computed, type ComputedRef, type Ref } from 'vue'

import type { MomentSummary } from '@/api/types'

export interface RiverNode {
  moment: MomentSummary
  /** 左边缘在轨道坐标系中的位置 */
  x: number
  centerX: number
  width: number
  height: number
  /** 底部距光河的抬升高度（px），值越大越"高" */
  lift: number
  /** 轻微的倾斜角度，让照片像是随手贴上去的 */
  tilt: number
  kind: 'photo' | 'video' | 'text'
}

export interface RiverMarker {
  key: string
  x: number
  label: string
  strong: boolean
}

const HOUR = 3_600_000

/** 时间间隔 → 水平间距：非线性压缩，避免久远的空白撑爆轨道 */
function gapFor(hours: number, min: number, max: number): number {
  const raw = 16 * Math.pow(Math.max(hours, 0.6), 0.6)
  return Math.min(max, Math.max(min, raw))
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value))
}

interface Metrics {
  baseWidth: number
  minHeight: number
  maxHeight: number
  minGap: number
  maxGap: number
  headPad: number
  tailPad: number
}

function metricsFor(viewportWidth: number): Metrics {
  const compact = viewportWidth < 760
  return compact
    ? {
        baseWidth: 214,
        minHeight: 132,
        maxHeight: 300,
        minGap: 112,
        maxGap: 250,
        headPad: 150,
        tailPad: 300,
      }
    : {
        // 间距刻意压得比较紧：一屏里能看见两三段记忆，
        // 画面才不至于空荡，但彼此仍有呼吸。
        baseWidth: 292,
        minHeight: 172,
        maxHeight: 400,
        minGap: 152,
        maxGap: 352,
        headPad: 320,
        tailPad: 580,
      }
}

export function useRiverLayout(
  moments: Ref<MomentSummary[]> | ComputedRef<MomentSummary[]>,
  viewportWidth: Ref<number> | ComputedRef<number>,
) {
  const metrics = computed(() => metricsFor(viewportWidth.value))
  const isCompact = computed(() => viewportWidth.value < 760)

  const nodes = computed<RiverNode[]>(() => {
    const list = moments.value
    const m = metrics.value
    const result: RiverNode[] = []

    let cursor = m.headPad
    let previousTime: number | null = null

    list.forEach((moment, index) => {
      const time = new Date(moment.happenedAt).getTime()

      if (previousTime !== null) {
        const hours = Math.abs(time - previousTime) / HOUR
        cursor += gapFor(hours, m.minGap, m.maxGap)
      }
      previousTime = time

      const cover = moment.cover
      let kind: RiverNode['kind'] = 'text'
      let height: number

      if (cover && cover.width && cover.height) {
        kind = cover.kind === 'video' ? 'video' : 'photo'
        const ratio = cover.height / cover.width
        height = clamp(m.baseWidth * ratio, m.minHeight, m.maxHeight)
      } else if (cover) {
        kind = cover.kind === 'video' ? 'video' : 'photo'
        height = m.baseWidth * 0.75
      } else {
        const text = moment.title || moment.excerpt || '这一刻'
        const charsPerLine = m.baseWidth / (isCompact.value ? 16 : 17)
        const lines = Math.ceil(text.length / charsPerLine)
        height = clamp(m.minHeight + (lines - 1) * 30, m.minHeight, m.maxHeight)
      }

      const width = m.baseWidth
      // 悬挂在光河上方的轻微起伏，形成节奏
      const lift = 34 + (index % 3) * 18 + Math.sin(index * 1.27) * 14
      // 确定的伪随机倾斜：像被随手贴上去的照片
      const tilt = (((index * 37) % 5) - 2) * (isCompact.value ? 0.5 : 0.85)

      const x = cursor
      result.push({
        moment,
        x,
        centerX: x + width / 2,
        width,
        height,
        lift: Math.round(lift),
        tilt: Math.round(tilt * 100) / 100,
        kind,
      })

      cursor += width
    })

    return result
  })

  const totalWidth = computed(() => {
    const last = nodes.value[nodes.value.length - 1]
    const end = last ? last.x + last.width : 0
    return Math.max(end + metrics.value.tailPad, viewportWidth.value * 1.4)
  })

  const markers = computed<RiverMarker[]>(() => {
    const list: RiverMarker[] = []
    const source = nodes.value
    let lastYear = ''
    let lastMonth = ''
    let lastMarkerX = -Infinity

    source.forEach((node, index) => {
      const date = new Date(node.moment.happenedAt)
      const year = String(date.getFullYear())
      const month = `${year}-${date.getMonth()}`

      const isNewYear = year !== lastYear
      const isNewMonth = month !== lastMonth
      if (!isNewYear && !isNewMonth) return

      // 刻度落在两段记忆之间的空隙里，避免压住卡片
      const previous = source[index - 1]
      let anchor = node.centerX
      if (previous) {
        const leftEdge = previous.x + previous.width
        const gap = node.x - leftEdge
        if (gap > 64) anchor = leftEdge + gap / 2
      }

      const tooClose = !isNewYear && anchor - lastMarkerX < 84
      if (tooClose) {
        lastYear = year
        lastMonth = month
        return
      }

      list.push({
        key: `${isNewYear ? 'y' : 'm'}-${month}-${node.moment.id}`,
        x: anchor,
        label: isNewYear ? year : `${date.getMonth() + 1}月`,
        strong: isNewYear,
      })

      lastMarkerX = anchor
      lastYear = year
      lastMonth = month
    })

    return list
  })

  /** 二分查找：相机左边缘对应的节点区域 */
  function indexAt(x: number): number {
    const list = nodes.value
    if (!list.length) return -1
    let low = 0
    let high = list.length - 1
    while (low < high) {
      const mid = (low + high) >> 1
      if (list[mid]!.centerX < x) low = mid + 1
      else high = mid
    }
    return low
  }

  return { nodes, totalWidth, markers, isCompact, indexAt }
}
