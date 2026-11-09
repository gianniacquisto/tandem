#!/usr/bin/env node
// The opt-in bikeshed sweep: prove the block model's round trip over a real
// note folder. Read-only by construction - it opens each `.md` for reading,
// compares the serialised document with the bytes on disk, and reports only the
// path and line of a failure. It never writes, copies or prints note content,
// never runs in CI, and is not wired into `npm test`.
//
//   node Scripts/bikeshed-sweep.mjs <notes-dir>
//
// The block model is TypeScript, so Node's own type stripping loads it; no
// build step and no second copy of the parser.

import { readFileSync, readdirSync } from 'node:fs'
import path from 'node:path'

import { parseDocument, serialiseDocument } from '../app/renderer/src/shell/blocks.ts'

const target = process.argv[2]
if (target === undefined) {
  console.error('usage: node Scripts/bikeshed-sweep.mjs <notes-dir>')
  process.exit(2)
}

function walk(dir) {
  const out = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) out.push(...walk(full))
    else if (entry.name.endsWith('.md')) out.push(full)
  }
  return out
}

function firstDifference(left, right) {
  const shorter = Math.min(left.length, right.length)
  for (let i = 0; i < shorter; i++) if (left[i] !== right[i]) return i
  return left.length === right.length ? -1 : shorter
}

function lineAt(text, offset) {
  let line = 1
  for (let i = 0; i < offset && i < text.length; i++) if (text[i] === '\n') line++
  return line
}

let checked = 0
let failed = 0
for (const file of walk(target).sort()) {
  const original = readFileSync(file)
  const text = original.toString('utf8')
  checked++

  let output
  try {
    output = Buffer.from(serialiseDocument(parseDocument(text)), 'utf8')
  } catch {
    // A thrown parse or serialise is still a failure at the note's first line.
    console.log(`${file}:1`)
    failed++
    continue
  }
  if (output.equals(original)) continue

  console.log(`${file}:${lineAt(text, firstDifference(output, original))}`)
  failed++
}

console.log(`${checked} notes checked, ${failed} failed`)
process.exit(failed === 0 ? 0 : 1)
