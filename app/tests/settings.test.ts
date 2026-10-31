import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { checkChanges } from '../renderer/src/shell/setup'
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
  // declared, which is the one way a renderer could invent a writer.
  it('calls only methods the wire declares, so a UI-only write cannot appear', () => {
    const known = wireMethods()
    expect(known.size).toBeGreaterThan(0)

    for (const name of ['App.tsx', 'faces/SettingsFace.tsx']) {
      for (const tag of calledMethods(source(name))) {
        expect(known, `${name} calls ${tag}`).toContain(tag)
      }
    }
  })

  it('adds no writer of its own: one control, and it is the CLI setup install', () => {
    const face = source('faces/SettingsFace.tsx')
    const called = [...new Set(calledMethods(face))].sort()

    expect(called).toEqual(['doctor.report', 'setup.install', 'setup.status'])
    expect([...face.matchAll(/<button\b/g)]).toHaveLength(1)
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
})
