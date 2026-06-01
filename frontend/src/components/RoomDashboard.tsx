import { useEffect, useState, useCallback } from 'react'
import axios from 'axios'
import { useTranslation } from 'react-i18next'
import { useRoomStore } from '../stores/useRoomStore'
import { extractApiError } from '../utils/errors'
import GameRunPanel from './GameRunPanel'

interface Props {
  roomId: string
  onBack: () => void
  onStartGame: (gameId: string, runId: string, gameIndex: number, totalGames: number) => void
  onCreateRun: (roomId: string) => void
  refreshKey?: number
}

interface RoomDetail {
  id: string
  name: string
  creator_id: string
  invitation_code: string
  created_at: string
  members: Array<{ user_id: string; pseudo: string; joined_at: string }>
  member_count: number
  active_run?: {
    id: string
    num_games: number
    bet_per_game: number
    current_game_index: number
    status: string
    all_games_created?: boolean
    current_game?: {
      game_id: string
      game_index: number
      status: string
    }
  }
}

const RoomDashboard: React.FC<Props> = ({ roomId, onBack, onStartGame, onCreateRun, refreshKey }) => {
  const { setActiveRun } = useRoomStore()
  const { t } = useTranslation()
  const [room, setRoom] = useState<RoomDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [inviteEmail, setInviteEmail] = useState('')
  const [showInvite, setShowInvite] = useState(false)
  const [inviting, setInviting] = useState(false)
  const [toast, setToast] = useState<string | null>(null)

  const showT = (msg: string) => {
    setToast(msg)
    setTimeout(() => setToast(null), 3000)
  }

  const fetchRoom = useCallback(() => {
    setLoading(true)
    setError(null)
    axios.get<RoomDetail>(`/api/me/rooms/${roomId}`)
      .then((res) => {
        setRoom(res.data)
        if (res.data.active_run) {
          setActiveRun({
            id: res.data.active_run.id,
            room_id: roomId,
            num_games: res.data.active_run.num_games,
            bet_per_game: res.data.active_run.bet_per_game,
            current_game_index: res.data.active_run.current_game_index,
            status: res.data.active_run.status,
          })
          if (res.data.active_run.current_game?.game_id) {
            useRoomStore.getState().setCurrentRunGameId(res.data.active_run.current_game.game_id)
          }
        }
      })
      .catch((err) => {
        setError(extractApiError(err).message || t('rooms.failedLoad'))
      })
      .finally(() => setLoading(false))
  }, [roomId, setActiveRun, t])

  useEffect(() => {
    fetchRoom()
  }, [roomId, refreshKey, fetchRoom])

  const copyCode = () => {
    if (room?.invitation_code) {
      navigator.clipboard.writeText(room.invitation_code)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    }
  }

  const handleInvite = async () => {
    if (!inviteEmail.trim()) return
    setInviting(true)
    try {
      await axios.post(`/api/me/rooms/${roomId}/invite`, { email: inviteEmail.trim() })
      showT(t('rooms.inviteSent'))
      setInviteEmail('')
      setShowInvite(false)
    } catch {
      showT(t('rooms.failedSendInvite'))
    } finally {
      setInviting(false)
    }
  }

  const handleLeaveRoom = async () => {
    if (!confirm(t('rooms.leaveConfirm'))) return
    try {
      await axios.post(`/api/me/rooms/${roomId}/leave`)
      onBack()
    } catch (err: unknown) {
      showT(extractApiError(err).message || t('rooms.failedLeave'))
    }
  }

  if (loading) {
    return (
      <div className="container mx-auto p-4 sm:p-8 text-center">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto mb-4"></div>
        <p className="text-gray-600">{t('rooms.loading')}</p>
      </div>
    )
  }

  if (error || !room) {
    return (
      <div className="container mx-auto p-4 sm:p-8">
        <button onClick={onBack} className="mb-4 px-3 py-1.5 border border-gray-300 rounded-lg text-sm hover:bg-gray-100">
          &larr; {t('rooms.backToDashboard')}
        </button>
        <div className="p-4 bg-red-100 text-red-700 rounded">{error || t('rooms.notFound')}</div>
      </div>
    )
  }

  return (
    <div className="container mx-auto p-4 sm:p-8">
      <div className="flex items-center justify-between mb-6">
        <div>
          <button onClick={onBack} className="mb-2 px-3 py-1.5 border border-gray-300 rounded-lg text-sm hover:bg-gray-100">
            &larr; {t('rooms.backToDashboard')}
          </button>
          <h1 className="text-2xl sm:text-3xl font-bold">{room.name}</h1>
        </div>
        <button
          onClick={handleLeaveRoom}
          className="px-3 py-1.5 border border-red-300 text-red-600 text-sm rounded-lg hover:bg-red-50"
        >
          {t('rooms.leaveRoom')}
        </button>
      </div>

      <div className="flex flex-wrap items-center gap-3 mb-6">
        <div className="bg-gray-100 px-3 py-1.5 rounded-lg flex items-center gap-2">
          <span className="text-sm text-gray-600">{t('rooms.invitationCode')}:</span>
          <span className="font-mono font-semibold text-sm">{room.invitation_code}</span>
          <button onClick={copyCode} className="px-2 py-0.5 bg-blue-100 text-blue-700 text-xs rounded hover:bg-blue-200">
            {copied ? t('rooms.copied') : t('rooms.copyCode')}
          </button>
        </div>
        <button
          onClick={() => setShowInvite(!showInvite)}
          className="px-3 py-1.5 border border-gray-300 text-gray-700 text-sm rounded-lg hover:bg-gray-100"
        >
          {t('rooms.inviteByEmail')}
        </button>
        <span className="text-sm text-gray-500">{room.member_count} {t('rooms.members')}</span>
      </div>

      {showInvite && (
        <div className="bg-gray-50 p-4 rounded-lg mb-4 flex gap-2 items-center">
          <input
            type="email"
            placeholder={t('rooms.emailPlaceholder')}
            className="flex-1 px-3 py-1.5 border border-gray-300 rounded text-sm"
            value={inviteEmail}
            onChange={(e) => setInviteEmail(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleInvite()}
          />
          <button
            className="px-3 py-1.5 bg-blue-600 text-white text-sm rounded-lg hover:bg-blue-700 disabled:opacity-50"
            onClick={handleInvite}
            disabled={inviting}
          >
            {inviting ? t('rooms.sending') : t('rooms.sendInvite')}
          </button>
          <button
            className="px-3 py-1.5 border border-gray-300 text-gray-700 text-sm rounded-lg hover:bg-gray-100"
            onClick={() => setShowInvite(false)}
          >
            {t('common.cancel')}
          </button>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-gray-100 p-4 sm:p-6 rounded-lg shadow">
          <h2 className="text-lg font-semibold mb-3">{t('rooms.members')} ({room.member_count})</h2>
          <div className="space-y-1">
            {room.members.map((m) => (
              <div key={m.user_id} className="flex justify-between items-center p-2 bg-white rounded text-sm">
                <span className="font-medium">{m.pseudo}</span>
                <span className="text-gray-400 text-xs">{new Date(m.joined_at).toLocaleDateString()}</span>
              </div>
            ))}
          </div>
        </div>

        <div>
          {room.active_run ? (
            <GameRunPanel
              run={{
                id: room.active_run.id,
                room_id: roomId,
                num_games: room.active_run.num_games,
                bet_per_game: room.active_run.bet_per_game,
                current_game_index: room.active_run.current_game_index,
                status: room.active_run.status,
                all_games_created: room.active_run.all_games_created,
              }}
              currentGameId={room.active_run.current_game?.game_id}
              onStartGame={onStartGame}
              onBack={() => {
                setActiveRun(null)
                fetchRoom()
              }}
            />
          ) : (
            <div className="bg-gray-100 p-4 sm:p-6 rounded-lg shadow">
              <h2 className="text-lg font-semibold mb-3">{t('rooms.noRun')}</h2>
              <p className="text-sm text-gray-600 mb-4">{t('rooms.noRunDesc')}</p>
              <button
                className="px-4 py-2 bg-purple-600 text-white font-semibold rounded-lg hover:bg-purple-700 text-sm"
                onClick={() => onCreateRun(roomId)}
              >
                {t('rooms.createRun')}
              </button>
            </div>
          )}
        </div>
      </div>

      {toast && (
        <div className="fixed bottom-4 right-4 z-50 px-4 py-2 bg-gray-800 text-white rounded shadow-lg text-sm">
          {toast}
        </div>
      )}
    </div>
  )
}

export default RoomDashboard
