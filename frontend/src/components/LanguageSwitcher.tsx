import { useLanguageStore } from '../stores/useLanguageStore'

export default function LanguageSwitcher() {
  const { language, setLanguage, availableLanguages } = useLanguageStore()

  if (!availableLanguages || availableLanguages.length < 2) {
    return null
  }

  return (
    <div className="inline-flex rounded-full bg-gray-200 p-1">
      {availableLanguages.map((lang) => (
        <button
          key={lang.code}
          onClick={() => setLanguage(lang.code)}
          className={`px-3 py-1 text-xs font-semibold rounded-full transition-colors ${
            language === lang.code
              ? 'bg-white text-blue-600 shadow'
              : 'text-gray-600 hover:text-gray-800'
          }`}
        >
          {lang.code.toUpperCase()}
        </button>
      ))}
    </div>
  )
}
