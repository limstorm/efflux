/**
 * 给每个标签一个稳定的色相：同一个标签永远同一个颜色，
 * 不同标签之间又有颜色层次，让纸面上不至于只有一种赭金。
 */
const TAG_HUES = [58, 34, 22, 118, 196, 248, 316, 348]

export function tagHue(name: string): number {
  let hash = 0
  for (let index = 0; index < name.length; index += 1) {
    hash = (hash * 131 + name.charCodeAt(index)) % 999983
  }
  return TAG_HUES[hash % TAG_HUES.length] ?? 58
}
