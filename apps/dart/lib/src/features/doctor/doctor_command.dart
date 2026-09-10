import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../app/cli_options.dart';
import 'doctor_presenter.dart';
import 'doctor_service.dart';

abstract interface class DoctorCommand {
  AppEffect<int> execute(CliOptions options);
}

final class DoctorCommandLive implements DoctorCommand {
  const DoctorCommandLive();

  @override
  AppEffect<int> execute(CliOptions options) => Effect.result((use) async {
    final report = await use.unwrap(use<DoctorService>().inspect(options));
    await use.unwrap(use<DoctorPresenter>().show(report));
    return report.failures == 0 ? 0 : 1;
  });
}
