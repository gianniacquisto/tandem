import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import type { Page } from '@playwright/test'

import { REPO_ROOT } from './roots'

const TOKENS = join(REPO_ROOT, 'app', 'renderer', 'src', 'tokens.css')

/** `#F1E9D7` as the `rgb(...)` a computed style reports. */
function rgb(hex: string): string {
  const value = parseInt(hex.slice(1), 16)
  return `rgb(${(value >> 16) & 255}, ${(value >> 8) & 255}, ${value & 255})`
}

/**
 * A palette token's value, as a computed style reports it: a colour as
 * `rgb(...)`, a length as its px value. Read from `tokens.css`, never restated,
 * so a palette change still passes.
 *
 * The day block is the unindented `:root` rule; the night block lives inside
 * the media query and is indented, so a file that loses the day block fails
 * here rather than silently asserting the night palette.
 */
export function token(name: string, scheme: 'day' | 'night' = 'day'): string {
  const css = readFileSync(TOKENS, 'utf8').replace(/\r\n/g, '\n')
  const block =
    scheme === 'day'
      ? css.match(/^:root\s*\{([\s\S]*?)^\}/m)?.[1]
      : css.match(/@media \(prefers-color-scheme: dark\)\s*\{\s*:root\s*\{([\s\S]*?)\n  \}/)?.[1]
  if (block === undefined) throw new Error(`tokens.css has no ${scheme} :root block`)
  const value = block.match(new RegExp(`--${name}:\\s*([^;]+);`))?.[1]?.trim()
  if (value === undefined) throw new Error(`the ${scheme} :root block has no --${name}`)
  return value.startsWith('#') ? rgb(value) : value
}

/** One element's computed value for a CSS property, as the page reports it. */
export async function computed(window: Page, selector: string, property: string): Promise<string> {
  const query = JSON.stringify(selector)
  return window.evaluate<string>(
    `(() => {
      const element = document.querySelector(${query})
      if (element === null) throw new Error('no element matches ' + ${query})
      return getComputedStyle(element).getPropertyValue(${JSON.stringify(property)})
    })()`,
  )
}
