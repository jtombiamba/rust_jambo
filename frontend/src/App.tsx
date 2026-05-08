import { useEffect, useState } from 'react'
import axios from 'axios'
import './App.css'
import GameTable from './components/GameTable'
import AuthModal from './components/AuthModal'
import GameRules from './components/GameRules'
import UserDashboard from './components/UserDashboard'
import { useGameStore } from './stores/useGameStore'
import { useAuthStore } from './stores/useAuthStore'
import { useGameWebSocket } from './hooks/useGameWebSocket'
import { getStoredStats, saveStats, AnonymousStats } from './utils/storage'

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

function App() {
  const [stats, setStats] = useState<AnonymousStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [startingGame, setStartingGame] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [cardError, setCardError] = useState<string | null>(null)
  const [playingCard, setPlayingCard] = useState<number | null>(null)
  const [rulesOpen, setRulesOpen] = useState(false)
  const { gameId, players, currentTurn, deckSlots, remainingCards, gameOver, roundWinner, setGame: setGameStore, resetGame, clearGameOver } = useGameStore()
  const { isAuthenticated, openAuthModal, checkAuth } = useAuthStore()
  useGameWebSocket(gameId)

  useEffect(() => {
    checkAuth()
  }, [checkAuth])

  useEffect(() => {
    if (isAuthenticated) {
      // Don't auto-navigate to active game — let the dashboard handle resume
      setLoading(false)
      return
    }
    const storedStats = getStoredStats()
    if (storedStats) {
      setStats(storedStats)
      setLoading(false)
      return
    }
    axios.get('/api/anonymous')
      .then(response => {
        const data = response.data as AnonymousStats
        setStats(data)
        saveStats(data)
        setLoading(false)
      })
      .catch(error => {
        console.error('Failed to fetch stats', error)
        setLoading(false)
      })
  }, [isAuthenticated])

  const startGame = () => {
    setStartingGame(true)
    setError(null)
    const endpoint = isAuthenticated ? '/api/me/games' : '/api/quickie'
    axios.post<QuickGameResponse>(endpoint)
      .then(response => {
        setGameStore(response.data.game_id, response.data.players, response.data.status, response.data.current_turn, response.data.bet)
        setStartingGame(false)
      })
      .catch(err => {
        console.error('Failed to start game', err)
        setError(err.response?.data?.error || 'Unknown error')
        setStartingGame(false)
      })
  }

  const handleCardClick = (playerId: string, cardIndex: number) => {
    if (!gameId || playingCard !== null) return;
    setCardError(null);
    setPlayingCard(cardIndex);
    axios.post(`/api/game/${gameId}/play`, {
      player_id: playerId,
      card_index: cardIndex,
    })
      .catch(err => {
        console.error('Failed to play card', err);
        setCardError(err.response?.data?.error || 'Failed to play card');
      })
      .finally(() => setPlayingCard(null));
  };

  if (loading) return <div className="container mx-auto p-8">Loading...</div>

  if (gameId) {
    return (
      <div>
        <GameTable
          players={players}
          currentTurn={currentTurn}
          deckSlots={deckSlots}
          remainingCards={remainingCards}
          roundWinner={roundWinner}
          gameOver={gameOver}
          onCardClick={handleCardClick}
          onPlayAgain={startGame}
          onReturnToLobby={resetGame}
          onCloseGameOver={clearGameOver}
        />
        {cardError && (
          <div className="container mx-auto px-8">
            <div className="p-3 bg-red-100 text-red-700 rounded">
              {cardError}
            </div>
          </div>
        )}
        <div className="container mx-auto p-8">
          <button
            className="mt-4 px-4 py-2 bg-gray-500 text-white rounded"
            onClick={() => resetGame()}
          >
            Back to Dashboard
          </button>
        </div>
      </div>
    )
  }

  const handleResumeGame = (data: QuickGameResponse) => {
    setGameStore(data.game_id, data.players, data.status, data.current_turn, data.bet)
  }

  if (isAuthenticated) {
    return (
      <div>
        <AuthModal />
        <GameRules isOpen={rulesOpen} onClose={() => setRulesOpen(false)} />
        <UserDashboard
          onStartGame={startGame}
          onResumeGame={handleResumeGame}
          starting={startingGame}
          error={error}
        />
      </div>
    )
  }

  const gamesPlayed = stats?.games_played ?? 0
  const gamesAllowed = stats?.games_allowed ?? 10
  const gamesRemaining = Math.max(0, gamesAllowed - gamesPlayed)

  return (
    <div>
      <AuthModal />
      <GameRules isOpen={rulesOpen} onClose={() => setRulesOpen(false)} />
      <button
        onClick={openAuthModal}
        className="fixed top-4 right-4 z-40 px-5 py-2 bg-emerald-600 text-white font-semibold rounded-lg hover:bg-emerald-700 shadow-lg"
      >
        Create account / Connect
      </button>
      <div className="container mx-auto p-8">
        <h1 className="text-3xl font-bold mb-6">FapFap Card Game</h1>
        <div className="bg-gray-100 p-6 rounded-lg shadow mb-8">
          <h2 className="text-xl font-semibold mb-4">Dashboard</h2>
          <p className="mb-2">
            You are not logged in. You are allowed {gamesAllowed} games.
            Create an account to play more.
          </p>
          <div className="grid grid-cols-2 gap-4">
            <div className="bg-white p-4 rounded shadow">
              <p className="text-lg">Games Played</p>
              <p className="text-2xl font-bold">{gamesPlayed}</p>
            </div>
            <div className="bg-white p-4 rounded shadow">
              <p className="text-lg">Total Wins</p>
              <p className="text-2xl font-bold">{stats?.total_wins ?? 0}</p>
            </div>
            <div className="bg-white p-4 rounded shadow">
              <p className="text-lg">Credits</p>
              <p className="text-2xl font-bold">{stats?.credits ?? 0}</p>
            </div>
            <div className="bg-white p-4 rounded shadow">
              <p className="text-lg">Remaining Games</p>
              <p className="text-2xl font-bold">{gamesRemaining}</p>
            </div>
          </div>
          <div className="flex gap-3 mt-6">
            {gamesPlayed < gamesAllowed && (
              <button
                className="px-6 py-3 bg-blue-600 text-white font-semibold rounded-lg hover:bg-blue-700 disabled:opacity-50"
                disabled={startingGame}
                onClick={startGame}
              >
                {startingGame ? 'Starting game...' : 'Start a quick game'}
              </button>
            )}
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
        <div className="text-gray-500 text-sm">
          Sprint 3: Real time gameplay.
        </div>
      </div>
    </div>
  )
}

export default App
