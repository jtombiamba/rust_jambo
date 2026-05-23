interface LegalMentionsProps {
  isOpen: boolean
  onClose: () => void
}

export default function LegalMentions({ isOpen, onClose }: LegalMentionsProps) {
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

        <h2 className="text-xl font-semibold mb-4">Legal mentions</h2>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">1. Site Editor</h3>
          <p className="text-sm text-gray-600">
            This website is edited by the FapFap Card Game team.
            For any inquiries, please use the Contact Us form available in the footer.
          </p>
        </section>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">2. Hosting</h3>
          <p className="text-sm text-gray-600">
            The site is hosted on secure infrastructure.
            Technical specifications and hosting provider details are available upon request.
          </p>
        </section>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">3. Intellectual Property</h3>
          <p className="text-sm text-gray-600">
            All content present on this site (text, graphics, logos, game mechanics) is protected by
            intellectual property laws. Any reproduction, distribution, modification, or use of this
            content without explicit authorization is prohibited.
          </p>
        </section>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">4. Personal Data</h3>
          <p className="text-sm text-gray-600">
            In accordance with applicable data protection regulations, you have the right to access,
            modify, and delete your personal data. To exercise these rights, please contact us
            through the Contact Us form. We collect only the data necessary for the operation of
            the service: email address, pseudonym, and game statistics.
          </p>
        </section>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">5. Cookies</h3>
          <p className="text-sm text-gray-600">
            This site uses essential cookies for authentication and session management.
            No advertising or tracking cookies are used.
          </p>
        </section>

        <section className="mb-4">
          <h3 className="font-semibold mb-2">6. Liability</h3>
          <p className="text-sm text-gray-600">
            The site editor strives to provide accurate and up-to-date information but cannot
            guarantee the absence of errors or omissions. The user acknowledges using the site
            under their own responsibility.
          </p>
        </section>
      </div>
    </div>
  )
}
