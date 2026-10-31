import { describe, expect, it } from 'vitest'

import { initialSnapshot, snapshotReducer } from '../renderer/src/shell/snapshot'

describe('the shell snapshot', () => {
  it('starts connecting, with no revision of its own', () => {
    expect(initialSnapshot).toEqual({
      status: 'connecting',
      revision: null,
      tickets: 0,
      error: null,
    })
  })

  it('stores the listing revision and the ticket count', () => {
    const next = snapshotReducer(initialSnapshot, {
      type: 'loaded',
      reply: { revision: 12, tickets: [] },
    })

    expect(next.status).toBe('ready')
    expect(next.revision).toBe(12)
    expect(next.tickets).toBe(0)
    expect(next.error).toBeNull()
  })

  it('keeps the last revision when a read fails, so the face does not lie', () => {
    const ready = snapshotReducer(initialSnapshot, {
      type: 'loaded',
      reply: { revision: 12, tickets: [] },
    })
    const failed = snapshotReducer(ready, { type: 'failed', message: 'the store is locked' })

    expect(failed.status).toBe('failed')
    expect(failed.revision).toBe(12)
    expect(failed.error).toBe('the store is locked')
  })
})
