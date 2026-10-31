import { useEffect, useMemo, useRef, useState } from 'react'

import type { BikeshedHealth } from '../wire/BikeshedHealth'
import type { BacklinkView } from '../wire/BacklinkView'
import type { NoteNodeView } from '../wire/NoteNodeView'
import type { RecoveryView } from '../wire/RecoveryView'
import type { Reply } from '../wire/Reply'
import type { TicketSummary } from '../wire/TicketSummary'
import {
  ancestorsOf,
  folderPaths,
  notePaths,
  resolveWikilink,
} from '../shell/bikeshed'
import {
  keepMine,
  noteContentOf,
  openBuffer,
  saveNote,
  takeTheirs,
  type NoteBuffer,
  type Theirs,
} from '../shell/note'
import { BikeshedEditor } from './BikeshedEditor'
import { BikeshedTree } from './BikeshedTree'
import { Face } from './Face'
import { NoteRail } from './NoteRail'

const HIDDEN_KEY = 'tandem.bikeshed.hidden'

/** `ticket.list`'s absent flags: the open board, the tickets a note may link. */
const OPEN_BOARD = {
  filter: null,
  text: null,
  status: null,
  assignee: null,
  lane: null,
  archived: false,
  blocked: false,
} as const

function treeOf(reply: Reply): NoteNodeView[] | null {
  return 'nodes' in reply ? reply.nodes : null
}

function healthOf(reply: Reply): BikeshedHealth | null {
  return 'health' in reply ? reply.health : null
}

function backlinksOf(reply: Reply): BacklinkView[] | null {
  return 'backlinks' in reply ? reply.backlinks : null
}

function recoveriesOf(reply: Reply): RecoveryView[] | null {
  return 'recoveries' in reply ? reply.recoveries : null
}

function ticketsOf(reply: Reply): TicketSummary[] | null {
  return 'tickets' in reply && 'revision' in reply ? reply.tickets : null
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : 'The bikeshed could not be read.'
}

/**
 * The Bikeshed face: the tree, the editor frame, backlinks, health and the
 * save discipline (`BikeFaceView` in the Swift app).
 *
 * The face owns the reads: the vault is not the store, so a change event only
 * tells it to look again, and a window focus does the same for a note an agent
 * wrote while it was away. Every write is a message the CLI also answers
 * (`note write`, `note move`), and a save compares the hash it read with the
 * hash on disk before it writes: a concurrent write is a conflict with both
 * bodies, never a lost one.
 */
