import { ref } from 'vue'

export interface ToastItem {
  id: number
  message: string
  tone: 'error' | 'info' | 'success'
}

const items = ref<ToastItem[]>([])
let seq = 0

function push(message: string, tone: ToastItem['tone'], duration = 3600) {
  const id = ++seq
  items.value.push({ id, message, tone })
  window.setTimeout(() => dismiss(id), duration)
}

function dismiss(id: number) {
  const index = items.value.findIndex((item) => item.id === id)
  if (index >= 0) items.value.splice(index, 1)
}

export function useToast() {
  return {
    items,
    dismiss,
    error: (message: string) => push(message, 'error', 4200),
    info: (message: string) => push(message, 'info'),
    success: (message: string) => push(message, 'success'),
  }
}
