import type {
  BackupFile,
  BackupSettings,
  CityMapResponse,
  GalleryQuery,
  GalleryResponse,
  GalleryTagStat,
  MediaItem,
  MediaKind,
  MomentDetail,
  MomentInput,
  MomentListQuery,
  MomentListResponse,
  ProvinceMapResponse,
  RestoreReport,
  SimilarItem,
  StatsResponse,
  TagItem,
} from './types'

const BASE = import.meta.env.VITE_API_BASE ?? ''

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

interface ErrorPayload {
  error?: { code?: string; message?: string }
}

/** 会话失效时回到登录页（已经在登录页就不动，避免循环） */
function redirectToLogin() {
  if (typeof window === 'undefined') return
  if (window.location.pathname.startsWith('/login')) return
  window.location.replace('/login')
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  let response: Response
  try {
    response = await fetch(`${BASE}${path}`, {
      ...init,
      headers: {
        ...(init.body instanceof FormData ? {} : { 'Content-Type': 'application/json' }),
        ...init.headers,
      },
    })
  } catch {
    throw new ApiError(0, 'NETWORK_ERROR', '连不上服务器，检查一下网络吧')
  }

  if (!response.ok) {
    let payload: ErrorPayload | null = null
    try {
      payload = (await response.json()) as ErrorPayload
    } catch {
      payload = null
    }

    if (response.status === 401 && !path.startsWith('/api/auth/')) {
      redirectToLogin()
    }

    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'UNKNOWN',
      payload?.error?.message ?? '出了点小状况，请稍后再试',
    )
  }

  if (response.status === 204) return undefined as T
  return (await response.json()) as T
}

function toQuery(params: MomentListQuery): string {
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== null && value !== '') {
      search.set(key, String(value))
    }
  }
  const qs = search.toString()
  return qs ? `?${qs}` : ''
}

export interface UploadHandle {
  promise: Promise<MediaItem>
  abort: () => void
}

interface UploadPayload {
  file: File
  thumb?: Blob | null
  width?: number | null
  height?: number | null
  durationMs?: number | null
}

/** 用 XHR 上传以便拿到真实进度（fetch 不支持上传进度） */
export function uploadMedia(
  kind: MediaKind,
  payload: UploadPayload,
  onProgress?: (ratio: number) => void,
): UploadHandle {
  const xhr = new XMLHttpRequest()

  const promise = new Promise<MediaItem>((resolve, reject) => {
    const form = new FormData()
    if (payload.width) form.append('width', String(Math.round(payload.width)))
    if (payload.height) form.append('height', String(Math.round(payload.height)))
    if (payload.durationMs) form.append('durationMs', String(Math.round(payload.durationMs)))
    if (payload.thumb) form.append('thumb', payload.thumb, 'thumb')
    form.append('file', payload.file, payload.file.name || 'upload')

    xhr.open('POST', `${BASE}/api/media/upload/${kind}`)
    xhr.responseType = 'json'

    xhr.upload.onprogress = (event) => {
      if (event.lengthComputable && onProgress) {
        onProgress(event.loaded / event.total)
      }
    }

    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        onProgress?.(1)
        resolve(xhr.response as MediaItem)
        return
      }
      if (xhr.status === 401) redirectToLogin()

      const body = xhr.response as ErrorPayload | null
      reject(
        new ApiError(
          xhr.status,
          body?.error?.code ?? 'UPLOAD_FAILED',
          body?.error?.message ?? '上传失败了，再试一次吧',
        ),
      )
    }

    xhr.onerror = () => reject(new ApiError(0, 'NETWORK_ERROR', '上传中断了，检查一下网络'))
    xhr.onabort = () => reject(new ApiError(0, 'ABORTED', '已取消上传'))

    xhr.send(form)
  })

  return { promise, abort: () => xhr.abort() }
}

