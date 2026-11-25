import { useTranslation } from 'react-i18next'
import type { CashoutHistoryData } from './types'

interface CashoutHistoryProps {
  cashoutHistory: CashoutHistoryData | null
  loading: boolean
}

export default function CashoutHistory({ cashoutHistory, loading }: CashoutHistoryProps) {
  const { t } = useTranslation()

  return (
    <div className="bg-white rounded-lg shadow border p-4 sm:p-6 mb-6">
      <h2 className="text-lg font-semibold mb-4">
        {t('dashboard.cashoutHistory')}
        {cashoutHistory && (
          <span className="text-gray-500 text-sm ml-2">({cashoutHistory.total} total)</span>
        )}
      </h2>
      {loading && !cashoutHistory ? (
        <p className="text-gray-500">{t('dashboard.loadingDashboard')}</p>
      ) : cashoutHistory && cashoutHistory.items.length === 0 ? (
        <p className="text-gray-500">{t('dashboard.cashoutNoHistory')}</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead>
              <tr className="border-b">
                <th className="py-2 pr-4">{t('dashboard.date')}</th>
                <th className="py-2 pr-4">{t('dashboard.cashoutAmount')}</th>
                <th className="py-2 pr-4">{t('dashboard.status')}</th>
                <th className="py-2">{t('dashboard.cashoutDestination')}</th>
              </tr>
            </thead>
            <tbody>
              {cashoutHistory?.items.map((item) => (
                <tr key={item.id} className="border-b">
                  <td className="py-2 pr-4 text-gray-500 text-xs">
                    {new Date(item.created_at).toLocaleDateString()}
                  </td>
                  <td className="py-2 pr-4 font-semibold">
                    {(item.amount_eur_cents / 100).toFixed(2)} EUR
                    <span className="text-gray-400 text-xs ml-1">
                      ({item.credits} {t('dashboard.credit').toLowerCase()})
                    </span>
                  </td>
                  <td className="py-2 pr-4">
                    <span
                      className={`inline-block px-2 py-0.5 rounded text-xs font-medium ${
                        item.status === 'requested'
                          ? 'bg-yellow-100 text-yellow-700'
                          : item.status === 'approved'
                            ? 'bg-blue-100 text-blue-700'
                            : item.status === 'paid'
                              ? 'bg-green-100 text-green-700'
                              : 'bg-red-100 text-red-700'
                      }`}
                    >
                      {item.status}
                    </span>
                  </td>
                  <td className="py-2 text-gray-500 text-xs">{item.payout_destination}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
