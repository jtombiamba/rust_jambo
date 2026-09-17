const CACHE_KEY = 'app_config'
const TTL_MS = 60 * 60 * 1000

export interface ClientConfig {
  paypal_donate_url: string
  bot_thinking_delay_ms: number
  round_pause_delay_ms: number
}

interface CacheEntry {
  value: ClientConfig
  expiresAt: number
}

function isValidConfig(obj: unknown): obj is ClientConfig {
  if (typeof obj !== 'object' || obj === null) return false
  const c = obj as Record<string, unknown>
  return (
    typeof c.paypal_donate_url === 'string' &&
    typeof c.bot_thinking_delay_ms === 'number' &&
    typeof c.round_pause_delay_ms === 'number'
  )
}

export function getCachedConfig(): ClientConfig | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as CacheEntry
    if (
      typeof parsed !== 'object' ||
      parsed === null ||
      !isValidConfig(parsed.value) ||
      typeof parsed.expiresAt !== 'number'
    ) {
      localStorage.removeItem(CACHE_KEY)
      return null
    }
    if (Date.now() >= parsed.expiresAt) {
      localStorage.removeItem(CACHE_KEY)
      return null
    }
    return parsed.value
  } catch {
    localStorage.removeItem(CACHE_KEY)
    return null
  }
}

export function saveConfig(config: ClientConfig): void {
  const entry: CacheEntry = {
    value: config,
    expiresAt: Date.now() + TTL_MS,
  }
  localStorage.setItem(CACHE_KEY, JSON.stringify(entry))
}
