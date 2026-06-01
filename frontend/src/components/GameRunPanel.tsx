import { useState } from 'react'
import axios from 'axios'
import { extractApiError } from '../utils/errors'
import { useTranslation } from 'react-i18next'
import { ActiveRun } from '../stores/useRoomStore'

interface Props {
  run: ActiveRun
  currentGameId?: string
  onStartGame: (gameId: string, runId: string, gameIndex: number, totalGames: number) => void
  onBack: () => void
}

const GameRunPanel: React.FC<Props> = ({ run, currentGameId, onStartGame, onBack }) => {
  const { t } = useTranslation()
  const [starting, setStarting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [toast, setToast] = useState<string | null>(null)

  const allGamesStarted = run.current_game_index >= run.num_games
  const isActive = run.status === 'active' || (allGamesStarted && currentGameId != null)

  const showToast = (msg: string) => {
    setToast(msg)
    setTimeout(() => setToast(null), 3000)
  }

  const handleStartNextGame = async () => {
    setStarting(true)
    setError(null)
    try {
      const res = await axios.post<{ game_id: string; game_index: number; total_games: number; current_game_index: number }>(`/api/me/runs/${run.id}/next-game`)
      if (res.data.game_id) {
        onStartGame(res.data.game_id, run.id, res.data.current_game_index ?? res.data.game_index + 1, res.data.total_games)
        showToast(t('rooms.gameStarted', { index: res.data.game_index + 1, total: res.data.total_games }))
      }
    } catch (err: unknown) {
      const msg = extractApiError(err).message || t('run.failedStart')
      setError(msg)
    } finally {
      setStarting(false)
    }
  }

  const progress = run.num_games > 0 ? Math.round((run.current_game_index / run.num_games) * 100) : 0

  return (
    <div className="bg-white p-4 sm:p-6 rounded-lg shadow">
      <div className="flex justify-between items-center mb-4">
        <h3 className="text-lg font-semibold">{t('rooms.activeRun')}</h3>
        <button
          onClick={onBack}
          className="px-3 py-1.5 border border-gray-300 text-gray-700 text-sm rounded-lg hover:bg-gray-100"
        >
          {t('rooms.backToRoom')}
        </button>
      </div>

      <div className="mb-4">
        <div className="flex justify-between text-sm text-gray-600 mb-1">
          <span>{t('run.progress')}: {run.current_game_index} / {run.num_games}</span>
          <span>{progress}%</span>
        </div>
        <div className="w-full bg-gray-200 rounded-full h-2.5">
          <div
            className="bg-blue-600 h-2.5 rounded-full transition-all duration-500"
            style={{ width: `${progress}%` }}
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3 mb-4 text-sm">
        <div className="bg-gray-50 p-3 rounded">
          <span className="text-gray-500">{t('run.betPerGame')}</span>
          <p className="font-semibold">{run.bet_per_game} credits</p>
        </div>
        <div className="bg-gray-50 p-3 rounded">
          <span className="text-gray-500">{t('run.status')}</span>
          <p className="font-semibold capitalize">{run.status}</p>
        </div>
      </div>

      {run.players && run.players.length > 0 && (
        <div className="mb-4">
          <h4 className="text-sm font-semibold text-gray-700 mb-2">{t('run.players')}</h4>
          <div className="grid gap-1">
            {run.players.map((p) => (
              <div key={p.user_id} className={`flex justify-between items-center p-2 rounded text-sm ${p.kicked ? 'bg-red-50 text-red-500' : 'bg-gray-50'}`}>
                <span>{p.pseudo}</span>
                <span className="text-gray-500">{p.provisioned_credits} credits</span>
              </div>
            ))}
          </div>
        </div>
      )}

      <button
        className="w-full px-4 py-3 bg-green-600 text-white font-semibold rounded-lg hover:bg-green-700 disabled:opacity-50"
        onClick={handleStartNextGame}
        disabled={starting || !isActive || allGamesStarted}
      >
        {starting
          ? t('run.starting')
          : !isActive
            ? `Run ${run.status}`
            : allGamesStarted
              ? t('run.complete')
              : run.current_game_index === 0
                ? t('run.startFirst')
                : t('run.playNext')}
      </button>

      {currentGameId && isActive && (
        <button
          className="w-full mt-2 px-4 py-3 bg-blue-600 text-white font-semibold rounded-lg hover:bg-blue-700"
          onClick={() => onStartGame(currentGameId, run.id, run.current_game_index, run.num_games)}
        >
          {t('rooms.enterCurrentGame')}
        </button>
      )}

      {error && (
        <div className="mt-3 p-2 bg-red-100 text-red-700 rounded text-sm">{error}</div>
      )}

      {toast && (
        <div className="fixed bottom-4 right-4 z-50 px-4 py-2 bg-gray-800 text-white rounded shadow-lg text-sm">
          {toast}
        </div>
      )}
    </div>
  )
}

export default GameRunPanel
