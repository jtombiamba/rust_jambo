import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import axios from 'axios'
import { useGameStore } from '../stores/useGameStore'
import { getCachedConfig, saveConfig } from '../utils/configCache'
import LegalMentions from './LegalMentions'
import ContactForm from './ContactForm'

interface FooterProps {
  hideOnMobile?: boolean
}

export default function Footer({ hideOnMobile = false }: FooterProps) {
  const { t } = useTranslation()
  const [legalOpen, setLegalOpen] = useState(false)
  const [contactOpen, setContactOpen] = useState(false)

  useEffect(() => {
    const cached = getCachedConfig()
    if (cached) {
      if (cached.bot_thinking_delay_ms || cached.round_pause_delay_ms) {
        useGameStore.getState().setBotDelays(
          cached.bot_thinking_delay_ms ?? 1500,
          cached.round_pause_delay_ms ?? 2500,
        )
      }
      return
    }

    axios.get('/api/config')
      .then((res) => {
        saveConfig(res.data)
        if (res.data.bot_thinking_delay_ms || res.data.round_pause_delay_ms) {
          useGameStore.getState().setBotDelays(
            res.data.bot_thinking_delay_ms ?? 1500,
            res.data.round_pause_delay_ms ?? 2500,
          )
        }
      })
      .catch(() => {
        // ignore config fetch failure; game defaults apply
      })
  }, [])

  return (
    <>
      <footer className={`border-t border-gray-200 bg-gray-50 mt-auto ${hideOnMobile ? 'hidden md:block' : ''}`}>
        <div className="container mx-auto px-4 py-4">
          <div className="hidden sm:flex items-center justify-between text-sm text-gray-500">
            <div className="flex gap-4">
              <button
                onClick={() => setLegalOpen(true)}
                className="hover:text-gray-700 hover:underline"
              >
                {t('footer.legalMentions')}
              </button>
              <button
                onClick={() => setContactOpen(true)}
                className="hover:text-gray-700 hover:underline"
              >
                {t('footer.contactUs')}
              </button>
            </div>
          </div>

          <div className="sm:hidden flex flex-col gap-2 text-sm">
            <button
              onClick={() => setLegalOpen(true)}
              className="text-left text-gray-500 hover:text-gray-700 py-1"
            >
              {t('footer.legalMentions')}
            </button>
            <button
              onClick={() => setContactOpen(true)}
              className="text-left text-gray-500 hover:text-gray-700 py-1"
            >
              {t('footer.contactUs')}
            </button>
          </div>
        </div>
      </footer>

      <LegalMentions isOpen={legalOpen} onClose={() => setLegalOpen(false)} />
      <ContactForm isOpen={contactOpen} onClose={() => setContactOpen(false)} />
    </>
  )
}
