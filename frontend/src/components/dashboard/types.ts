export interface ProfileData {
  credit: number
  game_played: number
  wins: number
  kora_wins: number
  frozen_until: string | null
  cashout_locked: boolean
}

export interface CashoutConfig {
  enabled: boolean
  minCredits: number
  creditsPerEur: number
  maxEurCents: number
}

export interface CashoutHistoryItem {
  id: string
  credits: number
  amount_eur_cents: number
  status: string
  payout_method: string
  payout_destination: string
  created_at: string
  processed_at: string | null
}

export interface CashoutHistoryData {
  items: CashoutHistoryItem[]
  total: number
  page: number
  per_page: number
}

export interface GameItem {
  game_id: string
  status: string
  bet: number
  result: string
  played_at: string
  player_count: number
}

export interface GameHistoryData {
  games: GameItem[]
  total: number
  page: number
  per_page: number
}

export interface QuickGameResponse {
  game_id: string
  players: Array<{
    id: string
    type: 'human' | 'bot'
    name: string
    position: number
    display_position: number
    cards: number[]
    cards_count: number
  }>
  status: string
  current_turn: number
  bet: number
  max_players: number
  deck_slots?: (number | null)[]
}

export type SortField = 'date' | 'bet' | null
export type SortDir = 'asc' | 'desc'
