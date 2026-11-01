import { useEffect, useState } from 'react'

import { checkChanges, type CheckChange } from '../shell/setup'
import { addRosterName, removeRosterName } from '../shell/roster'
import type { DoctorReply } from '../wire/DoctorReply'
import type { DoctorReport } from '../wire/DoctorReport'
import type { PathAction } from '../wire/PathAction'
import type { Reply } from '../wire/Reply'
import type { SettingsReply } from '../wire/SettingsReply'
import type { SetupReply } from '../wire/SetupReply'
import { Face } from './Face'

/** The machine's setup as the three reads answered it. */
type Settings = {
  setup: SetupReply
  doctor: DoctorReport
  roster: string[]
}

/** A repair's receipt: the files `setup install` wrote and the rows it flipped. */
type Repair = {
  paths: PathAction[]
  changes: CheckChange[]
}

function isSetupReply(reply: Reply): reply is SetupReply {
  return 'report' in reply && 'hosts' in reply && 'bikeshed' in reply
}

function isDoctorReply(reply: Reply): reply is DoctorReply {
  return 'report' in reply && 'checks' in reply.report
}

function isSettingsReply(reply: Reply): reply is SettingsReply {
  return 'roster' in reply
}

/**
 * The one Settings surface (TAN-216, `docs/port.md`): the machine's paths, the
 * app's one theme rule, the store's roster, and the agent face's setup.
 *
 * The setup section is `tandem-cli setup status` and `tandem-cli doctor` over
 * the stream, and its one button is `tandem-cli setup install`: the repair is
 * that verb run again, never a writer of the app's own (ADR 0034). The roster
 * section is `settings.get` read and `settings.set` written, the CLI's
 * `settings set` under a Save: the draft edits in place, the write replaces
 * the stored list whole, and the list redraws from the write's receipt, never
 * from the draft it sent. The face computes no path and no state; it draws the
 * reports' own.
 */
