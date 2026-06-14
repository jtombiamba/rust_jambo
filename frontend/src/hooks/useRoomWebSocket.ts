import { useEffect, useRef } from 'react'
import { useToast } from '../components/useToast'

interface RoomEvent {
  type: 'member_joined' | 'member_left' | 'run_created' | 'game_started' | 'run_completed'
  room_id: string
  user_id?: string
  pseudo?: string
  run_id?: string
  game_id?: string
  game_index?: number
  total_games?: number
  num_games?: number
  bet_per_game?: number
}

interface Props {
  roomId: string | null
  onEvent?: (event: RoomEvent) => void
}

export function useRoomWebSocket({ roomId, onEvent }: Props) {
  const { showToast } = useToast()
  const socketRef = useRef<WebSocket | null>(null)
  const onEventRef = useRef(onEvent)
  onEventRef.current = onEvent
  const showToastRef = useRef(showToast)
  showToastRef.current = showToast

  useEffect(() => {
    if (!roomId) return

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const host = window.location.host
    const url = `${protocol}//${host}/ws/room/${roomId}`

    let reconnectTimeout: ReturnType<typeof setTimeout>
    let shouldReconnect = true

    const connect = () => {
      console.log("creating new websocket");
      const ws = new WebSocket(url)
      socketRef.current = ws

      ws.onopen = () => {
        console.log(`Room WS connected for room ${roomId}`)
      }

      ws.onmessage = (evt) => {
        try {
          const data = JSON.parse(evt.data) as RoomEvent
          if (!data.type) return

          switch (data.type) {
            case 'member_joined':
              showToastRef.current(`${data.pseudo || 'Someone'} joined the room`, 'info')
              break
            case 'member_left':
              showToastRef.current(`${data.pseudo || 'Someone'} left the room`, 'info')
              break
            case 'run_created':
              showToastRef.current('A new game run was created!', 'info')
              break
            case 'game_started':
              showToastRef.current(`Game ${(data.game_index ?? 0) + 1} of ${data.total_games ?? data.num_games ?? '?'} started!`, 'success')
              break
            case 'run_completed':
              showToastRef.current('Game run completed!', 'success')
              break
          }

          onEventRef.current?.(data)
        } catch {
          // ignore parse errors
        }
      }

      ws.onclose = () => {
        if (shouldReconnect) {
          reconnectTimeout = setTimeout(connect, 5000)
        }
      }

      ws.onerror = () => {
        ws.close()
      }
    }

    connect()

    return () => {
      shouldReconnect = false
      clearTimeout(reconnectTimeout)
      socketRef.current?.close()
    }
  }, [roomId])

  return null
}

export default useRoomWebSocket
