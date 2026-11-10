import { writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { expect } from '@playwright/test'

import { openTab, test } from '../../shots/app'
import { seedPath } from '../../shots/home'
import { REPO_ROOT } from '../../shots/roots'
import {
  isKatexChunk,
  isMermaidChunk,
  readRendererManifest,
} from '../lazy-manifest'

/**
 * The request-log half of UI-24: a note without a mermaid fence or math opens
 * without asking for either chunk, and a note with both asks for each and then
 * draws them. No scene and no picture: the evidence is the built app's network
 * log, not a photograph.
 */

const APP_ROOT = join(REPO_ROOT, 'app')
const manifest = readRendererManifest(APP_ROOT)
const mermaidFiles = Object.values(manifest)
  .filter(isMermaidChunk)
  .map((chunk) => chunk.file)
const katexFiles = Object.values(manifest)
  .filter(isKatexChunk)
  .map((chunk) => chunk.file)

function requestedChunks(urls: string[], files: string[]): string[] {
  return urls.filter((url) => files.some((file) => url.endsWith(file)))
}

test('a note without a fence or math asks for neither chunk', async ({ window }) => {
  writeFileSync(
    join(seedPath(), 'bikeshed', 'lazy-plain.md'),
    '# Plain\n\nOnly words here.\n',
  )
  writeFileSync(
    join(seedPath(), 'bikeshed', 'lazy-rich.md'),
    '# Render\n\n```mermaid\ngraph TD; A-->B\n```\n\n$$\nx = 1\n$$\n',
  )

  // The app indexes the vault on the face's mount; reload so the two seeded
  // notes are in the tree whichever tab the shell remembered from a past run.
  await window.reload()
  await window.waitForSelector('.shell')

  await openTab(window, 'bikeshed')
  await window.evaluate("window.localStorage.setItem('tandem.bikeshed.mode', 'write')")
  await window.evaluate("window.dispatchEvent(new Event('focus'))")

  const requested: string[] = []
  window.on('request', (request) => requested.push(request.url()))

  await window.locator('.tree__label[title="lazy-plain.md"]').click()
  await expect(window.locator('.blocks')).toContainText('Only words here')

  expect(requestedChunks(requested, mermaidFiles)).toEqual([])
  expect(requestedChunks(requested, katexFiles)).toEqual([])

  requested.length = 0

  await window.locator('.tree__label[title="lazy-rich.md"]').click()
  await expect(window.locator('.block--mermaid svg')).toBeVisible()
  await expect(window.locator('.block--math .katex')).toBeVisible()

  expect(requestedChunks(requested, mermaidFiles).length).toBeGreaterThan(0)
  expect(requestedChunks(requested, katexFiles).length).toBeGreaterThan(0)
})
