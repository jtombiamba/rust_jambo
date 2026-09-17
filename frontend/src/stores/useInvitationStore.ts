import { create } from 'zustand'

export interface InvitationItem {
  invite_id: string
  game_id: string
  creator_pseudo: string
  bet: number
  player_count: number
  max_players: number
  created_at: string
  expires_at: string | null
}

interface InvitationState {
  invitations: InvitationItem[]
  setInvitations: (invitations: InvitationItem[]) => void
  seedInvitations: (invitations: InvitationItem[]) => void
  addInvitation: (invitation: InvitationItem) => void
  removeInvitation: (gameId: string) => void
  clear: () => void
}

export const useInvitationStore = create<InvitationState>((set) => ({
  invitations: [],
  setInvitations: (invitations) => set({ invitations }),
  seedInvitations: (invitations) =>
    set((state) => {
      const existingIds = new Set(state.invitations.map((inv) => inv.invite_id))
      const additions = invitations.filter((inv) => !existingIds.has(inv.invite_id))
      if (additions.length === 0) {
        return state
      }
      return { invitations: [...state.invitations, ...additions] }
    }),
  addInvitation: (invitation) =>
    set((state) => {
      if (state.invitations.some((inv) => inv.invite_id === invitation.invite_id)) {
        return state
      }
      return { invitations: [...state.invitations, invitation] }
    }),
  removeInvitation: (gameId) =>
    set((state) => ({
      invitations: state.invitations.filter((inv) => inv.game_id !== gameId),
    })),
  clear: () => set({ invitations: [] }),
}))
