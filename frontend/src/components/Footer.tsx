import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import axios from 'axios'
import LegalMentions from './LegalMentions'
import ContactForm from './ContactForm'

export default function Footer() {
  const { t } = useTranslation()
  const [legalOpen, setLegalOpen] = useState(false)
  const [contactOpen, setContactOpen] = useState(false)
  const [donateUrl, setDonateUrl] = useState('https://www.paypal.com/donate')

  useEffect(() => {
    axios.get('/api/config')
      .then((res) => {
        setDonateUrl(res.data.paypal_donate_url)
      })
      .catch(() => {
        // fallback to default URL
      })
  }, [])

  return (
    <>
      <footer className="border-t border-gray-200 bg-gray-50 mt-auto">
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
            <div>
              <a
                href={donateUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 px-3 py-1.5 bg-[#0070ba] text-white text-xs font-semibold rounded-full hover:bg-[#005ea6] transition-colors"
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M7.076 21.337H2.47a.641.641 0 0 1-.633-.74L4.944.901C5.026.382 5.474 0 5.998 0h7.46c2.57 0 4.578.543 5.69 1.81 1.01 1.15 1.304 2.42 1.012 4.287-.023.143-.047.288-.077.437-.983 5.05-4.349 6.797-8.647 6.797H9.604c-.35 0-.648.254-.703.6l-.553 3.513-.327 2.072a.638.638 0 0 1-.63.54h-2.695a.64.64 0 0 1-.63-.54l.01-.079z"/>
                  <path d="M19.065 6.034c-.284 1.456-.64 2.446-1.06 3.247-.432.825-.98 1.498-1.637 2.008-.638.496-1.38.87-2.21 1.114-.51.15-1.046.233-1.612.248l.06-.377c.983-5.05 4.349-6.797 8.647-6.797h1.444c.35 0 .648.254.703.6l.01.08a.62.62 0 0 1-.635.678h-.009l-.27.002c-1.39.012-2.405.348-3.031 1.146-.605.77-.912 1.944-.912 3.646v.152z"/>
                </svg>
                {t('footer.support')}
              </a>
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
            <a
              href={donateUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-gray-500 hover:text-gray-700 py-1"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                <path d="M7.076 21.337H2.47a.641.641 0 0 1-.633-.74L4.944.901C5.026.382 5.474 0 5.998 0h7.46c2.57 0 4.578.543 5.69 1.81 1.01 1.15 1.304 2.42 1.012 4.287-.023.143-.047.288-.077.437-.983 5.05-4.349 6.797-8.647 6.797H9.604c-.35 0-.648.254-.703.6l-.553 3.513-.327 2.072a.638.638 0 0 1-.63.54h-2.695a.64.64 0 0 1-.63-.54l.01-.079z"/>
                <path d="M19.065 6.034c-.284 1.456-.64 2.446-1.06 3.247-.432.825-.98 1.498-1.637 2.008-.638.496-1.38.87-2.21 1.114-.51.15-1.046.233-1.612.248l.06-.377c.983-5.05 4.349-6.797 8.647-6.797h1.444c.35 0 .648.254.703.6l.01.08a.62.62 0 0 1-.635.678h-.009l-.27.002c-1.39.012-2.405.348-3.031 1.146-.605.77-.912 1.944-.912 3.646v.152z"/>
              </svg>
              {t('footer.supportPayPal')}
            </a>
          </div>
        </div>
      </footer>

      <LegalMentions isOpen={legalOpen} onClose={() => setLegalOpen(false)} />
      <ContactForm isOpen={contactOpen} onClose={() => setContactOpen(false)} />
    </>
  )
}
