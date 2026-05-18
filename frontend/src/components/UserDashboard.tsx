import { useEffect, useState, useRef, useCallback } from 'react'
import axios from 'axios'
import { useAuthStore } from '../stores/useAuthStore'
import GameRules from './GameRules'
import LeaderboardPanel from './LeaderboardPanel'

interface ProfileData {
  credit: number
  game_played: number
  wins: number
  kora_wins: number
}

interface GameItem {
  game_id: string
  status: string
  bet: number
  result: string
  played_at: string
  credits_after: number
  player_count: number
}

interface GameHistoryData {
  games: GameItem[]
  total: number
  page: number
  per_page: number
}

interface QuickGameResponse {
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

interface Props {
  onStartGame: () => void
  onStartMultiplayerGame: (bet: number, maxPlayers: number) => Promise<{ gameId: string; error: string | null }>
  onResumeGame: (data: QuickGameResponse) => void
  onViewLobby: (gameId: string) => void
  starting: boolean
  error: string | null
}

interface InvitationItem {
  invite_id: string
  game_id: string
  creator_pseudo: string
  bet: number
  player_count: number
  max_players: number
  created_at: string
  expires_at: string | null
}

type SortField = 'date' | 'bet' | null
type SortDir = 'asc' | 'desc'

const STATUS_OPTIONS = [
  { label: 'All', value: '' },
  { label: 'Pending', value: 'pending' },
  { label: 'Active', value: 'active' },
  { label: 'Ready', value: 'ready' },
  { label: 'Finished', value: 'finished' },
  { label: 'Kora', value: 'kora,double_kora' },
  { label: 'Cancelled', value: 'cancelled' },
]

export default function UserDashboard({ onStartGame, onStartMultiplayerGame, onResumeGame, onViewLobby, starting, error }: Props) {
  const { user, logout } = useAuthStore()
  const [profile, setProfile] = useState<ProfileData | null>(null)
  const [history, setHistory] = useState<GameHistoryData | null>(null)
  const [page, setPage] = useState(1)
  const [loading, setLoading] = useState(true)
  const [toast, setToast] = useState<string | null>(null)
  const [rulesOpen, setRulesOpen] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const [multiplayerOpen, setMultiplayerOpen] = useState(false)
  const [multiplayerStep, setMultiplayerStep] = useState<1 | 2>(1)
  const [multiplayerBet, setMultiplayerBet] = useState(10)
  const [multiplayerMaxPlayers, setMultiplayerMaxPlayers] = useState(4)
  const [multiplayerCreating, setMultiplayerCreating] = useState(false)
  const [multiplayerError, setMultiplayerError] = useState<string | null>(null)
  const [multiplayerPseudos, setMultiplayerPseudos] = useState<Record<number, string>>({})
  const [invitations, setInvitations] = useState<InvitationItem[]>([])
  const [joiningGameId, setJoiningGameId] = useState<string | null>(null)
  const [showGames, setShowGames] = useState(false)
  const [showLeaderboard, setShowLeaderboard] = useState(false)

  const [statusFilter, setStatusFilter] = useState('')
  const [sortField, setSortField] = useState<SortField>(null)
  const [sortDir, setSortDir] = useState<SortDir>('desc')
  const [perPage, setPerPage] = useState(10)
  const toastTimer = useRef<ReturnType<typeof setTimeout>>()

  const showToast = (msg: string) => {
    if (toastTimer.current) clearTimeout(toastTimer.current)
    setToast(msg)
    toastTimer.current = setTimeout(() => setToast(null), 3000)
  }

  useEffect(() => {
    return () => {
      if (toastTimer.current) clearTimeout(toastTimer.current)
    }
  }, [])

  const fetchData = useCallback(() => {
    setLoading(true)
    const params: Record<string, string | number> = { page, per_page: perPage }
    if (statusFilter) params.status = statusFilter
    if (sortField) {
      params.order_by = `${sortField}_${sortDir}`
    }

    Promise.all([
      axios.get<ProfileData>('/api/me/profile'),
      axios.get<GameHistoryData>('/api/me/games', { params }),
      axios.get<{ invitations: InvitationItem[] }>('/api/me/invitations'),
    ])
      .then(([profileRes, historyRes, invRes]) => {
        setProfile(profileRes.data)
        setHistory(historyRes.data)
        setInvitations(invRes.data.invitations)
        setLoading(false)
      })
      .catch((err) => {
        console.error('Failed to load dashboard', err)
        setLoading(false)
      })
  }, [page, perPage, statusFilter, sortField, sortDir])

  useEffect(() => {
    fetchData()
  }, [fetchData])

  useEffect(() => {
    setPage(1)
  }, [statusFilter, sortField, sortDir, perPage])

  const handleGameClick = (gameId: string) => {
    axios.get<QuickGameResponse>(`/api/me/games/${gameId}`)
      .then((res) => {
        onResumeGame(res.data)
      })
      .catch((err) => {
        if (err.response?.status === 410) {
          showToast('Game already finished')
        } else {
          showToast('Failed to load game')
        }
      })
  }

  const handleStep1Submit = async () => {
    if (multiplayerBet <= 0 || multiplayerBet > (profile?.credit ?? 0)) {
      setMultiplayerError('Insufficient credits or invalid bet')
      return
    }

    const filledPseudos = Object.values(multiplayerPseudos)
      .map(p => p.trim())
      .filter(Boolean)
      .map(p => p.startsWith('@') ? p.slice(1) : p)

    const seen = new Set<string>()
    for (const p of filledPseudos) {
      const lower = p.toLowerCase()
      if (seen.has(lower)) {
        setMultiplayerError(`Duplicate pseudo: ${p}`)
        return
      }
      seen.add(lower)
    }

    setMultiplayerCreating(true)
    setMultiplayerError(null)

    const result = await onStartMultiplayerGame(multiplayerBet, multiplayerMaxPlayers)
    setMultiplayerCreating(false)

    if (result.error) {
      setMultiplayerError(result.error)
    } else {
      setMultiplayerOpen(false)
      setMultiplayerStep(1)
      setMultiplayerPseudos({})
      showToast('Multiplayer game created! Inviting players...')

      if (filledPseudos.length > 0) {
        try {
          await axios.post(`/api/games/${result.gameId}/invites`, { pseudos: filledPseudos })
        } catch (err: unknown) {
          showToast((err as { response?: { data?: { error?: string } } }).response?.data?.error || 'Failed to send some invites')
        }
      }
      onViewLobby(result.gameId)
    }
  }

  const openMultiplayerModal = () => {
    setMultiplayerStep(1)
    setMultiplayerBet(10)
    setMultiplayerMaxPlayers(4)
    setMultiplayerPseudos({})
    setMultiplayerError(null)
    setMultiplayerOpen(true)
  }

  const handleAcceptInvite = async (gameId: string) => {
    setJoiningGameId(gameId)
    try {
      const res = await axios.post(`/api/games/${gameId}/respond?action=accept`)
      showToast(res.data.message)
      setInvitations(prev => prev.filter(inv => inv.game_id !== gameId))
      onViewLobby(gameId)
    } catch (err: unknown) {
      showToast((err as { response?: { data?: { error?: string } } }).response?.data?.error || 'Failed to join game')
    } finally {
      setJoiningGameId(null)
    }
  }

  const handleDeclineInvite = async (gameId: string) => {
    setJoiningGameId(gameId)
    try {
      await axios.post(`/api/games/${gameId}/respond?action=decline`)
      setInvitations(prev => prev.filter(inv => inv.game_id !== gameId))
      showToast('Invitation declined')
    } catch (err: unknown) {
      showToast((err as { response?: { data?: { error?: string } } }).response?.data?.error || 'Failed to decline invitation')
    } finally {
      setJoiningGameId(null)
    }
  }

  const handleSort = (field: SortField) => {
    if (sortField === field) {
      if (sortDir === 'desc') {
        setSortDir('asc')
      } else {
        setSortField(null)
        setSortDir('desc')
      }
    } else {
      setSortField(field)
      setSortDir('desc')
    }
  }

  const sortIndicator = (field: SortField) => {
    if (sortField !== field) return ''
    return sortDir === 'asc' ? ' \u25B2' : ' \u25BC'
  }

  const totalPages = history ? Math.ceil(history.total / history.per_page) : 0

  if (loading && !history) {
    return (
      <div className="container mx-auto p-4 sm:p-8">
        <p className="text-gray-600">Loading dashboard...</p>
      </div>
    )
  }

  return (
    <div>
      <GameRules isOpen={rulesOpen} onClose={() => setRulesOpen(false)} />
      <div className="fixed top-4 right-4 z-40">
        <div className="hidden sm:flex gap-2 sm:gap-3">
          <button
            onClick={() => setRulesOpen(true)}
            className="px-3 sm:px-5 py-2 border border-gray-400 text-gray-700 font-semibold rounded-lg hover:bg-gray-100 shadow-lg text-sm sm:text-base"
          >
            Rules
          </button>
          <button
            onClick={logout}
            className="px-5 py-2 bg-red-600 text-white font-semibold rounded-lg hover:bg-red-700 shadow-lg"
          >
            Logout
          </button>
        </div>
        <div className="sm:hidden relative">
          <button
            onClick={() => setMenuOpen(!menuOpen)}
            className="w-10 h-10 flex items-center justify-center bg-white border border-gray-300 rounded-lg shadow-lg text-gray-700 hover:bg-gray-100"
            aria-label="Menu"
          >
            <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
              <circle cx="10" cy="3" r="2"/>
              <circle cx="10" cy="10" r="2"/>
              <circle cx="10" cy="17" r="2"/>
            </svg>
          </button>
          {menuOpen && (
            <div className="absolute right-0 top-12 bg-white border border-gray-200 rounded-lg shadow-xl py-1 min-w-[120px] z-50">
              <button
                onClick={() => { setRulesOpen(true); setMenuOpen(false) }}
                className="w-full text-left px-4 py-2 text-sm text-gray-700 hover:bg-gray-100"
              >
                Rules
              </button>
              <button
                onClick={() => { logout(); setMenuOpen(false) }}
                className="w-full text-left px-4 py-2 text-sm text-red-600 hover:bg-red-50"
              >
                Logout
              </button>
            </div>
          )}
        </div>
      </div>

      {toast && (
        <div className="fixed bottom-8 left-1/2 -translate-x-1/2 z-50 px-6 py-3 bg-gray-800 text-white text-sm rounded-lg shadow-lg animate-fade-in-out">
          {toast}
        </div>
      )}

      <div className="container mx-auto p-4 sm:p-8">
        <h1 className="text-2xl sm:text-3xl font-bold mb-2">Welcome, {user?.pseudo}</h1>
        <p className="text-gray-500 mb-4 sm:mb-6">{user?.email}</p>

        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 sm:gap-4 mb-6 sm:mb-8">
          <div className="bg-white p-3 sm:p-4 rounded-lg shadow border">
            <p className="text-xs sm:text-sm text-gray-500">Credit</p>
            <p className="text-xl sm:text-2xl font-bold">{profile?.credit ?? 0}</p>
          </div>
          <div className="bg-white p-3 sm:p-4 rounded-lg shadow border">
            <p className="text-xs sm:text-sm text-gray-500">Games Played</p>
            <p className="text-xl sm:text-2xl font-bold">{profile?.game_played ?? 0}</p>
          </div>
          <div className="bg-white p-3 sm:p-4 rounded-lg shadow border">
            <p className="text-xs sm:text-sm text-gray-500">Wins</p>
            <p className="text-xl sm:text-2xl font-bold text-green-600">{profile?.wins ?? 0}</p>
          </div>
          <div className="bg-white p-3 sm:p-4 rounded-lg shadow border">
            <p className="text-xs sm:text-sm text-gray-500">Kora Wins</p>
            <p className="text-xl sm:text-2xl font-bold text-yellow-600">{profile?.kora_wins ?? 0}</p>
          </div>
        </div>

        <div className="mb-6 sm:mb-8">
          <div className="flex flex-wrap gap-2 sm:gap-3">
            <button
              className="px-4 sm:px-6 py-2 sm:py-3 bg-blue-600 text-white text-sm sm:text-base font-semibold rounded-lg hover:bg-blue-700 disabled:opacity-50"
              disabled={starting}
              onClick={onStartGame}
            >
              {starting ? 'Starting game...' : 'Solo Game'}
            </button>
            <button
              className="px-4 sm:px-6 py-2 sm:py-3 bg-purple-600 text-white text-sm sm:text-base font-semibold rounded-lg hover:bg-purple-700 disabled:opacity-50"
              disabled={starting}
              onClick={openMultiplayerModal}
            >
              Multiplayer Game
            </button>
            <button
              className="px-4 sm:px-6 py-2 sm:py-3 bg-teal-600 text-white text-sm sm:text-base font-semibold rounded-lg hover:bg-teal-700"
              onClick={() => setShowGames(!showGames)}
            >
              {showGames ? 'Hide Games' : 'Games'}
            </button>
            <button
              className="px-4 sm:px-6 py-2 sm:py-3 bg-orange-600 text-white text-sm sm:text-base font-semibold rounded-lg hover:bg-orange-700"
              onClick={() => setShowLeaderboard(!showLeaderboard)}
            >
              {showLeaderboard ? 'Hide Leaderboard' : 'Leaderboard'}
            </button>
          </div>
          {error && (
            <div className="mt-4 p-3 bg-red-100 text-red-700 rounded text-sm flex items-center justify-between">
              <span>Failed to start game: {error}</span>
              <button onClick={() => {}} className="text-red-500 hover:text-red-700 ml-2">&times;</button>
            </div>
          )}
        </div>

        {invitations.length > 0 && (
          <div className="bg-purple-50 p-4 sm:p-6 rounded-lg shadow mb-6 sm:mb-8 border border-purple-200">
            <h2 className="text-lg sm:text-xl font-semibold mb-3 sm:mb-4 text-purple-800">
              Pending Invitations ({invitations.length})
            </h2>
            <div className="space-y-2 sm:space-y-3">
              {invitations.map((inv) => (
                <div
                  key={inv.invite_id}
                  className="flex flex-col sm:flex-row sm:items-center justify-between bg-white p-3 sm:p-4 rounded-lg shadow-sm gap-2 sm:gap-0"
                >
                  <div>
                    <p className="font-semibold">
                      {inv.creator_pseudo}'s game
                    </p>
                    <p className="text-xs sm:text-sm text-gray-500">
                      Bet: {inv.bet} | Players: {inv.player_count}/{inv.max_players}
                    </p>
                    {inv.expires_at && (
                      <p className="text-xs text-gray-400">
                        Expires: {new Date(inv.expires_at).toLocaleTimeString()}
                      </p>
                    )}
                  </div>
                  <div className="flex gap-2">
                    <button
                      onClick={() => handleAcceptInvite(inv.game_id)}
                      disabled={joiningGameId === inv.game_id}
                      className="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 disabled:opacity-50 text-sm font-medium w-full sm:w-auto"
                    >
                      {joiningGameId === inv.game_id ? '...' : 'Accept'}
                    </button>
                    <button
                      onClick={() => handleDeclineInvite(inv.game_id)}
                      disabled={joiningGameId === inv.game_id}
                      className="px-4 py-2 bg-gray-400 text-white rounded-lg hover:bg-gray-500 disabled:opacity-50 text-sm font-medium w-full sm:w-auto"
                    >
                      Decline
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {showGames && (
        <div className="bg-gray-100 p-4 sm:p-6 rounded-lg shadow mb-6 sm:mb-8">
          <div className="flex flex-wrap items-center justify-between mb-4">
            <h2 className="text-lg sm:text-xl font-semibold">
              Game History
              {history && <span className="text-gray-500 text-sm ml-2">({history.total} total)</span>}
            </h2>
            <div className="flex items-center gap-2 mt-2 sm:mt-0">
              <span className="text-xs text-gray-500">Per page:</span>
              <select
                value={perPage}
                onChange={(e) => setPerPage(Number(e.target.value))}
                className="text-xs border border-gray-300 rounded px-2 py-1"
              >
                <option value={10}>10</option>
                <option value={20}>20</option>
                <option value={50}>50</option>
              </select>
            </div>
          </div>

          <div className="flex flex-wrap gap-1 sm:gap-2 mb-4">
            {STATUS_OPTIONS.map((opt) => (
              <button
                key={opt.value}
                onClick={() => setStatusFilter(opt.value)}
                className={`px-2 sm:px-3 py-1 rounded text-xs sm:text-sm font-medium transition-colors ${
                  statusFilter === opt.value
                    ? 'bg-blue-600 text-white'
                    : 'bg-white text-gray-600 hover:bg-gray-200 border border-gray-300'
                }`}
              >
                {opt.label}
              </button>
            ))}
          </div>

          {history && history.games.length === 0 ? (
            <p className="text-gray-500">No games found.</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead>
                  <tr className="border-b">
                    <th className="py-2 pr-4">Status</th>
                    <th className="py-2 pr-4">Result</th>
                    <th
                      className="py-2 pr-4 cursor-pointer select-none hover:text-blue-600"
                      onClick={() => handleSort('bet')}
                    >
                      Bet{sortIndicator('bet')}
                    </th>
                    <th className="py-2 pr-4">Credits</th>
                    <th className="py-2 pr-4">Players</th>
                    <th
                      className="py-2 cursor-pointer select-none hover:text-blue-600"
                      onClick={() => handleSort('date')}
                    >
                      Date{sortIndicator('date')}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {history?.games.map((game) => (
                    <tr
                      key={game.game_id}
                      className="border-b cursor-pointer hover:bg-gray-200 transition-colors"
                      onClick={() => handleGameClick(game.game_id)}
                    >
                      <td className="py-2 pr-4">
                        <span
                          className={`inline-block px-2 py-0.5 rounded text-xs font-medium ${
                            game.status === 'active'
                              ? 'bg-blue-100 text-blue-700'
                              : game.status === 'finished'
                                ? 'bg-green-100 text-green-700'
                                : game.status === 'pending'
                                  ? 'bg-yellow-100 text-yellow-700'
                                  : game.status === 'kora' || game.status === 'double_kora'
                                    ? 'bg-yellow-100 text-yellow-700'
                                    : game.status === 'cancelled'
                                      ? 'bg-red-100 text-red-700'
                                      : 'bg-gray-100 text-gray-600'
                          }`}
                        >
                          {game.status}
                        </span>
                      </td>
                      <td className="py-2 pr-4">
                        <span
                          className={
                            game.result === 'win'
                              ? 'text-green-600 font-semibold'
                              : game.result === 'loss'
                                ? 'text-red-600'
                                : 'text-gray-500'
                          }
                        >
                          {game.result}
                        </span>
                      </td>
                      <td className="py-2 pr-4">{game.bet}</td>
                      <td className="py-2 pr-4">{game.credits_after}</td>
                      <td className="py-2 pr-4 text-gray-500">{game.player_count}</td>
                      <td className="py-2 text-gray-500 text-xs">
                        {new Date(game.played_at).toLocaleDateString()}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {history && history.total > history.per_page && (
            <div className="flex justify-center items-center gap-1 mt-4">
              <button
                onClick={() => setPage((p) => Math.max(1, p - 1))}
                disabled={page <= 1}
                className="px-3 py-1 bg-gray-200 rounded disabled:opacity-50 hover:bg-gray-300 text-sm"
              >
                Prev
              </button>
              {Array.from({ length: Math.min(totalPages, 7) }, (_, i) => {
                let pageNum: number
                if (totalPages <= 7) {
                  pageNum = i + 1
                } else if (page <= 4) {
                  pageNum = i + 1
                } else if (page >= totalPages - 3) {
                  pageNum = totalPages - 6 + i
                } else {
                  pageNum = page - 3 + i
                }
                return (
                  <button
                    key={pageNum}
                    onClick={() => setPage(pageNum)}
                    className={`px-3 py-1 rounded text-sm ${
                      pageNum === page
                        ? 'bg-blue-600 text-white'
                        : 'bg-gray-200 hover:bg-gray-300'
                    }`}
                  >
                    {pageNum}
                  </button>
                )
              })}
              <button
                onClick={() => setPage((p) => p + 1)}
                disabled={page >= totalPages}
                className="px-3 py-1 bg-gray-200 rounded disabled:opacity-50 hover:bg-gray-300 text-sm"
              >
                Next
              </button>
            </div>
          )}
        </div>
        )}

        {showLeaderboard && <LeaderboardPanel />}
      </div>

      {multiplayerOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-white rounded-lg shadow-xl p-6 w-full max-w-md mx-4">
            <div className="flex justify-between items-center mb-4">
              <h2 className="text-xl font-bold">
                {multiplayerStep === 1 ? 'Create Multiplayer Game' : 'Invite Players'}
              </h2>
              <button
                onClick={() => { setMultiplayerOpen(false); setMultiplayerError(null); setMultiplayerStep(1) }}
                className="text-gray-400 hover:text-gray-600 text-2xl leading-none"
              >
                &times;
              </button>
            </div>

            {multiplayerStep === 1 && (
              <>
                <div className="mb-4">
                  <label className="block text-sm font-medium text-gray-700 mb-1">
                    Bet Amount
                  </label>
                  <input
                    type="number"
                    min={1}
                    max={profile?.credit ?? 500}
                    value={multiplayerBet}
                    onChange={(e) => setMultiplayerBet(parseInt(e.target.value) || 0)}
                    className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-purple-500"
                  />
                  <p className="text-sm text-gray-500 mt-1">
                    Your credit: {profile?.credit ?? 0}
                  </p>
                </div>

                <div className="mb-4">
                  <label className="block text-sm font-medium text-gray-700 mb-1">
                    Number of Players
                  </label>
                  <select
                    value={multiplayerMaxPlayers}
                    onChange={(e) => setMultiplayerMaxPlayers(parseInt(e.target.value))}
                    className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-purple-500"
                  >
                    <option value={2}>2 players</option>
                    <option value={3}>3 players</option>
                    <option value={4}>4 players</option>
                  </select>
                  <p className="text-sm text-gray-400 mt-1">
                    Game will be cancelled after 6 minutes if not full.
                  </p>
                </div>

                {multiplayerError && (
                  <div className="mb-4 p-3 bg-red-100 text-red-700 rounded text-sm">
                    {multiplayerError}
                  </div>
                )}

                <div className="flex justify-end gap-3">
                  <button
                    onClick={() => { setMultiplayerOpen(false); setMultiplayerError(null) }}
                    className="px-4 py-2 border border-gray-300 rounded-lg text-gray-700 hover:bg-gray-100"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={() => setMultiplayerStep(2)}
                    disabled={multiplayerBet <= 0 || multiplayerBet > (profile?.credit ?? 0)}
                    className="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 disabled:opacity-50"
                  >
                    Next: Invite Players
                  </button>
                </div>
              </>
            )}

            {multiplayerStep === 2 && (
              <>
                <p className="text-sm text-gray-500 mb-4">
                  Bet: {multiplayerBet} credits | Players: {multiplayerMaxPlayers} (you + {multiplayerMaxPlayers - 1} others)
                </p>

                {Array.from({ length: multiplayerMaxPlayers - 1 }, (_, i) => i + 1).map((slot) => (
                  <div key={slot} className="mb-3">
                    <label className="block text-sm font-medium text-gray-700 mb-1">
                      Player {slot} (@pseudo)
                    </label>
                    <input
                      type="text"
                      placeholder="@pseudo"
                      value={multiplayerPseudos[slot] || ''}
                      onChange={(e) => {
                        let val = e.target.value
                        if (val && !val.startsWith('@')) val = '@' + val
                        setMultiplayerPseudos(prev => ({ ...prev, [slot]: val }))
                      }}
                      className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-purple-500"
                    />
                    <p className="text-xs text-gray-400 mt-0.5">Leave empty to skip</p>
                  </div>
                ))}

                {multiplayerError && (
                  <div className="mb-4 p-3 bg-red-100 text-red-700 rounded text-sm">
                    {multiplayerError}
                  </div>
                )}

                <div className="flex justify-end gap-3">
                  <button
                    onClick={() => setMultiplayerStep(1)}
                    className="px-4 py-2 border border-gray-300 rounded-lg text-gray-700 hover:bg-gray-100"
                  >
                    Back
                  </button>
                  <button
                    onClick={handleStep1Submit}
                    disabled={multiplayerCreating}
                    className="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 disabled:opacity-50"
                  >
                    {multiplayerCreating ? 'Creating...' : 'Create & Invite'}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
