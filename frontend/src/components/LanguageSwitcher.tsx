import { useLanguageStore } from '../stores/useLanguageStore'

export default function LanguageSwitcher() {
  const { language, setLanguage, availableLanguages } = useLanguageStore()

  if (!availableLanguages || availableLanguages.length < 2) {
    return null
  }

  return (
    <div className="flex gap-1">
      {availableLanguages.map((lang) => (
        <button
          key={lang.code}
          onClick={() => setLanguage(lang.code)}
          className={`px-2 py-1 text-xs font-semibold rounded ${
            language === lang.code
              ? 'bg-blue-600 text-white'
              : 'bg-gray-100 text-gray-600 hover:bg-gray-200'
          }`}
        >
          {lang.code.toUpperCase()}
        </button>
      ))}
    </div>
  )
}
