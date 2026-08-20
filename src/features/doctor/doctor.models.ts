export type DoctorStatus = 'ok' | 'warn' | 'fail'

export type DoctorCheck = {
  component: string
  status: DoctorStatus
  detail: string
  fix?: string
}

export type DoctorReport = {
  checks: DoctorCheck[]
  failures: number
  warnings: number
}
