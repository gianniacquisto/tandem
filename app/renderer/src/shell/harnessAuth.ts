import type {
  HarnessIntegration,
  HarnessStatusReply,
} from '../../../src/shared/bridge'

/** The sign-in section's four states: what the Settings face draws. */
export type HarnessPhase = 'signed-out' | 'pending' | 'stored' | 'failure'

/** One per-integration row: the name the face draws and who is stored. */
export type HarnessRow = {
  id: string
  name: string
  detail: string
  stored: boolean
}

/** The Console row with a stored account, if the harness home holds one. */
export function consoleStored(integrations: HarnessIntegration[]): HarnessIntegration | null {
  return integrations.find((row) => row.id === 'opencode' && row.connections.length > 0) ?? null
}

/**
 * The section's phase from one status read. Stored wins: a credential that
 * landed is the state even while the device child still exits. A failure
 * reads only while unsigned, so a lapsed attempt beside an older sign-in
 * still draws the sign-in.
 */
export function harnessPhase(status: HarnessStatusReply): HarnessPhase {
  if (consoleStored(status.integrations) !== null) return 'stored'
  if (status.failure !== null) return 'failure'
  if (status.pending !== null) return 'pending'
  return 'signed-out'
}

/** One row per integration, so a later provider is a second row. */
export function harnessRows(status: HarnessStatusReply): HarnessRow[] {
  return status.integrations.map((row) => ({
    id: row.id,
    name: row.name,
    detail:
      row.connections.length === 0
        ? 'not stored'
        : row.connections.map((account) => `${account.label} (${account.method})`).join(', '),
    stored: row.connections.length > 0,
  }))
}
