import type { DoctorCheck } from '../wire/DoctorCheck'

/** One grouped block of the setup report, in the order the face shows it. */
export type CheckGroup = {
  title: string
  checks: DoctorCheck[]
}

const STORE = new Set(['home', 'store', 'comments', 'sessions', 'settings', 'todos-json', 'lock'])
const BIKESHED = new Set(['bikeshed'])
const AGENT_FACE = new Set(['skills', 'agent-face'])

/**
 * Group the doctor checks the way Settings shows them: the store and its
 * retired sidecars, the notes folder, the agent face, and the install state.
 * Order inside each group follows the report's own pinned order, and every
 * check lands in exactly one group, so a future check name still shows.
 */
export function groupChecks(checks: readonly DoctorCheck[]): CheckGroup[] {
  const store: DoctorCheck[] = []
  const bikeshed: DoctorCheck[] = []
  const agentFace: DoctorCheck[] = []
  const installState: DoctorCheck[] = []
  for (const check of checks) {
    if (STORE.has(check.name)) store.push(check)
    else if (BIKESHED.has(check.name)) bikeshed.push(check)
    else if (AGENT_FACE.has(check.name)) agentFace.push(check)
    else installState.push(check)
  }
  return [
    { title: 'Store', checks: store },
    { title: 'Bikeshed', checks: bikeshed },
    { title: 'Agent face', checks: agentFace },
    { title: 'Install state', checks: installState },
  ].filter((group) => group.checks.length > 0)
}

/**
 * What Install does about one failing check, in one line. Passing checks
 * need no fix, so they answer null. The agent-face rows are the ones Install
 * rewrites; every other row says Install leaves it alone and where to look
 * instead, so a red row never sends the reader to a log.
 */
export function fixForCheck(check: DoctorCheck): string | null {
  if (check.ok) return null
  switch (check.name) {
    case 'agent-face':
      return 'Install fixes this: it rewrites the bundled skills, prompts and host links.'
    case 'skills':
      return 'Install fixes this: it relinks the host skills.'
    case 'lock':
      return 'Install leaves the lock alone: wait for the other process to finish, then retry.'
    case 'store':
      return 'Install leaves the store alone: fix the file the detail names, or restore the backup.'
    case 'bikeshed':
      return 'Install leaves the notes folder alone: pick a folder at the path the detail names.'
    case 'comments':
      return 'Install leaves comments alone: the detail names whether the legacy log is still the store.'
    case 'home':
      return 'Install leaves the home alone: the detail names where it resolved.'
    case 'app':
      return 'Install leaves the app alone: it never installs Tandem itself, the detail names the quarantine fix.'
    case 'harness-auth':
      return 'Install leaves sign-in alone: sign in below instead.'
    case 'mcp':
      return 'Install leaves this alone: rebuilding the app restores the tool count.'
    default:
      return 'Install leaves this alone: follow the detail above.'
  }
}
