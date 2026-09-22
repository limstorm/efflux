/**
 * 氛围音：现场合成，零文件。
 *
 * 为什么是合成而不是几首曲子——用户手上很难有音频文件，上传那一步对大多数
 * 人来说就是个死胡同。合成不需要任何素材：四个名字对应四段声音，打开记忆时
 * 按名字现搭一张声音图，关掉就拆干净。备份包不会变大，分享出去的链接在别人
 * 手机上也能响（声音是浏览器自己算出来的，不用下载）。
 *
 * ── 三条踩过的坑，改声音之前先看这里 ──
 *
 * 1. 别拿白噪声当底料。白噪频谱是平的，听着就是"嘶——"的电流声；真实世界的
 *    环境声几乎都是粉噪（每倍频程 -3dB）或棕噪（-6dB）。所以底料一律粉/棕，
 *    白噪只在最上面撒一点点当颗粒。
 * 2. 别用陡滤波器切瞬态。高 Q 的带通接上一个方波似的包络，振铃听着就是"呲呲"
 *    的杂音。爆裂类一律低 Q（≤1.5）+ 宽一点。
 * 3. 包络别从 0 起跳。4ms 的起音就是"啪"的一声爆音；用 8~12ms 起音，
 *    衰减走 setTargetAtTime（指数、天然无爆音），别用 exponentialRamp。
 *
 * 参数是听出来的，不是推出来的。改之前戴上耳机听一遍，四条都要"安静到能当
 * 背景"，而不是"好听到让人去听它"。
 */

export type AmbientId = 'rain' | 'night' | 'fire' | 'pad'

export interface AmbientOption {
  id: AmbientId
  name: string
  /** 一句话说清是什么声音，给选择器当副标题 */
  hint: string
}

export const AMBIENT_OPTIONS: readonly AmbientOption[] = [
  { id: 'rain', name: '细雨', hint: '一层绵密的雨，落在远处' },
  { id: 'night', name: '夜虫', hint: '夏夜，虫声一唱一和' },
  { id: 'fire', name: '炉火', hint: '柴火噼啪，偶尔塌一声' },
  { id: 'pad', name: '低频', hint: '一层缓慢的共鸣，几乎没有旋律' },
]

/**
 * 四条各自的响度。master 只淡这一条线，内部各声部再自己配比。
 *
 * 数值是量出来的：棕噪的低频能量大，手机上根本放不出来，所以光看均方根
 * 会低估"听起来有多响"——配比要往中频倾斜，四条才在手机喇叭上一样轻重。
 */
const LEVELS: Record<AmbientId, number> = {
  rain: 0.18,
  night: 0.42,
  fire: 0.26,
  pad: 0.18,
}

const FADE_IN = 1.4
const FADE_OUT = 0.6

export function isAmbientId(value: unknown): value is AmbientId {
  return typeof value === 'string' && AMBIENT_OPTIONS.some((option) => option.id === value)
}

/** 把 id 换成人看的中文名；认不出来就给 null，界面上当"没有氛围"处理 */
export function ambientName(id: string | null | undefined): string | null {
  return AMBIENT_OPTIONS.find((option) => option.id === id)?.name ?? null
}

// ---------------------------------------------------------------- 引擎

interface Session {
  /** 立刻拆掉：清定时器、停源、断开。漏一个就会在后台一直响 */
  stop: () => void
}

let ctx: AudioContext | null = null
let master: GainNode | null = null
let current: Session | null = null
let currentId: AmbientId | null = null

function getContext(): AudioContext | null {
  if (ctx) return ctx
  type Ctor = typeof AudioContext
  const scope = window as unknown as { webkitAudioContext?: Ctor }
  const Context = window.AudioContext ?? scope.webkitAudioContext
  if (!Context) return null // 太老的浏览器：安静地不放，别把页面拖垮
  ctx = new Context()
  return ctx
}

