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
    output.heading('Doctor');
    for (final item in report.checks) {
      final message = '${item.component}: ${item.detail}';
      switch (item.status) {
        case doctorOk:
          output.success(message);
        case doctorWarning:
          output.warning(message);
        case doctorFailure:
          output.writeError(message);
      }
      if (item.fix != null) output.detail('Como resolver: ${item.fix}');
    }
    final summary = report.failures > 0
        ? 'Doctor encontrou ${report.failures} falha(s) e ${report.warnings} aviso(s).'
        : report.warnings > 0
        ? 'Doctor concluído com ${report.warnings} aviso(s).'
        : 'Doctor concluído: todos os componentes estão prontos.';
    if (report.failures > 0) {
      output.writeError(summary);
    } else if (report.warnings > 0) {
      output.warning(summary);
    } else {
      output.success(summary);
    }
    return unit;
  });
}
