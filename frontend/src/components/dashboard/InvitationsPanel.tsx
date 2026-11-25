import { useState } from 'react'
import axios from 'axios'
import { useTranslation } from 'react-i18next'
import { useInvitationStore } from '../../stores/useInvitationStore'
import { useLanguageStore } from '../../stores/useLanguageStore'
import { extractApiError } from '../../utils/errors'

interface InvitationsPanelProps {
  freezeRemainingSec: number
  onViewLobby: (gameId: string) => void
  showToast: (msg: string) => void
}

export default function InvitationsPanel({
  freezeRemainingSec,
  onViewLobby,
  showToast,
}: InvitationsPanelProps) {
  const { t } = useTranslation()
  const invitations = useInvitationStore((s) => s.invitations)
  const [joiningGameId, setJoiningGameId] = useState<string | null>(null)

  if (invitations.length === 0) return null

  const handleAcceptInvite = async (gameId: string) => {
    setJoiningGameId(gameId)
    try {
      const res = await axios.post(`/api/games/${gameId}/respond?action=accept`)
      showToast(res.data.message)
      useInvitationStore.getState().removeInvitation(gameId)
      onViewLobby(gameId)
    } catch (err: unknown) {
      showToast(extractApiError(err).message || t('dashboard.failedJoinGame'))
    } finally {
      setJoiningGameId(null)
    }
  }

  const handleDeclineInvite = async (gameId: string) => {
    setJoiningGameId(gameId)
    try {
      await axios.post(`/api/games/${gameId}/respond?action=decline`)
      useInvitationStore.getState().removeInvitation(gameId)
      showToast(t('dashboard.invitationDeclined'))
    } catch (err: unknown) {
      showToast(extractApiError(err).message || t('dashboard.failedDeclineInvitation'))
    } finally {
      setJoiningGameId(null)
    }
  }

  return (
    <div className="bg-purple-50 p-4 sm:p-6 rounded-lg shadow mb-6 border border-purple-200">
      <h2 className="text-lg sm:text-xl font-semibold mb-3 sm:mb-4 text-purple-800">
        {t('dashboard.pendingInvitations')} ({invitations.length})
      </h2>
      <div className="space-y-2 sm:space-y-3">
        {invitations.map((inv) => (
          <div
            key={inv.invite_id}
            className="flex flex-col sm:flex-row sm:items-center justify-between bg-white p-3 sm:p-4 rounded-lg shadow-sm gap-2 sm:gap-0"
          >
            <div>
              <p className="font-semibold">{t('dashboard.possessionGame', { pseudo: inv.creator_pseudo })}</p>
              <p className="text-xs sm:text-sm text-gray-500">
                {t('dashboard.betInfo', { bet: inv.bet, current: inv.player_count, max: inv.max_players })}
              </p>
              {inv.expires_at && (
                <p className="text-xs text-gray-400">
                  {t('dashboard.expires', {
                    time: new Date(inv.expires_at).toLocaleTimeString(useLanguageStore.getState().language),
                  })}
                </p>
              )}
            </div>
            <div className="flex gap-2">
              <button
                onClick={() => handleAcceptInvite(inv.game_id)}
                disabled={joiningGameId === inv.game_id || freezeRemainingSec > 0}
                title={freezeRemainingSec > 0 ? t('dashboard.accountIsFrozen') : undefined}
                className="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 disabled:opacity-50 text-sm font-medium w-full sm:w-auto"
              >
                {joiningGameId === inv.game_id
                  ? '...'
                  : freezeRemainingSec > 0
                    ? t('dashboard.frozen')
                    : t('dashboard.accept')}
              </button>
              <button
                onClick={() => handleDeclineInvite(inv.game_id)}
                disabled={joiningGameId === inv.game_id}
                className="px-4 py-2 bg-gray-400 text-white rounded-lg hover:bg-gray-500 disabled:opacity-50 text-sm font-medium w-full sm:w-auto"
              >
                {t('dashboard.decline')}
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
