import { useEffect } from 'react'
import axios from 'axios'
import { useTranslation } from 'react-i18next'
import { useRoomStore } from '../stores/useRoomStore'

interface Props {
  onSelectRoom: (roomId: string) => void
  onCreateRoom: () => void
  onJoinRoom: () => void
}

const RoomList: React.FC<Props> = ({ onSelectRoom, onCreateRoom, onJoinRoom }) => {
  const { rooms, setRooms, loadingRooms, setLoadingRooms, roomError, setRoomError } = useRoomStore()
  const { t } = useTranslation()

  useEffect(() => {
    setLoadingRooms(true)
    setRoomError(null)
    axios.get('/api/me/rooms')
      .then((res) => setRooms(res.data))
      .catch((err) => {
        const msg = err.response?.data?.error || t('rooms.failedLoad')
        setRoomError(msg)
      })
      .finally(() => setLoadingRooms(false))
  }, [setRooms, setLoadingRooms, setRoomError, t])

  return (
    <div className="bg-gray-100 p-4 sm:p-6 rounded-lg shadow mb-6 sm:mb-8">
      <div className="flex justify-between items-center mb-4">
        <h2 className="text-lg sm:text-xl font-semibold">{t('rooms.title')}</h2>
        <div className="flex gap-2">
          <button
            className="px-3 py-1.5 border border-gray-400 text-gray-700 text-sm font-semibold rounded-lg hover:bg-gray-200"
            onClick={onJoinRoom}
          >
            {t('rooms.joinRoom')}
          </button>
          <button
            className="px-3 py-1.5 bg-blue-600 text-white text-sm font-semibold rounded-lg hover:bg-blue-700"
            onClick={onCreateRoom}
          >
            {t('rooms.createRoom')}
          </button>
        </div>
      </div>

      {loadingRooms ? (
        <div className="text-center py-4">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600 mx-auto mb-2"></div>
          <p className="text-gray-600 text-sm">{t('rooms.loading')}</p>
        </div>
      ) : roomError ? (
        <div className="p-3 bg-red-100 text-red-700 rounded text-sm">
          {roomError}
        </div>
      ) : rooms.length === 0 ? (
        <p className="text-gray-500 text-sm text-center py-4">
          {t('rooms.noRooms')}
        </p>
      ) : (
        <div className="grid gap-3">
          {rooms.map((room) => (
            <button
              key={room.id}
              onClick={() => onSelectRoom(room.id)}
              className="bg-white p-4 rounded-lg shadow-sm hover:shadow-md transition-shadow text-left w-full"
            >
              <div className="flex justify-between items-center">
                <span className="font-semibold text-gray-800">{room.name}</span>
                <span className="text-xs text-gray-400 font-mono">{room.invitation_code}</span>
              </div>
              <div className="flex gap-4 mt-1 text-xs text-gray-500">
                <span>{room.member_count ?? '?'} {t('rooms.members')}</span>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

export default RoomList
