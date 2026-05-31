import { useState } from 'react'
import axios from 'axios'
import { extractApiError } from '../utils/errors'
import { useTranslation } from 'react-i18next'

interface Props {
  isOpen: boolean
  onClose: () => void
  onJoined: (roomId: string) => void
}

const JoinRoomForm: React.FC<Props> = ({ isOpen, onClose, onJoined }) => {
  const { t } = useTranslation()
  const [code, setCode] = useState('')
  const [joining, setJoining] = useState(false)
  const [error, setError] = useState<string | null>(null)

  if (!isOpen) return null

  const handleJoin = async () => {
    if (!code.trim()) {
      setError(t('rooms.codeRequired'))
      return
    }
    setJoining(true)
    setError(null)
    try {
      const res = await axios.post<{ id: string }>('/api/me/rooms/join', { invitation_code: code.trim().toUpperCase() })
      onJoined(res.data.id)
      setCode('')
      onClose()
    } catch (err: unknown) {
      const msg = extractApiError(err).message || t('rooms.failedJoin')
      setError(msg)
    } finally {
      setJoining(false)
    }
  }

  return (
    <>
      <div className="fixed inset-0 bg-black bg-opacity-50 z-50" onClick={onClose} />
      <div className="fixed inset-0 flex items-center justify-center z-50 p-4">
        <div className="bg-white rounded-lg shadow-xl w-full max-w-md p-6">
          <h3 className="text-lg font-semibold mb-4">{t('rooms.joinRoom')}</h3>
          <p className="text-sm text-gray-600 mb-3">{t('rooms.enterCode')}</p>
          <input
            type="text"
            placeholder={t('rooms.codePlaceholder')}
            className="w-full px-4 py-2 border border-gray-300 rounded-lg mb-3 text-sm font-mono uppercase focus:outline-none focus:border-blue-500"
            value={code}
            onChange={(e) => setCode(e.target.value.toUpperCase())}
            onKeyDown={(e) => e.key === 'Enter' && handleJoin()}
            autoFocus
            maxLength={8}
          />
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
              className="px-4 py-2 bg-blue-600 text-white rounded-lg text-sm hover:bg-blue-700 disabled:opacity-50"
              onClick={handleJoin}
              disabled={joining}
            >
              {joining ? t('rooms.joining') : t('rooms.join')}
            </button>
          </div>
        </div>
      </div>
    </>
  )
}

export default JoinRoomForm
