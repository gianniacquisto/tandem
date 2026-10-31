import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'

import { createBridge, type IpcSurface } from './surface'

/**
 * The one preload. Sandboxed, and narrow on purpose: a call goes to the main
 * process, a change event comes back, and nothing else crosses the wall. The
 * renderer never sees `ipcRenderer`, Node, or a path.
 */
const ipc: IpcSurface = {
  invoke: (channel, call) => ipcRenderer.invoke(channel, call),
  on: (channel, listener) => {
    const wrapped = (_event: IpcRendererEvent, revision: number) => listener(revision)
    ipcRenderer.on(channel, wrapped)
    return () => {
      ipcRenderer.off(channel, wrapped)
    }
  },
}

contextBridge.exposeInMainWorld('tandem', createBridge(ipc))
