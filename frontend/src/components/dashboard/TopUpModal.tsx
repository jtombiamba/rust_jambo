import { useTranslation } from 'react-i18next'

const TOPUP_AMOUNTS_EUR_CENTS = [500, 1000]

interface TopUpModalProps {
  open: boolean
  toppingUp: boolean
  onClose: () => void
  onConfirm: (amountEurCents: number) => void
}

export default function TopUpModal({ open, toppingUp, onClose, onConfirm }: TopUpModalProps) {
  const { t } = useTranslation()

  if (!open) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white rounded-lg shadow-xl p-6 w-full max-w-md mx-4">
        <div className="flex justify-between items-center mb-4">
          <h2 className="text-xl font-bold">{t('dashboard.topUpTitle')}</h2>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-gray-600 text-2xl leading-none"
            aria-label={t('common.close')}
          >
            &times;
          </button>
        </div>

        <p className="text-sm text-gray-500 mb-4">{t('dashboard.topUpChooseAmount')}</p>

        <div className="grid grid-cols-2 gap-3 mb-4">
          {TOPUP_AMOUNTS_EUR_CENTS.map((cents) => (
            <button
              key={cents}
              onClick={() => onConfirm(cents)}
              disabled={toppingUp}
              className="px-4 py-4 bg-emerald-600 text-white text-lg font-bold rounded-lg hover:bg-emerald-700 disabled:opacity-50"
            >
              {t('dashboard.topUpAmount', { amount: (cents / 100).toFixed(0) })}
            </button>
          ))}
        </div>

        <div className="flex justify-end">
          <button
            onClick={onClose}
            disabled={toppingUp}
            className="px-4 py-2 border border-gray-300 rounded-lg text-gray-700 hover:bg-gray-100 disabled:opacity-50"
          >
            {t('common.cancel')}
          </button>
        </div>
      </div>
    </div>
  )
}
