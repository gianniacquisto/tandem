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

  it('admits exactly the editor frame the bundle loads in, and no remote frame', () => {
    const policy = contentSecurityPolicy(false)

    expect(policy).toContain("frame-src 'self'")
    expect(policy).not.toContain("frame-ancestors 'none'")
    expect(policy).not.toMatch(/frame-src[^;]*https?:/)
  })

  it('admits the dev server socket only in development', () => {
    expect(contentSecurityPolicy(true)).toContain('ws://localhost:*')
    expect(contentSecurityPolicy(false)).not.toContain('ws://')
  })
})
