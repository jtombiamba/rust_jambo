import { createContext, useContext } from 'react'

export interface Toast {
  id: number
  message: string
  type: 'success' | 'error' | 'info' | 'warning'
  requestId?: string
}

interface ToastContextType {
  showToast: (message: string, type?: Toast['type'], requestId?: string) => void
}

export const ToastContext = createContext<ToastContextType>({ showToast: () => {} })

export function useToast() {
  return useContext(ToastContext)
}
