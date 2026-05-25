import { useTranslation } from 'react-i18next'

interface Props {
  isOpen: boolean
  onClose: () => void
}

export default function GameRules({ isOpen, onClose }: Props) {
  const { t } = useTranslation()

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="relative w-full max-w-lg max-h-[85vh] overflow-y-auto bg-white rounded-xl shadow-2xl mx-4">
        <button
          onClick={onClose}
          className="absolute top-3 right-3 w-8 h-8 flex items-center justify-center rounded-full bg-gray-100 hover:bg-gray-200 text-gray-500 text-lg font-bold"
          aria-label="Close rules"
        >
          &times;
        </button>

        <div className="p-6">
          <h2 className="text-2xl font-bold mb-6">{t('rules.title')}</h2>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">{t('rules.objective')}</h3>
            <p className="text-gray-700">{t('rules.objectiveText')}</p>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">{t('rules.theDeck')}</h3>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>{t('rules.deckText1')}</li>
              <li>{t('rules.deckText2', { hearts: t('rules.hearts'), spades: t('rules.spades'), diamonds: t('rules.diamonds'), clubs: t('rules.clubs') })}</li>
              <li>{t('rules.deckText3')}</li>
            </ul>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">{t('rules.setup')}</h3>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>{t('rules.setupText1')}</li>
              <li>{t('rules.setupText2')}</li>
              <li>{t('rules.setupText3')}</li>
            </ul>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">{t('rules.turnFlow')}</h3>
            <ol className="list-decimal pl-5 space-y-1 text-gray-700">
              <li>{t('rules.turnFlowText1')}</li>
              <li>{t('rules.turnFlowText2')}</li>
              <li>{t('rules.turnFlowText3')}</li>
              <li>{t('rules.turnFlowText4')}</li>
            </ol>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">{t('rules.winningRound')}</h3>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>{t('rules.winningRoundText1')}</li>
              <li>{t('rules.winningRoundText2')}</li>
              <li>{t('rules.winningRoundText3')}</li>
              <li>{t('rules.winningRoundText4')}</li>
              <li>{t('rules.winningRoundText5')}</li>
            </ul>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">{t('rules.kora')}</h3>
            <p className="text-gray-700 mb-1">{t('rules.koraText')}</p>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>{t('rules.koraText1')}</li>
              <li>{t('rules.koraText2')}</li>
            </ul>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">{t('rules.doubleKora')}</h3>
            <p className="text-gray-700 mb-1">{t('rules.doubleKoraText')}</p>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>{t('rules.doubleKoraText1')}</li>
              <li>{t('rules.doubleKoraText2')}</li>
            </ul>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">{t('rules.gameEnd')}</h3>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>{t('rules.gameEndText1')}</li>
              <li>{t('rules.gameEndText2')}</li>
              <li>{t('rules.gameEndText3')}</li>
            </ul>
          </section>

          <button
            onClick={onClose}
            className="w-full mt-4 px-6 py-3 bg-blue-600 text-white font-semibold rounded-lg hover:bg-blue-700"
          >
            {t('rules.gotIt')}
          </button>
        </div>
      </div>
    </div>
  )
}
