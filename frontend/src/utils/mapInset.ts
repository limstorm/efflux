/**
 * 南海诸岛折进右下角的小框。
 *
 * 边界数据是按真实经纬度画的：三沙市的岛礁一路铺到北纬 4 度，整个包围盒被拽得
 * 又高又窄，大陆挤在中间显小。中文地图的常规做法是——主图只留大陆（下钻到海南时
 * 留下本岛），南海诸岛另行画在右下角一个小框里。
 *
 * 这个模块做两件事：
 *
 *   1. 把该折的岛礁从主图上摘下来；
 *   2. 在右下角补上那个小框，并给出折完之后地图的宽高比，让容器贴着地图定尺寸。
 *
 * 框里画的是 ECharts 官方那套「南海诸岛」（见 echarts 的 coord/geo/fix/nanhai.js，
 * 九段线与框线都在里面）。它本来只认 `china` 这个地图名，这里借过来，省级图也有一份。
 */

type Ring = number[][]
type Polygon = Ring[]

type Geometry =
  | { type: 'Polygon'; coordinates: Polygon }
  | { type: 'MultiPolygon'; coordinates: Polygon[] }

interface Feature {
  type: 'Feature'
  properties: { name?: string; [key: string]: unknown }
  geometry: Geometry
}

export interface GeoJson {
  type: 'FeatureCollection'
  features: Feature[]
}

export interface MapShape {
  /** 折过之后的地图，直接交给 echarts.registerMap */
  geo: GeoJson
  /** 容器该用的宽高比（ECharts 的 aspectScale 已经算进去了） */
  aspect: number
  /** 有没有折出小框——有才需要那条不影响数据的小框样式 */
  framed: boolean
}

/** 小框的名字 */
export const SOUTH_SEA = '南海诸岛'
/** 框底那行字单独占一块：地图标注的位置由 ECharts 自己算，跟着整块形状走不稳 */
export const SOUTH_SEA_LABEL = '南海诸岛标注'

/**
 * ECharts 官方的南海诸岛形状，经纬度相对值：原点在小框左下，横向 0~6.0952、纵向 0~6.65。
 * 前半截是九段线，最后一段是框线（一条带缝的闭合线，中间因此是空的）。
 */
const NANHAI_RINGS: Ring[] = [
  [[0.0, 6.4], [0.6667, 5.85], [1.4286, 5.8], [2.8571, 6.15], [4.0, 6.6], [4.9524, 6.6], [5.3333, 6.1], [5.619, 6.6], [6.0952, 6.6], [6.0952, 6.65], [0.4762, 6.65], [0.0, 6.4]],
  [[1.2381, 5.5], [1.8095, 5.6], [1.5238, 5.1], [1.0476, 5.0], [1.2381, 5.5]],
  [[1.1429, 4.35], [1.3333, 3.9], [1.4286, 3.9], [1.2381, 4.35], [1.1429, 4.35]],
  [[1.5238, 3.25], [1.1429, 2.85], [1.2381, 2.85], [1.7143, 3.25], [1.5238, 3.25]],
  [[0.5714, 2.05], [0.7619, 1.65], [0.8571, 1.65], [0.7619, 2.05], [0.5714, 2.05]],
  [[2.1905, 0.75], [2.7619, 0.95], [2.8571, 0.95], [2.381, 0.75], [2.1905, 0.75]],
  [[3.5238, 1.6], [4.0952, 2.2], [4.1905, 2.2], [3.7143, 1.6], [3.5238, 1.6]],
  [[4.5714, 3.0], [4.8571, 3.4], [5.0476, 3.4], [4.7619, 3.0], [4.5714, 3.0]],
  [[4.8571, 4.15], [4.8571, 4.6], [5.0476, 4.6], [5.0476, 4.15], [4.8571, 4.15]],
  [[4.9524, 5.05], [5.2381, 5.4], [5.3333, 5.4], [5.0476, 5.05], [4.9524, 5.05]],
  [[5.5238, 5.75], [5.9048, 6.15], [6.0, 6.15], [5.7143, 5.75], [5.5238, 5.75]],
  [[0.0, 6.4], [0.0, 0.0], [6.0952, 0.0], [6.0952, 6.65], [6.0, 6.65], [6.0, 0.05], [0.0952, 0.05], [0.0952, 6.4], [0.0, 6.4]],
]

/** 官方数据里这十二段各自成块，包成多边形交给下面的搬运 */
const NANHAI: Polygon[] = NANHAI_RINGS.map((ring) => [ring])

/** ECharts 的 aspectScale。它压的是经度方向：画出来的宽高比 = 包围盒宽高比 × 0.75 */
const ASPECT = 0.75
/** 这条纬线以南的岛礁一律折进小框。海南本岛最南端在北纬 18.14°，三沙的岛礁都在 17.1° 以南 */
const SOUTH_CUT = 18.1
/** 整个单位就是南海诸岛的，不必再看纬度 */
const WHOLE = ['三沙市']
/** 小框：高度占全图的比例、框内留白、框底留给标注的一行、离右下角的边距 */
const BOX_HEIGHT = 0.24
const BOX_PAD = 0.04
const BOX_LABEL = 0.18
const CORNER = 0.02
/** 容器比地图本体稍宽一点，免得主图顶着画布边缘 */
const BREATH = 1.06
/**
 * ECharts 给地图留的余地：既不写 layoutSize 也不写 width/height 时，它只占容器的八成
 * （见 util/layout.js 里那句 height = containerHeight * 0.8）。乘回来才铺得满。
 */
