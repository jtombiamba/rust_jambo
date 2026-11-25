import { useState } from 'react'
import axios from 'axios'
import { useTranslation } from 'react-i18next'
import { extractApiError } from '../utils/errors'
import type { ProfileData } from '../components/dashboard/types'

type StartMultiplayer = (
  bet: number,
  maxPlayers: number,
) => Promise<{ gameId: string; error: string | null }>

export function useMultiplayer(
  profile: ProfileData | null,
  onStartMultiplayerGame: StartMultiplayer,
  onViewLobby: (gameId: string) => void,
  showToast: (msg: string) => void,
) {
  const { t } = useTranslation()
  const [multiplayerOpen, setMultiplayerOpen] = useState(false)
  const [multiplayerStep, setMultiplayerStep] = useState<1 | 2>(1)
  const [multiplayerBet, setMultiplayerBet] = useState(10)
  const [multiplayerMaxPlayers, setMultiplayerMaxPlayers] = useState(4)
  const [multiplayerCreating, setMultiplayerCreating] = useState(false)
  const [multiplayerError, setMultiplayerError] = useState<string | null>(null)
  const [multiplayerPseudos, setMultiplayerPseudos] = useState<Record<number, string>>({})

  const openMultiplayerModal = () => {
    setMultiplayerStep(1)
    setMultiplayerBet(10)
    setMultiplayerMaxPlayers(4)
    setMultiplayerPseudos({})
    setMultiplayerError(null)
    setMultiplayerOpen(true)
  }

  const closeMultiplayerModal = () => {
    setMultiplayerOpen(false)
    setMultiplayerError(null)
    setMultiplayerStep(1)
  }

  const handleStep1Submit = async () => {
    if (multiplayerBet <= 0 || multiplayerBet > (profile?.credit ?? 0)) {
      setMultiplayerError(t('dashboard.insufficientCredits'))
      return
    }

    const filledPseudos = Object.values(multiplayerPseudos)
      .map((p) => p.trim())
      .filter(Boolean)
      .map((p) => (p.startsWith('@') ? p.slice(1) : p))

    const seen = new Set<string>()
    for (const p of filledPseudos) {
      const lower = p.toLowerCase()
      if (seen.has(lower)) {
        setMultiplayerError(t('dashboard.duplicatePseudo', { pseudo: p }))
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
      showToast(t('dashboard.multiplayerCreated'))

      if (filledPseudos.length > 0) {
        try {
          await axios.post(`/api/games/${result.gameId}/invites`, { pseudos: filledPseudos })
        } catch (err: unknown) {
          showToast(extractApiError(err).message || t('dashboard.failedSendInvites'))
        }
      }
      onViewLobby(result.gameId)
    }
  }

  return {
    multiplayerOpen,
    multiplayerStep,
    multiplayerBet,
    multiplayerMaxPlayers,
    multiplayerCreating,
    multiplayerError,
    multiplayerPseudos,
    openMultiplayerModal,
    closeMultiplayerModal,
    setMultiplayerStep,
    setMultiplayerBet,
    setMultiplayerMaxPlayers,
    setMultiplayerPseudos,
    handleStep1Submit,
  }
}
