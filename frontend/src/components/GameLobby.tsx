import { useEffect, useState, useRef, useCallback } from 'react'
import axios from 'axios'
import { useAuthStore } from '../stores/useAuthStore'

interface LobbyPlayer {
  pseudo: string
  position: number
  isCurrentUser: boolean
}

interface UserSearchItem {
  id: string
  pseudo: string
}

interface Props {
  gameId: string
  onBack: () => void
  onGameStart: (data: Record<string, unknown>) => void
}

export default function GameLobby({ gameId, onBack, onGameStart }: Props) {
  const { user } = useAuthStore()
  const [players, setPlayers] = useState<LobbyPlayer[]>([])
  const [status, setStatus] = useState<string>('pending')
  const [bet, setBet] = useState(0)
  const [maxPlayers, setMaxPlayers] = useState(4)
  const [expiresAt, setExpiresAt] = useState<string | null>(null)
  const [timeLeft, setTimeLeft] = useState<string>('')
  const [isCreator, setIsCreator] = useState(false)
  const [starting, setStarting] = useState(false)
  const [toast, setToast] = useState<string | null>(null)

  const [slotPseudos, setSlotPseudos] = useState<Record<number, string>>({})
  const [slotSearchResults, setSlotSearchResults] = useState<Record<number, UserSearchItem[]>>({})
  const [slotSearching, setSlotSearching] = useState<Record<number, boolean>>({})
  const [slotInviting, setSlotInviting] = useState<Record<number, boolean>>({})
  const [activeSlot, setActiveSlot] = useState<number | null>(null)
  const searchTimers = useRef<Record<number, ReturnType<typeof setTimeout>>>({})
  const pollTimer = useRef<ReturnType<typeof setInterval>>()

  const showToast = (msg: string) => {
    setToast(msg)
    setTimeout(() => setToast(null), 3000)
  }

  const refreshLobby = useCallback(async () => {
    try {
      const res = await axios.get(`/api/me/games/${gameId}`)
      const data = res.data
      if (data.status === 'cancelled') {
        showToast('Game has been cancelled.')
        onBack()
        return
      }
      setStatus(data.status)
      setBet(data.bet ?? 0)
      setMaxPlayers(data.max_players ?? data.players?.length ?? 4)

      const lobbyPlayers: LobbyPlayer[] = (data.players || []).map((p: { name: string; position: number }) => ({
        pseudo: p.name,
        position: p.position,
        isCurrentUser: true,
      }))
      setPlayers(lobbyPlayers)
      setIsCreator(lobbyPlayers.some(p => p.position === 0 && p.isCurrentUser))

      const expiresHeader = data.invite_expires_at
      if (expiresHeader) setExpiresAt(expiresHeader)
    } catch (err) {
      console.error('Failed to refresh lobby', err)
    }
  }, [gameId, onBack])

  useEffect(() => {
    refreshLobby()
    pollTimer.current = setInterval(refreshLobby, 5000)
    const searchTimersRef = searchTimers.current
    return () => {
      if (pollTimer.current) clearInterval(pollTimer.current)
      Object.values(searchTimersRef).forEach(clearTimeout)
    }
  }, [refreshLobby])

  useEffect(() => {
    if (!expiresAt) {
      setTimeLeft('')
      return
    }
    const updateTimer = () => {
      const now = Date.now()
      const expiry = new Date(expiresAt).getTime()
      const diff = expiry - now
      if (diff <= 0) {
        setTimeLeft('Expired')
        return
      }
      const mins = Math.floor(diff / 60000)
      const secs = Math.floor((diff % 60000) / 1000)
      setTimeLeft(`${mins}:${secs.toString().padStart(2, '0')}`)
    }
    updateTimer()
    const timer = setInterval(updateTimer, 1000)
    return () => clearInterval(timer)
  }, [expiresAt])

  const handleSlotSearch = (slot: number, query: string) => {
    const val = query.startsWith('@') ? query.slice(1) : query
    setSlotPseudos(prev => ({ ...prev, [slot]: query }))

    if (searchTimers.current[slot]) clearTimeout(searchTimers.current[slot])

    if (val.length < 2) {
      setSlotSearchResults(prev => ({ ...prev, [slot]: [] }))
      return
    }

    setSlotSearching(prev => ({ ...prev, [slot]: true }))
    searchTimers.current[slot] = setTimeout(async () => {
      try {
        const res = await axios.get<{ users: UserSearchItem[] }>('/api/users/search', {
          params: { q: val, limit: 5 },
        })
        setSlotSearchResults(prev => ({
          ...prev,
          [slot]: res.data.users.filter(u => u.id !== user?.id),
        }))
      } catch {
        setSlotSearchResults(prev => ({ ...prev, [slot]: [] }))
      } finally {
        setSlotSearching(prev => ({ ...prev, [slot]: false }))
      }
    }, 300)
  }

  const handleSlotSelect = async (slot: number, userItem: UserSearchItem) => {
    setSlotInviting(prev => ({ ...prev, [slot]: true }))
    setActiveSlot(null)
    setSlotSearchResults(prev => ({ ...prev, [slot]: [] }))
    try {
      await axios.post(`/api/games/${gameId}/invites`, { user_ids: [userItem.id] })
      showToast(`Invited ${userItem.pseudo}`)
      setSlotPseudos(prev => ({ ...prev, [slot]: `@${userItem.pseudo}` }))
    } catch (err: unknown) {
      showToast((err as { response?: { data?: { error?: string } } }).response?.data?.error || 'Failed to send invite')
    } finally {
      setSlotInviting(prev => ({ ...prev, [slot]: false }))
    }
  }

  const handleStart = async () => {
    setStarting(true)
    try {
      const res = await axios.post(`/api/games/${gameId}/start`)
      onGameStart(res.data)
    } catch (err: unknown) {
      showToast((err as { response?: { data?: { error?: string } } }).response?.data?.error || 'Failed to start game')
    } finally {
      setStarting(false)
    }
  }

  return (
    <div className="container mx-auto p-4 sm:p-8">
      {toast && (
        <div className="fixed bottom-4 sm:bottom-8 left-1/2 -translate-x-1/2 z-50 px-4 sm:px-6 py-2 sm:py-3 bg-gray-800 text-white text-xs sm:text-sm rounded-lg shadow-lg">
          {toast}
        </div>
      )}

      <button
        onClick={onBack}
        className="mb-4 px-3 sm:px-4 py-2 bg-gray-500 text-white text-sm sm:text-base rounded-lg hover:bg-gray-600"
      >
        Back to Dashboard
      </button>

      <h1 className="text-2xl sm:text-3xl font-bold mb-2">Game Lobby</h1>
      <p className="text-gray-500 mb-1 text-sm sm:text-base">Bet: {bet} credits</p>
      <p className="text-xs sm:text-sm text-gray-400 mb-4 sm:mb-6">
        {status === 'ready'
          ? 'All players have joined. Start the game!'
          : `Waiting for players... (${players.length}/${maxPlayers})`}
      </p>

      {timeLeft && status === 'pending' && (
        <div className="mb-4 sm:mb-6 p-3 bg-yellow-50 border border-yellow-200 rounded-lg">
          <p className="text-yellow-700 font-mono text-sm sm:text-base">
            Time remaining: {timeLeft}
          </p>
        </div>
      )}

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 sm:gap-4 mb-6 sm:mb-8">
        {Array.from({ length: maxPlayers }).map((_, idx) => {
          const p = players.find(pl => pl.position === idx)
          if (p) {
            return (
              <div
                key={idx}
                className="p-3 sm:p-4 rounded-lg border bg-green-50 border-green-300"
              >
                <p className="text-xs sm:text-sm text-gray-500">Position {idx}</p>
                <p className="font-semibold text-base sm:text-lg">{p.pseudo}</p>
              </div>
            )
          }

          if (!isCreator || status !== 'pending') {
            return (
              <div
                key={idx}
                className="p-3 sm:p-4 rounded-lg border bg-gray-50 border-gray-200"
              >
                <p className="text-xs sm:text-sm text-gray-500">Position {idx}</p>
                <p className="text-gray-400 italic text-sm">Waiting for player...</p>
              </div>
            )
          }

          const slotPseudo = slotPseudos[idx] || ''
          const results = slotSearchResults[idx] || []
          const searching = slotSearching[idx] || false
          const inviting = slotInviting[idx] || false

          return (
            <div
              key={idx}
              className="p-3 sm:p-4 rounded-lg border bg-white border-purple-200 relative"
            >
              <p className="text-xs sm:text-sm text-gray-500 mb-1">Position {idx}</p>
              {inviting ? (
                <p className="text-purple-600 text-sm">Inviting...</p>
              ) : (
                <>
                  <input
                    type="text"
                    value={slotPseudo}
                    onFocus={() => setActiveSlot(idx)}
                    onChange={(e) => handleSlotSearch(idx, e.target.value)}
                    placeholder="@pseudo"
                    className="w-full px-2 py-1 border border-gray-300 rounded text-sm focus:ring-2 focus:ring-purple-500 focus:border-purple-500"
                  />
                  {searching && <p className="text-xs text-gray-400 mt-1">Searching...</p>}
                  {activeSlot === idx && results.length > 0 && (
                    <div className="absolute top-full left-0 right-0 z-20 bg-white border border-gray-200 rounded-lg shadow-lg mt-1 max-h-40 overflow-y-auto">
                      {results.map((u) => (
                        <button
                          key={u.id}
                          onClick={() => handleSlotSelect(idx, u)}
                          className="w-full text-left px-3 py-2 text-sm hover:bg-purple-50 flex items-center justify-between"
                        >
                          <span className="font-medium">@{u.pseudo}</span>
                          <span className="text-xs text-purple-600">Invite</span>
                        </button>
                      ))}
                    </div>
                  )}
                </>
              )}
            </div>
          )
        })}
      </div>

      {isCreator && status === 'ready' && (
        <div className="text-center">
          <button
            onClick={handleStart}
            disabled={starting}
            className="px-8 py-4 bg-green-600 text-white text-xl font-bold rounded-lg hover:bg-green-700 disabled:opacity-50 shadow-lg"
          >
            {starting ? 'Starting...' : 'Start Game'}
          </button>
        </div>
      )}

      {!isCreator && status === 'pending' && (
        <div className="text-center p-6 bg-blue-50 rounded-lg border border-blue-200">
          <p className="text-blue-700">Waiting for the game creator to invite more players...</p>
        </div>
      )}

      {!isCreator && status === 'ready' && (
        <div className="text-center p-6 bg-yellow-50 rounded-lg border border-yellow-200">
          <p className="text-yellow-700">Waiting for the game creator to start the game...</p>
        </div>
      )}
    </div>
  )
}
