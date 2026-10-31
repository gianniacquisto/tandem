import { useEffect, useState } from 'react'

import { checkChanges, type CheckChange } from '../shell/setup'
import type { DoctorReply } from '../wire/DoctorReply'
import type { DoctorReport } from '../wire/DoctorReport'
import type { PathAction } from '../wire/PathAction'
import type { Reply } from '../wire/Reply'
import type { SetupReply } from '../wire/SetupReply'
import { Face } from './Face'

/** The machine's setup as the two reads answered it. */
type Settings = {
  setup: SetupReply
  doctor: DoctorReport
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

/**
 * The one Settings surface (TAN-216, `docs/port.md`): the machine's paths, the
 * app's one theme rule, and the agent face's setup.
 *
 * The setup section is `tandem-cli setup status` and `tandem-cli doctor` over
 * the stream, and its one button is `tandem-cli setup install`: the repair is
 * that verb run again, never a writer of the app's own (ADR 0034). The face
 * computes no path and no state; it draws the report's own.
 */
export function SettingsFace() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [failure, setFailure] = useState<string | null>(null)
  const [repair, setRepair] = useState<Repair | null>(null)

  useEffect(() => {
    let live = true

    async function read(): Promise<void> {
      const [status, doctor] = await Promise.all([
        window.tandem.call({ method: 'setup.status', params: {} }),
        window.tandem.call({ method: 'doctor.report', params: {} }),
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
      if (!isSetupReply(status.reply) || !isDoctorReply(doctor.reply)) {
        setFailure('The setup report was not readable.')
        return
      }
      setSettings({ setup: status.reply, doctor: doctor.reply.report })
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

    const doctor = await window.tandem.call({ method: 'doctor.report', params: {} })
    if ('error' in doctor) {
      setFailure(doctor.error.message)
      return
    }
    if (!isDoctorReply(doctor.reply)) {
      setFailure('The doctor report was not readable.')
      return
    }

    setSettings({ setup: written.reply, doctor: doctor.reply.report })
    setRepair({
      paths: written.reply.report.paths,
      changes: checkChanges(before, doctor.reply.report),
    })
    setFailure(null)
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
