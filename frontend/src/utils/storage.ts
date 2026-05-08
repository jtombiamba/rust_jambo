const STORAGE_KEY = 'anonymous_stats'

export interface AnonymousStats {
  games_allowed: number
  games_played: number
  total_wins: number
  credits: number
}

/**
 * Validate that a parsed object has all required AnonymousStats fields
 * with proper types (number, not null/undefined/string).
 */
function isValidStats(obj: unknown): obj is AnonymousStats {
  if (typeof obj !== 'object' || obj === null) return false
  const s = obj as Record<string, unknown>
  return (
    typeof s.games_allowed === 'number' &&
    typeof s.games_played === 'number' &&
    typeof s.total_wins === 'number' &&
    typeof s.credits === 'number'
  )
}

/**
 * Read stats from localStorage.
 * Returns null if the data is missing, corrupted, or has invalid fields
 * (e.g. old camelCase format where snake_case fields are undefined).
 * The caller (App.tsx) will fall through to the backend API when null is returned.
 */
export function getStoredStats(): AnonymousStats | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw)
    if (!isValidStats(parsed)) {
      // Data is stale/corrupted — remove it so the caller fetches fresh data from backend
      localStorage.removeItem(STORAGE_KEY)
      return null
    }
    return parsed
  } catch {
    localStorage.removeItem(STORAGE_KEY)
    return null
  }
}

export function saveStats(stats: AnonymousStats): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(stats))
}

export function updateAnonymousStatsAfterGame(
  bet: number,
  won: boolean,
  status: 'finished' | 'kora' | 'doubleKora',
): void {
  const stats = getStoredStats()
  if (!stats) return

  const multiplier =
    status === 'doubleKora' ? 4 : status === 'kora' ? 2 : 1

  stats.games_played += 1
  if (won) {
    stats.total_wins += 1
    stats.credits += bet * 3 * multiplier
  } else {
    stats.credits -= bet * multiplier
  }

  saveStats(stats)
}
