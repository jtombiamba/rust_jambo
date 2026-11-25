import { useTranslation } from 'react-i18next'
import LanguageSwitcher from '../LanguageSwitcher'

interface AccountMenuProps {
  pseudo: string
  email: string
  cashoutAvailable: boolean
  showCashoutHistory: boolean
  onLogout: () => void
  onOpenRules: () => void
  onOpenCashout: () => void
  onOpenAccountSettings: () => void
  onToggleCashoutHistory: () => void
  onClose: () => void
}

export default function AccountMenu({
  pseudo,
  email,
  cashoutAvailable,
  showCashoutHistory,
  onLogout,
  onOpenRules,
  onOpenCashout,
  onOpenAccountSettings,
  onToggleCashoutHistory,
  onClose,
}: AccountMenuProps) {
  const { t } = useTranslation()

  const itemClass =
    'w-full text-left px-4 py-2 text-sm text-gray-700 hover:bg-gray-100 disabled:text-gray-400 disabled:hover:bg-transparent disabled:cursor-not-allowed'

  return (
    <div className="absolute right-0 top-12 w-64 bg-white border border-gray-200 rounded-lg shadow-xl py-1 z-50">
      <div className="px-4 py-3 border-b border-gray-100">
        <p className="font-semibold text-gray-900">{pseudo}</p>
        <p className="text-xs text-gray-500 truncate">{email}</p>
      </div>

      <div className="px-4 py-3 border-b border-gray-100">
        <p className="text-xs font-semibold text-gray-500 uppercase tracking-wide mb-2">
          {t('account.language')}
        </p>
        <LanguageSwitcher />
      </div>

      <button
        type="button"
        onClick={() => {
          onOpenAccountSettings()
          onClose()
        }}
        className={itemClass}
      >
        {t('account.accountSettings')}
      </button>
      <button type="button" disabled className={itemClass}>
        {t('account.notifications')}
      </button>
      <button type="button" disabled className={itemClass}>
        {t('account.themeCustomization')}
      </button>
      <button type="button" disabled className={itemClass}>
        {t('account.security2fa')}
      </button>
      <button
        type="button"
        onClick={() => {
          onToggleCashoutHistory()
          onClose()
        }}
        className={`${itemClass} ${showCashoutHistory ? 'bg-blue-50 text-blue-700' : ''}`}
      >
        {t('account.transactionHistory')}
      </button>

      <div className="border-t border-gray-100 my-1" />

      <button
        type="button"
        onClick={() => {
          onOpenCashout()
          onClose()
        }}
        disabled={!cashoutAvailable}
        className="block w-[calc(100%-1rem)] text-left px-4 py-2 mx-2 my-1 text-sm text-slate-500 border border-slate-200 rounded-md hover:bg-slate-50 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {t('account.withdrawCashout')}
      </button>

      <div className="border-t border-gray-100 my-1" />

      <button
        type="button"
        onClick={() => {
          onOpenRules()
          onClose()
        }}
        className={itemClass}
      >
        {t('account.helpSupport')}
      </button>
      <button
        type="button"
        onClick={() => {
          onLogout()
          onClose()
        }}
        className="w-full text-left px-4 py-2 text-sm text-red-600 hover:bg-red-50"
      >
        {t('auth.logout')}
      </button>
    </div>
  )
}
