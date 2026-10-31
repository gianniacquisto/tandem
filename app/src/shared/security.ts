/**
 * The policy both the packaged page and the dev server answer to.
 *
 * `docs/port.md` keeps the renderer a browser: no Node, no remote content, no
 * embedded frame. The script source is the bundle the shell itself loaded, so
 * a page that gains an inline script fails rather than runs. Development is the
 * one loosening (`unsafe-inline` for the refresh preamble, and the dev
 * server's socket), and only when the window was opened against a loopback dev
 * server.
 */
export function contentSecurityPolicy(development: boolean): string {
  const script = development ? "'self' 'unsafe-inline'" : "'self'"
  const connect = development ? "'self' ws://localhost:* ws://127.0.0.1:*" : "'self'"

  return [
    "default-src 'self'",
    `script-src ${script}`,
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data:",
    "font-src 'self'",
    `connect-src ${connect}`,
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
    "frame-ancestors 'none'",
  ].join('; ')
}
