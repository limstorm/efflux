import { ref } from 'vue'

export interface ThemeDef {
  id: string
  name: string
  note: string
  /** 移动端状态栏 / 浏览器 UI 的颜色，与纸面对齐 */
  bar: string
}

export const THEMES: ThemeDef[] = [
  { id: 'xuan', name: '宣纸', note: '象牙 · 赭金', bar: '#f8f3ea' },
  { id: 'mist', name: '晨雾', note: '冷白 · 远山', bar: '#f3f6f8' },
  { id: 'dusk', name: '暮山', note: '暖褐 · 落日', bar: '#f7f0e5' },
  { id: 'moss', name: '苔痕', note: '灰绿 · 苔青', bar: '#f1f5ee' },
  { id: 'relic', name: '旧书', note: '泛黄 · 深棕', bar: '#f4ead6' },
  { id: 'maple', name: '红叶', note: '赭黄 · 枫红', bar: '#f5ecdb' },
  { id: 'sakura', name: '樱', note: '淡粉 · 桃红', bar: '#f9f0ef' },
  { id: 'ocean', name: '深海', note: '青蓝 · 海光', bar: '#eff4f6' },
  { id: 'night', name: '夜航', note: '深墨 · 月光', bar: '#1b1f26' },
  { id: 'starry', name: '星空', note: '夜空 · 星光', bar: '#191a2b' },
]

export const THEME_KEY = 'efflux.theme'
const DEFAULT_THEME = 'xuan'

function readSaved(): string {
  try {
    const saved = localStorage.getItem(THEME_KEY)
    if (saved && THEMES.some((theme) => theme.id === saved)) return saved
  } catch {
    /* 隐私模式下读不到，用默认主题 */
  }
  return DEFAULT_THEME
}

/** 全局单例：设置页换一次，所有引用它的地方一起走 */
const current = ref(readSaved())

function applyMeta(id: string) {
  const theme = THEMES.find((item) => item.id === id)
  if (theme) {
    document.querySelector('meta[name="theme-color"]')?.setAttribute('content', theme.bar)
  }
}

function setTheme(id: string) {
  if (!THEMES.some((theme) => theme.id === id)) return
  current.value = id

  const root = document.documentElement
  if (id === DEFAULT_THEME) root.removeAttribute('data-theme')
  else root.setAttribute('data-theme', id)

  applyMeta(id)

  try {
    localStorage.setItem(THEME_KEY, id)
  } catch {
    /* 存不下也不影响这一次使用 */
  }
}

/** 首屏由 index.html 的内联脚本先行设好，这里只是补一次状态栏颜色 */
function syncMeta() {
  applyMeta(current.value)
}

export function useTheme() {
  return { themeId: current, themes: THEMES, setTheme, syncMeta }
}
