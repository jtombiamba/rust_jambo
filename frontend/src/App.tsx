import { useEffect, useState, useCallback, useRef } from 'react'
import axios from 'axios'
import { useTranslation } from 'react-i18next'
import './App.css'
import GameTable from './components/GameTable'
import AuthModal from './components/AuthModal'
import GameRules from './components/GameRules'
import UserDashboard from './components/UserDashboard'
import GameLobby from './components/GameLobby'
import Footer from './components/Footer'
import { ToastProvider } from './components/Toast'
import { useToast } from './components/useToast'
import { useGameStore } from './stores/useGameStore'
import { useAuthStore } from './stores/useAuthStore'
import { useLanguageStore } from './stores/useLanguageStore'
import { useRoomStore } from './stores/useRoomStore'
import { extractApiError } from './utils/errors'
import LanguageSwitcher from './components/LanguageSwitcher'
import RoomList from './components/RoomList'
import RoomDashboard from './components/RoomDashboard'
import CreateRoomModal from './components/CreateRoomModal'
import JoinRoomForm from './components/JoinRoomForm'
import CreateRunModal from './components/CreateRunModal'
import { useGameWebSocket } from './hooks/useGameWebSocket'
import { useRoomWebSocket } from './hooks/useRoomWebSocket'
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
  ws_token?: string
  step_by_step?: boolean
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
  const [menuOpen, setMenuOpen] = useState(false)
  const [lobbyGameId, setLobbyGameId] = useState<string | null>(null)
  const [pendingInvite, setPendingInvite] = useState<{ gameId: string; action: string } | null>(null)
  const [wsToken, setWsToken] = useState<string | null>(null)
  const { gameId, players, currentTurn, deckSlots, remainingCards, gameOver, roundWinner, setGame: setGameStore, resetGame, clearGameOver, setStepByStep } = useGameStore()
  const isMultiplayer = players.length > 0 && players.every(p => p.type === 'human')
  const { isAuthenticated, openAuthModal, checkAuth, clearPendingInvite, user } = useAuthStore()
  const { isConnected } = useWebSocket({ gameId: gameId || '' })
  const { showToast } = useToast()
  const { t } = useTranslation()
  const { init: initLanguage, syncFromUser } = useLanguageStore()
  const { setActiveRoomId, showCreateRoom, setShowCreateRoom, showJoinRoom, setShowJoinRoom, clear: clearRoomStore } = useRoomStore()
  const [roomId, setRoomId] = useState<string | null>(null)
  const [showCreateRun, setShowCreateRun] = useState(false)
  const [createRunRoomId, setCreateRunRoomId] = useState<string | null>(null)
  const [stepByStepToggle, setStepByStepToggle] = useState(false)
  const [runGameIndex, setRunGameIndex] = useState(0)
  const [runTotalGames, setRunTotalGames] = useState(0)
  const [runId, setRunId] = useState<string | null>(null)
  const [autoStartCountdown, setAutoStartCountdown] = useState(0)
  const autoStartRef = useRef(false)
  const [roomRefreshKey, setRoomRefreshKey] = useState(0)
  useGameWebSocket(gameId, wsToken)
  useRoomWebSocket({
    roomId: isAuthenticated && roomId ? roomId : null,
    onEvent: (event) => {
      if (event.type === 'game_started' && event.game_id && event.run_id) {
        setRunId(event.run_id)
        setRoomRefreshKey(k => k + 1)
        if (event.total_games) setRunTotalGames(event.total_games)
        if (event.game_index !== undefined) setRunGameIndex(event.game_index + 1)
        useRoomStore.getState().setCurrentRunGameId(event.game_id)
      }
      if (event.type === 'run_completed' || event.type === 'run_created' || event.type === 'member_joined' || event.type === 'member_left') {
        setRoomRefreshKey(k => k + 1)
      }
    },
  })

  const processInvite = useCallback((gameId: string, action: string) => {
    const endpoint = `/api/games/${gameId}/respond?action=${encodeURIComponent(action)}`
    axios.post(endpoint)
      .then((res) => {
        if (action === 'accept') {
          showToast(res.data.message || t('dashboard.multiplayerCreated'), 'success')
          setLobbyGameId(gameId)
        } else {
          showToast(t('dashboard.invitationDeclined'), 'success')
        }
      })
      .catch((err) => {
        const error = extractApiError(err)
        showToast(error.message || t('dashboard.failedJoinGame'), 'error', error.requestId)
      })
  }, [showToast, t])

  useEffect(() => {
    checkAuth()
  }, [checkAuth])

  useEffect(() => {
    initLanguage()
  }, [initLanguage])

  useEffect(() => {
    if (isAuthenticated && user?.language) {
      syncFromUser(user.language)
    }
  }, [isAuthenticated, user?.language, syncFromUser])

  useEffect(() => {
    document.documentElement.lang = useLanguageStore.getState().language
    const unsubscribe = useLanguageStore.subscribe((state, prevState) => {
      if (state.language !== prevState.language) {
        document.documentElement.lang = state.language
      }
    })
    return unsubscribe
  }, [])

  useEffect(() => {
    const params = new URLSearchParams(window.location.search)
    const inviteGameId = params.get('invite_game_id')
    const inviteAction = params.get('invite_action')
    if (inviteGameId && inviteAction) {
      if (isAuthenticated) {
        clearPendingInvite()
        processInvite(inviteGameId, inviteAction)
        const url = new URL(window.location.href)
        url.searchParams.delete('invite_game_id')
        url.searchParams.delete('invite_action')
        window.history.replaceState({}, '', url.toString())
      } else {
        setPendingInvite({ gameId: inviteGameId, action: inviteAction })
        openAuthModal(t('auth.loginForInvite'))
      }
    }
  }, [isAuthenticated, openAuthModal, processInvite, clearPendingInvite, t])

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
        showToast(t('dashboard.failedLoadGame'), 'error')
        setLoading(false)
      })
  }, [isAuthenticated, showToast, t])

  const startGame = (stepByStepParam = false) => {
    const useStepByStep = stepByStepParam || stepByStepToggle
    console.log("use step by step = ", useStepByStep);
    setStartingGame(true)
    setError(null)
    if (isAuthenticated) {
      axios.post<QuickGameResponse>('/api/me/games', { game_mode: 'solo', step_by_step: useStepByStep })
        .then(response => {
          setGameStore(response.data.game_id, response.data.players, response.data.status, response.data.current_turn, response.data.bet)
          setStepByStep(response.data.step_by_step ?? useStepByStep)
          setStartingGame(false)
        })
        .catch(err => {
          console.error('Failed to start game', err)
          const error = extractApiError(err)
          setError(error.message)
          showToast(error.message, 'error', error.requestId)
          setStartingGame(false)
        })
    } else {
      const url = useStepByStep ? '/api/quickie?step_by_step=true' : '/api/quickie'
      axios.post<QuickGameResponse>(url)
        .then(response => {
          setGameStore(response.data.game_id, response.data.players, response.data.status, response.data.current_turn, response.data.bet)
          setStepByStep(response.data.step_by_step ?? useStepByStep)
          // Store the one-time game token for WebSocket authentication
          if (response.data.ws_token) {
            setWsToken(response.data.ws_token)
          }
          setStartingGame(false)
        })
        .catch(err => {
          console.error('Failed to start game', err)
          const error = extractApiError(err)
          setError(error.message)
          showToast(error.message, 'error', error.requestId)
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
        const error = extractApiError(err)
        setError(error.message)
        showToast(error.message, 'error', error.requestId)
        setStartingGame(false)
        return { gameId: '', error: error.message }
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
        const error = extractApiError(err);
        setCardError(error.message);
        showToast(error.message, 'error');// , error.requestId);
      })
      .finally(() => setPlayingCard(null));
  };

  const handleAdvanceBot = () => {
    if (!gameId) return;
    const humanPlayer = players.find(p => p.type === 'human');
    if (!humanPlayer) return;
    setPlayingCard(-1);
    const tokenParam = wsToken ? `?token=${wsToken}` : '';
    axios.post(`/api/game/${gameId}/advance-bot${tokenParam}`, {
      player_id: humanPlayer.id,
    })
      .catch(err => {
        console.error('Failed to advance bot', err);
        const error = extractApiError(err);
        showToast(error.message, 'error');
      })
      .finally(() => setPlayingCard(null));
  };

  const handleEvaluateRound = () => {
    if (!gameId) return;
    const humanPlayer = players.find(p => p.type === 'human');
    if (!humanPlayer) return;
    setPlayingCard(-1);
    const tokenParam = wsToken ? `?token=${wsToken}` : '';
    axios.post(`/api/game/${gameId}/evaluate-round${tokenParam}`, {
      player_id: humanPlayer.id,
    })
      .catch(err => {
        console.error('Failed to evaluate round', err);
        const error = extractApiError(err);
        showToast(error.message, 'error');
      })
      .finally(() => setPlayingCard(null));
  };

  const handleLocalStats = () => {
   const storedStats = getStoredStats()
    if (storedStats) {
      setStats(storedStats)
    }
  }

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

  const handleOpenRoom = (rid: string) => {
    setRoomId(rid)
    setActiveRoomId(rid)
  }

  const handleRoomBack = () => {
    setRoomId(null)
    setActiveRoomId(null)
    setRunId(null)
    setRunGameIndex(0)
    setRunTotalGames(0)
    clearRoomStore()
  }

  const handleOpenCreateRun = (rid: string) => {
    setCreateRunRoomId(rid)
    setShowCreateRun(true)
  }

  const handlePlayNextInRun = useCallback(async () => {
    if (!runId) return
    autoStartRef.current = false
    setAutoStartCountdown(0)
    setStartingGame(true)
    try {
      const res = await axios.post<{ game_id: string; game_index: number; total_games: number; current_game_index: number }>(`/api/me/runs/${runId}/next-game`)
      if (res.data.game_id) {
        const newIndex = res.data.current_game_index ?? res.data.game_index + 1
        setRunGameIndex(newIndex)
        setRunTotalGames(res.data.total_games)
        useRoomStore.getState().setCurrentRunGameId(res.data.game_id)
        const gameRes = await axios.get<QuickGameResponse>(`/api/me/games/${res.data.game_id}`)
        if (gameRes.data.game_id && gameRes.data.players) {
          setGameStore(
            gameRes.data.game_id,
            gameRes.data.players,
            gameRes.data.status,
            gameRes.data.current_turn,
            gameRes.data.bet,
            gameRes.data.deck_slots || null
          )
          clearGameOver()
        }
      }
    } catch {
      showToast('Failed to start next game', 'error')
    } finally {
      setStartingGame(false)
    }
  }, [runId, clearGameOver, setGameStore, showToast])

  useEffect(() => {
    if (!gameOver?.isGameOver || !runId || runGameIndex >= runTotalGames) return
    autoStartRef.current = true
    setAutoStartCountdown(10)
    const interval = setInterval(() => {
      setAutoStartCountdown((prev) => {
        if (prev <= 1) {
          clearInterval(interval)
          return 0
        }
        return prev - 1
      })
    }, 1000)
    const timeout = setTimeout(() => {
      if (autoStartRef.current) {
        handlePlayNextInRun()
      }
    }, 10000)
    return () => {
      autoStartRef.current = false
      clearInterval(interval)
      clearTimeout(timeout)
    }
  }, [gameOver?.isGameOver, runId, runGameIndex, runTotalGames, handlePlayNextInRun])

  if (loading) {
    return (
      <div className="container mx-auto p-4 sm:p-8 flex items-center justify-center min-h-screen">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto mb-4"></div>
          <p className="text-gray-600">{t('common.loading')}</p>
        </div>
      </div>
    )
  }

  if (gameId) {
    return (
      <div className="min-h-screen flex flex-col">
        <div className="flex-1">
          {!isConnected && (
            <div className="sticky top-0 z-30 bg-yellow-500 text-white text-center py-2 px-4 text-sm font-medium">
              {t('common.reconnecting')}
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
            showPlayAgain={!isMultiplayer && !runId}
            onPlayAgain={runId ? handlePlayNextInRun : startGame}
            onReturnToLobby={() => {
              handleLocalStats()
              resetGame()
              setWsToken(null)
              setRunId(null)
              setAutoStartCountdown(0)
            }}
            onCloseGameOver={clearGameOver}
            onAdvanceBot={handleAdvanceBot}
            onEvaluateRound={handleEvaluateRound}
          />
          {gameOver?.isGameOver && runId && (
            <div className="container mx-auto px-4 sm:px-8">
              <div className="p-4 bg-blue-50 border border-blue-200 rounded-lg text-center">
                <p className="text-sm text-blue-700 mb-2">
                  Game {runGameIndex} of {runTotalGames}
                </p>
                {runGameIndex >= runTotalGames ? (
                  <button
                    className="px-4 py-2 bg-gray-400 text-white text-sm font-semibold rounded-lg cursor-not-allowed"
                    disabled
                  >
                    Run Complete
                  </button>
                ) : autoStartCountdown > 0 ? (
                  <div>
                    <p className="text-sm text-blue-600 mb-2">
                      Next game starting in {autoStartCountdown}s...
                    </p>
                    <button
                      className="px-4 py-2 bg-green-600 text-white text-sm font-semibold rounded-lg hover:bg-green-700 mr-2"
                      onClick={handlePlayNextInRun}
                    >
                      Play Now
                    </button>
                    <button
                      className="px-4 py-2 border border-gray-300 text-gray-700 text-sm rounded-lg hover:bg-gray-100"
                      onClick={() => {
                        setAutoStartCountdown(0)
                        resetGame()
                        setWsToken(null)
                        setRunId(null)
                      }}
                    >
                      Back to Room
                    </button>
                  </div>
                ) : (
                  <button
                    className="px-4 py-2 bg-green-600 text-white text-sm font-semibold rounded-lg hover:bg-green-700"
                    onClick={handlePlayNextInRun}
                    disabled={startingGame}
                  >
                    {startingGame ? 'Starting...' : 'Play Next Game'}
                  </button>
                )}
              </div>
            </div>
          )}
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
              onClick={() => {
                handleLocalStats()
                resetGame()
                setWsToken(null)
                setRunId(null)
                setAutoStartCountdown(0)
              }}
            >
              {t('common.backToDashboard')}
            </button>
          </div>
        </div>
        <Footer />
      </div>
    )
  }

  if (lobbyGameId) {
    return (
      <div className="min-h-screen flex flex-col">
        <AuthModal />
        <div className="flex-1">
          <GameLobby
            gameId={lobbyGameId}
            onBack={handleLobbyBack}
            onGameStart={handleGameStartFromLobby}
          />
        </div>
        <Footer />
      </div>
    )
  }

  const handleRoomCreated = (rid: string) => {
    handleOpenRoom(rid)
  }

  const handleRunCreated = () => {
    setShowCreateRun(false)
    setCreateRunRoomId(null)
    if (roomId) {
      handleOpenRoom(roomId)
    }
  }

  if (isAuthenticated && roomId) {
    return (
      <div className="min-h-screen flex flex-col">
        <AuthModal />
        <CreateRoomModal
          isOpen={showCreateRoom}
          onClose={() => setShowCreateRoom(false)}
          onCreated={handleRoomCreated}
        />
        <JoinRoomForm
          isOpen={showJoinRoom}
          onClose={() => setShowJoinRoom(false)}
          onJoined={(rid) => handleOpenRoom(rid)}
        />
        {createRunRoomId && (
          <CreateRunModal
            isOpen={showCreateRun}
            roomId={createRunRoomId}
            onClose={() => { setShowCreateRun(false); setCreateRunRoomId(null) }}
            onCreated={handleRunCreated}
          />
        )}
        <div className="flex-1">
          <RoomDashboard
            roomId={roomId}
            onBack={handleRoomBack}
              onStartGame={async (gameIdVal: string, runIdVal: string, gameIdx: number, total: number) => {
              setRunId(runIdVal)
              setRunGameIndex(gameIdx)
              setRunTotalGames(total)
              useRoomStore.getState().setCurrentRunGameId(gameIdVal)
              try {
                const gameRes = await axios.get<QuickGameResponse>(`/api/me/games/${gameIdVal}`)
                if (gameRes.data.game_id && gameRes.data.players) {
                  setGameStore(
                    gameRes.data.game_id,
                    gameRes.data.players,
                    gameRes.data.status,
                    gameRes.data.current_turn,
                    gameRes.data.bet,
                    gameRes.data.deck_slots || null
                  )
                }
              } catch {
                showToast('Failed to load game', 'error')
              }
            }}
            onCreateRun={handleOpenCreateRun}
            refreshKey={roomRefreshKey}
          />
        </div>
        <Footer />
      </div>
    )
  }

  const handleResumeGame = (data: QuickGameResponse) => {
    if (data.status === 'pending' || data.status === 'ready') {
      setLobbyGameId(data.game_id)
    } else {
      setGameStore(data.game_id, data.players, data.status, data.current_turn, data.bet, data.deck_slots || null)
      setStepByStep(data.step_by_step ?? false)
    }
  }

  if (isAuthenticated) {
    return (
      <div className="min-h-screen flex flex-col">
        <AuthModal />
        <GameRules isOpen={rulesOpen} onClose={() => setRulesOpen(false)} />
        <CreateRoomModal
          isOpen={showCreateRoom}
          onClose={() => setShowCreateRoom(false)}
          onCreated={handleRoomCreated}
        />
        <JoinRoomForm
          isOpen={showJoinRoom}
          onClose={() => setShowJoinRoom(false)}
          onJoined={(rid) => handleOpenRoom(rid)}
        />
        {createRunRoomId && (
          <CreateRunModal
            isOpen={showCreateRun}
            roomId={createRunRoomId}
            onClose={() => { setShowCreateRun(false); setCreateRunRoomId(null) }}
            onCreated={handleRunCreated}
          />
        )}
        <div className="flex-1">
          <UserDashboard
            onStartGame={startGame}
            onStartMultiplayerGame={startMultiplayerGame}
            onResumeGame={handleResumeGame}
            onViewLobby={handleViewLobby}
            starting={startingGame}
            error={error}
            stepByStep={stepByStepToggle}
            onStepByStepChange={setStepByStepToggle}
          />
          <div className="container mx-auto px-4 sm:px-8">
            <RoomList
              onSelectRoom={handleOpenRoom}
              onCreateRoom={() => setShowCreateRoom(true)}
              onJoinRoom={() => setShowJoinRoom(true)}
            />
          </div>
        </div>
        <Footer />
      </div>
    )
  }

  const gamesPlayed = stats?.games_played ?? 0
  const gamesAllowed = stats?.games_allowed ?? 10
  const gamesRemaining = Math.max(0, gamesAllowed - gamesPlayed)
  const anonymousCredits = stats?.credits ?? 0
  const anonymousOutOfCredits = anonymousCredits <= 0

  return (
    <div className="min-h-screen flex flex-col">
      <AuthModal />
      <GameRules isOpen={rulesOpen} onClose={() => setRulesOpen(false)} />
      <div className="fixed top-4 right-4 z-40">
        <div className="hidden sm:flex gap-2 sm:gap-3">
          <LanguageSwitcher />
          <button
            onClick={() => setRulesOpen(true)}
            className="px-3 sm:px-5 py-2 border border-gray-400 text-gray-700 font-semibold rounded-lg hover:bg-gray-100 shadow-lg text-sm sm:text-base"
          >
            {t('dashboard.rules')}
          </button>
          <button
            onClick={() => openAuthModal()}
            className="px-4 sm:px-5 py-2 bg-emerald-600 text-white font-semibold rounded-lg hover:bg-emerald-700 shadow-lg text-sm sm:text-base"
          >
            {t('auth.createAccountConnect')}
          </button>
        </div>
        <div className="sm:hidden relative">
          <button
            onClick={() => setMenuOpen(!menuOpen)}
            className="w-10 h-10 flex items-center justify-center bg-white border border-gray-300 rounded-lg shadow-lg text-gray-700 hover:bg-gray-100"
            aria-label={t('common.menu')}
          >
            <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
              <circle cx="10" cy="3" r="2"/>
              <circle cx="10" cy="10" r="2"/>
              <circle cx="10" cy="17" r="2"/>
            </svg>
          </button>
          {menuOpen && (
            <div className="absolute right-0 top-12 bg-white border border-gray-200 rounded-lg shadow-xl py-1 min-w-[140px] z-50">
              <LanguageSwitcher />
              <button
                onClick={() => { setRulesOpen(true); setMenuOpen(false) }}
                className="w-full text-left px-4 py-2 text-sm text-gray-700 hover:bg-gray-100"
              >
                {t('dashboard.rules')}
              </button>
              <button
                onClick={() => { openAuthModal(); setMenuOpen(false) }}
                className="w-full text-left px-4 py-2 text-sm text-gray-700 hover:bg-gray-100"
              >
                {t('auth.createAccountConnect')}
              </button>
            </div>
          )}
        </div>
      </div>
      <div className="container mx-auto p-4 sm:p-8 flex-1">
        <h1 className="text-2xl sm:text-3xl font-bold mb-4 sm:mb-6">{t('common.title')}</h1>
        <div className="bg-gray-100 p-4 sm:p-6 rounded-lg shadow mb-6 sm:mb-8">
          <h2 className="text-lg sm:text-xl font-semibold mb-3 sm:mb-4">{t('dashboard.title')}</h2>
          <p className="mb-2 text-sm sm:text-base">
            {t('dashboard.notLoggedIn', { allowed: gamesAllowed })}
          </p>
          <div className="grid grid-cols-2 gap-3 sm:gap-4">
            <div className="bg-white p-3 sm:p-4 rounded shadow">
              <p className="text-sm sm:text-lg">{t('dashboard.gamesPlayed')}</p>
              <p className="text-xl sm:text-2xl font-bold">{gamesPlayed}</p>
            </div>
            <div className="bg-white p-3 sm:p-4 rounded shadow">
              <p className="text-sm sm:text-lg">{t('dashboard.totalWins')}</p>
              <p className="text-xl sm:text-2xl font-bold">{stats?.total_wins ?? 0}</p>
            </div>
            <div className="bg-white p-3 sm:p-4 rounded shadow">
              <p className="text-sm sm:text-lg">{t('dashboard.credit')}</p>
              <p className="text-xl sm:text-2xl font-bold">{anonymousCredits}</p>
            </div>
            <div className="bg-white p-3 sm:p-4 rounded shadow">
              <p className="text-sm sm:text-lg">{t('dashboard.remaining')}</p>
              <p className="text-xl sm:text-2xl font-bold">{gamesRemaining}</p>
            </div>
          </div>
          <div className="flex flex-wrap gap-2 sm:gap-3 mt-4 sm:mt-6">
            {anonymousOutOfCredits ? (
              <div className="w-full bg-amber-50 border border-amber-300 rounded-lg p-4">
                <p className="text-amber-800 font-semibold mb-1">
                  {t('dashboard.outOfCredits')}
                </p>
                <p className="text-amber-600 text-sm">
                  {t('dashboard.createAccountCredits')}
                </p>
              </div>
            ) : gamesPlayed < gamesAllowed && (
              <button
                className="px-4 sm:px-6 py-2 sm:py-3 bg-blue-600 text-white text-sm sm:text-base font-semibold rounded-lg hover:bg-blue-700 disabled:opacity-50"
                disabled={startingGame}
                onClick={() => startGame()}
              >
                {startingGame ? t('dashboard.startingGame') : t('dashboard.startQuickGame')}
              </button>
            )}
            {!anonymousOutOfCredits && (
              <label className="flex items-center gap-2 px-3 py-2 bg-white border border-gray-300 rounded-lg cursor-pointer hover:bg-gray-50">
                <input
                  type="checkbox"
                  checked={stepByStepToggle}
                  onChange={(e) => setStepByStepToggle(e.target.checked)}
                  className="w-4 h-4 text-blue-600 rounded"
                />
                <span className="text-sm text-gray-700">{t('game.stepByStep')}</span>
              </label>
            )}
          </div>
          {error && (
            <div className="mt-4 p-3 bg-red-100 text-red-700 rounded text-sm">
              {t('dashboard.failedStartGame')}: {error}
              <button
                onClick={() => setError(null)}
                className="ml-2 text-red-500 hover:text-red-700"
              >
                {t('common.dismiss')}
              </button>
            </div>
          )}
        </div>
      </div>
      <Footer />
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