export const api = {
  /** 用家庭口令换一个会话 Cookie（未设置口令的部署会直接返回成功） */
  login(code: string): Promise<{ ok: boolean }> {
    return request<{ ok: boolean }>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify({ code }),
    })
  },
  logout(): Promise<void> {
    return request<void>('/api/auth/logout', { method: 'POST' })
  },
  /** 公开接口：这个部署是否需要口令 */
  fetchStatus(): Promise<{ protected: boolean }> {
    return request<{ protected: boolean }>('/api/auth/status')
  },
  /** 当前会话概览（需要已登录） */
  fetchSession(): Promise<{ protected: boolean; sessionDays: number }> {
    return request<{ protected: boolean; sessionDays: number }>('/api/auth/session')
  },
  changeCode(currentCode: string, newCode: string): Promise<{ ok: boolean }> {
    return request<{ ok: boolean }>('/api/auth/change-code', {
      method: 'POST',
      body: JSON.stringify({ currentCode, newCode }),
    })
  },
  listMoments(params: MomentListQuery = {}): Promise<MomentListResponse> {
    return request<MomentListResponse>(`/api/moments${toQuery(params)}`)
  },
  getMoment(id: string): Promise<MomentDetail> {
    return request<MomentDetail>(`/api/moments/${id}`)
  },
  createMoment(input: MomentInput): Promise<MomentDetail> {
    return request<MomentDetail>('/api/moments', {
      method: 'POST',
      body: JSON.stringify(input),
    })
  },
  updateMoment(id: string, input: MomentInput): Promise<MomentDetail> {
    return request<MomentDetail>(`/api/moments/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(input),
    })
  },
  deleteMoment(id: string): Promise<void> {
    return request<void>(`/api/moments/${id}`, { method: 'DELETE' })
  },
  /** 拿分享凭证，已有就沿用——已经发出去的链接不该因为再点一次而失效 */
  shareMoment(id: string): Promise<{ token: string }> {
    return request<{ token: string }>(`/api/moments/${id}/share`, { method: 'POST' })
  },
  /** 收回分享，旧链接立刻作废 */
  unshareMoment(id: string): Promise<void> {
    return request<void>(`/api/moments/${id}/share`, { method: 'DELETE' })
  },
  /** 分享页读的只读内容。不需要登录：凭证就是钥匙 */
  getShared(token: string): Promise<MomentDetail> {
    return request<MomentDetail>(`/api/shared/${token}`)
  },
  listGallery(params: GalleryQuery = {}): Promise<GalleryResponse> {
    return request<GalleryResponse>(`/api/gallery${toQuery(params)}`)
  },
  galleryTags(): Promise<GalleryTagStat[]> {
    return request<GalleryTagStat[]>('/api/gallery/tags')
  },
  listTags(): Promise<TagItem[]> {
    return request<TagItem[]>('/api/tags')
  },
  stats(): Promise<StatsResponse> {
    return request<StatsResponse>('/api/moments/stats')
  },
  /** 问后端这个坐标属于哪个市（城市表在后端，前端不重复养一份） */
  cityAt(latitude: number, longitude: number): Promise<string | null> {
    return request<{ city: string | null }>(
      `/api/geo/city?latitude=${latitude}&longitude=${longitude}`,
    ).then((res) => res.city)
  },
  listBackups(): Promise<BackupFile[]> {
    return request<BackupFile[]>('/api/backup')
  },
  /** 找相似：拿这张图的 CLIP 向量和库里所有照片比一遍 */
  similarPhotos(id: string, limit = 12): Promise<SimilarItem[]> {
    return request<SimilarItem[]>(`/api/gallery/${id}/similar?limit=${limit}`)
  },
  /** 现打一份包 */
  createBackup(): Promise<BackupFile> {
    return request<BackupFile>('/api/backup', { method: 'POST' })
  },
  /**
   * 吃包进来。可以一次给多个（一个基准 + 若干增量），
   * 后端会按文件名里的时间戳从旧到新依次重放
   */
  restoreBackup(files: File[]): Promise<RestoreReport> {
    const form = new FormData()
    for (const file of files) form.append('file', file)
    return request<RestoreReport>('/api/backup/restore', { method: 'POST', body: form })
  },
  /** 点亮地图：一次拿到全国的点亮情况 */
  mapProvinces(): Promise<ProvinceMapResponse> {
    return request<ProvinceMapResponse>('/api/map/provinces')
  },
  /** 点亮地图：某个省下面点亮的市 */
  mapCities(province: number): Promise<CityMapResponse> {
    return request<CityMapResponse>(`/api/map/cities?province=${province}`)
  },
  backupSettings(): Promise<BackupSettings> {
    return request<BackupSettings>('/api/backup/settings')
  },
  /**
   * 改自动备份设置。改完即时生效，不用重启。
   * dir 留空表示回到默认位置；换目录时后端会把已有的包一起搬过去
   */
  saveBackupSettings(intervalDays: number, dir?: string): Promise<BackupSettings> {
    return request<BackupSettings>('/api/backup/settings', {
      method: 'PATCH',
      body: JSON.stringify({ intervalDays, dir }),
    })
  },
}
