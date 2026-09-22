/**
 * 上传前在浏览器里生成缩略图与读取媒体元数据。
 * 好处：时间轴上的图片都是小图（加载快），后端不必依赖 ffmpeg。
 */

export interface PreparedMedia {
  thumb: Blob | null
  width: number | null
  height: number | null
  durationMs: number | null
}

const THUMB_MAX_SIDE = 1280
const THUMB_QUALITY = 0.82

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value))
}

function canvasToBlob(canvas: HTMLCanvasElement): Promise<Blob | null> {
  return new Promise((resolve) => {
    canvas.toBlob((blob) => resolve(blob), 'image/webp', THUMB_QUALITY)
  })
}

/** 图片：等比缩放到长边 1280，输出 webp */
export async function prepareImage(file: File): Promise<PreparedMedia> {
  try {
    const bitmap = await createImageBitmap(file)
    const { width, height } = bitmap
    const scale = clamp(THUMB_MAX_SIDE / Math.max(width, height), 0.05, 1)
    const targetW = Math.max(1, Math.round(width * scale))
    const targetH = Math.max(1, Math.round(height * scale))

    const canvas = document.createElement('canvas')
    canvas.width = targetW
    canvas.height = targetH
    const ctx = canvas.getContext('2d')
    if (!ctx) {
      bitmap.close()
      return { thumb: null, width, height, durationMs: null }
    }
    ctx.drawImage(bitmap, 0, 0, targetW, targetH)
    bitmap.close()

    const thumb = await canvasToBlob(canvas)
    return { thumb, width, height, durationMs: null }
  } catch {
    return { thumb: null, width: null, height: null, durationMs: null }
  }
}

/** 视频：抓取第一帧作为封面，并读取时长与尺寸 */
export async function prepareVideo(file: File): Promise<PreparedMedia> {
  const url = URL.createObjectURL(file)
  const video = document.createElement('video')
  video.preload = 'metadata'
  video.muted = true
  video.playsInline = true
  video.src = url

  const cleanup = () => URL.revokeObjectURL(url)

  try {
    const meta = await new Promise<{ duration: number; width: number; height: number }>(
      (resolve, reject) => {
        const timer = window.setTimeout(() => reject(new Error('读取视频超时')), 20_000)
        video.onloadedmetadata = () => {
          window.clearTimeout(timer)
          resolve({
            duration: Number.isFinite(video.duration) ? video.duration : 0,
            width: video.videoWidth,
            height: video.videoHeight,
          })
        }
        video.onerror = () => {
          window.clearTimeout(timer)
          reject(new Error('无法读取这个视频'))
        }
      },
    )

    const seekTo = meta.duration > 0 ? Math.min(1, meta.duration * 0.1) : 0
    await new Promise<void>((resolve) => {
      const timer = window.setTimeout(() => resolve(), 6000)
      video.onseeked = () => {
        window.clearTimeout(timer)
        resolve()
      }
      try {
        video.currentTime = seekTo
      } catch {
        window.clearTimeout(timer)
        resolve()
      }
    })

    const scale = clamp(THUMB_MAX_SIDE / Math.max(meta.width, meta.height), 0.05, 1)
    const canvas = document.createElement('canvas')
    canvas.width = Math.max(1, Math.round(meta.width * scale))
    canvas.height = Math.max(1, Math.round(meta.height * scale))
    const ctx = canvas.getContext('2d')
    let thumb: Blob | null = null
    if (ctx && meta.width > 0) {
      ctx.drawImage(video, 0, 0, canvas.width, canvas.height)
      thumb = await canvasToBlob(canvas)
    }

    return {
      thumb,
      width: meta.width || null,
      height: meta.height || null,
      durationMs: meta.duration ? Math.round(meta.duration * 1000) : null,
    }
  } catch {
    return { thumb: null, width: null, height: null, durationMs: null }
  } finally {
    cleanup()
  }
}

/** 音频：读取时长 */
export async function prepareAudio(file: File): Promise<PreparedMedia> {
  const url = URL.createObjectURL(file)
  const audio = document.createElement('audio')
  audio.preload = 'metadata'
  audio.src = url

  try {
    const duration = await new Promise<number>((resolve) => {
      const timer = window.setTimeout(() => resolve(0), 12_000)
      audio.onloadedmetadata = () => {
        window.clearTimeout(timer)
        resolve(Number.isFinite(audio.duration) ? audio.duration : 0)
      }
      audio.onerror = () => {
        window.clearTimeout(timer)
        resolve(0)
      }
    })
    return {
      thumb: null,
      width: null,
      height: null,
      durationMs: duration ? Math.round(duration * 1000) : null,
    }
  } finally {
    URL.revokeObjectURL(url)
  }
}

export function prepareByKind(file: File, kind: 'image' | 'video' | 'audio') {
  if (kind === 'image') return prepareImage(file)
  if (kind === 'video') return prepareVideo(file)
  return prepareAudio(file)
}

export function detectKind(file: File): 'image' | 'video' | 'audio' | null {
  if (file.type.startsWith('image/')) return 'image'
  if (file.type.startsWith('video/')) return 'video'
  if (file.type.startsWith('audio/')) return 'audio'
  return null
}
