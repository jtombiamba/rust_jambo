import { create } from 'zustand'
import axios from 'axios'

export interface UserInfo {
  id: string
  pseudo: string
  email: string
  language?: string
}

export type AuthView = 'choice' | 'register' | 'login' | 'forgot-password'

interface RegisterPayload {
  pseudo: string
  email: string
  password: string
  password_confirm: string
}

interface LoginPayload {
  email: string
  password: string
}

interface AuthState {
  isAuthenticated: boolean
  user: UserInfo | null
  frozenUntil: string | null
  authModalOpen: boolean
  authView: AuthView
  authError: { error: string; field?: string } | null
  authLoading: boolean
  pendingInviteMessage: string | null
  openAuthModal: (message?: string) => void
  closeAuthModal: () => void
  setAuthView: (view: AuthView) => void
  clearPendingInvite: () => void
  setFrozenUntil: (frozenUntil: string | null) => void
  checkAuth: () => Promise<void>
  register: (data: RegisterPayload) => Promise<boolean>
  login: (data: LoginPayload) => Promise<boolean>
  forgotPassword: (email: string) => Promise<void>
  logout: () => Promise<void>
}

export const useAuthStore = create<AuthState>((set) => ({
  isAuthenticated: false,
  user: null,
  frozenUntil: null,
  authModalOpen: false,
  authView: 'choice',
  authError: null,
  authLoading: false,
  pendingInviteMessage: null,

  openAuthModal: (message) => set({ authModalOpen: true, authView: 'choice', authError: null, pendingInviteMessage: message || null }),
  closeAuthModal: () => set({ authModalOpen: false, authError: null }),
  setAuthView: (view) => set({ authView: view, authError: null }),
  clearPendingInvite: () => set({ pendingInviteMessage: null }),
  setFrozenUntil: (frozenUntil) => set({ frozenUntil }),

  checkAuth: async () => {
    try {
      const res = await axios.get<UserInfo>('/api/auth/me')
      set({ isAuthenticated: true, user: res.data })
    } catch {
      set({ isAuthenticated: false, user: null })
    }
  },

  register: async (data) => {
    set({ authLoading: true, authError: null })
    try {
      const res = await axios.post('/api/auth/register', data)
      set({
        isAuthenticated: true,
        user: res.data.user,
        authModalOpen: false,
        authLoading: false,
      })
      return true
    } catch (err: unknown) {
      const errorData = (err as { response?: { data?: { error?: string; field?: string } } })
        .response?.data
      set({
        authError: {
          error: errorData?.error || 'Registration failed',
          field: errorData?.field,
        },
        authLoading: false,
      })
      return false
    }
  },

  login: async (data) => {
    set({ authLoading: true, authError: null })
    try {
      const res = await axios.post('/api/auth/login', data)
      set({
        isAuthenticated: true,
        user: res.data.user,
        authModalOpen: false,
        authLoading: false,
      })
      return true
    } catch (err: unknown) {
      const errorData = (err as { response?: { data?: { error?: string; field?: string } } })
        .response?.data
      set({
        authError: {
          error: errorData?.error || 'Login failed',
          field: errorData?.field,
        },
        authLoading: false,
      })
      return false
    }
  },

  forgotPassword: async (email) => {
    set({ authLoading: true, authError: null })
    try {
      const res = await axios.post('/api/auth/forgot-password', { email })
      set({
        authError: { error: res.data.message, field: undefined },
        authLoading: false,
      })
    } catch {
      set({
        authError: { error: 'Something went wrong. Please try again.' },
        authLoading: false,
      })
    }
  },

  logout: async () => {
    try {
      await axios.post('/api/auth/logout')
    } catch {
      // ignore
    }
    set({ isAuthenticated: false, user: null, frozenUntil: null })
  },
}))
