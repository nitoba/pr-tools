import '../../domain/change_context.dart';

typedef DoctorStatus = String;

const doctorOk = 'ok';
const doctorWarning = 'warn';
const doctorFailure = 'fail';

final class DoctorCheck {
  const DoctorCheck({
    required this.component,
    required this.status,
    required this.detail,
    this.fix,
  });

  final String component;
  final DoctorStatus status;
  final String detail;
  final String? fix;
}

final class DoctorReport {
  DoctorReport(this.checks)
    : failures = _count(checks, doctorFailure),
      warnings = _count(checks, doctorWarning);

  final List<DoctorCheck> checks;
  final int failures;
  final int warnings;
}

final class DoctorProbeResult {
  const DoctorProbeResult({required this.ok, this.status, this.error});

  final bool ok;
  final int? status;
  final String? error;
}

final class DoctorAzureReport {
  const DoctorAzureReport({required this.repository, required this.workItems});

  final DoctorProbeResult repository;
  final DoctorProbeResult workItems;
}

DoctorCheck check(
  DoctorStatus status,
  String component,
  String detail, [
  String? fix,
]) =>
    DoctorCheck(component: component, status: status, detail: detail, fix: fix);

int _count(List<DoctorCheck> checks, DoctorStatus status) =>
    checks.where((value) => value.status == status).length;

String remoteLabel(RepositoryRemote remote) =>
    '${remote.organization}/${remote.project}/${remote.repository}';