export function SettingsFace() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [draft, setDraft] = useState<string[]>([])
  const [candidate, setCandidate] = useState('')
  const [failure, setFailure] = useState<string | null>(null)
  const [repair, setRepair] = useState<Repair | null>(null)

  useEffect(() => {
    let live = true

    async function read(): Promise<void> {
      const [status, doctor, roster] = await Promise.all([
        window.tandem.call({ method: 'setup.status', params: {} }),
        window.tandem.call({ method: 'doctor.report', params: {} }),
        window.tandem.call({ method: 'settings.get', params: {} }),
      ])
      if (!live) return
      if ('error' in status) {
        setFailure(status.error.message)
        return
      }
      if ('error' in doctor) {
        setFailure(doctor.error.message)
        return
      }
      if ('error' in roster) {
        setFailure(roster.error.message)
        return
      }
      if (!isSetupReply(status.reply) || !isDoctorReply(doctor.reply)) {
        setFailure('The setup report was not readable.')
        return
      }
      if (!isSettingsReply(roster.reply)) {
        setFailure('The roster was not readable.')
        return
      }
      setSettings({ setup: status.reply, doctor: doctor.reply.report, roster: roster.reply.roster })
      setDraft(roster.reply.roster)
    }

    void read()
    return () => {
      live = false
    }
  }, [])

  // The one write: `setup install` at its CLI defaults (OpenCode, no force), so
  // a checkout link stays and a real file is never touched. The doctor read
  // after it is the diff the result shows.
  async function repairFace(): Promise<void> {
    const before = settings?.doctor ?? null
    const written = await window.tandem.call({
      method: 'setup.install',
      params: { host: null, force: false },
    })
    if ('error' in written) {
      setFailure(written.error.message)
      return
    }
    if (!isSetupReply(written.reply)) {
      setFailure('The install report was not readable.')
      return
    }

    const setup = written.reply
    const doctor = await window.tandem.call({ method: 'doctor.report', params: {} })
    if ('error' in doctor) {
      setFailure(doctor.error.message)
      return
    }
    if (!isDoctorReply(doctor.reply)) {
      setFailure('The doctor report was not readable.')
      return
    }
    const diagnosis = doctor.reply.report

    setSettings((previous) =>
      previous === null ? previous : { setup, doctor: diagnosis, roster: previous.roster },
    )
    setRepair({
      paths: setup.report.paths,
      changes: checkChanges(before, diagnosis),
    })
    setFailure(null)
  }

  // The roster write: `settings.set` with the draft's whole list, the CLI's
  // `settings set` over the stream. The list redraws from the write's
  // receipt, never from the draft it sent, so the face draws what the store
  // holds; a refusal lands in the error row and the draft stays for another
  // try. One explicit Save, so a half-typed list never reaches the store.
  async function saveRoster(): Promise<void> {
    const written = await window.tandem.call({ method: 'settings.set', params: { roster: draft } })
    if ('error' in written) {
      setFailure(written.error.message)
      return
    }
    if (!isSettingsReply(written.reply)) {
      setFailure('The roster was not writable.')
      return
    }
    const receipt = written.reply.roster
    setSettings((previous) => (previous === null ? previous : { ...previous, roster: receipt }))
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

  const state = settings?.setup.report.state
  const trouble = state?.warnings.some((warning) => warning.severity !== 'info') ?? false

  return (
    <Face title="Settings" blurb="The machine, the agent host and the app's own preferences.">
      <section className="settings__section">
        <h2 className="settings__heading">Paths</h2>
        <dl className="settings__facts">
          <div className="settings__fact">
            <dt className="settings__term">Home</dt>
            <dd className="settings__value">{settings?.setup.home ?? 'Reading…'}</dd>
          </div>
          <div className="settings__fact">
            <dt className="settings__term">Bikeshed</dt>
            <dd className="settings__value">{settings?.setup.bikeshed ?? 'Reading…'}</dd>
          </div>
          {settings?.setup.hosts.map((host) => (
            <div className="settings__fact" key={host.host}>
              <dt className="settings__term">{host.host}</dt>
              <dd className="settings__value">{host.path}</dd>
            </div>
          ))}
          <div className="settings__fact">
            <dt className="settings__term">Theme</dt>
            <dd className="settings__value">System: the app follows the OS appearance</dd>
          </div>
        </dl>
      </section>

      <section className="settings__section">
        <h2 className="settings__heading">Roster</h2>
        <p className="settings__versions">
          {settings === null
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
      </section>

      <section className="settings__section">
        <h2 className="settings__heading">Setup</h2>
        <p className="settings__versions">
          {state === undefined
            ? 'Reading the agent face…'
            : `Installed ${state.installedVersion ?? 'nothing'}; bundled ${state.bundledVersion ?? 'nothing'}`}
        </p>
        <ul className="settings__checks">
          {settings?.doctor.checks.map((check) => (
            <li className="settings__check" key={check.name}>
              <span className="settings__check-name">{check.name}</span>
              <span className="settings__state" data-state={check.ok ? 'ok' : 'trouble'}>
                {check.ok ? 'ok' : 'needs a fix'}
              </span>
              <span className="settings__detail">{check.detail}</span>
            </li>
          ))}
        </ul>
        <button type="button" className="settings__action" onClick={() => void repairFace()}>
          {trouble ? 'Repair' : 'Install'}
        </button>
        <p className="settings__hint">
          Runs `tandem-cli setup install`: the bundled skills, the four agent prompts and
          AGENTS.md go into Tandem's home, and the host links point at that copy. Nothing is
          written to a host until you press it.
        </p>
        {repair === null ? null : (
          <div className="settings__result" aria-live="polite">
            <p className="settings__result-title">
              {repair.paths.length === 0
                ? 'Nothing was written'
                : `${repair.paths.length} path(s) written`}
            </p>
            <ul className="settings__paths">
              {repair.paths.map((path) => (
                <li className="settings__path" key={path.path}>
                  <span className="settings__path-action">{path.action}</span>
                  <span className="settings__value">{path.path}</span>
                </li>
              ))}
            </ul>
            <ul className="settings__checks">
              {repair.changes.length === 0 ? (
                <li className="settings__detail">No check changed.</li>
              ) : (
                repair.changes.map((change) => (
                  <li className="settings__check" key={change.name}>
                    <span className="settings__check-name">{change.name}</span>
                    <span
                      className="settings__state"
                      data-state={change.after ? 'ok' : 'trouble'}
                    >
                      {change.before ? 'was ok' : 'was broken'} → {change.after ? 'ok' : 'broken'}
                    </span>
                    <span className="settings__detail">{change.detail}</span>
                  </li>
                ))
              )}
            </ul>
          </div>
        )}
        {failure === null ? null : (
          <p className="settings__error" role="alert">
            {failure}
          </p>
        )}
      </section>
    </Face>
  )
}
