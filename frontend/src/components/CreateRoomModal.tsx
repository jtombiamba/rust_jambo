import { useState } from 'react'
import axios from 'axios'
import { useTranslation } from 'react-i18next'

interface Props {
  isOpen: boolean
  onClose: () => void
  onCreated: (roomId: string) => void
}

const CreateRoomModal: React.FC<Props> = ({ isOpen, onClose, onCreated }) => {
  const { t } = useTranslation()
  const [name, setName] = useState('')
  const [creating, setCreating] = useState(false)
  const [error, setError] = useState<string | null>(null)

  if (!isOpen) return null

  const handleCreate = async () => {
    if (!name.trim()) {
      setError(t('rooms.nameRequired'))
      return
    }
    setCreating(true)
    setError(null)
    try {
      const res = await axios.post<{ id: string; name: string; invitation_code: string }>('/api/me/rooms', { name: name.trim() })
      onCreated(res.data.id)
      setName('')
      onClose()
    } catch (err: unknown) {
      const msg = (err as { response?: { data?: { error?: string } } })?.response?.data?.error || t('rooms.failedCreate')
      setError(msg)
    } finally {
      setCreating(false)
    }
  }

  return (
    <>
      <div className="fixed inset-0 bg-black bg-opacity-50 z-50" onClick={onClose} />
      <div className="fixed inset-0 flex items-center justify-center z-50 p-4">
        <div className="bg-white rounded-lg shadow-xl w-full max-w-md p-6">
          <h3 className="text-lg font-semibold mb-4">{t('rooms.createRoom')}</h3>
          <input
            type="text"
            placeholder={t('rooms.roomName')}
            className="w-full px-4 py-2 border border-gray-300 rounded-lg mb-3 text-sm focus:outline-none focus:border-blue-500"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
            autoFocus
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
              onClick={handleCreate}
              disabled={creating}
            >
              {creating ? t('rooms.creating') : t('rooms.create')}
            </button>
          </div>
        </div>
      </div>
    </>
  )
}

export default CreateRoomModal
