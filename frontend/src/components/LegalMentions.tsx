import { useTranslation } from 'react-i18next'

interface LegalMentionsProps {
  isOpen: boolean
  onClose: () => void
}

export default function LegalMentions({ isOpen, onClose }: LegalMentionsProps) {
  const { t } = useTranslation()

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div className="bg-white rounded-xl shadow-2xl w-full max-w-lg max-h-[80vh] overflow-y-auto p-6 relative">
        <button
          onClick={onClose}
          className="absolute top-3 right-3 text-gray-400 hover:text-gray-600 text-2xl leading-none"
        >
          &times;
        </button>

        <h2 className="text-xl font-semibold mb-4">{t('legal.title')}</h2>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">{t('legal.section1Title')}</h3>
          <p className="text-sm text-gray-600">{t('legal.section1Text')}</p>
        </section>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">{t('legal.section2Title')}</h3>
          <p className="text-sm text-gray-600">{t('legal.section2Text')}</p>
        </section>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">{t('legal.section3Title')}</h3>
          <p className="text-sm text-gray-600">{t('legal.section3Text')}</p>
        </section>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">{t('legal.section4Title')}</h3>
          <p className="text-sm text-gray-600">{t('legal.section4Text')}</p>
        </section>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">{t('legal.section5Title')}</h3>
          <p className="text-sm text-gray-600">{t('legal.section5Text')}</p>
        </section>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">{t('legal.section6Title')}</h3>
          <p className="text-sm text-gray-600">{t('legal.section6Text')}</p>
        </section>
      </div>
    </div>
  )
}
