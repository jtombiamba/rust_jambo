import { useEffect, useState, useCallback } from 'react'
import axios from 'axios'
import './App.css'
import GameTable from './components/GameTable'
import AuthModal from './components/AuthModal'
import GameRules from './components/GameRules'
import UserDashboard from './components/UserDashboard'
import GameLobby from './components/GameLobby'
import { ToastProvider } from './components/Toast'
import { useToast } from './components/useToast'
import { useGameStore } from './stores/useGameStore'
import { useAuthStore } from './stores/useAuthStore'
import { useGameWebSocket } from './hooks/useGameWebSocket'
import { useWebSocket } from './hooks/useWebSocket'
import { getStoredStats, saveStats, AnonymousStats } from './utils/storage'

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

interface MultiplayerGameResponse {
  game_id: string
  status: string
  bet: number
  max_players: number
  invite_expires_at: string
}

function AppContent() {
  const [stats, setStats] = useState<AnonymousStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [startingGame, setStartingGame] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [cardError, setCardError] = useState<string | null>(null)
  const [playingCard, setPlayingCard] = useState<number | null>(null)
  const [rulesOpen, setRulesOpen] = useState(false)
  const [lobbyGameId, setLobbyGameId] = useState<string | null>(null)
  const [pendingInvite, setPendingInvite] = useState<{ gameId: string; action: string } | null>(null)
  const { gameId, players, currentTurn, deckSlots, remainingCards, gameOver, roundWinner, setGame: setGameStore, resetGame, clearGameOver } = useGameStore()
  const isMultiplayer = players.length > 0 && players.every(p => p.type === 'human')
  const { isAuthenticated, openAuthModal, checkAuth, clearPendingInvite } = useAuthStore()
  const { isConnected } = useWebSocket({ gameId: gameId || '' })
  const { showToast } = useToast()
  useGameWebSocket(gameId)

  const processInvite = useCallback((gameId: string, action: string) => {
    const endpoint = `/api/games/${gameId}/respond?action=${encodeURIComponent(action)}`
    axios.post(endpoint)
      .then((res) => {
        if (action === 'accept') {
          showToast(res.data.message || 'Joined game!', 'success')
          setLobbyGameId(gameId)
        } else {
          showToast('Invitation declined', 'success')
        }
      })
      .catch((err) => {
        const msg = err.response?.data?.error || 'Failed to process invitation'
        showToast(msg, 'error')
      })
  }, [showToast])

  useEffect(() => {
    checkAuth()
  }, [checkAuth])

  useEffect(() => {
    const params = new URLSearchParams(window.location.search)
    const inviteGameId = params.get('invite_game_id')
    const inviteAction = params.get('invite_action')
    if (inviteGameId && inviteAction) {
      if (isAuthenticated) {
        processInvite(inviteGameId, inviteAction)
        const url = new URL(window.location.href)
        url.searchParams.delete('invite_game_id')
        url.searchParams.delete('invite_action')
        window.history.replaceState({}, '', url.toString())
      } else {
        setPendingInvite({ gameId: inviteGameId, action: inviteAction })
        openAuthModal('Log in to respond to the game invitation.')
      }
    }
  }, [isAuthenticated, openAuthModal, processInvite])

  useEffect(() => {
    if (isAuthenticated && pendingInvite) {
      processInvite(pendingInvite.gameId, pendingInvite.action)
      setPendingInvite(null)
      clearPendingInvite()
      const url = new URL(window.location.href)
      url.searchParams.delete('invite_game_id')
      url.searchParams.delete('invite_action')
      window.history.replaceState({}, '', url.toString())
    }
  }, [isAuthenticated, pendingInvite, clearPendingInvite, processInvite])

  useEffect(() => {
    if (isAuthenticated) {
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
      .catch(err => {
        console.error('Failed to fetch stats', err)
        showToast('Failed to load game stats', 'error')
        setLoading(false)
      })
  }, [isAuthenticated, showToast])

  const startGame = () => {
    setStartingGame(true)
    setError(null)
    if (isAuthenticated) {
      axios.post<QuickGameResponse>('/api/me/games', { game_mode: 'solo' })
        .then(response => {
          setGameStore(response.data.game_id, response.data.players, response.data.status, response.data.current_turn, response.data.bet)
          setStartingGame(false)
        })
        .catch(err => {
          console.error('Failed to start game', err)
          const msg = err.response?.data?.error || 'Unknown error'
          setError(msg)
          showToast(msg, 'error')
          setStartingGame(false)
        })
    } else {
      axios.post<QuickGameResponse>('/api/quickie')
        .then(response => {
          setGameStore(response.data.game_id, response.data.players, response.data.status, response.data.current_turn, response.data.bet)
          setStartingGame(false)
        })
        .catch(err => {
          console.error('Failed to start game', err)
          const msg = err.response?.data?.error || 'Unknown error'
          setError(msg)
          showToast(msg, 'error')
          setStartingGame(false)
        })
    }
  }

  const startMultiplayerGame = (bet: number, maxPlayers: number): Promise<{ gameId: string; error: string | null }> => {
    setStartingGame(true)
    setError(null)
    return axios.post<MultiplayerGameResponse>('/api/me/games', { bet, game_mode: 'multiplayer', max_players: maxPlayers })
      .then((res) => {
        setStartingGame(false)
        return { gameId: res.data.game_id, error: null }
      })
      .catch(err => {
        console.error('Failed to create multiplayer game', err)
        const msg = err.response?.data?.error || 'Unknown error'
        setError(msg)
        showToast(msg, 'error')
        setStartingGame(false)
        return { gameId: '', error: msg }
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
        const msg = err.response?.data?.error || 'Failed to play card';
        setCardError(msg);
        showToast(msg, 'error');
      })
      .finally(() => setPlayingCard(null));
  };

  const handleViewLobby = (gameId: string) => {
    setLobbyGameId(gameId)
  }

  const handleLobbyBack = () => {
    setLobbyGameId(null)
  }

  const handleGameStartFromLobby = (data: unknown) => {
    const d = data as QuickGameResponse
    if (d.game_id && d.players) {
      setGameStore(d.game_id, d.players, d.status || 'active', d.current_turn || 0, d.bet || 10, d.deck_slots || null)
      setLobbyGameId(null)
    }
  }

  if (loading) {
    return (
      <div className="container mx-auto p-4 sm:p-8 flex items-center justify-center min-h-screen">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto mb-4"></div>
          <p className="text-gray-600">Loading...</p>
        </div>
      </div>
    )
  }

  if (gameId) {
    return (
      <div>
        {!isConnected && (
          <div className="sticky top-0 z-30 bg-yellow-500 text-white text-center py-2 px-4 text-sm font-medium">
            Reconnecting to game server...
          </div>
        )}
        <GameTable
          players={players}
          currentTurn={currentTurn}
          deckSlots={deckSlots}
          remainingCards={remainingCards}
          roundWinner={roundWinner}
          gameOver={gameOver}
          onCardClick={handleCardClick}
          showPlayAgain={!isMultiplayer}
          onPlayAgain={startGame}
          onReturnToLobby={resetGame}
          onCloseGameOver={clearGameOver}
        />
        {cardError && (
          <div className="container mx-auto px-4 sm:px-8">
            <div className="p-3 bg-red-100 text-red-700 rounded flex items-center justify-between">
              <span>{cardError}</span>
              <button
                onClick={() => setCardError(null)}
                className="text-red-500 hover:text-red-700 ml-2"
              >
                &times;
              </button>
            </div>
          </div>
        )}
        <div className="container mx-auto px-4 sm:px-8 pb-8">
          <button
            className="mt-4 px-4 py-2 bg-gray-500 text-white rounded hover:bg-gray-600"
            onClick={() => resetGame()}
          >
            Back to Dashboard
          </button>
        </div>
      </div>
    )
  }

  if (lobbyGameId) {
    return (
      <div>
        <AuthModal />
        <GameLobby
          gameId={lobbyGameId}
          onBack={handleLobbyBack}
          onGameStart={handleGameStartFromLobby}
        />
      </div>
    )
  }

  const handleResumeGame = (data: QuickGameResponse) => {
    if (data.status === 'pending' || data.status === 'ready') {
      setLobbyGameId(data.game_id)
    } else {
      setGameStore(data.game_id, data.players, data.status, data.current_turn, data.bet, data.deck_slots || null)
    }
  }

  if (isAuthenticated) {
    return (
      <div>
        <AuthModal />
        <GameRules isOpen={rulesOpen} onClose={() => setRulesOpen(false)} />
        <UserDashboard
          onStartGame={startGame}
          onStartMultiplayerGame={startMultiplayerGame}
          onResumeGame={handleResumeGame}
          onViewLobby={handleViewLobby}
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
        onClick={() => openAuthModal()}
        className="fixed top-4 right-4 z-40 px-4 sm:px-5 py-2 bg-emerald-600 text-white font-semibold rounded-lg hover:bg-emerald-700 shadow-lg text-sm sm:text-base"
      >
        Create account / Connect
      </button>
      <div className="container mx-auto p-4 sm:p-8">
        <h1 className="text-2xl sm:text-3xl font-bold mb-4 sm:mb-6">FapFap Card Game</h1>
        <div className="bg-gray-100 p-4 sm:p-6 rounded-lg shadow mb-6 sm:mb-8">
          <h2 className="text-lg sm:text-xl font-semibold mb-3 sm:mb-4">Dashboard</h2>
          <p className="mb-2 text-sm sm:text-base">
            You are not logged in. You are allowed {gamesAllowed} games.
            Create an account to play more.
          </p>
          <div className="grid grid-cols-2 gap-3 sm:gap-4">
            <div className="bg-white p-3 sm:p-4 rounded shadow">
              <p className="text-sm sm:text-lg">Games Played</p>
              <p className="text-xl sm:text-2xl font-bold">{gamesPlayed}</p>
            </div>
            <div className="bg-white p-3 sm:p-4 rounded shadow">
              <p className="text-sm sm:text-lg">Total Wins</p>
              <p className="text-xl sm:text-2xl font-bold">{stats?.total_wins ?? 0}</p>
            </div>
            <div className="bg-white p-3 sm:p-4 rounded shadow">
              <p className="text-sm sm:text-lg">Credits</p>
              <p className="text-xl sm:text-2xl font-bold">{stats?.credits ?? 0}</p>
            </div>
            <div className="bg-white p-3 sm:p-4 rounded shadow">
              <p className="text-sm sm:text-lg">Remaining</p>
              <p className="text-xl sm:text-2xl font-bold">{gamesRemaining}</p>
            </div>
          </div>
          <div className="flex flex-wrap gap-2 sm:gap-3 mt-4 sm:mt-6">
            {gamesPlayed < gamesAllowed && (
              <button
                className="px-4 sm:px-6 py-2 sm:py-3 bg-blue-600 text-white text-sm sm:text-base font-semibold rounded-lg hover:bg-blue-700 disabled:opacity-50"
                disabled={startingGame}
                onClick={startGame}
              >
                {startingGame ? 'Starting...' : 'Start a quick game'}
              </button>
            )}
            <button
              className="px-4 sm:px-6 py-2 sm:py-3 border border-gray-400 text-gray-700 text-sm sm:text-base font-semibold rounded-lg hover:bg-gray-100"
              onClick={() => setRulesOpen(true)}
            >
              Rules
            </button>
          </div>
          {error && (
            <div className="mt-4 p-3 bg-red-100 text-red-700 rounded text-sm">
              Failed to start game: {error}
              <button
                onClick={() => setError(null)}
                className="ml-2 text-red-500 hover:text-red-700"
              >
                Dismiss
              </button>
            </div>
          )}
        </div>
        <div className="text-gray-500 text-xs sm:text-sm">
          Sprint 5: Resilience &amp; production polish.
        </div>
      </div>
    </div>
  )
}

function App() {
  return (
    <ToastProvider>
      <AppContent />
    </ToastProvider>
  )
}

export default App
