import { useState, useCallback, useRef, ReactNode } from 'react'
import { ToastContext } from './useToast'

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([])
  const nextId = useRef(0)

  const showToast = useCallback((message: string, type: Toast['type'] = 'info') => {
    const id = nextId.current++
    setToasts(prev => [...prev.slice(-4), { id, message, type }])
    setTimeout(() => {
      setToasts(prev => prev.filter(t => t.id !== id))
    }, 4000)
  }, [])

  const removeToast = (id: number) => {
    setToasts(prev => prev.filter(t => t.id !== id))
  }

  const bgColor = {
    success: 'bg-green-600',
    error: 'bg-red-600',
    info: 'bg-blue-600',
    warning: 'bg-yellow-500',
  }

  return (
    <ToastContext.Provider value={{ showToast }}>
      {children}
      <div className="fixed bottom-4 right-4 z-[100] flex flex-col gap-2 max-w-sm">
        {toasts.map(toast => (
          <div
            key={toast.id}
            className={`${bgColor[toast.type]} text-white px-4 py-3 rounded-lg shadow-lg text-sm flex items-center justify-between gap-2 animate-fade-in-out`}
          >
            <span>{toast.message}</span>
            <button
              onClick={() => removeToast(toast.id)}
              className="text-white/80 hover:text-white ml-2 text-lg leading-none"
            >
              &times;
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  )
}
