import { useState, useEffect } from 'react'
import axios from 'axios'
import { extractApiError } from '../utils/errors'
import { useTranslation } from 'react-i18next'

interface Props {
  isOpen: boolean
  roomId: string
  onClose: () => void
  onCreated: () => void
}

interface MemberInfo {
  user_id: string
  pseudo: string
}

const CreateRunModal: React.FC<Props> = ({ isOpen, roomId, onClose, onCreated }) => {
  const { t } = useTranslation()
  const [numGames, setNumGames] = useState(3)
  const [bet, setBet] = useState(10)
  const [members, setMembers] = useState<MemberInfo[]>([])
  const [selectedPlayers, setSelectedPlayers] = useState<string[]>([])
  const [creating, setCreating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [loadingMembers, setLoadingMembers] = useState(true)

  useEffect(() => {
    if (isOpen) {
      setLoadingMembers(true)
      axios.get<{ members: MemberInfo[] }>(`/api/me/rooms/${roomId}`)
        .then((res) => {
          setMembers(res.data.members || [])
          setLoadingMembers(false)
        })
        .catch(() => setLoadingMembers(false))
    }
  }, [isOpen, roomId])

  if (!isOpen) return null

  const togglePlayer = (userId: string) => {
    setSelectedPlayers((prev) =>
      prev.includes(userId)
        ? prev.filter((id) => id !== userId)
        : [...prev, userId]
    )
  }

  const handleCreate = async () => {
    if (selectedPlayers.length < 2) {
      setError(t('rooms.selectAtLeastTwo'))
      return
    }
    if (selectedPlayers.length > 4) {
      setError(t('rooms.maxFourPlayers'))
      return
    }
    setCreating(true)
    setError(null)
    try {
      await axios.post(`/api/me/rooms/${roomId}/runs`, {
        num_games: numGames,
        bet,
        player_ids: selectedPlayers,
      })
      onCreated()
      onClose()
    } catch (err: unknown) {
      const msg = extractApiError(err).message || t('rooms.failedCreateRun')
      setError(msg)
    } finally {
      setCreating(false)
    }
  }

  return (
    <>
      <div className="fixed inset-0 bg-black bg-opacity-50 z-50" onClick={onClose} />
      <div className="fixed inset-0 flex items-center justify-center z-50 p-4">
        <div className="bg-white rounded-lg shadow-xl w-full max-w-md p-6 max-h-[90vh] overflow-y-auto">
          <h3 className="text-lg font-semibold mb-4">{t('rooms.createRun')}</h3>

          <label className="block text-sm font-medium text-gray-700 mb-1">{t('run.numGames')}</label>
          <select
            className="w-full px-3 py-2 border border-gray-300 rounded-lg mb-3 text-sm"
            value={numGames}
            onChange={(e) => setNumGames(Number(e.target.value))}
          >
            {[1, 2, 3, 4, 5, 10].map((n) => (
              <option key={n} value={n}>{n} {t('rooms.gamesCount', { count: n })}</option>
            ))}
          </select>

          <label className="block text-sm font-medium text-gray-700 mb-1">{t('run.bet')}</label>
          <select
            className="w-full px-3 py-2 border border-gray-300 rounded-lg mb-4 text-sm"
            value={bet}
            onChange={(e) => setBet(Number(e.target.value))}
          >
            {[5, 10, 20, 50, 100].map((b) => (
              <option key={b} value={b}>{b} credits</option>
            ))}
          </select>

          <label className="block text-sm font-medium text-gray-700 mb-1">{t('run.selectPlayers')}</label>
          {loadingMembers ? (
            <p className="text-gray-400 text-sm">{t('rooms.loadingMembers')}</p>
          ) : (
            <div className="space-y-1 mb-4 max-h-48 overflow-y-auto">
              {members.map((m) => (
                <label key={m.user_id} className="flex items-center gap-2 p-2 hover:bg-gray-50 rounded cursor-pointer text-sm">
                  <input
                    type="checkbox"
                    checked={selectedPlayers.includes(m.user_id)}
                    onChange={() => togglePlayer(m.user_id)}
                    className="accent-blue-600"
                  />
                  {m.pseudo}
                </label>
              ))}
            </div>
          )}

          <div className="bg-gray-50 p-3 rounded text-sm mb-4">
            <p><strong>{t('run.totalCost')}:</strong> {numGames * bet} credits</p>
            <p className="text-gray-500">{t('run.provisionNote')}</p>
          </div>

          {error && (
            <div className="p-2 bg-red-100 text-red-700 rounded text-sm mb-3">{error}</div>
          )}

          <div className="flex gap-2 justify-end">
            <button
              className="px-4 py-2 border border-gray-300 text-gray-700 rounded-lg text-sm hover:bg-gray-100"
              onClick={onClose}
            >
              {t('common.cancel')}
            </button>
            <button
              className="px-4 py-2 bg-purple-600 text-white rounded-lg text-sm hover:bg-purple-700 disabled:opacity-50"
              onClick={handleCreate}
              disabled={creating}
            >
              {creating ? t('run.creating') : t('rooms.createRunBtn')}
            </button>
          </div>
        </div>
      </div>
    </>
  )
}

export default CreateRunModal