/** 总线：四条都汇到这里，停的时候只淡它一条 */
function ensureMaster(context: AudioContext): GainNode {
  if (master) return master
  master = context.createGain()
  master.gain.value = 0
  // 两端各切一刀：底下滤掉 30Hz 以下的轰隆（那是"闷响"，不是气氛），
  // 上面滤掉 11kHz 以上的"咝"（合成噪声最容易在这里露馅）
  const rumble = band(context, 'highpass', 30, 0.5)
  const sheen = band(context, 'lowpass', 11000, 0.5)
  master.connect(rumble).connect(sheen).connect(context.destination)
  return master
}

/**
 * 播一段氛围音。返回它是否真的响起来了：浏览器的自动播放策略会让
 * 没有用户手势的第一次尝试落空，这时调用方该把按钮画成"暂停"。
 */
export async function playAmbient(id: AmbientId): Promise<boolean> {
  const context = getContext()
  if (!context) return false

  stopAmbient() // 换一段先把旧的收掉，不做交叉淡入：两种气氛叠在一起只会糊
  const bus = ensureMaster(context)
  const now = context.currentTime
  bus.gain.cancelScheduledValues(now)
  bus.gain.setValueAtTime(0, now)

  if (context.state === 'suspended') {
    try {
      await context.resume()
    } catch {
      // 交回给返回值判断
    }
  }

  // 搭图万一出错（某个源启不来），宁可退回"没在响"，也不要留下一个
  // 画着播放键却一片死寂的状态
  try {
    current = BUILDERS[id](context, bus)
  } catch {
    return false
  }
  currentId = id
  bus.gain.linearRampToValueAtTime(LEVELS[id], context.currentTime + FADE_IN)
  return context.state === 'running'
}

/** 停：先淡出再拆图。留 0.6 秒的余韵，别"咔"一下掐断 */
export function stopAmbient(): void {
  const context = ctx
  const session = current
  if (!context || !session) return
  current = null
  currentId = null

  const now = context.currentTime
  const bus = master
  if (bus) {
    bus.gain.cancelScheduledValues(now)
    bus.gain.setValueAtTime(bus.gain.value, now)
    bus.gain.linearRampToValueAtTime(0, now + FADE_OUT)
  }

  window.setTimeout(() => {
    session.stop()
    // 没人听的时候把音频线程挂起：手机上省电比"随时待命"重要
    if (!current && context.state === 'running') void context.suspend()
  }, FADE_OUT * 1000 + 80)
}

export function isAmbientPlaying(): boolean {
  return current !== null
}

export function playingAmbientId(): AmbientId | null {
  return currentId
}

/** 氛围音的总线。想接自己的音量表或录音，从这里分出去 */
export function ambientBus(): GainNode | null {
  return master
}

// ---------------------------------------------------------------- 底料

type NoiseKind = 'white' | 'pink' | 'brown'

/**
 * 噪声底料。
 *
 * 粉噪（-3dB/倍频程）和棕噪（-6dB/倍频程）才是自然环境声的样子；
 * 白噪只在最上面当颗粒用。末尾交叉淡化进开头——不然每循环一次就"咔"一声，
 * 那种偶尔一下的轻响最容易被听成"有杂音"。
 */
function noiseBuffer(context: AudioContext, kind: NoiseKind, seconds = 4): AudioBuffer {
  const fade = Math.floor(context.sampleRate * 0.05)
  const length = Math.floor(context.sampleRate * seconds)
  const raw = new Float32Array(length + fade)

  let b0 = 0
  let b1 = 0
  let b2 = 0
  let brown = 0
  for (let i = 0; i < raw.length; i += 1) {
    const white = Math.random() * 2 - 1
    if (kind === 'white') {
      raw[i] = white
    } else if (kind === 'pink') {
      // Kellet 的三极近似：足够像粉噪，又不用做 FFT
      b0 = 0.99765 * b0 + white * 0.099046
      b1 = 0.963 * b1 + white * 0.2965164
      b2 = 0.57 * b2 + white * 1.0526913
      raw[i] = (b0 + b1 + b2 + white * 0.1848) * 0.22
    } else {
      // 漏积分器：白噪积起来就是棕噪，低频厚重
      brown = (brown + 0.02 * white) / 1.02
      raw[i] = brown * 3.2
    }
  }

  for (let i = 0; i < fade; i += 1) {
    const k = i / fade
    raw[i] = raw[i] * k + raw[length + i] * (1 - k)
  }

  const buffer = context.createBuffer(1, length, context.sampleRate)
  buffer.getChannelData(0).set(raw.subarray(0, length))
  return buffer
}

