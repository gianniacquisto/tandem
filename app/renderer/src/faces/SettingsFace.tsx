import { useEffect, useState } from 'react'

import { addRosterName, removeRosterName } from '../shell/roster'
import type { Reply } from '../wire/Reply'
import type { SettingsReply } from '../wire/SettingsReply'
import { Face } from './Face'

function isSettingsReply(reply: Reply): reply is SettingsReply {
  return 'roster' in reply
}

/**
 * The one Settings surface (`docs/port.md`): the app's one theme rule and the
 * store's roster.
 *
 * The roster section is `settings.get` read and `settings.set` written, the
 * CLI's `settings set` under a Save: the draft edits in place, the write
 * replaces the stored list whole, and the list redraws from the write's
 * receipt, never from the draft it sent. The setup half of this face retired
 * with the agent face (ADR 0045): agent runs live outside the app, behind the
 * MCP server and the CLI, and the machine's paths are the CLI's own read. The
 * face computes no path and no state; it draws the report's own.
 */
export function SettingsFace() {
  const [roster, setRoster] = useState<string[] | null>(null)
  const [draft, setDraft] = useState<string[]>([])
  const [candidate, setCandidate] = useState('')
  const [failure, setFailure] = useState<string | null>(null)

  useEffect(() => {
    let live = true

    async function read(): Promise<void> {
      const answer = await window.tandem.call({ method: 'settings.get', params: {} })
      if (!live) return
      if ('error' in answer) {
        setFailure(answer.error.message)
        return
      }
      if (!isSettingsReply(answer.reply)) {
        setFailure('The roster was not readable.')
        return
      }
      setRoster(answer.reply.roster)
      setDraft(answer.reply.roster)
    }

    void read()
    return () => {
      live = false
    }
  }, [])

  // The roster write: `settings.set` with the draft's whole list, the CLI's
  // `settings set` over the stream. The list redraws from the write's
  // receipt, never from the draft it sent, so the face draws what the store
  // holds; a refusal lands in the error row and the draft stays for another
  // try. One explicit Save, so a half-typed list never reaches the store.
  async function saveRoster(): Promise<void> {
    const written = await window.tandem.call({
      method: 'settings.set',
      params: { roster: draft, project: null },
    })
    if ('error' in written) {
      setFailure(written.error.message)
      return
    }
    if (!isSettingsReply(written.reply)) {
      setFailure('The roster was not writable.')
      return
    }
    const receipt = written.reply.roster
    setRoster(receipt)
    setDraft(receipt)
    setFailure(null)
  }

  function addCandidate(): void {
    const grown = addRosterName(draft, candidate)
    if (grown.length !== draft.length) {
      setDraft(grown)
      setCandidate('')
    }
  }

  return (
    <Face wide label="Settings">
      <div className="settings">
        <section className="settings__section">
          <h2 className="settings__heading">Preferences</h2>
          <dl className="settings__facts">
            <div className="settings__fact">
              <dt className="settings__term">Theme</dt>
              <dd className="settings__value">System: the app follows the OS appearance</dd>
            </div>
          </dl>
        </section>

        <section className="settings__section">
          <h2 className="settings__heading">Roster</h2>
          <p className="settings__versions">
            {roster === null
              ? 'Reading the roster…'
              : 'The names the ticket sheet picker and the board filter offer.'}
          </p>
          <ul className="settings__checks">
            {draft.map((name, index) => (
              <li className="settings__check" key={`${name}-${index}`}>
                <span className="settings__check-name">{name}</span>
                <button
                  type="button"
                  className="settings__action"
                  aria-label={`Remove ${name}`}
                  onClick={() => setDraft(removeRosterName(draft, index))}
                >
                  Remove
                </button>
              </li>
            ))}
          </ul>
          <form
            className="settings__add"
            onSubmit={(event) => {
              event.preventDefault()
              addCandidate()
            }}
          >
            <input
              className="settings__name"
              type="text"
              value={candidate}
              placeholder="A name to offer"
              aria-label="A name to add to the roster"
              onChange={(event) => setCandidate(event.target.value)}
            />
            <button type="submit" className="settings__action">
              Add
            </button>
          </form>
          <button type="button" className="settings__action" onClick={() => void saveRoster()}>
            Save roster
          </button>
          <p className="settings__hint">
            Runs `tandem-cli settings set` with the whole list: the stored roster is replaced, and
            the list above redraws from what the store kept. Nothing is written until you press it.
          </p>
          {failure === null ? null : (
            <p className="settings__error" role="alert">
              {failure}
            </p>
          )}
        </section>
      </div>
    </Face>
  )
}
