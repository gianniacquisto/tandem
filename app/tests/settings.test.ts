import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { checkChanges } from '../renderer/src/shell/setup'
import { fixForCheck, groupChecks } from '../renderer/src/shell/settingsGroups'
import {
  addRosterName,
  cleanRoster,
  removeRosterName,
} from '../renderer/src/shell/roster'
import type { DoctorReport } from '../renderer/src/wire/DoctorReport'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const renderer = join(appRoot, 'renderer', 'src')

function source(name: string): string {
  return readFileSync(join(renderer, name), 'utf8')
}

/** The method tags `Method.ts` declares: the wire's whole surface. */
function wireMethods(): Set<string> {
  const method = readFileSync(join(renderer, 'wire', 'Method.ts'), 'utf8')
  const tags = new Set<string>()
  for (const match of method.matchAll(/"method": "([^"]+)"/g)) {
    if (match[1] !== undefined) tags.add(match[1])
  }
  return tags
}

/** The method tags a source names at a call site. */
function calledMethods(text: string): string[] {
  return [...text.matchAll(/method: '([^']+)'/g)].flatMap((match) =>
    match[1] === undefined ? [] : [match[1]],
  )
}

const report = (rows: Array<[string, boolean, string]>): DoctorReport => ({
  ok: rows.every(([, ok]) => ok),
  checks: rows.map(([name, ok, detail]) => ({ name, ok, detail })),
})

