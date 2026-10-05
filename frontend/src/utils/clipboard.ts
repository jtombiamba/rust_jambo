/**
 * Copy text to the system clipboard with a robust fallback.
 *
 * `navigator.clipboard.writeText` is only available in secure contexts and
 * requires the document to be focused (and, in some browsers, a recent user
 * activation). Because the stream URL is fetched asynchronously before we copy
 * it, the transient user activation can be lost by the time `writeText` runs,
 * which makes it silently reject and leave the clipboard empty.
 *
 * When the async Clipboard API is unavailable or rejects, we fall back to the
 * legacy `document.execCommand('copy')` technique on a hidden textarea. This
 * keeps the text in the clipboard in more environments (including plain HTTP).
 *
 * @returns `true` when the text is (believed to be) in the clipboard.
 */
export async function copyTextToClipboard(text: string): Promise<boolean> {
  // Preferred: async Clipboard API (secure contexts only).
  if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      console.log("Copied to clipboard using navigator.clipboard.writeText");
      return true;
    } catch {
      // Fall through to the legacy fallback below.
    }
  }

  // Legacy fallback: hidden textarea + execCommand('copy').
  try {
    const textarea = document.createElement('textarea');
    textarea.value = text;
    textarea.setAttribute('readonly', '');
    // Keep it out of the viewport and avoid scrolling the page to it.
    textarea.style.position = 'fixed';
    textarea.style.top = '-9999px';
    textarea.style.left = '-9999px';
    document.body.appendChild(textarea);
    textarea.focus();
    textarea.select();
    textarea.setSelectionRange(0, text.length);
    const ok = document.execCommand('copy');
    document.body.removeChild(textarea);
    console.log("Copied to clipboard using document.execCommand('copy')");
    return ok;
  } catch {
    return false;
  }
}
