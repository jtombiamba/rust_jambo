import { useTranslation } from 'react-i18next'

interface ToggleSwitchProps {
  checked: boolean
  onChange: (value: boolean) => void
  label: string
}

export default function ToggleSwitch({ checked, onChange, label }: ToggleSwitchProps) {
  const { t } = useTranslation()

  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className={`inline-flex items-center gap-2 rounded-full px-1 py-1 pr-3 text-xs font-semibold select-none transition-colors ${
        checked ? 'bg-blue-600 text-white' : 'bg-gray-300 text-gray-700'
      }`}
    >
      <span
        className={`h-6 w-6 rounded-full bg-white shadow transition-transform ${
          checked ? 'order-last' : ''
        }`}
      />
      <span className="leading-none">{label}</span>
      <span className="font-bold leading-none">{checked ? t('common.on') : t('common.off')}</span>
    </button>
  )
}
