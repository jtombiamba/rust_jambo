import { useTranslation } from 'react-i18next'

interface MultiplayerModalProps {
  open: boolean
  step: 1 | 2
  bet: number
  maxPlayers: number
  creating: boolean
  error: string | null
  pseudos: Record<number, string>
  credit: number
  onClose: () => void
  onStepChange: (step: 1 | 2) => void
  onBetChange: (bet: number) => void
  onMaxPlayersChange: (max: number) => void
  onPseudoChange: (slot: number, value: string) => void
  onSubmit: () => void
}

export default function MultiplayerModal({
  open,
  step,
  bet,
  maxPlayers,
  creating,
  error,
  pseudos,
  credit,
  onClose,
  onStepChange,
  onBetChange,
  onMaxPlayersChange,
  onPseudoChange,
  onSubmit,
}: MultiplayerModalProps) {
  const { t } = useTranslation()

  if (!open) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white rounded-lg shadow-xl p-6 w-full max-w-md mx-4">
        <div className="flex justify-between items-center mb-4">
          <h2 className="text-xl font-bold">
            {step === 1 ? t('dashboard.createMultiplayerGame') : t('dashboard.invitePlayers')}
          </h2>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-gray-600 text-2xl leading-none"
            aria-label={t('common.close')}
          >
            &times;
          </button>
        </div>

        {step === 1 && (
          <>
            <div className="mb-4">
              <label className="block text-sm font-medium text-gray-700 mb-1">
                {t('dashboard.betAmount')}
              </label>
              <input
                type="number"
                min={1}
                max={credit}
                value={bet}
                onChange={(e) => onBetChange(parseInt(e.target.value) || 0)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-purple-500"
              />
              <p className="text-sm text-gray-500 mt-1">{t('dashboard.yourCredit', { credit })}</p>
            </div>

            <div className="mb-4">
              <label className="block text-sm font-medium text-gray-700 mb-1">
                {t('dashboard.numberOfPlayers')}
              </label>
              <select
                value={maxPlayers}
                onChange={(e) => onMaxPlayersChange(parseInt(e.target.value))}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-purple-500"
              >
                <option value={2}>{t('dashboard.xPlayers', { count: 2 })}</option>
                <option value={3}>{t('dashboard.xPlayers', { count: 3 })}</option>
                <option value={4}>{t('dashboard.xPlayers', { count: 4 })}</option>
              </select>
              <p className="text-sm text-gray-400 mt-1">{t('dashboard.gameCancelExpiry')}</p>
            </div>

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
                onClick={() => onStepChange(2)}
                disabled={bet <= 0 || bet > credit}
                className="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 disabled:opacity-50"
              >
                {t('dashboard.nextInvitePlayers')}
              </button>
            </div>
          </>
        )}

        {step === 2 && (
          <>
            <p className="text-sm text-gray-500 mb-4">
              {t('dashboard.playersDesc', {
                credits: bet,
                players: maxPlayers,
                others: maxPlayers - 1,
              })}
            </p>

            {Array.from({ length: maxPlayers - 1 }, (_, i) => i + 1).map((slot) => (
              <div key={slot} className="mb-3">
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  {t('dashboard.playerSlot', { slot })}
                </label>
                <input
                  type="text"
                  placeholder={t('dashboard.pseudoPlaceholder')}
                  value={pseudos[slot] || ''}
                  onChange={(e) => {
                    let val = e.target.value
                    if (val && !val.startsWith('@')) val = '@' + val
                    onPseudoChange(slot, val)
                  }}
                  className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-purple-500"
                />
                <p className="text-xs text-gray-400 mt-0.5">{t('dashboard.leaveEmpty')}</p>
              </div>
            ))}

            {error && (
              <div className="mb-4 p-3 bg-red-100 text-red-700 rounded text-sm">{error}</div>
            )}

            <div className="flex justify-end gap-3">
              <button
                onClick={() => onStepChange(1)}
                className="px-4 py-2 border border-gray-300 rounded-lg text-gray-700 hover:bg-gray-100"
              >
                {t('dashboard.back')}
              </button>
              <button
                onClick={onSubmit}
                disabled={creating}
                className="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 disabled:opacity-50"
              >
                {creating ? t('dashboard.creating') : t('dashboard.createAndInvite')}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  )
}
