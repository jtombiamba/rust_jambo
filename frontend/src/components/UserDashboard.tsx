import { useEffect, useState, useRef } from 'react'
import axios from 'axios'
import { useAuthStore } from '../stores/useAuthStore'
import GameRules from './GameRules'

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
    cards: number[]
    cards_count: number
  }>
  status: string
  current_turn: number
  bet: number
}

interface Props {
  onStartGame: () => void
  onResumeGame: (data: QuickGameResponse) => void
  starting: boolean
  error: string | null
}

export default function UserDashboard({ onStartGame, onResumeGame, starting, error }: Props) {
  const { user, logout } = useAuthStore()
  const [profile, setProfile] = useState<ProfileData | null>(null)
  const [history, setHistory] = useState<GameHistoryData | null>(null)
  const [page, setPage] = useState(1)
  const [loading, setLoading] = useState(true)
  const [toast, setToast] = useState<string | null>(null)
  const [rulesOpen, setRulesOpen] = useState(false)
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

  useEffect(() => {
    setLoading(true)
    Promise.all([
      axios.get<ProfileData>('/api/me/profile'),
      axios.get<GameHistoryData>('/api/me/games', { params: { page, per_page: 10 } }),
    ])
      .then(([profileRes, historyRes]) => {
        setProfile(profileRes.data)
        setHistory(historyRes.data)
        setLoading(false)
      })
      .catch((err) => {
        console.error('Failed to load dashboard', err)
        setLoading(false)
      })
  }, [page])

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

  if (loading) {
    return (
      <div className="container mx-auto p-8">
        <p className="text-gray-600">Loading dashboard...</p>
      </div>
    )
  }

  return (
    <div>
      <GameRules isOpen={rulesOpen} onClose={() => setRulesOpen(false)} />
      <button
        onClick={logout}
        className="fixed top-4 right-4 z-40 px-5 py-2 bg-red-600 text-white font-semibold rounded-lg hover:bg-red-700 shadow-lg"
      >
        Logout
      </button>

      {toast && (
        <div className="fixed bottom-8 left-1/2 -translate-x-1/2 z-50 px-6 py-3 bg-gray-800 text-white text-sm rounded-lg shadow-lg animate-fade-in-out">
          {toast}
        </div>
      )}

      <div className="container mx-auto p-8">
        <h1 className="text-3xl font-bold mb-2">Welcome, {user?.pseudo}</h1>
        <p className="text-gray-500 mb-6">{user?.email}</p>

        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white p-4 rounded-lg shadow border">
            <p className="text-sm text-gray-500">Credit</p>
            <p className="text-2xl font-bold">{profile?.credit ?? 0}</p>
          </div>
          <div className="bg-white p-4 rounded-lg shadow border">
            <p className="text-sm text-gray-500">Games Played</p>
            <p className="text-2xl font-bold">{profile?.game_played ?? 0}</p>
          </div>
          <div className="bg-white p-4 rounded-lg shadow border">
            <p className="text-sm text-gray-500">Wins</p>
            <p className="text-2xl font-bold text-green-600">{profile?.wins ?? 0}</p>
          </div>
          <div className="bg-white p-4 rounded-lg shadow border">
            <p className="text-sm text-gray-500">Kora Wins</p>
            <p className="text-2xl font-bold text-yellow-600">{profile?.kora_wins ?? 0}</p>
          </div>
        </div>

        <div className="mb-8">
          <div className="flex gap-3">
            <button
              className="px-6 py-3 bg-blue-600 text-white font-semibold rounded-lg hover:bg-blue-700 disabled:opacity-50"
              disabled={starting}
              onClick={onStartGame}
            >
              {starting ? 'Starting game...' : 'Start a solo game'}
            </button>
            <button
              className="px-6 py-3 border border-gray-400 text-gray-700 font-semibold rounded-lg hover:bg-gray-100"
              onClick={() => setRulesOpen(true)}
            >
              Rules
            </button>
          </div>
          {error && (
            <div className="mt-4 p-3 bg-red-100 text-red-700 rounded">
              Failed to start game: {error}
            </div>
          )}
        </div>

        <div className="bg-gray-100 p-6 rounded-lg shadow mb-8">
          <h2 className="text-xl font-semibold mb-4">Game History</h2>

          {history && history.games.length === 0 ? (
            <p className="text-gray-500">No games played yet.</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead>
                  <tr className="border-b">
                    <th className="py-2 pr-4">Game</th>
                    <th className="py-2 pr-4">Status</th>
                    <th className="py-2 pr-4">Result</th>
                    <th className="py-2 pr-4">Bet</th>
                    <th className="py-2 pr-4">Credits After</th>
                    <th className="py-2">Date</th>
                  </tr>
                </thead>
                <tbody>
                  {history?.games.map((game) => (
                    <tr
                      key={game.game_id}
                      className="border-b cursor-pointer hover:bg-gray-200 transition-colors"
                      onClick={() => handleGameClick(game.game_id)}
                    >
                      <td className="py-2 pr-4 font-mono text-xs">
                        {game.game_id.slice(0, 8)}...
                      </td>
                      <td className="py-2 pr-4">
                        <span
                          className={`inline-block px-2 py-0.5 rounded text-xs font-medium ${
                            game.status === 'active'
                              ? 'bg-blue-100 text-blue-700'
                              : game.status === 'finished'
                                ? 'bg-green-100 text-green-700'
                                : game.status === 'kora' || game.status === 'double_kora'
                                  ? 'bg-yellow-100 text-yellow-700'
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
            <div className="flex justify-center gap-2 mt-4">
              <button
                onClick={() => setPage((p) => Math.max(1, p - 1))}
                disabled={page <= 1}
                className="px-3 py-1 bg-gray-200 rounded disabled:opacity-50 hover:bg-gray-300"
              >
                Previous
              </button>
              <span className="px-3 py-1 text-sm text-gray-600">
                Page {page} of {Math.ceil(history.total / history.per_page)}
              </span>
              <button
                onClick={() => setPage((p) => p + 1)}
                disabled={page >= Math.ceil(history.total / history.per_page)}
                className="px-3 py-1 bg-gray-200 rounded disabled:opacity-50 hover:bg-gray-300"
              >
                Next
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
