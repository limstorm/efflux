<script setup lang="ts">
// 点亮地图：省级整块填充，点进去下钻到市。
//
// 两点值得留意：
//
// 一、**颜色得从主题里读出来**。ECharts 不认 CSS 变量，喂 `var(--x)` 给它是不认的，
//    所以要用 getComputedStyle 读实际色值；主题一变就重画一遍。
//
// 二、**不要动 aspectScale**。它的默认值 0.75 把经度方向压掉四分之一，画出来的宽高比
//    就是包围盒宽高比的 0.75 倍——按经纬度直画会显得扁，压一压才顺眼。
//
// 边界数据里的南海诸岛由 `utils/mapInset` 折进右下角的小框——主图因此矮了一截，
// 容器也就敢贴着地图定尺寸了（见下面 .chart 的 --map-aspect）。
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import * as echarts from 'echarts/core'
import { MapChart } from 'echarts/charts'
import { TooltipComponent, VisualMapComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'

import { api } from '@/api/client'
import type { CityLit, ProvinceLit } from '@/api/types'
import { useTheme } from '@/composables/useTheme'
import { useToast } from '@/composables/useToast'
import { formatFullDate } from '@/utils/format'
import {
  FILL,
  foldSouthSea,
  SOUTH_SEA,
  SOUTH_SEA_LABEL,
  type MapShape,
} from '@/utils/mapInset'

echarts.use([MapChart, TooltipComponent, VisualMapComponent, CanvasRenderer])

const route = useRoute()
const router = useRouter()
const toast = useToast()
const { themeId } = useTheme()

const chartEl = ref<HTMLDivElement | null>(null)
let chart: echarts.ECharts | null = null

/** 容器的宽高比。贴上地图本身的比例，四周才不会空出一大片纸 */
const chartAspect = ref(1.6)

/** 已注册进 ECharts 的地图（折过南海诸岛之后的样子），避免重复拉同一份 geojson */
const registered = new Map<string, MapShape>()

const provinces = ref<ProvinceLit[]>([])
const cities = ref<CityLit[]>([])
const current = ref<ProvinceLit | null>(null)
const loading = ref(true)
const drilling = ref(false)
const failed = ref(false)

const summary = ref({ provinces: 0, cities: 0, visits: 0 })
const overseas = ref<string[]>([])

const provinceVisits = computed(() => cities.value.reduce((sum, c) => sum + c.visits, 0))

/** 图底下那行小字：两个视图各说各的话 */
const hint = computed(() => {
  if (current.value) return '点一个亮起来的市，看那里的记忆'
  if (overseas.value.length) {
    return `此外还去过 ${overseas.value.length} 座海外城市：${overseas.value.join('、')}`
  }
  return '点一个亮起来的省，看看那里的市'
})

// ---------------------------------------------------------------- 主题取色

function readTheme() {
  const css = getComputedStyle(document.documentElement)
  const pick = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback
  return {
    /** 没点亮的地 */
    land: pick('--color-ink-800', '#e6dcc8'),
    line: pick('--color-ink-600', '#cec0a4'),
    /** 四档，从浅到深 */
    lit1: pick('--color-glow-300', '#f0b656'),
    lit2: pick('--color-glow-400', '#e2951f'),
    lit3: pick('--color-glow-500', '#c87b0b'),
    lit4: pick('--color-glow-600', '#a9630a'),
    borderLit: pick('--color-glow-600', '#a9630a'),
    text: pick('--color-ink-100', '#6b4610'),
    /** 备注性质的小字：右下角小框的标注 */
    soft: pick('--color-ink-400', '#a08c6d'),
  }
}

type MapTheme = ReturnType<typeof readTheme>

/** 分四档，免得「路过一次」和「住过半年」看起来一样 */
function pieces(theme: MapTheme) {
  return [
    { min: 1, max: 1, color: theme.lit1 },
    { min: 2, max: 3, color: theme.lit2 },
    { min: 4, max: 6, color: theme.lit3 },
    { min: 7, color: theme.lit4 },
  ]
}

const tooltipStyle = {
  backgroundColor: 'rgba(28,24,19,0.9)',
  borderWidth: 0,
  padding: [8, 11],
  textStyle: { color: '#f4eee2', fontSize: 12, lineHeight: 18 },
} as const

function whenLabel(iso: string | null): string {
  if (!iso) return ''
  return `最近一次 ${formatFullDate(new Date(iso))}`
}

// ---------------------------------------------------------------- 组装 option

/**
 * 右下角的南海诸岛小框：只作参照，不参与点亮，也不接鼠标——
 * 不然点它一下，像是点到了什么地方。
 */
function insetItem(theme: MapTheme, name: string) {
  if (!registered.get(name)?.framed) return []
  return [
    {
      name: SOUTH_SEA,
      silent: true,
      itemStyle: { areaColor: theme.line, borderColor: theme.line, borderWidth: 0 },
    },
    {
      // 框底那行小字占一块空地，字就落在小框里头
      name: SOUTH_SEA_LABEL,
      silent: true,
      itemStyle: { areaColor: 'transparent', borderWidth: 0 },
      label: {
        show: true,
        position: 'inside' as const,
        formatter: SOUTH_SEA,
        fontSize: 10,
        color: theme.soft,
      },
    },
  ]
}

function countryOption(theme: MapTheme) {
  return {
    backgroundColor: 'transparent',
    tooltip: {
      trigger: 'item',
      ...tooltipStyle,
      formatter: (params: { name: string }) => {
        const hit = provinces.value.find((p) => p.name === params.name)
        if (!hit) return `${params.name}<br/><span style="opacity:.6">还没有点亮</span>`
        return `${hit.name}<br/>${hit.cities} 座城 · ${hit.visits} 段记忆<br/><span style="opacity:.7">${whenLabel(hit.lastAt)}</span>`
      },
    },
    visualMap: { show: false, seriesIndex: 0, type: 'piecewise', pieces: pieces(theme) },
    series: [
      {
        type: 'map',
        map: 'china',
        roam: false,
        zoom: FILL,
        label: { show: false },
        itemStyle: { areaColor: theme.land, borderColor: theme.line, borderWidth: 0.8 },
        emphasis: {
          label: { show: true, color: theme.text, fontSize: 11 },
          itemStyle: {
            areaColor: theme.lit2,
            borderColor: theme.borderLit,
            borderWidth: 1.3,
          },
        },
        select: { disabled: true },
        data: [
          ...provinces.value.map((p) => ({ name: p.name, value: p.visits })),
          ...insetItem(theme, 'china'),
        ],
      },
    ],
  }
}

function provinceOption(theme: MapTheme, province: ProvinceLit) {
  return {
    backgroundColor: 'transparent',
    tooltip: {
      trigger: 'item',
      ...tooltipStyle,
      formatter: (params: { name: string }) => {
        const hit = cities.value.find((c) => c.mapName === params.name)
        if (!hit) return `${params.name}<br/><span style="opacity:.6">还没有点亮</span>`
        return `${hit.name}<br/>${hit.visits} 段记忆<br/><span style="opacity:.7">${whenLabel(hit.lastAt)}</span>`
      },
    },
    visualMap: { show: false, seriesIndex: 0, type: 'piecewise', pieces: pieces(theme) },
    series: [
      {
        type: 'map',
        map: `province-${province.adcode}`,
        roam: false,
        zoom: FILL,
        label: { show: false },
        itemStyle: { areaColor: theme.land, borderColor: theme.line, borderWidth: 0.9 },
        emphasis: {
          label: { show: true, color: theme.text, fontSize: 11 },
          itemStyle: {
            areaColor: theme.lit2,
            borderColor: theme.borderLit,
            borderWidth: 1.3,
          },
        },
        select: { disabled: true },
        // 用官方名去对多边形
        data: [
          ...cities.value.map((c) => ({ name: c.mapName, value: c.visits })),
          ...insetItem(theme, `province-${province.adcode}`),
        ],
      },
    ],
  }
}

// ---------------------------------------------------------------- 地图数据

async function ensureMap(name: string, url: string): Promise<boolean> {
  if (registered.has(name)) return true
  try {
    // 折过南海诸岛再注册：主图只留大陆，岛礁去右下角的小框（见 utils/mapInset）
    const shape = foldSouthSea(await fetch(url).then((r) => r.json()))
    echarts.registerMap(name, shape.geo)
    registered.set(name, shape)
    return true
  } catch {
    return false
  }
}

/** 换地图就得换容器的比例，换完让 ECharts 重新量一次画布 */
async function adoptAspect(name: string) {
  const shape = registered.get(name)
  if (!shape || shape.aspect === chartAspect.value) return
  chartAspect.value = shape.aspect
  await nextTick()
  chart?.resize()
}

/**
 * 当前视图写回地址栏。分享出去的链接要能落回来，从搜索页退回地图也得还在原来那个省
 * （本页支持 ?province=330000，见 onMounted）
 */
function syncQuery(province: ProvinceLit | null) {
  const want = province ? String(province.adcode) : ''
  if (String(route.query.province ?? '') === want) return
  void router.replace({ name: 'map', query: want ? { province: want } : {} })
}

async function showCountry() {
  const ok = await ensureMap('china', '/map/china-provinces.json')
  if (!ok) {
    failed.value = true
    return
  }
  await adoptAspect('china')
  current.value = null
  cities.value = []
  syncQuery(null)
  chart?.setOption(countryOption(readTheme()), true)
}

async function drillInto(province: ProvinceLit) {
  if (drilling.value) return
  drilling.value = true
  try {
    const ok = await ensureMap(
      `province-${province.adcode}`,
      `/map/province-${province.adcode}.json`,
    )
    if (!ok) {
      toast.error('这个省的地图没能加载出来')
      return
    }
    const detail = await api.mapCities(province.adcode)
    await adoptAspect(`province-${province.adcode}`)
    cities.value = detail.cities
    current.value = province
    syncQuery(province)
    chart?.setOption(provinceOption(readTheme(), province), true)
  } catch {
    toast.error('没能打开这个省')
  } finally {
    drilling.value = false
  }
}

/**
 * 直辖市与港澳：市一级就是它本身，底下不再分区，`public/map` 里也没有它们的省级文件。
 * 这几种点下去得直接去看记忆——否则会去拉一个不存在的文件，只弹一句「地图没能加载出来」。
 */
const SELF_CITY = new Set([110000, 120000, 310000, 500000, 810000, 820000])

/** 「北京市」→「北京」；取出来的名字要和记忆里 city 字段存的那份一致 */
function selfCity(province: ProvinceLit): string | null {
  if (!SELF_CITY.has(province.adcode)) return null
  return province.name.replace(/(特别行政区|市)$/, '')
}

/** 去搜索页翻一座城的记忆。搜索本来就认城市名，不必另加一套筛选 */
function openCity(name: string) {
  void router.push({ name: 'search', query: { q: name } })
}

function onChartClick(params: { name?: string }) {
  if (!params.name) return
  // 全国视图：点亮过的省才下钻（没点亮的点进去也是空的）
  if (!current.value) {
    const hit = provinces.value.find((p) => p.name === params.name)
    if (!hit) return
    const direct = selfCity(hit)
    if (direct) openCity(direct)
    else void drillInto(hit)
    return
  }
  // 省视图：点到哪座城，就去翻哪座城的记忆
  const city = cities.value.find((c) => c.mapName === params.name)
  if (city) openCity(city.name)
}

// ---------------------------------------------------------------- 生命周期

function onResize() {
  chart?.resize()
}

onMounted(async () => {
  try {
    const data = await api.mapProvinces()
    provinces.value = data.provinces
    overseas.value = data.overseas
    summary.value = {
      provinces: data.totalProvinces,
      cities: data.totalCities,
      visits: data.totalVisits,
    }
  } catch {
    failed.value = true
  } finally {
    loading.value = false
  }

  await nextTick()
  if (!chartEl.value) return
  chart = echarts.init(chartEl.value)
  chart.on('click', onChartClick)
  window.addEventListener('resize', onResize)

  if (failed.value) return

  // 支持 ?province=330000 直接落到某个省——收藏和分享都用得上
  const wanted = Number(route.query.province)
  const target = Number.isFinite(wanted)
    ? provinces.value.find((p) => p.adcode === wanted)
    : undefined
  // 直辖市没有下级地图，落到它们身上就照旧看全国；下钻失败也退回全国，别撂一张白图
  if (target && !selfCity(target)) await drillInto(target)
  if (!current.value) await showCountry()
})

onBeforeUnmount(() => {
  window.removeEventListener('resize', onResize)
  chart?.dispose()
  chart = null
})

// 换主题就重画：ECharts 只认具体色值，读一遍再喂给它
watch(themeId, () => {
  const theme = readTheme()
  if (current.value) chart?.setOption(provinceOption(theme, current.value), true)
  else chart?.setOption(countryOption(theme), true)
})

function goBack() {
  const state = window.history.state as { back?: string | null } | null
  if (state?.back) router.back()
  else void router.push({ name: 'home' })
}


</script>

<template>
  <div class="map-view">
    <header class="topbar">
      <button type="button" class="hud-icon" aria-label="返回" title="返回（Esc）" @click="goBack">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" class="h-5 w-5">
          <path d="M14.5 5.5 8 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <span class="topbar__title title-serif">
        {{ current ? current.name : '点亮地图' }}
      </span>
      <span class="topbar__spacer" />
    </header>

    <main class="stage">
      <p v-if="loading" class="notice">正在铺开地图…</p>
      <p v-else-if="failed" class="notice">地图没能加载出来，稍后再试</p>

      <div v-show="!loading && !failed" class="board">
        <div ref="chartEl" class="chart" :style="{ '--map-aspect': chartAspect }" />

        <Transition name="page">
          <button v-if="current" type="button" class="back-chip" @click="showCountry">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" class="h-3.5 w-3.5">
              <path d="M14.5 5.5 8 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
            回到全国
          </button>
        </Transition>

        <div class="caption">
          <p class="caption__main">
            <template v-if="current">
              {{ current.name }} · {{ cities.length }} 座城 · {{ provinceVisits }} 段记忆
            </template>
            <template v-else>
              {{ summary.provinces }} 个省 · {{ summary.cities }} 座城 ·
              {{ summary.visits }} 段记忆
            </template>
          </p>
          <p class="caption__soft">{{ hint }}</p>
        </div>
      </div>
    </main>
  </div>
</template>

<style scoped>
.map-view {
  min-height: 100dvh;
  display: flex;
  flex-direction: column;
}

.topbar {
  display: flex;
  align-items: center;
  gap: 0.8rem;
  padding: max(1.1rem, calc(var(--safe-top) + 0.7rem)) clamp(1rem, 3vw, 2rem) 0.6rem;
}

.topbar__title {
  font-size: 1.02rem;
  letter-spacing: 0.1em;
  color: var(--color-ink-100);
}

.topbar__spacer {
  flex: 1;
}

.stage {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 0 clamp(1rem, 3vw, 2rem) clamp(1.4rem, 3vw, 2.4rem);
}

.notice {
  margin-top: 22dvh;
  font-size: 0.9rem;
  color: color-mix(in oklab, var(--color-ink-300) 92%, transparent);
}

.board {
  position: relative;
  width: 100%;
  max-width: 68rem;
  /* 地图能长到多高 */
  --map-height: 74dvh;
}

/* 容器贴着地图自身的比例走：ECharts 会保持地图比例，容器比它胖或瘦都会白空出一条边。
   比例由地图自己报上来（见 utils/mapInset 的 aspect），宽高都据此推算 */
.chart {
  width: min(100%, calc(var(--map-height) * var(--map-aspect, 1.6)));
  margin: 0 auto;
  aspect-ratio: var(--map-aspect, 1.6);
}

/* 从市视图返回。做成悬浮的，不占地图的地方 */
.back-chip {
  position: absolute;
  top: 0.4rem;
  left: 0.2rem;
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  padding: 0.32rem 0.85rem 0.32rem 0.6rem;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-ink-600) 62%, transparent);
  background: color-mix(in oklab, var(--color-ink-850) 88%, transparent);
  font-size: 0.78rem;
  color: color-mix(in oklab, var(--color-ink-200) 94%, transparent);
  transition:
    border-color 0.3s var(--ease-out-soft),
    color 0.3s var(--ease-out-soft);
}

.back-chip:hover {
  border-color: color-mix(in oklab, var(--color-glow-500) 60%, transparent);
  color: var(--color-glow-300);
}

.caption {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0.5rem 0.9rem;
  margin-top: 0.6rem;
  padding: 0 0.3rem;
}

.caption__main {
  margin: 0;
  font-size: 0.86rem;
  letter-spacing: 0.05em;
  color: color-mix(in oklab, var(--color-ink-200) 94%, transparent);
}

.caption__soft {
  margin: 0;
  font-size: 0.76rem;
  color: color-mix(in oklab, var(--color-ink-400) 90%, transparent);
}

@media (max-width: 760px) {
  .board {
    --map-height: 56dvh;
  }
}
</style>