describe('the Settings surface', () => {
  // `docs/port.md` (Transport and the UI contract): every UI mutation maps to a
  // message the CLI also exposes. The Rust half is
  // `cli/src/main.rs::wire_parity`; this half catches a method the wire never
  // declared, which is the one way a renderer could invent a writer. The one
  // exception is `harness.*` (TAN-265): answered by the main process, never by
  // the core, so no CLI verb spells it and the wire never declares it.
  const HARNESS_OWNED = new Set(['harness.status', 'harness.login', 'harness.cancel', 'harness.logout'])

  it('calls only methods the wire declares, so a UI-only write cannot appear', () => {
    const known = wireMethods()
    expect(known.size).toBeGreaterThan(0)

    for (const name of ['App.tsx', 'faces/SettingsFace.tsx']) {
      for (const tag of calledMethods(source(name))) {
        if (HARNESS_OWNED.has(tag)) continue
        expect(known, `${name} calls ${tag}`).toContain(tag)
      }
    }
  })

  it('edits the roster through the wire write, with one explicit save', () => {
    const face = source('faces/SettingsFace.tsx')
    const called = [...new Set(calledMethods(face))].sort()

    expect(called).toEqual([
      'doctor.report',
      'harness.cancel',
      'harness.login',
      'harness.logout',
      'harness.status',
      'settings.get',
      'settings.set',
      'setup.install',
      'setup.status',
    ])
    // Add, Save roster, one Remove per row (one site in source), three path
    // Copy buttons (Home, Bikeshed, one host site in source), Install/Repair,
    // and the sign-in's three: sign in, cancel, sign out.
    expect([...face.matchAll(/<button\b/g)]).toHaveLength(10)
  })

  it('shows a repair as what changed, not as a verdict', () => {
    const before = report([
      ['home', true, 'the home'],
      ['agent-face', false, '1 warning: partial-face'],
    ])
    const after = report([
      ['home', true, 'the home'],
      ['agent-face', true, 'no warnings'],
    ])

    expect(checkChanges(before, after)).toEqual([
      { name: 'agent-face', before: false, after: true, detail: 'no warnings' },
    ])
    expect(checkChanges(after, after)).toEqual([])
  })

  it('reports every row when the read before the repair failed', () => {
    const after = report([
      ['home', true, 'the home'],
      ['agent-face', true, 'no warnings'],
    ])

    expect(checkChanges(null, after).map((change) => change.name)).toEqual(['home', 'agent-face'])
  })

  it('cleans a roster the way the store does, so the face never parses a row', () => {
    expect(cleanRoster(['  alpha  ', '', '   ', 'beta', 'alpha', 'beta '])).toEqual([
      'alpha',
      'beta',
    ])
    expect(cleanRoster([])).toEqual([])
  })

  it('adds a name once, trimmed, and never a blank', () => {
    expect(addRosterName(['alpha'], 'beta')).toEqual(['alpha', 'beta'])
    expect(addRosterName(['alpha'], '  beta  ')).toEqual(['alpha', 'beta'])
    expect(addRosterName(['alpha'], 'alpha')).toEqual(['alpha'])
    expect(addRosterName(['alpha'], '  alpha  ')).toEqual(['alpha'])
    expect(addRosterName(['alpha'], '   ')).toEqual(['alpha'])
    expect(addRosterName([], '')).toEqual([])
  })

  it('removes the named row and leaves the rest in order', () => {
    expect(removeRosterName(['alpha', 'beta', 'gamma'], 1)).toEqual(['alpha', 'gamma'])
    expect(removeRosterName(['alpha'], 0)).toEqual([])
    expect(removeRosterName(['alpha'], 7)).toEqual(['alpha'])
    expect(removeRosterName(['alpha'], -1)).toEqual(['alpha'])
  })

  it('groups the doctor checks instead of one flat list', () => {
    const checks = [
      'home',
      'store',
      'bikeshed',
      'comments',
      'sessions',
      'settings',
      'todos-json',
      'lock',
      'app',
      'skills',
      'agent-face',
      'harness-auth',
      'mcp',
    ].map((name) => ({ name, ok: true, detail: `${name} detail` }))
    const groups = groupChecks(checks)
    expect(groups.map((group) => group.title)).toEqual([
      'Store',
      'Bikeshed',
      'Agent face',
      'Install state',
    ])
    const byTitle = new Map(groups.map((group) => [group.title, group.checks.map((check) => check.name)]))
    expect(byTitle.get('Store')).toEqual(['home', 'store', 'comments', 'sessions', 'settings', 'todos-json', 'lock'])
    expect(byTitle.get('Bikeshed')).toEqual(['bikeshed'])
    expect(byTitle.get('Agent face')).toEqual(['skills', 'agent-face'])
    expect(byTitle.get('Install state')).toEqual(['app', 'harness-auth', 'mcp'])
    // Every row keeps its name, state and detail inside its group.
    for (const group of groups) {
      for (const check of group.checks) {
        expect(check.detail).toBe(`${check.name} detail`)
      }
    }
  })

  it('names what Install fixes on every failing check', () => {
    const failing = [
      { name: 'store', ok: false, detail: 'present but unreadable' },
      { name: 'bikeshed', ok: false, detail: 'a file, not a folder' },
      { name: 'lock', ok: false, detail: 'held by another process' },
      { name: 'agent-face', ok: false, detail: '1 warning' },
      { name: 'app', ok: false, detail: 'quarantined' },
    ]
    for (const check of failing) {
      const fix = fixForCheck(check)
      expect(fix, `${check.name} names Install`).toMatch(/Install/)
      expect((fix as string).length, `${check.name} fix is one line`).toBeGreaterThan(0)
    }
    expect(fixForCheck({ name: 'agent-face', ok: false, detail: '1 warning' })).toMatch(/rewrites|repairs|fixes/)
    expect(fixForCheck({ name: 'store', ok: true, detail: 'all good' })).toBeNull()
  })

  it('keeps Theme out of Paths and copies any path in one click', () => {
    const face = source('faces/SettingsFace.tsx')
    const pathsStart = face.indexOf('<h2 className="settings__heading">Paths</h2>')
    expect(pathsStart).toBeGreaterThan(-1)
    const nextHeading = face.indexOf('<h2 className="settings__heading">', pathsStart + 1)
    const pathsSection = face.slice(pathsStart, nextHeading === -1 ? undefined : nextHeading)
    expect(pathsSection).not.toContain('Theme')
    expect(face).toContain('Preferences')
    expect(face).toContain('System: the app follows the OS appearance')
    expect(face).toMatch(/Copy .*path|navigator\.clipboard/)
  })

  it('says what Install does in one line with no inline code and keeps the promise', () => {
    const face = source('faces/SettingsFace.tsx')
    const setupStart = face.indexOf('<h2 className="settings__heading">Setup</h2>')
    expect(setupStart).toBeGreaterThan(-1)
    const setupSection = face.slice(setupStart)
    expect(setupSection).not.toMatch(/`tandem-cli/)
    expect(setupSection).toMatch(/Nothing is written until you press it/)
  })
})
