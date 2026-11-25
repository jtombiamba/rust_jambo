import { useTranslation } from 'react-i18next'
import ToggleSwitch from '../ToggleSwitch'
import { formatFreezeTime } from './utils'

interface HeroPlayHubProps {
  frozen: boolean
  freezeRemainingSec: number
  unfreezing: boolean
  starting: boolean
  stepByStep: boolean
  onStepByStepChange: (value: boolean) => void
  onUnfreeze: () => void
  onStartSolo: () => void
  onOpenMultiplayer: () => void
}

export default function HeroPlayHub({
  frozen,
  freezeRemainingSec,
  unfreezing,
  starting,
  stepByStep,
  onStepByStepChange,
  onUnfreeze,
  onStartSolo,
  onOpenMultiplayer,
}: HeroPlayHubProps) {
  const { t } = useTranslation()

  if (frozen) {
    return (
      <div className="bg-amber-50 border border-amber-300 rounded-xl p-5 mb-6">
        <p className="text-amber-800 font-semibold mb-1">{t('dashboard.accountFrozen')}</p>
        <p className="text-amber-600 text-sm mb-3">
          {t('dashboard.autoUnfreeze')}{' '}
          <span className="font-bold font-mono text-lg">{formatFreezeTime(freezeRemainingSec)}</span>
        </p>
        <button
          className="px-4 py-2 text-sm font-semibold rounded-lg disabled:opacity-50 bg-[#0070ba] hover:bg-[#005ea6] text-white inline-flex items-center gap-2"
          disabled={unfreezing}
          onClick={onUnfreeze}
        >
          {unfreezing ? (
            <>
              <svg className="animate-spin h-4 w-4" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
              </svg>
              {t('dashboard.processing')}
            </>
          ) : (
            t('dashboard.requestUnfreeze')
          )}
        </button>
      </div>
    )
  }

  return (
    <div className="bg-gradient-to-r from-blue-600 to-purple-600 rounded-xl p-5 sm:p-6 mb-6 text-white">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h2 className="text-xl sm:text-2xl font-bold mb-1">{t('dashboard.quickPlay')}</h2>
          <p className="text-blue-100 text-sm">{t('dashboard.quickPlaySubtitle')}</p>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <button
            type="button"
            onClick={onStartSolo}
            disabled={starting}
            className="px-5 py-3 bg-white text-blue-700 text-sm sm:text-base font-bold rounded-lg hover:bg-blue-50 disabled:opacity-50"
          >
            {starting ? t('dashboard.startGameLoading') : t('dashboard.soloGame')}
          </button>
          <button
            type="button"
            onClick={onOpenMultiplayer}
            disabled={starting}
            className="px-5 py-3 bg-purple-500 text-white text-sm sm:text-base font-bold rounded-lg hover:bg-purple-400 disabled:opacity-50"
          >
            {t('dashboard.multiplayerGame')}
          </button>
          <ToggleSwitch checked={stepByStep} onChange={onStepByStepChange} label={t('game.stepByStep')} />
        </div>
      </div>
    </div>
  )
}