function noise(context: AudioContext, kind: NoiseKind): AudioBufferSourceNode {
  const source = context.createBufferSource()
  source.buffer = noiseBuffer(context, kind)
  source.loop = true
  return source
}

/** 一段一次性用的短噪声，给爆裂类当素材 */
function grainBuffer(context: AudioContext): AudioBuffer {
  return noiseBuffer(context, 'white', 0.2)
}

// ---------------------------------------------------------------- 零件

function gain(context: AudioContext, value: number): GainNode {
  const node = context.createGain()
  node.gain.value = value
  return node
}

/** 统一入口：Q 默认压得很低，陡滤波器接瞬态会振铃，听着就是杂音 */
function band(
  context: AudioContext,
  type: BiquadFilterType,
  frequency: number,
  q = 1,
): BiquadFilterNode {
  const node = context.createBiquadFilter()
  node.type = type
  node.frequency.value = frequency
  node.Q.value = q
  return node
}

/** 左右位置：一声虫鸣在左、一声在右，听起来才像一片野地，而不是一个喇叭 */
function panner(context: AudioContext, pan: number): AudioNode {
  if (typeof context.createStereoPanner !== 'function') return gain(context, 1)
  const node = context.createStereoPanner()
  node.pan.value = pan
  return node
}

/** 一条很慢的正弦，用来推音量或滤波——"活着"的感觉全靠它 */
function lfo(
  context: AudioContext,
  frequency: number,
  depth: number,
  target: AudioParam,
): OscillatorNode {
  const osc = context.createOscillator()
  osc.type = 'sine'
  osc.frequency.value = frequency
  const amount = gain(context, depth)
  osc.connect(amount).connect(target)
  return osc
}

/**
 * 一次"点"的包络：慢起音，衰减走 setTargetAtTime。
 *
 * 起音快了就是"啪"的一声爆音（旧版就是 4ms，正是杂音的来源）；
 * exponentialRamp 从 0 起步同样会响一下，所以这里不用它。
 * 每次"点"都该有自己的增益节点——共用一个节点会互相踩。
 */
function ping(param: AudioParam, at: number, level: number, attack: number, release: number) {
  param.value = 0
  param.linearRampToValueAtTime(level, at + attack)
  param.setTargetAtTime(0, at + attack, Math.max(0.008, release / 3))
}

/** 一次性节点用完自己断开，长时间开着也不会越积越多 */
function selfClean(...nodes: AudioNode[]) {
  return (source: AudioScheduledSourceNode) => {
    source.onended = () => {
      try {
        source.disconnect()
      } catch {
        // 已经断了
      }
      nodes.forEach((node) => node.disconnect())
    }
  }
}

// ---------------------------------------------------------------- 四条

type Builder = (context: AudioContext, out: GainNode) => Session

