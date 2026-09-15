import { describe, it, expect, beforeEach, vi } from 'vitest'
import { getCachedConfig, saveConfig, type ClientConfig } from './configCache'

const CACHE_KEY = 'app_config'

const validConfig: ClientConfig = {
  paypal_donate_url: 'https://www.paypal.com/donate',
  bot_thinking_delay_ms: 1500,
  round_pause_delay_ms: 2500,
}

describe('configCache', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.restoreAllMocks()
  })

  it('returns null when nothing is cached', () => {
    expect(getCachedConfig()).toBeNull()
  })

  it('round-trips a valid config', () => {
    saveConfig(validConfig)
    expect(getCachedConfig()).toEqual(validConfig)
  })

  it('returns null when the cache has expired', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2024-01-01T00:00:00Z'))

    saveConfig(validConfig)
    expect(getCachedConfig()).toEqual(validConfig)

    vi.advanceTimersByTime(60 * 60 * 1000)
    expect(getCachedConfig()).toBeNull()
    vi.useRealTimers()
  })

  it('removes and returns null for corrupt JSON', () => {
    localStorage.setItem(CACHE_KEY, 'not json')
    expect(getCachedConfig()).toBeNull()
    expect(localStorage.getItem(CACHE_KEY)).toBeNull()
  })

  it('removes and returns null for invalid fields', () => {
    localStorage.setItem(
      CACHE_KEY,
      JSON.stringify({
        value: { paypal_donate_url: 42, bot_thinking_delay_ms: 'x', round_pause_delay_ms: 'y' },
        expiresAt: Date.now() + 60 * 60 * 1000,
      }),
    )
    expect(getCachedConfig()).toBeNull()
    expect(localStorage.getItem(CACHE_KEY)).toBeNull()
  })

  it('removes and returns null when expiresAt is missing', () => {
    localStorage.setItem(CACHE_KEY, JSON.stringify({ value: validConfig }))
    expect(getCachedConfig()).toBeNull()
    expect(localStorage.getItem(CACHE_KEY)).toBeNull()
  })
})
