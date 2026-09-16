import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { useToast } from '../components/useToast'
import { useInvitationStore, InvitationItem } from '../stores/useInvitationStore'
import { getWsUrl } from '../utils/runtimeConfig'

interface InviteReceivedEvent {
  type: 'invite_received'
  user_id: string
  invite_id: string
  game_id: string
  creator_pseudo: string
  bet: number
  player_count: number
  max_players: number
  created_at: string
  expires_at: string | null
}

interface InviteRemovedEvent {
  type: 'invite_removed'
  user_id: string
  game_id: string
}

function isInviteReceived(event: { type?: string }): event is InviteReceivedEvent {
  return event.type === 'invite_received'
}

function isInviteRemoved(event: { type?: string }): event is InviteRemovedEvent {
  return event.type === 'invite_removed'
}

const PING_INTERVAL_MS = 30_000
const RECONNECT_DELAY_MS = 5_000

export function useUserWebSocket(enabled: boolean) {
  const { showToast } = useToast()
  const { t } = useTranslation()
  const socketRef = useRef<WebSocket | null>(null)
  const pingTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const reconnectRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const showToastRef = useRef(showToast)
  showToastRef.current = showToast
  const tRef = useRef(t)
  tRef.current = t

  useEffect(() => {
    if (!enabled) return

    let shouldReconnect = true
    let socket: WebSocket | null = null

    const clearPing = () => {
      if (pingTimerRef.current) {
        clearInterval(pingTimerRef.current)
        pingTimerRef.current = null
      }
    }

    const startPing = (ws: WebSocket) => {
      clearPing()
      pingTimerRef.current = setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: 'ping' }))
        }
      }, PING_INTERVAL_MS)
    }

    const connect = () => {
      const url = getWsUrl('/ws/me')
      const ws = new WebSocket(url)
      socket = ws
      socketRef.current = ws

      ws.onopen = () => {
        startPing(ws)
      }

      ws.onmessage = (evt) => {
        let data: { type?: string }
        try {
          data = JSON.parse(evt.data) as { type?: string }
        } catch {
          return
        }

        if (isInviteReceived(data)) {
          const invitation: InvitationItem = {
            invite_id: data.invite_id,
            game_id: data.game_id,
            creator_pseudo: data.creator_pseudo,
            bet: data.bet,
            player_count: data.player_count,
            max_players: data.max_players,
            created_at: data.created_at,
            expires_at: data.expires_at,
          }
          useInvitationStore.getState().addInvitation(invitation)
          showToastRef.current(tRef.current('dashboard.newInvitation', { pseudo: data.creator_pseudo }), 'info')
        } else if (isInviteRemoved(data)) {
          useInvitationStore.getState().removeInvitation(data.game_id)
        }
      }

      ws.onclose = () => {
        clearPing()
        if (shouldReconnect) {
          reconnectRef.current = setTimeout(connect, RECONNECT_DELAY_MS)
        }
      }

      ws.onerror = () => {
        ws.close()
      }
    }

    connect()

    return () => {
      shouldReconnect = false
      clearPing()
      if (reconnectRef.current) clearTimeout(reconnectRef.current)
      if (socket) socket.close()
      socketRef.current = null
    }
  }, [enabled])

  return null
}

export default useUserWebSocket