/** 细雨：三层雨幕（身体 / 沙沙 / 窗上的近雨），雨势慢慢起落，偶尔几滴 */
const buildRain: Builder = (context, out) => {
  const sources: AudioScheduledSourceNode[] = []
  const nodes: AudioNode[] = []

  // 雨的"身体"：棕噪压低。手机喇叭听不见，戴耳机时全靠它撑厚度
  const body = noise(context, 'brown')
  const bodyAmp = gain(context, 0.32)
  const bodyFilter = band(context, 'lowpass', 900, 0.6)
  body.connect(bodyFilter).connect(bodyAmp).connect(out)

  // 雨幕：粉噪走一段很宽的中频，就是那片"沙沙"。它和上面那层一起，
  // 才是手机上真正听得见的部分，所以配比给得比低频还重
  const sheet = noise(context, 'pink')
  const sheetAmp = gain(context, 0.6)
  sheet.connect(band(context, 'bandpass', 1500, 0.45)).connect(sheetAmp).connect(out)

  // 窗上的近雨：白噪只留一点点，多了就成了电流声
  const patter = noise(context, 'white')
  const patterAmp = gain(context, 0.16)
  patter.connect(band(context, 'highpass', 3200, 0.5)).connect(patterAmp).connect(out)

  // 雨势：两条极慢的 LFO 各推一层，别让整片一起呼吸（那像有人在拧音量）
  const tideSlow = lfo(context, 0.023, 0.18, bodyAmp.gain)
  const tideFast = lfo(context, 0.041, 0.13, sheetAmp.gain)

  const grain = grainBuffer(context)

  /** 一滴雨：一小段宽带噪声，软起音；偶尔是一记"叮"（打在窗檐上的那种） */
  const drop = (at: number) => {
    const dur = 0.05 + Math.random() * 0.07
    const source = context.createBufferSource()
    source.buffer = grain
    const amp = gain(context, 0)
    const where = panner(context, (Math.random() - 0.5) * 1.4)
    const color = band(context, 'bandpass', 900 + Math.random() * 1600, 1.1)
    source.connect(color).connect(amp).connect(where)
    where.connect(out)
    // 从颗粒里随机取一段：每滴都用开头那几个采样点，听起来会像在打拍子
    source.start(at, Math.random() * 0.14)
    source.stop(at + dur + 0.05)
    ping(amp.gain, at, 0.5 + Math.random() * 0.4, 0.009, dur)
    selfClean(amp, where, color)(source)

    // 十滴里有一滴落在硬东西上，会有一点点音高
    if (Math.random() < 0.12) {
      const ring = context.createOscillator()
      ring.type = 'sine'
      ring.frequency.value = 900 + Math.random() * 1400
      const ringAmp = gain(context, 0)
      const ringPan = panner(context, (Math.random() - 0.5) * 0.9)
      ring.connect(ringAmp).connect(ringPan)
      ringPan.connect(out)
      ring.start(at)
      ring.stop(at + 0.3)
      ping(ringAmp.gain, at, 0.1 + Math.random() * 0.08, 0.008, 0.13)
      selfClean(ringAmp, ringPan, ring)(ring)
    }
  }

  const timer = window.setInterval(() => {
    let at = context.currentTime + 0.05 + Math.random() * 0.5
    const cluster = Math.random() < 0.3 ? 2 + Math.floor(Math.random() * 2) : 1
    for (let i = 0; i < cluster; i += 1) {
      drop(at)
      at += 0.06 + Math.random() * 0.2
    }
  }, 1100)

  sources.push(body, sheet, patter, tideSlow, tideFast)
  sources.forEach((source) => source.start())
  nodes.push(body, sheet, patter, bodyAmp, sheetAmp, patterAmp, bodyFilter, tideSlow, tideFast)

  return {
    stop: () => {
      window.clearInterval(timer)
      sources.forEach((source) => {
        try {
          source.stop()
        } catch {
          // 已经停了
        }
      })
      nodes.forEach((node) => node.disconnect())
    },
  }
}

/**
 * 一声虫鸣。
 *
 * 关键是"毛刺感"：昆虫的鸣声不是一个音，是一串每秒七八十下的极短脉冲。
 * 所以用一个带谐波的锯齿波做载波，再用 70Hz 上下的方波把音量切碎——
 * 纯正弦出来的是电子哔声，一点都不像。
 */
function chirp(
  context: AudioContext,
  out: AudioNode,
  frequency: number,
  level: number,
  pan: number,
  at: number,
) {
  const osc = context.createOscillator()
  osc.type = 'sawtooth'
  osc.frequency.value = frequency * (0.97 + Math.random() * 0.06)

  // 两级增益：一级被方波切碎（毛刺），一级是整声的包络，互不干扰
  const rasp = gain(context, 0.5)
  const shape = gain(context, 0)
  const where = panner(context, pan)
  osc.connect(band(context, 'bandpass', frequency, 5)).connect(rasp).connect(shape).connect(where)
  where.connect(out)

  const trill = context.createOscillator()
  trill.type = 'square'
  trill.frequency.value = 68 + Math.random() * 34
  const depth = gain(context, 0.5)
  trill.connect(depth).connect(rasp.gain)

  const duration = 0.1 + Math.random() * 0.12
  ping(shape.gain, at, level, 0.012, duration)

  osc.start(at)
  osc.stop(at + duration + 0.2)
  trill.start(at)
  trill.stop(at + duration + 0.2)
  selfClean(rasp, shape, where, depth)(osc)
}

