import type { DoctorCheck } from '../wire/DoctorCheck'
import type { DoctorReport } from '../wire/DoctorReport'

/** One doctor row a repair flipped, with the words the report gave it. */
export type CheckChange = {
  name: string
  before: boolean
  after: boolean
  detail: string
}

/**
 * What a repair changed: the doctor rows whose state differs from the read
 * before it, in the report's own order.
 *
 * Settings shows this instead of a verdict, so a repair that did nothing reads
 * as nothing (`[]`) and not as success. A first read that failed (`before` is
 * null) reports every row, because every row is then new information.
 */
export function checkChanges(before: DoctorReport | null, after: DoctorReport): CheckChange[] {
  if (before === null) {
    return after.checks.map((check: DoctorCheck) => ({
      name: check.name,
      before: false,
      after: check.ok,
      detail: check.detail,
    }))
  }

  const previous = new Map(before.checks.map((check) => [check.name, check.ok]))
  return after.checks
    .filter((check) => previous.get(check.name) !== check.ok)
    .map((check) => ({
      name: check.name,
      before: previous.get(check.name) ?? false,
      after: check.ok,
      detail: check.detail,
    }))
}