export function BikeshedFace() {
  const [hidden, setHidden] = useState(() => window.localStorage.getItem(HIDDEN_KEY) === 'true')
  const [nodes, setNodes] = useState<NoteNodeView[]>([])
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [open, setOpen] = useState<NoteBuffer | null>(null)
  const [conflict, setConflict] = useState<Theirs | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [backlinks, setBacklinks] = useState<BacklinkView[]>([])
  const [linked, setLinked] = useState<TicketSummary[]>([])
  const [health, setHealth] = useState<BikeshedHealth | null>(null)
  const [recoveries, setRecoveries] = useState<RecoveryView[]>([])
  const [token, setToken] = useState(0)
  const seeded = useRef(false)
  const openRef = useRef<NoteBuffer | null>(null)
  openRef.current = open
  const savingRef = useRef(false)

  const noteIndex = useMemo(() => notePaths(nodes), [nodes])

  async function refreshVault(): Promise<void> {
    try {
      const [tree, report, drafts] = await Promise.all([
        window.tandem.call({ method: 'note.tree', params: { hidden } }),
        window.tandem.call({ method: 'note.health', params: {} }),
        window.tandem.call({ method: 'note.recoveries', params: {} }),
      ])
      if ('error' in tree || 'error' in report || 'error' in drafts) {
        const failure = 'error' in tree ? tree : 'error' in report ? report : drafts
        setError('error' in failure ? failure.error.message : 'The bikeshed could not be read.')
        return
      }
      const read = treeOf(tree.reply)
      if (read !== null) {
        setNodes(read)
        if (!seeded.current) {
          seeded.current = true
          setExpanded(new Set(folderPaths(read)))
        }
      }
      const nextHealth = healthOf(report.reply)
      if (nextHealth !== null) setHealth(nextHealth)
      const nextRecoveries = recoveriesOf(drafts.reply)
      if (nextRecoveries !== null) setRecoveries(nextRecoveries)
    } catch (failure) {
      setError(message(failure))
    }
  }

  async function refreshNote(path: string): Promise<void> {
    const [links, board] = await Promise.all([
      window.tandem.call({ method: 'note.backlinks', params: { path } }),
      window.tandem.call({ method: 'ticket.list', params: OPEN_BOARD }),
    ])
    if (!('error' in links)) {
      const rows = backlinksOf(links.reply)
      if (rows !== null) setBacklinks(rows)
    }
    if (!('error' in board)) {
      const tickets = ticketsOf(board.reply)
      if (tickets !== null) setLinked(tickets.filter((ticket) => ticket.link === path))
    }
  }

  // The vault, re-read on a reveal, a focus, and every store change (a ticket
  // may have learned to link to a note).
  useEffect(() => {
    void refreshVault()
    function onFocus(): void {
      void refreshVault()
    }
    window.addEventListener('focus', onFocus)
    const stop = window.tandem.onChanged(() => {
      void refreshVault()
      const path = openRef.current?.path
      if (path !== undefined) void refreshNote(path)
    })
    return () => {
      window.removeEventListener('focus', onFocus)
      stop()
    }
    // `hidden` is the read's one parameter; the open note is the ref's.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hidden])

  // A note's own reads: who links here, and the tickets that name it.
  useEffect(() => {
    if (open === null) {
      setBacklinks([])
      setLinked([])
      return
    }
    void refreshNote(open.path)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open?.path])

  async function save(buffer: NoteBuffer): Promise<void> {
    if (!buffer.dirty || savingRef.current) return
    savingRef.current = true
    setSaving(true)
    const outcome = await saveNote(window.tandem.call, buffer)
    savingRef.current = false
    setSaving(false)
    if (outcome.type === 'saved') {
      setOpen((current) =>
        current !== null && current.path === buffer.path && current.text === buffer.text
          ? { ...current, dirty: false, baseline: outcome.hash }
          : current,
      )
      // A write can move the links and the orphan set, so the panels re-read.
      void refreshVault()
      void refreshNote(buffer.path)
      return
    }
    if (outcome.type === 'conflict') {
      setConflict(outcome.theirs)
      return
    }
    setError(outcome.message)
  }

  // A tab switch unmounts the face. The pending timer dies with it, so the
  // buffer is handed to the core one last time; a conflict at that moment is
  // not written, and the dirty text was never going to win a race with itself.
  useEffect(() => {
    return () => {
      const buffer = openRef.current
      if (buffer !== null && buffer.dirty) void saveNote(window.tandem.call, buffer)
    }
  }, [])

  // The autosave: ~1 s after the last keystroke, withheld while a conflict has
  // the buffer frozen.
  useEffect(() => {
    if (open === null || !open.dirty || conflict !== null || saving) return
    const timer = setTimeout(() => {
      void save(open)
    }, 1000)
    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, conflict, saving])

  async function loadNote(path: string): Promise<void> {
    const answer = await window.tandem.call({ method: 'note.read', params: { path } })
    if ('error' in answer) {
      setError(answer.error.message)
      return
    }
    const note = noteContentOf(answer.reply)
    if (note === null) {
      setError('The note could not be read.')
      return
    }
    setOpen(openBuffer({ path: note.path, text: note.text, hash: note.hash }))
    setConflict(null)
    setExpanded((current) => new Set([...current, ...ancestorsOf(path)]))
    setToken((current) => current + 1)
    setError(null)
  }

  async function openNote(path: string): Promise<void> {
    // The bikeshed is frozen while a conflict is pending: the banner's two
    // buttons are the only way out, so a switch cannot silently drop the choice.
    if (conflict !== null) return
    if (open !== null && open.path === path) return
    if (open !== null && open.dirty) await save(open)
    await loadNote(path)
  }

  async function move(source: string, into: string): Promise<void> {
    if (conflict !== null) return
    // The move carries the bytes that are on disk: flush the buffer first, so a
    // dirty open note is not resurrected at its old path by a later save.
    if (open !== null && open.path === source && open.dirty) await save(open)
    const answer = await window.tandem.call({
      method: 'note.move',
      params: { path: source, into },
    })
    if ('error' in answer) {
      setError(answer.error.message)
      return
    }
    const moved =
      'moved' in answer.reply && typeof answer.reply.moved === 'object'
        ? answer.reply.moved
        : null
    await refreshVault()
    const current = openRef.current
    if (current === null || moved === null) return
    const carried = moved.movedNotes.find((note) => note.oldPath === current.path)
    if (carried !== undefined) {
      await loadNote(carried.newPath)
    } else {
      await refreshNote(current.path)
    }
  }

  function chooseMine(): void {
    if (open === null || conflict === null) return
    setOpen(keepMine(open, conflict))
    setConflict(null)
  }

  function chooseTheirs(): void {
    if (open === null || conflict === null) return
    setOpen(takeTheirs(open, conflict))
    setConflict(null)
    setToken((current) => current + 1)
    void refreshNote(open.path)
  }

  function onEditorMessage(next: { type: string; text?: string; target?: string }): void {
    if (next.type === 'dirty' && typeof next.text === 'string') {
      const text = next.text
      setOpen((current) => (current === null ? current : { ...current, text, dirty: true }))
      return
    }
    if (next.type === 'link-opened' && typeof next.target === 'string') {
      const target = next.target.trim()
      if (target.includes('://')) return
      const path = resolveWikilink(target, noteIndex)
      if (path !== null) void openNote(path)
    }
  }

  function reveal(next: boolean): void {
    window.localStorage.setItem(HIDDEN_KEY, next ? 'true' : 'false')
    setHidden(next)
  }

  return (
    <Face wide title="Bikeshed" blurb="The notes vault: tree, editor, backlinks and health.">
      {error === null ? null : (
        <div className="banner" role="alert">
          <span>{error}</span>
          <button type="button" className="banner__dismiss" onClick={() => setError(null)}>
            Dismiss
          </button>
        </div>
      )}

      {conflict === null || open === null ? null : (
        <div className="conflict" role="alert">
          <div className="conflict__header">
            <h2 className="conflict__title">This note changed on disk</h2>
            <p className="conflict__body">
              {open.path} was written by someone else while you were typing. Nothing has been
              written yet.
            </p>
          </div>
          <div className="conflict__bodies">
            <div className="conflict__side">
              <h3 className="conflict__side-title">Mine</h3>
              <pre className="conflict__text">{open.text}</pre>
            </div>
            <div className="conflict__side">
              <h3 className="conflict__side-title">Theirs</h3>
              <pre className="conflict__text">{conflict.text}</pre>
            </div>
          </div>
          <div className="conflict__choices">
            <button type="button" className="button" onClick={chooseMine}>
              Keep mine
            </button>
            <button type="button" className="button" onClick={chooseTheirs}>
              Take theirs
            </button>
          </div>
        </div>
      )}

      <div className="bikeshed">
        <BikeshedTree
          nodes={nodes}
          expanded={expanded}
          hidden={hidden}
          openPath={open?.path ?? null}
          health={health}
          onToggle={(path) =>
            setExpanded((current) => {
              const next = new Set(current)
              if (next.has(path)) next.delete(path)
              else next.add(path)
              return next
            })
          }
          onReveal={reveal}
          onOpen={(path) => void openNote(path)}
          onMove={(source, into) => void move(source, into)}
        />
        <div className="bikeshed__main">
          <BikeshedEditor
            path={open?.path ?? ''}
            text={open?.text ?? ''}
            token={token}
            noteIndex={noteIndex}
            saving={saving}
            onMessage={onEditorMessage}
          />
          <NoteRail
            backlinks={backlinks}
            linked={linked}
            recoveries={recoveries}
            onOpenNote={(path) => void openNote(path)}
          />
        </div>
      </div>
    </Face>
  )
}
