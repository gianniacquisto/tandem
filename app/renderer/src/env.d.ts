import type { Bridge } from '../../src/shared/bridge'

declare global {
  interface Window {
    /** The one surface the preload exposes: a call and a subscription. */
    readonly tandem: Bridge
  }
}

export {}
