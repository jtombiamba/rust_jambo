import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import AccountMenu from './AccountMenu'

interface TopBarProps {
  pseudo: string
  email: string
  credit: number
  toppingUp: boolean
  cashoutAvailable: boolean
  showCashoutHistory: boolean
  onOpenTopUp: () => void
  onLogout: () => void
  onOpenRules: () => void
  onOpenCashout: () => void
  onOpenAccountSettings: () => void
  onToggleCashoutHistory: () => void
  onOpenSidebar: () => void
}

export default function DashboardTopBar({
  pseudo,
  email,
  credit,
  toppingUp,
  cashoutAvailable,
  showCashoutHistory,
  onOpenTopUp,
  onLogout,
  onOpenRules,
  onOpenCashout,
  onOpenAccountSettings,
  onToggleCashoutHistory,
  onOpenSidebar,
}: TopBarProps) {
  const { t } = useTranslation()
  const [menuOpen, setMenuOpen] = useState(false)

  return (
    <header className="sticky top-0 z-50 h-16 flex items-center gap-3 px-3 sm:px-4 bg-white border-b border-gray-200 shadow-sm">
      <button
        type="button"
        onClick={onOpenSidebar}
        className="lg:hidden w-10 h-10 flex items-center justify-center rounded-lg border border-gray-300 text-gray-700 hover:bg-gray-100"
        aria-label={t('common.menu')}
      >
        <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
          <circle cx="10" cy="3" r="2" />
          <circle cx="10" cy="10" r="2" />
          <circle cx="10" cy="17" r="2" />
        </svg>
      </button>

      <div className="flex items-center gap-2">
        <img src="/color_logo.png" alt="Jambo Card Game" className="h-8 w-auto" />
        <span className="hidden sm:inline font-bold text-gray-900">{t('common.title')}</span>
      </div>

      <div className="ml-auto flex items-center gap-2 sm:gap-3">
        <div className="px-3 py-1.5 bg-gray-100 rounded-lg text-sm">
          <span className="text-gray-500 mr-1">{t('dashboard.credit')}:</span>
          <span className="font-bold text-gray-900">{credit}</span>
        </div>
        <button
          type="button"
          onClick={onOpenTopUp}
          disabled={toppingUp}
          className="px-3 sm:px-4 py-2 bg-emerald-600 text-white text-sm font-semibold rounded-lg hover:bg-emerald-700 disabled:opacity-50 inline-flex items-center gap-1"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 4v16M4 12h16" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
          </svg>
          {toppingUp ? t('dashboard.processing') : t('dashboard.topUp')}
        </button>

        <div className="relative">
          <button
            type="button"
            onClick={() => setMenuOpen((v) => !v)}
            className="w-10 h-10 flex items-center justify-center bg-white border border-gray-300 rounded-lg shadow-sm text-gray-700 hover:bg-gray-100"
            aria-label={t('common.menu')}
          >
            <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
              <circle cx="10" cy="3" r="2" />
              <circle cx="10" cy="10" r="2" />
              <circle cx="10" cy="17" r="2" />
            </svg>
          </button>
          {menuOpen && (
            <AccountMenu
              pseudo={pseudo}
              email={email}
              cashoutAvailable={cashoutAvailable}
              showCashoutHistory={showCashoutHistory}
              onLogout={onLogout}
              onOpenRules={onOpenRules}
              onOpenCashout={onOpenCashout}
              onOpenAccountSettings={onOpenAccountSettings}
              onToggleCashoutHistory={onToggleCashoutHistory}
              onClose={() => setMenuOpen(false)}
            />
          )}
        </div>
      </div>
    </header>
  )
}
