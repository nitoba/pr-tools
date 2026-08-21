import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/app/app_failure.dart';
import 'package:pr_tools/src/app/cli_options.dart';
import 'package:pr_tools/src/features/doctor/doctor_command.dart';
import 'package:pr_tools/src/features/doctor/doctor_models.dart';
import 'package:pr_tools/src/features/doctor/doctor_presenter.dart';
import 'package:pr_tools/src/features/doctor/doctor_service.dart';
import 'package:test/test.dart';

void main() {
  test('resolves doctor command dependencies from module bindings', () async {
    final report = DoctorReport([
      const DoctorCheck(
        component: 'Git',
        status: doctorOk,
        detail: 'Executável disponível.',
      ),
    ]);
    final presenter = _Presenter();
    final result =
        await Module([
          .instance<DoctorService>(_Service(report)),
          .instance<DoctorPresenter>(presenter),
          .provide<DoctorCommand>(DoctorCommandLive.new),
        ]).run(
          Effect<int, AppFailure>.result(
            (use) => use.unwrap(use<DoctorCommand>().execute(_options())),
          ),
        );

    expect(result.getOrNull(), 0);
    expect(presenter.report, same(report));
  });

  test('returns a failing exit code when the report has failures', () async {
    final report = DoctorReport([
      const DoctorCheck(
        component: 'Azure DevOps PAT',
        status: doctorFailure,
        detail: 'PAT não configurado.',
      ),
    ]);
    final result =
        await Module([
          .instance<DoctorService>(_Service(report)),
          .instance<DoctorPresenter>(_Presenter()),
          .provide<DoctorCommand>(DoctorCommandLive.new),
        ]).run(
          Effect<int, AppFailure>.result(
            (use) => use.unwrap(use<DoctorCommand>().execute(_options())),
          ),
        );

    expect(result.getOrNull(), 1);
  });
}

CliOptions _options() => const CliOptions(
  command: Command.doctor,
  targets: [],
  create: false,
  noCreate: false,
  dryRun: false,
  raw: false,
  copy: false,
);

final class _Service implements DoctorService {
  const _Service(this.report);

  final DoctorReport report;

  @override
  AppEffect<DoctorReport> inspect(CliOptions options) => Effect.succeed(report);
}

final class _Presenter implements DoctorPresenter {
  DoctorReport? report;

  @override
  AppEffect<Unit> show(DoctorReport value) {
    report = value;
    return Effect.succeed(unit);
  }
}
