import { createContext, useContext } from 'react'

export interface Toast {
  id: number
  message: string
  type: 'success' | 'error' | 'info' | 'warning'
}

interface ToastContextType {
  showToast: (message: string, type?: Toast['type']) => void
}

export const ToastContext = createContext<ToastContextType>({ showToast: () => {} })

export function useToast() {
  return useContext(ToastContext)
}
