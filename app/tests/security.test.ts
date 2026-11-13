import { describe, expect, it } from 'vitest'

import { contentSecurityPolicy } from '../src/shared/security'

describe('the content security policy', () => {
  it('allows nothing but the page the shell itself loaded', () => {
    const policy = contentSecurityPolicy(false)

    expect(policy).toContain("default-src 'self'")
    expect(policy).toContain("script-src 'self';")
    expect(policy).not.toContain("script-src 'self' 'unsafe-inline'")
    expect(policy).toContain("object-src 'none'")
    expect(policy).toContain("frame-ancestors 'self'")
  })

  it('admits no frame and no remote frame', () => {
    const policy = contentSecurityPolicy(false)

    expect(policy).toContain("frame-src 'none'")
    expect(policy).not.toContain("frame-src 'self'")
    expect(policy).not.toContain("frame-ancestors 'none'")
    expect(policy).not.toMatch(/frame-src[^;]*https?:/)
  })

  it('admits the dev server socket only in development', () => {
    expect(contentSecurityPolicy(true)).toContain('ws://localhost:*')
    expect(contentSecurityPolicy(false)).not.toContain('ws://')
  })

  it('admits no scheme for images, the asset protocol being cut', () => {
    for (const development of [false, true]) {
      const policy = contentSecurityPolicy(development)
      expect(policy).toContain("img-src 'self' data:")
      expect(policy).not.toContain('tandem:')
      expect(policy).not.toMatch(/img-src[^;]*https?:/)
      expect(policy).not.toMatch(/img-src[^;]*\*/)
    }
  })

  it('keeps every other directive unchanged', () => {
    expect(contentSecurityPolicy(false)).toBe(
      [
        "default-src 'self'",
        "script-src 'self'",
        "style-src 'self' 'unsafe-inline'",
        "img-src 'self' data:",
        "font-src 'self'",
        "connect-src 'self'",
        "object-src 'none'",
        "base-uri 'none'",
        "form-action 'none'",
        "frame-src 'none'",
        "frame-ancestors 'self'",
      ].join('; '),
    )
  })
})
