import { useTranslation } from 'react-i18next'
import type { CashoutConfig } from './types'

interface CashoutModalProps {
  open: boolean
  config: CashoutConfig
  credits: string
  method: 'paypal_email' | 'stripe_connect'
  destination: string
  connecting: boolean
  submitting: boolean
  error: string | null
  onClose: () => void
  onCreditsChange: (value: string) => void
  onMethodChange: (value: 'paypal_email' | 'stripe_connect') => void
  onDestinationChange: (value: string) => void
  onConnectBank: () => void
  onSubmit: () => void
}

export default function CashoutModal({
  open,
  config,
  credits,
  method,
  destination,
  connecting,
  submitting,
  error,
  onClose,
  onCreditsChange,
  onMethodChange,
  onDestinationChange,
  onConnectBank,
  onSubmit,
}: CashoutModalProps) {
  const { t } = useTranslation()

  if (!open) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white rounded-lg shadow-xl p-6 w-full max-w-md mx-4">
        <div className="flex justify-between items-center mb-4">
          <h2 className="text-xl font-bold">{t('dashboard.requestCashout')}</h2>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-gray-600 text-2xl leading-none"
            aria-label={t('common.close')}
          >
            &times;
          </button>
        </div>

        <p className="text-sm text-gray-500 mb-4">
          {t('dashboard.cashoutRate', { credits: config.creditsPerEur, eur: '1.00' })}{' '}
          {t('dashboard.cashoutMin', {
            credits: config.minCredits,
            eur: (config.minCredits / config.creditsPerEur).toFixed(2),
          })}{' '}
          {t('dashboard.cashoutMax', { eur: (config.maxEurCents / 100).toFixed(2) })}
        </p>

        <div className="mb-4">
          <label htmlFor="cashout-credits" className="block text-sm font-medium text-gray-700 mb-1">
            {t('dashboard.cashoutCredits')}
          </label>
          <input
            id="cashout-credits"
            type="number"
            min={config.minCredits}
            step={config.creditsPerEur}
            value={credits}
            onChange={(e) => onCreditsChange(e.target.value)}
            className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-green-500 focus:border-green-500"
          />
        </div>

        <div className="mb-4">
          <label htmlFor="cashout-method" className="block text-sm font-medium text-gray-700 mb-1">
            {t('dashboard.cashoutMethod')}
          </label>
          <select
            id="cashout-method"
            value={method}
            onChange={(e) => onMethodChange(e.target.value as 'paypal_email' | 'stripe_connect')}
            className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-green-500 focus:border-green-500"
          >
            <option value="paypal_email">{t('dashboard.cashoutMethodPaypal')}</option>
            <option value="stripe_connect">{t('dashboard.cashoutMethodBank')}</option>
          </select>
        </div>

        {method === 'paypal_email' ? (
          <div className="mb-4">
            <label htmlFor="cashout-paypal-email" className="block text-sm font-medium text-gray-700 mb-1">
              {t('dashboard.cashoutPaypalEmail')}
            </label>
            <input
              id="cashout-paypal-email"
              type="email"
              value={destination}
              onChange={(e) => onDestinationChange(e.target.value)}
              placeholder="you@example.com"
              className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-green-500 focus:border-green-500"
            />
          </div>
        ) : (
          <div className="mb-4">
            <button
              type="button"
              onClick={onConnectBank}
              disabled={connecting}
              className="w-full px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50"
            >
              {connecting ? t('dashboard.processing') : t('dashboard.cashoutConnectBank')}
            </button>
          </div>
        )}

        {error && (
          <div className="mb-4 p-3 bg-red-100 text-red-700 rounded text-sm">{error}</div>
        )}

        <div className="flex justify-end gap-3">
          <button
            onClick={onClose}
            className="px-4 py-2 border border-gray-300 rounded-lg text-gray-700 hover:bg-gray-100"
          >
            {t('common.cancel')}
          </button>
          <button
            onClick={onSubmit}
            disabled={submitting}
            className="px-4 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700 disabled:opacity-50"
          >
            {submitting ? t('dashboard.cashoutSubmitting') : t('dashboard.cashoutSubmit')}
          </button>
        </div>
      </div>
    </div>
  )
}
