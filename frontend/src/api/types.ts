import type { AmbientId } from '@/audio/ambient'

export type MediaKind = 'image' | 'video' | 'audio'

export interface MediaItem {
  id: string
  kind: MediaKind
  role: string
  url: string
  thumbUrl: string | null
  mimeType: string
  width: number | null
  height: number | null
  durationMs: number | null
  sizeBytes: number
  position: number
}

/** 发生地点：名字、城市、坐标都可以缺；城市是地图点亮的粒度（一盏灯一座城） */
export interface Place {
  name: string
  city: string
  latitude: number | null
  longitude: number | null
}

export interface MomentSummary {
  id: string
  title: string
  excerpt: string
  happenedAt: string
  createdAt: string
  updatedAt: string
  tags: string[]
  cover: MediaItem | null
  mediaCount: number
  hasMusic: boolean
  place: Place | null
}

export interface MomentDetail {
  id: string
  title: string
  content: string
  happenedAt: string
  createdAt: string
  updatedAt: string
  tags: string[]
  media: MediaItem[]
  music: MediaItem | null
  /** 氛围音的名字（rain / night / fire / pad）。它与 music 只会有一个 */
  ambient: AmbientId | null
  place: Place | null
  /** 分享凭证；null 表示还没分享出去 */
  shareToken: string | null
}

export interface MomentListResponse {
  items: MomentSummary[]
  total: number
}

export interface StatsResponse {
  total: number
  firstHappenedAt: string | null
  lastHappenedAt: string | null
  tagCount: number
}

export interface TagItem {
  name: string
  count: number
}

export interface MediaLinkInput {
  id: string
  position?: number
}

export interface MomentInput {
  title?: string
  content?: string
  happenedAt?: string
  tags?: string[]
  media?: MediaLinkInput[]
  /** 氛围音：给名字就换上，给空串表示不要了，不给就不动 */
  ambient?: AmbientId | '' | null
  /** 发生地点：名字、城市、坐标都可以缺；带了坐标时后端会自动推断城市 */
  locationName?: string
  city?: string
  latitude?: number | null
  longitude?: number | null
}

export interface MomentListQuery {
  /** 关键词：标题、正文、地点名、城市都在它的射程内 */
  q?: string
  tags?: string
  from?: string
  to?: string
  order?: 'asc' | 'desc'
  limit?: number
  offset?: number
}

/** 找相似的结果：一张照片和它与原图的余弦相似度 */
export interface SimilarItem {
  id: string
  kind: string
  url: string
  thumbUrl: string | null
  width: number | null
  height: number | null
  momentId: string | null
  title: string
  happenedAt: string
  score: number
}

/** 一份备份包。kind 是 "base"（完整快照）或 "delta"（只带新增的媒体） */
export interface BackupFile {
  name: string
  sizeBytes: number
  createdAt: string
  kind: string
}

/** 导入结果：新写进去多少、跳过多少、重放掉几次删除 */
export interface RestoreReport {
  momentsAdded: number
  momentsSkipped: number
  mediaAdded: number
  mediaSkipped: number
  tagsAdded: number
  filesWritten: number
  filesSkipped: number
  deletionsApplied: number
  /** 没吃下的包，一个包一条说明 */
  failed: string[]
}

// ---------------------------------------------------------------- 点亮地图

/** 点亮的省 */
export interface ProvinceLit {
  name: string
  adcode: number
  /** 去过的城市数 */
  cities: number
  visits: number
  lastAt: string | null
}

export interface ProvinceMapResponse {
  provinces: ProvinceLit[]
  totalProvinces: number
  totalCities: number
  totalVisits: number
  /** 海外城市，地图上放不下 */
  overseas: string[]
  elsewhere: number
}

/** 点亮的市 */
export interface CityLit {
  /** 我存的名字（「杭州」） */
  name: string
  /** 官方名（「杭州市」），拿它和地图多边形对上 */
  mapName: string
  adcode: number
  visits: number
  firstAt: string | null
  lastAt: string | null
}

export interface CityMapResponse {
  province: number
  name: string
  cities: CityLit[]
}

/** 备份设置：间隔（天，0 表示关掉）与存放目录 */
export interface BackupSettings {
  intervalDays: number
  /** 当前的备份目录（绝对路径） */
  dir: string
  /** 默认位置，给「用默认」做参照 */
  defaultDir: string
}

/* ---------------- 图集 ---------------- */

/** 图片上自动识别出的标签 */
export interface GalleryTag {
  key: string
  name: string
  group: string
  score: number
}

export interface GalleryItem {
  id: string
  kind: 'image' | 'video'
  url: string
  thumbUrl: string | null
  width: number | null
  height: number | null
  momentId: string | null
  title: string
  happenedAt: string
  tags: GalleryTag[]
}

export interface GalleryResponse {
  items: GalleryItem[]
  total: number
  hasMore: boolean
}

export interface GalleryTagStat {
  key: string
  name: string
  group: string
  count: number
}

export interface GalleryQuery {
  /** 逗号分隔的标签 key，要求同时命中 */
  tags?: string
  /** 关键词：标题、正文、标签名 */
  q?: string
  limit?: number
  offset?: number
}