export const FILL = 1.25

interface Bounds {
  x0: number
  y0: number
  x1: number
  y1: number
  w: number
  h: number
}

function polygons(feature: Feature): Polygon[] {
  const { type, coordinates } = feature.geometry
  return type === 'Polygon' ? [coordinates as Polygon] : (coordinates as Polygon[])
}

/** 一个多边形最北的那条纬线 */
function north(poly: Polygon): number {
  let top = -Infinity
  for (const ring of poly) for (const [, y] of ring) if (y > top) top = y
  return top
}

function bounds(all: Polygon[]): Bounds {
  let x0 = Infinity
  let y0 = Infinity
  let x1 = -Infinity
  let y1 = -Infinity
  for (const poly of all) {
    for (const ring of poly) {
      for (const [x, y] of ring) {
        if (x < x0) x0 = x
        if (x > x1) x1 = x
        if (y < y0) y0 = y
        if (y > y1) y1 = y
      }
    }
  }
  return { x0, y0, x1, y1, w: x1 - x0, h: y1 - y0 }
}

/** 右下角那个框：贴住主图的右下角，框形与内容同比例，装进去才不空 */
function insetBox(frame: Bounds, content: Bounds): Bounds {
  const h = frame.h * BOX_HEIGHT
  const w = (content.w / content.h) * h
  const margin = CORNER * frame.w
  const x1 = frame.x1 - margin
  // 上下留白除以 aspectScale，渲染出来才和右边一样宽
  const y0 = frame.y0 + margin / ASPECT
  return { x0: x1 - w, x1, y0, y1: y0 + h, w, h }
}

/**
 * 把内容整体搬进小框：等比缩放、居中。
 * 纵横用同一个系数，形状才不会走样（ECharts 对经纬度是线性映射）。
 */
function relocate(all: Polygon[], from: Bounds, box: Bounds): Polygon[] {
  // 底下让出一行给「南海诸岛」，形状本身（含框线）就落在这一行上面
  const inner = {
    x0: box.x0 + box.w * BOX_PAD,
    x1: box.x1 - box.w * BOX_PAD,
    y0: box.y0 + box.h * BOX_LABEL,
    y1: box.y1 - box.h * BOX_PAD,
  }
  const k = Math.min((inner.x1 - inner.x0) / from.w, (inner.y1 - inner.y0) / from.h)
  const cx = (inner.x0 + inner.x1) / 2
  const cy = (inner.y0 + inner.y1) / 2
  const mx = (from.x0 + from.x1) / 2
  const my = (from.y0 + from.y1) / 2
  return all.map((poly) =>
    poly.map((ring) => ring.map(([x, y]) => [cx + (x - mx) * k, cy + (y - my) * k])),
  )
}

/** 框底那一行里给字占的小方块：字在里头居中，位置就跟着小框走 */
function labelPlate(box: Bounds): Ring {
  const x0 = box.x0 + box.w * 0.18
  const x1 = box.x1 - box.w * 0.18
  const y0 = box.y0 + box.h * BOX_LABEL * 0.15
  const y1 = box.y0 + box.h * BOX_LABEL * 0.85
  return [
    [x0, y0],
    [x1, y0],
    [x1, y1],
    [x0, y1],
    [x0, y0],
  ]
}

export function foldSouthSea(source: GeoJson): MapShape {
  const features: Feature[] = []
  const kept: Polygon[] = []
  let folded = 0

  for (const feature of source.features) {
    const name = feature.properties?.name ?? ''
    const all = polygons(feature)
    // 整个单位就是南海诸岛的（三沙），或者整块落在南边的岛礁 —— 都从主图上摘掉
    const mine = all.filter((poly) => !WHOLE.includes(name) && north(poly) >= SOUTH_CUT)
    folded += all.length - mine.length
    if (!mine.length) continue
    kept.push(...mine)
    features.push({
      ...feature,
      geometry:
        mine.length > 1
          ? { type: 'MultiPolygon', coordinates: mine }
          : { type: 'Polygon', coordinates: mine[0] },
    })
  }

  const frame = bounds(kept)
  const aspect = (frame.w / frame.h) * ASPECT * BREATH

  // 没有可折的（内陆各省就是这条路）：原样交回去
  if (!folded) return { geo: source, aspect, framed: false }

  const shape = bounds(NANHAI)
  const box = insetBox(frame, shape)
  features.push({
    type: 'Feature',
    properties: { name: SOUTH_SEA },
    // 形状里自带框线，整体等比缩进小框即可
    geometry: { type: 'MultiPolygon', coordinates: relocate(NANHAI, shape, box) },
  })
  features.push({
    type: 'Feature',
    properties: { name: SOUTH_SEA_LABEL },
    geometry: { type: 'Polygon', coordinates: [labelPlate(box)] },
  })

  return { geo: { ...source, features }, aspect, framed: true }
}
