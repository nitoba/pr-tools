import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../application/terminal/terminal_ports.dart';
import 'doctor_models.dart';

abstract interface class DoctorPresenter {
  AppEffect<Unit> show(DoctorReport report);
}

final class DoctorPresenterLive implements DoctorPresenter {
  const DoctorPresenterLive();

  @override
  AppEffect<Unit> show(DoctorReport report) => Effect.result((use) {
    final output = use<TerminalOutput>();
    output.write('┌ prt · doctor\n│');
    for (final item in report.checks) {
      final prefix = switch (item.status) {
        doctorOk => '✓ OK',
        doctorWarning => '⚠ AVISO',
        _ => '✗ FALHA',
      };
      final message = '$prefix · ${item.component}: ${item.detail}';
      if (item.status == doctorFailure) {
        output.writeError(message);
      } else {
        output.write(message);
      }
      if (item.fix != null) output.write('•   Como resolver: ${item.fix}');
    }
    final summary = report.failures > 0
        ? 'Doctor encontrou ${report.failures} falha(s) e ${report.warnings} aviso(s).'
        : report.warnings > 0
        ? 'Doctor concluído com ${report.warnings} aviso(s).'
        : 'Doctor concluído: todos os componentes estão prontos.';
    output.write('└ $summary');
    return unit;
  });
}
