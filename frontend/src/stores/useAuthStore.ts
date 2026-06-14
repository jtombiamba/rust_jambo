import { create } from 'zustand'
import { api } from '../api/api'
import { extractApiError } from '../utils/errors'

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
      console.log("check auth");
      const res = await api.get<UserInfo>('/api/auth/me')
      console.log("what happens check auth first status = " + res.status);
      console.log("what happens check auth" + res.data);
      set({ isAuthenticated: true, user: res.data })
    } catch (err: unknown) {
      console.log("err = "  + err);
      const msg = extractApiError(err).message || "dunno"
      console.log("check auth failed" + msg);
      set({ isAuthenticated: false, user: null })
    }
    // try {
    //   console.log("check auth");
    //   const res = await api.get<UserInfo>('/api/auth/me')
    //   console.log("what happens check auth first status = " + res.status);
    //   console.log("what happens check auth" + res.data);
    //   set({ isAuthenticated: true, user: res.data })
    // } catch (err: unknown) {
    //   const msg = extractApiError(err).message || "dunno"
    //   console.log("check auth failed" + msg);
    //   set({ isAuthenticated: false, user: null })
    // }
  },

  register: async (data) => {
    set({ authLoading: true, authError: null })
    try {
      const res = await api.post('/api/auth/register', data)
      set({
        isAuthenticated: true,
        user: res.data.user,
        authModalOpen: false,
        authLoading: false,
      })
      return true
    } catch (err: unknown) {
      const error = extractApiError(err)
      const field = api.isAxiosError(err) ? err.response?.data?.field : undefined
      set({
        authError: {
          error: error.message,
          field,
        },
        authLoading: false,
      })
      return false
    }
  },

  login: async (data) => {
    set({ authLoading: true, authError: null })
    try {
      console.log("call login");
      const res = await api.post('/api/auth/login', data)
      set({
        isAuthenticated: true,
        user: res.data.user,
        authModalOpen: false,
        authLoading: false,
      })
      return true
    } catch (err: unknown) {
      const error = extractApiError(err)
      const field = api.isAxiosError(err) ? err.response?.data?.field : undefined
      set({
        authError: {
          error: error.message,
          field,
        },
        authLoading: false,
      })
      return false
    }
  },

  forgotPassword: async (email) => {
    set({ authLoading: true, authError: null })
    try {
      const res = await api.post('/api/auth/forgot-password', { email })
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
      await api.post('/api/auth/logout')
    } catch {
      // ignore
    }
    set({ isAuthenticated: false, user: null, frozenUntil: null })
  },
}))
