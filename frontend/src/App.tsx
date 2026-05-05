import { useEffect, useState } from 'react'
import axios from 'axios'
import './App.css'
import GameTable from './components/GameTable'
import { useGameStore } from './stores/useGameStore'
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
  const { gameId, players, currentTurn, deckSlots, remainingCards, gameOver, roundWinner, setGame: setGameStore, resetGame, clearGameOver } = useGameStore()
  useGameWebSocket(gameId)

  useEffect(() => {
    const storedStats = getStoredStats()
    if (storedStats) {
      // console.log('[DEBUG] Loaded stats from localStorage:', storedStats)
      setStats(storedStats)
      setLoading(false)
      return
    }
    axios.get('/api/anonymous')
      .then(response => {
        // console.log('[DEBUG] Raw API response for /api/anonymous:', JSON.stringify(response.data))
        const data = response.data as AnonymousStats
        // console.log('[DEBUG] Parsed stats (snake_case interface):', data)
        setStats(data)
        saveStats(data)
        setLoading(false)
      })
      .catch(error => {
        console.error('Failed to fetch stats', error)
        setLoading(false)
      })
  }, [])

  const startGame = () => {
    setStartingGame(true)
    setError(null)
    axios.post<QuickGameResponse>('/api/quickie')
      .then(response => {
        // console.log('[DEBUG] Raw quickie response:', JSON.stringify(response.data))
        // console.log('[DEBUG] Accessing snake_case fields:', {
        //   game_id: response.data.game_id,
        //   current_turn: response.data.current_turn,
        //   players: response.data.players,
        //   status: response.data.status,
        //   bet: response.data.bet,
        // })
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

  return (
    <div className="container mx-auto p-8">
      <h1 className="text-3xl font-bold mb-6">FapFap Card Game</h1>
      <div className="bg-gray-100 p-6 rounded-lg shadow mb-8">
        <h2 className="text-xl font-semibold mb-4">Dashboard</h2>
        <p className="mb-2">
          You are not logged in. You are allowed {stats?.games_allowed} games.
          Create an account to play more.
        </p>
        <div className="grid grid-cols-2 gap-4">
          <div className="bg-white p-4 rounded shadow">
            <p className="text-lg">Games Played</p>
            <p className="text-2xl font-bold">{stats?.games_played}</p>
          </div>
          <div className="bg-white p-4 rounded shadow">
            <p className="text-lg">Total Wins</p>
            <p className="text-2xl font-bold">{stats?.total_wins}</p>
          </div>
          <div className="bg-white p-4 rounded shadow">
            <p className="text-lg">Credits</p>
            <p className="text-2xl font-bold">{stats?.credits}</p>
          </div>
          <div className="bg-white p-4 rounded shadow">
            <p className="text-lg">Remaining Games</p>
            <p className="text-2xl font-bold">
              {stats ? stats.games_allowed - stats.games_played : 0}
            </p>
          </div>
        </div>
        {stats && stats.games_played < 10 ? (
          <button
            className="mt-6 px-6 py-3 bg-blue-600 text-white font-semibold rounded-lg hover:bg-blue-700 disabled:opacity-50"
            disabled={startingGame}
            onClick={startGame}
          >
            {startingGame ? 'Starting game...' : 'Start a game'}
          </button>
        ) : (
          <button
            className="mt-6 px-6 py-3 bg-emerald-600 text-white font-semibold rounded-lg opacity-50 cursor-not-allowed"
            disabled
          >
            Create your account
          </button>
        )}
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
  )
}

export default App