/** 夜虫：夜气铺底，几只虫子散在左右两边各按自己的节奏唱和 */
const buildNight: Builder = (context, out) => {
  const sources: AudioScheduledSourceNode[] = []
  const nodes: AudioNode[] = []

  // 夜气：棕噪压低，很轻的一层，负责"空旷"
  const air = noise(context, 'brown')
  const airAmp = gain(context, 0.22)
  air.connect(band(context, 'lowpass', 700, 0.5)).connect(airAmp).connect(out)

  // 极远的地方有点低沉的底噪，像田野的呼吸。这条在手机上是听不见的，
  // 所以给得很轻——它只负责在耳机里把那点"空旷"垫出来
  const far = noise(context, 'brown')
  const farAmp = gain(context, 0.05)
  far.connect(band(context, 'lowpass', 140, 0.6)).connect(farAmp).connect(out)

  // 三只：近处一只、两侧各一只，音高与周期都错开，才不是一个人在唱
  const voices = [
    { frequency: 4300, level: 0.24, period: 0.54, jitter: 0.05, pan: -0.35 },
    { frequency: 3700, level: 0.15, period: 0.63, jitter: 0.09, pan: 0.45 },
    { frequency: 5000, level: 0.09, period: 0.81, jitter: 0.12, pan: 0.7 },
  ]
  const nextAt = voices.map((_, index) => context.currentTime + 0.4 + index * 0.9)

  const timer = window.setInterval(() => {
    const now = context.currentTime
    voices.forEach((voice, index) => {
      // 提前 1.2 秒排好，定时器抖动就听不出来
      while (nextAt[index] < now + 1.2) {
        chirp(
          context,
          out,
          voice.frequency,
          voice.level,
          voice.pan,
          Math.max(nextAt[index], now + 0.02),
        )
        nextAt[index] += voice.period * (1 + (Math.random() * 2 - 1) * voice.jitter)
      }
    })
  }, 400)

  sources.push(air, far)
  sources.forEach((source) => source.start())
  nodes.push(air, far, airAmp, farAmp)

  return {
    stop: () => {
      window.clearInterval(timer)
      sources.forEach((source) => {
        try {
          source.stop()
        } catch {
          // 已经停了
        }
      })
      nodes.forEach((node) => node.disconnect())
    },
  }
}

