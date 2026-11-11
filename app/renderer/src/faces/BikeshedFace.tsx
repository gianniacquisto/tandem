import { Fragment, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react'

import type { BikeshedHealth } from '../wire/BikeshedHealth'
import type { BacklinkView } from '../wire/BacklinkView'
import type { NoteNodeView } from '../wire/NoteNodeView'
import type { RecoveryView } from '../wire/RecoveryView'
import type { Reply } from '../wire/Reply'
import type { TicketSummary } from '../wire/TicketSummary'
import type { SnapshotProject } from '../shell/snapshot'
import {
  ancestorsOf,
  focusNodes,
  folderPaths,
  notePaths,
} from '../shell/bikeshed'
import {
  applyEditor,
  arrival,
  conflictStillApplies,
  keepMine,
  noteContentOf,
  openBuffer,
  saveNote,
  stashDraft,
  takeDraft,
  takeTheirs,
  type NoteBuffer,
  type SaveOutcome,
  type Theirs,
} from '../shell/note'
import {
  loadEditorFontSize,
  saveEditorFontSize,
  stepEditorFontSize,
} from '../shell/fontSize'
import { BikeshedBlocks } from './BikeshedBlocks'
import { BikeshedTree } from './BikeshedTree'
import { FocusControl } from './FocusControl'
import { NoteFooter } from './NoteFooter'
import { NoteSizeControl } from './NoteSizeControl'

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
 * (`note write`, `note move`, `note create`, `note create-folder`, `note daily`), and a save
 * compares the hash it read with the hash on disk before it writes: a
 * concurrent write is a conflict with both bodies, never a lost one.
 */
export type BikeshedFaceProps = {
  /** The note's ticket rows open the one sheet, over the bikeshed. */
  onOpenTicket?: (ticket: TicketSummary) => void
  /** The declared projects, in render order, for the focus picker. */
  projects: SnapshotProject[]
  /** The stored focus: the project the tree draws, or null for the vault. */
  activeProject: string | null
  /** Persist a new focus; null clears back to the whole vault. */
  onPickProject: (project: string | null) => void
}

export function BikeshedFace({
  onOpenTicket,
  projects,
  activeProject,
  onPickProject,
}: BikeshedFaceProps) {
  const [hidden, setHidden] = useState(() => window.localStorage.getItem(HIDDEN_KEY) === 'true')
  // One app-wide note size: ADR 0012's preference, applied once on the wrapper
  // the block surface sits in.
  const [size, setSize] = useState(() => loadEditorFontSize())
  const [nodes, setNodes] = useState<NoteNodeView[]>([])
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [open, setOpen] = useState<NoteBuffer | null>(null)
  const [conflict, setConflict] = useState<Theirs | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [backlinks, setBacklinks] = useState<BacklinkView[]>([])
  const [linked, setLinked] = useState<TicketSummary[]>([])
  const [health, setHealth] = useState<BikeshedHealth | null>(null)
  const [recoveries, setRecoveries] = useState<RecoveryView[]>([])
  const [token, setToken] = useState(0)
  // The show-all escape: the whole vault without clearing the stored focus.
  const [showAll, setShowAll] = useState(false)
  const seeded = useRef(false)
  const openRef = useRef<NoteBuffer | null>(null)
  openRef.current = open
  const conflictRef = useRef<Theirs | null>(null)
  conflictRef.current = conflict
  // One write at a time: a save that meets an autosave awaits that same promise
  // instead of starting a second one (ADR 0004's one save path).
  const inFlight = useRef<Promise<SaveOutcome> | null>(null)

  const noteIndex = useMemo(() => notePaths(nodes), [nodes])
  // The tree the focus draws: the focused project's folder alone, or the
  // whole vault when nothing is focused or no folder answers the focus.
  const drawn = focusNodes(nodes, showAll ? null : activeProject)

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
      void adoptExternal()
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

  /**
   * The one write path. A dirty buffer is handed to `saveNote`; a second
   * caller while a save is in flight awaits that same promise, so two callers
   * can never write twice.
   */
  function save(buffer: NoteBuffer): Promise<SaveOutcome> {
    if (!buffer.dirty) return Promise.resolve({ type: 'saved', hash: buffer.baseline })
    if (inFlight.current !== null) return inFlight.current
    const run = write(buffer)
    inFlight.current = run
    void run.finally(() => {
      if (inFlight.current === run) inFlight.current = null
    })
    return run
  }

  async function write(buffer: NoteBuffer): Promise<SaveOutcome> {
    setSaving(true)
    const outcome = await saveNote(window.tandem.call, buffer)
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
      return outcome
    }
    if (outcome.type === 'conflict') {
      // A save started before Take theirs lands after the choice with the same
      // disk body: the open buffer already adopted it, so the stale outcome
      // must not re-raise the banner the choice just cleared.
      if (!conflictStillApplies(openRef.current, buffer)) return outcome
      setConflict(outcome.theirs)
      return outcome
    }
    setError(outcome.message)
    return outcome
  }

  // A tab switch unmounts the face and the pending timer dies with it. The
  // buffer is handed to the core one last time; a save that does not land (a
  // conflict or an error) keeps the buffer as a draft, and the next visit
  // resumes it. That is what makes the unmount safe, not the flush alone.
  useEffect(() => {
    return () => {
      const buffer = openRef.current
      if (buffer === null || !buffer.dirty) return
      void saveNote(window.tandem.call, buffer).then((outcome) => {
        if (outcome.type !== 'saved') stashDraft(buffer)
      })
    }
  }, [])

  // The draft a previous visit could not save, resumed before the first read.
  useEffect(() => {
    const draft = takeDraft()
    if (draft === null) return
    setOpen(draft)
    setToken((current) => current + 1)
    setExpanded((current) => new Set([...current, ...ancestorsOf(draft.path)]))
    setNotice('A draft from your last visit was kept. Saving again.')
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

  /**
   * An external write arriving while the face is open. The store's change
   * event cannot carry it (a note is not a row), so a focus re-read is the
   * watcher: a dirty buffer becomes the conflict banner, a clean one adopts the
   * on-disk body (ADR 0004).
   */
  async function adoptExternal(): Promise<void> {
    const before = openRef.current
    if (before === null) return
    const answer = await window.tandem.call({ method: 'note.read', params: { path: before.path } })
    if ('error' in answer) return
    const disk = noteContentOf(answer.reply)
    if (disk === null) return
    // The read crossed Take theirs (or a note switch): the fresh buffer already
    // names the disk, so the stale capture must not re-raise the cleared banner.
    const current = openRef.current
    if (current === null || current.path !== before.path) return
    const next = arrival(current, { text: disk.text, hash: disk.hash })
    if (next.type === 'conflict') {
      setConflict(next.theirs)
    } else if (next.type === 'adopted') {
      setOpen(next.buffer)
      setToken((value) => value + 1)
      setConflict(null)
      void refreshVault()
      void refreshNote(current.path)
    }
  }

  async function fileTicket(text: string): Promise<void> {
    const current = openRef.current
    if (current === null) {
      setError('Open a note first: a ticket needs the note it came from.')
      return
    }
    const answer = await window.tandem.call({
      method: 'ticket.addFromNote',
      params: {
        note: current.path,
        text,
        status: null,
        assignee: null,
        lane: null,
        origin: null,
      },
    })
    if ('error' in answer) {
      setError(answer.error.message)
      return
    }
    setNotice('Filed a ticket from this note.')
    void refreshNote(current.path)
  }

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

  async function openDaily(): Promise<void> {
    if (conflict !== null) return
    if (open !== null && open.dirty) await save(open)
    const answer = await window.tandem.call({ method: 'note.daily', params: {} })
    if ('error' in answer) {
      setError(answer.error.message)
      return
    }
    const path = 'path' in answer.reply ? answer.reply.path : null
    if (typeof path !== 'string' || path === '') {
      setError("Today's note could not be opened.")
      return
    }
    await refreshVault()
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

  function createdPath(reply: unknown, fallback: string): string {
    if (typeof reply === 'object' && reply !== null && 'path' in reply) {
      const path = (reply as { path: unknown }).path
      if (typeof path === 'string') return path
    }
    return fallback
  }

  async function createNote(path: string): Promise<void> {
    if (conflict !== null) return
    const answer = await window.tandem.call({ method: 'note.create', params: { path } })
    if ('error' in answer) {
      setError(answer.error.message)
      return
    }
    const created = createdPath(answer.reply, path)
    await refreshVault()
    setExpanded((current) => new Set([...current, ...ancestorsOf(created)]))
    await loadNote(created)
  }

  async function createFolder(path: string): Promise<void> {
    if (conflict !== null) return
    const answer = await window.tandem.call({ method: 'note.createFolder', params: { path } })
    if ('error' in answer) {
      setError(answer.error.message)
      return
    }
    const created = createdPath(answer.reply, path)
    await refreshVault()
    setExpanded((current) => new Set([...current, ...ancestorsOf(created), created]))
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

  function reveal(next: boolean): void {
    window.localStorage.setItem(HIDDEN_KEY, next ? 'true' : 'false')
    setHidden(next)
  }

  /** A step is view state: it writes the preference, never the note. */
  function stepSize(delta: number): void {
    const next = stepEditorFontSize(size, delta)
    setSize(next)
    saveEditorFontSize(next)
  }

  const folder =
    open === null || !open.path.includes('/')
      ? ''
      : open.path.slice(0, open.path.lastIndexOf('/'))
  const title =
    open === null ? '' : (open.path.split('/').pop() ?? '').replace(/\.md$/i, '')
  const crumbs = (folder === '' ? ['Bikeshed'] : folder.split('/')).concat(
    title === '' ? [] : [title],
  )

  return (
    <section className="face face--wide bikeshed-face" aria-label="Bikeshed">
      {error === null ? null : (
        <div className="banner" role="alert">
          <span>{error}</span>
          <button type="button" className="banner__dismiss" onClick={() => setError(null)}>
            Dismiss
          </button>
        </div>
      )}

      {notice === null ? null : (
        <div className="banner banner--notice" role="status">
          <span>{notice}</span>
          <button type="button" className="banner__dismiss" onClick={() => setNotice(null)}>
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
        <aside className="bikeshed__tree" aria-label="Bikeshed notes">
          <FocusControl
            projects={projects}
            active={activeProject}
            showAll={showAll}
            onPick={onPickProject}
            onToggleShowAll={() => setShowAll((current) => !current)}
          />
          <BikeshedTree
            nodes={drawn}
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
            onCreateNote={(path) => void createNote(path)}
            onCreateFolder={(path) => void createFolder(path)}
            onDaily={() => void openDaily()}
          />
        </aside>

        <div className="bikeshed__document">
          {open === null ? null : (
            <header className="bikeshed__breadcrumb">
              <div className="bikeshed__breadcrumb-inner">
                <nav className="bikeshed__crumbs" aria-label="Breadcrumb">
                  {crumbs.map((crumb, index) => (
                    <Fragment key={`${index}-${crumb}`}>
                      {index === 0 ? null : (
                        <span className="bikeshed__crumb-sep" aria-hidden="true">
                          /
                        </span>
                      )}
                      <span
                        className={
                          index === crumbs.length - 1
                            ? 'bikeshed__crumb bikeshed__crumb--current'
                            : 'bikeshed__crumb'
                        }
                        aria-current={index === crumbs.length - 1 ? 'page' : undefined}
                      >
                        {crumb}
                      </span>
                    </Fragment>
                  ))}
                </nav>
                <div className="bikeshed__breadcrumb-end">
                  {saving ? <span className="editor__saving">saving{'\u2026'}</span> : null}
                  <NoteSizeControl size={size} onStep={stepSize} />
                </div>
              </div>
            </header>
          )}

          <div
            className="bikeshed__page"
            style={{ '--tandem-editor-font-size': `${size}px` } as CSSProperties}
          >
            {open === null ? (
              <div className="editor__empty">
                <p className="editor__empty-title">No note selected</p>
                <p className="editor__empty-body">Pick a note in the tree.</p>
              </div>
            ) : (
              <BikeshedBlocks
                document={open.document}
                focusToken={token}
                notePath={open.path}
                onChange={(document) => setOpen((current) => applyEditor(current, document))}
                onSendToTodos={(text) => {
                  if (text !== '') void fileTicket(text)
                }}
                noteIndex={noteIndex}
                onOpenNote={(path) => void openNote(path)}
              />
            )}
            <NoteFooter
              backlinks={backlinks}
              linked={linked}
              recoveries={recoveries}
              onOpenNote={(path) => void openNote(path)}
              onOpenTicket={onOpenTicket}
            />
          </div>
        </div>
      </div>
    </section>
  )
}
