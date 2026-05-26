import { create } from 'zustand'
import axios from 'axios'
import i18n from '../i18n/config'

interface Language {
  code: string
  label: string
}

interface LanguageState {
  language: string
  availableLanguages: Language[]
  loaded: boolean
  init: () => Promise<void>
  setLanguage: (lang: string) => Promise<void>
  syncFromUser: (userLanguage: string) => void
}

export const useLanguageStore = create<LanguageState>((set, get) => ({
  language: i18n.language || 'en',
  availableLanguages: [],
  loaded: false,

  init: async () => {
    if (get().loaded) return

    try {
      const res = await axios.get<{
        current: string
        languages: { code: string; label: string }[]
      }>('/api/languages')

      const backendLang = res.data.current
      if (backendLang && backendLang !== i18n.language && !get().loaded) {
        i18n.changeLanguage(backendLang)
      }

      set({
        language: backendLang || i18n.language,
        availableLanguages: res.data.languages,
        loaded: true,
      })
    } catch {
      set({
        language: i18n.language,
        availableLanguages: [
          { code: 'en', label: 'English' },
          { code: 'fr', label: 'Français' },
        ],
        loaded: true,
      })
    }
  },

  setLanguage: async (lang: string) => {
    const prevLanguage = get().language
    i18n.changeLanguage(lang)
    set({ language: lang })

    try {
      await axios.post('/api/lang', { lang })
    } catch {
      i18n.changeLanguage(prevLanguage)
      set({ language: prevLanguage })
    }
  },

  syncFromUser: (userLanguage: string) => {
    const { language } = get()
    if (userLanguage && userLanguage !== language) {
      i18n.changeLanguage(userLanguage)
      set({ language: userLanguage, loaded: true })
    }
  },
}))