/** 炉火：棕噪的"呼——"打底，上面撒成簇的软爆裂，偶尔塌一声 */
const buildFire: Builder = (context, out) => {
  const sources: AudioScheduledSourceNode[] = []
  const nodes: AudioNode[] = []

  const bed = noise(context, 'brown')
  const bedAmp = gain(context, 0.38)
  bed.connect(band(context, 'lowpass', 900, 0.6)).connect(bedAmp).connect(out)

  // 柴在烧的时候有一层细细的"呲"——只有低频的话听着是闷的，
  // 实测这条中频层一加，频谱倾斜才从 -9dB/倍频程回到自然的一档
  const sizzle = noise(context, 'pink')
  const sizzleAmp = gain(context, 0.12)
  sizzle.connect(band(context, 'bandpass', 2200, 0.5)).connect(sizzleAmp).connect(out)

  // 火不是一直烧得一样旺：很慢的呼吸
  const breath = lfo(context, 0.07, 0.16, bedAmp.gain)
  const flicker = lfo(context, 0.11, 0.05, sizzleAmp.gain)

  const grain = grainBuffer(context)

  /** 一记噼啪：宽带噪声，低 Q，软起音——是要"啵"的一声，不是"咔" */
  const crackle = (at: number) => {
    const dur = 0.03 + Math.random() * 0.06
    const source = context.createBufferSource()
    source.buffer = grain
    const amp = gain(context, 0)
    const where = panner(context, (Math.random() - 0.5) * 1.1)
    const color = band(context, 'bandpass', 1100 + Math.random() * 1800, 1.2)
    source.connect(color).connect(amp).connect(where)
    where.connect(out)
    // 随机取段，否则每一记噼啪都是同一段噪声，听着像采样循环
    source.start(at, Math.random() * 0.14)
    source.stop(at + dur + 0.05)
    ping(amp.gain, at, 0.3 + Math.random() * 0.35, 0.01, dur)
    selfClean(amp, where, color)(source)
  }

  /** 塌一声：一块柴落下去，是一团低频的闷响，不是低音鼓 */
  const collapse = (at: number) => {
    const source = context.createBufferSource()
    source.buffer = grain
    const amp = gain(context, 0)
    const where = panner(context, (Math.random() - 0.5) * 0.6)
    const color = band(context, 'lowpass', 220, 0.7)
    source.connect(color).connect(amp).connect(where)
    where.connect(out)
    source.start(at, Math.random() * 0.14)
    source.stop(at + 0.5)
    ping(amp.gain, at, 0.32 + Math.random() * 0.12, 0.02, 0.3)
    selfClean(amp, where, color)(source)
  }

  // 噼啪成簇出现——均匀地一声一声响，听起来像打字机
  const timer = window.setInterval(() => {
    let at = context.currentTime + 0.05 + Math.random() * 0.5
    const cluster = 1 + Math.floor(Math.random() * 3)
    for (let i = 0; i < cluster; i += 1) {
      crackle(at)
      at += 0.06 + Math.random() * 0.22
    }
    if (Math.random() < 0.16) collapse(at + 0.1)
  }, 1400)

  sources.push(bed, sizzle, breath, flicker)
  sources.forEach((source) => source.start())
  nodes.push(bed, sizzle, bedAmp, sizzleAmp, breath, flicker)

  return {
    stop: () => {
      window.clearInterval(timer)
      sources.forEach((source) => {
        try {
          source.stop()
        } catch {
          // 已经停了
        }
      })
      nodes.forEach((node) => node.disconnect())
    },
  }
}

/** 低频：只开五度的和弦。大三度会听起来"高兴"或"难过"，五度不会 */
const buildPad: Builder = (context, out) => {
  const nodes: AudioNode[] = []
  const sources: AudioScheduledSourceNode[] = []

  // 截止给到 1.2kHz：太低的话整条只剩 60~200Hz，手机喇叭上等于没放
  const filterNode = band(context, 'lowpass', 1200, 0.85)
  const amp = gain(context, 0.8)
  filterNode.connect(amp).connect(out)

  // 滤波与音量各被一条很慢的 LFO 推着走，像呼吸
  const swell = lfo(context, 0.035, 0.24, amp.gain)
  const sweep = lfo(context, 0.017, 420, filterNode.frequency)

  // 一层很轻的气声垫在底下，免得 pad 听起来像纯电子音；
  // 走中频而不是低频，才是手机上也听得见的那一点"空气"
  const air = noise(context, 'brown')
  const airAmp = gain(context, 0.18)
  air.connect(band(context, 'bandpass', 700, 0.6)).connect(airAmp).connect(out)

  const voices: AudioNode[] = []
  for (const frequency of [110, 164.81, 220, 329.63]) {
    // 每个音两个角，各自微微跑调，凑出"合唱"的厚度
    for (const detune of [-4, 4]) {
      const osc = context.createOscillator()
      osc.type = 'triangle'
      osc.frequency.value = frequency
      osc.detune.value = detune
      const voice = gain(context, 0.12)
      osc.connect(voice).connect(filterNode)
      sources.push(osc)
      voices.push(voice)
    }
  }

  sources.push(swell, sweep, air)
  sources.forEach((source) => source.start())
  nodes.push(...voices, filterNode, amp, airAmp)

  return {
    stop: () => {
      sources.forEach((source) => {
        try {
          source.stop()
        } catch {
          // 已经停了
        }
      })
      nodes.forEach((node) => node.disconnect())
    },
  }
}

const BUILDERS: Record<AmbientId, Builder> = {
  rain: buildRain,
  night: buildNight,
  fire: buildFire,
  pad: buildPad,
}
