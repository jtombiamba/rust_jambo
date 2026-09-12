const runtimeConfig = typeof window !== "undefined" ? window.__RUNTIME_CONFIG__ : undefined

/**
 * Resolve the API base URL to an absolute URL whose scheme always matches the
 * page's protocol. The configured value may be:
 *  - scheme-relative ("//api.jambo.local") -> inherits the page protocol so we
 *    never trigger mixed-content blocking when the page is served over HTTPS.
 *  - absolute ("https://api.jambo.local")  -> used as-is.
 *  - a bare host ("api.jambo.local")       -> treated as scheme-relative.
 */
function resolveApiBaseUrl(): string {
  const configured =
    runtimeConfig?.VITE_API_BASE_URL ||
    import.meta.env.VITE_API_BASE_URL ||
    "http://backend:5000"

  if (configured.startsWith("//") || !/^[a-z][a-z0-9+.-]*:\/\//i.test(configured)) {
    const protocol = typeof window !== "undefined" ? window.location.protocol : "http:"
    const host = configured.replace(/^\/\//, "")
    return `${protocol}//${host}`
  }

  return configured
}

export function getApiBaseUrl(): string {
  return resolveApiBaseUrl()
}

export function getWsUrl(wsPath: string): string {
  const apiBaseUrl = getApiBaseUrl()

  if (apiBaseUrl) {
    let host: string
    try {
      const parsed = new URL(apiBaseUrl)
      host = parsed.host
    } catch {
      host = apiBaseUrl.replace(/^https?:\/\//, "")
    }
    const wsProtocol = apiBaseUrl.startsWith("https") ? "wss:" : "ws:"
    return `${wsProtocol}//${host}${wsPath}`
  }

  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:"
  return `${protocol}//${window.location.host}${wsPath}`
}
