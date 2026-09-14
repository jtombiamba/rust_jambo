import { useTranslation } from 'react-i18next';

interface MobileTopBarProps {
  onBack: () => void;
  onOpenRules: () => void;
}

export default function MobileTopBar({ onBack, onOpenRules }: MobileTopBarProps) {
  const { t } = useTranslation();

  return (
    <div className="fixed top-0 inset-x-0 z-40 flex items-center justify-between px-3 py-2 md:hidden">
      <button
        type="button"
        onClick={onBack}
        aria-label={t('common.backToDashboard')}
        data-testid="mobile-back-button"
        className="w-11 h-11 flex items-center justify-center rounded-full bg-black/40 text-white hover:bg-black/60 active:scale-95 transition-colors shadow-lg"
      >
        <svg width="26" height="26" viewBox="0 0 24 24" fill="none" aria-hidden="true">
          <path
            d="M15 5 L8 12 L15 19"
            stroke="currentColor"
            strokeWidth="4"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      <button
        type="button"
        onClick={onOpenRules}
        data-testid="mobile-rules-button"
        className="px-4 py-2 text-sm font-semibold bg-black/40 text-white rounded-full hover:bg-black/60 active:scale-95 transition-colors shadow-lg"
      >
        {t('dashboard.rules')}
      </button>
    </div>
  );
}
