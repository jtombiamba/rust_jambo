import { create } from 'zustand'

interface RoomInfo {
  id: string
  name: string
  creator_id: string
  invitation_code: string
  created_at: string
  member_count?: number
}

interface ActiveRun {
  id: string
  room_id: string
  num_games: number
  bet_per_game: number
  current_game_index: number
  status: string
  all_games_created?: boolean
  players?: RunPlayer[]
}

interface RunPlayer {
  user_id: string
  pseudo: string
  position: number
  provisioned_credits: number
  kicked: boolean
}

interface RoomState {
  rooms: RoomInfo[]
  activeRoomId: string | null
  activeRoom: RoomInfo | null
  activeRun: ActiveRun | null
  currentRunGameId: string | null
  showCreateRoom: boolean
  showJoinRoom: boolean
  loadingRooms: boolean
  roomError: string | null

  setRooms: (rooms: RoomInfo[]) => void
  setActiveRoom: (room: RoomInfo | null) => void
  setActiveRoomId: (id: string | null) => void
  setActiveRun: (run: ActiveRun | null) => void
  setCurrentRunGameId: (id: string | null) => void
  setShowCreateRoom: (show: boolean) => void
  setShowJoinRoom: (show: boolean) => void
  setLoadingRooms: (loading: boolean) => void
  setRoomError: (error: string | null) => void
  clear: () => void
}

export type { RoomInfo, ActiveRun, RunPlayer }

export const useRoomStore = create<RoomState>((set) => ({
  rooms: [],
  activeRoomId: null,
  activeRoom: null,
  activeRun: null,
  currentRunGameId: null,
  showCreateRoom: false,
  showJoinRoom: false,
  loadingRooms: false,
  roomError: null,

  setRooms: (rooms) => set({ rooms }),
  setActiveRoom: (room) => set({ activeRoom: room }),
  setActiveRoomId: (id) => set({ activeRoomId: id }),
  setActiveRun: (run) => set({ activeRun: run }),
  setCurrentRunGameId: (id) => set({ currentRunGameId: id }),
  setShowCreateRoom: (show) => set({ showCreateRoom: show }),
  setShowJoinRoom: (show) => set({ showJoinRoom: show }),
  setLoadingRooms: (loading) => set({ loadingRooms: loading }),
  setRoomError: (error) => set({ roomError: error }),
  clear: () => set({
    rooms: [],
    activeRoomId: null,
    activeRoom: null,
    activeRun: null,
    currentRunGameId: null,
    showCreateRoom: false,
    showJoinRoom: false,
    roomError: null,
  }),
}))
