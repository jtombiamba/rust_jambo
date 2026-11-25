import { useTranslation } from 'react-i18next'
import type { ProfileData } from './types'

interface StatsOverviewProps {
  profile: ProfileData | null
  toppingUp: boolean
  onOpenTopUp: () => void
}

export default function StatsOverview({ profile, toppingUp, onOpenTopUp }: StatsOverviewProps) {
  const { t } = useTranslation()

  const gamesPlayed = profile?.game_played ?? 0
  const wins = profile?.wins ?? 0
  const winRate = gamesPlayed > 0 ? Math.round((wins / gamesPlayed) * 100) : 0

  return (
    <div className="bg-white rounded-lg border border-gray-200 shadow-sm hover:shadow-md transition-shadow p-4">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-semibold text-gray-500 uppercase tracking-wide">
          {t('dashboard.statsOverview')}
        </h3>
      </div>

      <div className="mb-3">
        <p className="text-xs text-gray-500">{t('dashboard.credit')}</p>
        <div className="flex items-center justify-between">
          <p className="text-2xl font-bold text-gray-900">{profile?.credit ?? 0}</p>
          <button
            type="button"
            onClick={onOpenTopUp}
            disabled={toppingUp}
            className="px-3 py-1.5 bg-emerald-600 text-white text-xs font-semibold rounded-lg hover:bg-emerald-700 disabled:opacity-50"
          >
            {toppingUp ? t('dashboard.processing') : t('dashboard.topUp')}
          </button>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div>
          <p className="text-xs text-gray-500">{t('dashboard.gamesPlayed')}</p>
          <p className="text-xl font-bold text-gray-900">{gamesPlayed}</p>
        </div>
        <div>
          <p className="text-xs text-gray-500">{t('dashboard.wins')}</p>
          <p className="text-xl font-bold text-green-600">{wins}</p>
        </div>
        <div>
          <p className="text-xs text-gray-500">{t('dashboard.koraWins')}</p>
          <p className="text-xl font-bold text-yellow-600">{profile?.kora_wins ?? 0}</p>
        </div>
        <div>
          <p className="text-xs text-gray-500">{t('dashboard.winRate')}</p>
          <p className="text-xl font-bold text-blue-600">{winRate}%</p>
        </div>
      </div>
    </div>
  )
}
