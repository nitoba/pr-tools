import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../app/cli_options.dart';
import '../../application/config/config_service.dart';
import '../../application/terminal/terminal_ports.dart';
import 'test_card_models.dart';
import 'test_card_presenter.dart';
import 'test_card_service.dart';
import 'test_card_validation.dart';

abstract interface class TestCardCommand {
  AppEffect<int> execute(CliOptions options);
}

final class TestCardCommandLive implements TestCardCommand {
  const TestCardCommandLive();

  @override
  AppEffect<int> execute(CliOptions options) => Effect.result((use) async {
    final service = use<TestCardService>();
    final presenter = use<TestCardPresenter>();
    final interactive = use<ConfigRuntime>().interactive;
    final preparation = await use.unwrap(service.prepare(options, interactive));
    await use.unwrap(presenter.intro(preparation.context.change.branch));

    if (options.dryRun) {
      await use.unwrap(
        presenter.showDryRun(preparation.config, preparation.prompt),
      );
      await use.unwrap(presenter.outro('Concluído.'));
      return 0;
    }

    final progress = use<ProgressReporter>();
    progress.start('Gerando card via IA');
    final generated = await use.unwrap(
      service
          .generate(
            preparation,
            report: (provider, model) {
              progress.message('Tentando $provider ($model)');
            },
          )
          .tapError((_) => progress.error('Falha ao gerar card')),
    );
    progress.stop('Card gerado (${generated.provider}/${generated.model})');
    if (options.raw) {
      await use.unwrap(presenter.raw(generated.description.body));
      await use.unwrap(presenter.outro('Concluído.'));
      return 0;
    }
    await use.unwrap(
      presenter.showSummary(
        preparation,
        generated.provider,
        generated.model,
        generated.description.title,
        generated.description.body,
        options,
      ),
    );
    if (options.noCreate) {
      await use.unwrap(presenter.outro('Concluído.'));
      return 0;
    }

    final shouldCreate =
        interactive &&
        await _confirm(
          use,
          'Criar este Test Case no Azure DevOps?',
          initialValue: options.create,
        );
    if (!shouldCreate) {
      if (!interactive) {
        await use.unwrap(
          presenter.info(
            'Ambiente não interativo; criação requer confirmação no terminal.',
          ),
        );
      }
      await use.unwrap(presenter.outro('Concluído.'));
      return 0;
    }

    final settings = await _settings(use, preparation, options, interactive);
    final created = await use.unwrap(
      service.create(
        preparation,
        buildCreateTestCaseInput(
          settings,
          preparation.context.workItem.id,
          generated.description.title,
          generated.description.body,
        ),
      ),
    );
    await use.unwrap(
      presenter.success(
        'Test Case #${created.id} criado: ${workItemUrl(preparation.context.change, created.id)}',
      ),
    );
    if (interactive &&
        await _confirm(
          use,
          'Atualizar o Work Item pai para Test QA?',
          initialValue: false,
        )) {
      final effort = workItemNumber(
        preparation.context.workItem,
        'Microsoft.VSTS.Scheduling.Effort',
      );
      final realEffort = workItemNumber(
        preparation.context.workItem,
        'Custom.RealEffort',
      );
      final nextEffort = effort == null
          ? await _promptNumber(
              use,
              'Effort (horas decimais)',
              0.5,
              validateNonNegativeDecimal,
            )
          : null;
      final nextRealEffort = realEffort == null
          ? await _promptNumber(
              use,
              'Real Effort (horas decimais)',
              nextEffort ?? effort ?? 0.5,
              validateNonNegativeDecimal,
            )
          : null;
      await use.unwrap(
        service.updateParent(
          preparation,
          effort: nextEffort,
          realEffort: nextRealEffort,
        ),
      );
      await use.unwrap(
        presenter.success(
          'Work Item #${preparation.context.workItem.id} atualizado para Test QA.',
        ),
      );
    }
    await use.unwrap(presenter.outro('Concluído.'));
    return 0;
  });
}

Future<TestCardSettings> _settings(
  EffectContext<AppFailure> use,
  TestCardPreparation preparation,
  CliOptions options,
  bool interactive,
) async {
  var areaPath = options.areaPath ?? preparation.config.testAreaPath;
  var assignedTo = options.assignedTo ?? preparation.config.testAssignedTo;
  var iterationPath =
      options.iterationPath ??
      workItemText(preparation.context.workItem, 'System.IterationPath');
  var priority = await use.result(
    parsePositiveDecimal(options.priority, 2, '--priority'),
  );
  var team = options.team ?? preparation.config.testTeam;
  var program = options.program ?? preparation.config.testProgram;
  if (interactive) {
    areaPath = await _promptOptional(use, 'AreaPath do Test Case', areaPath);
    assignedTo = await _promptOptional(
      use,
      'Responsável do Test Case',
      assignedTo,
    );
    iterationPath = await _promptOptional(
      use,
      'IterationPath do Test Case',
      iterationPath,
    );
    priority = await _promptNumber(
      use,
      'Prioridade do Test Case',
      priority,
      validatePositiveDecimal,
    );
    team = await _promptRequired(use, 'Custom.Team', team);
    program = await _promptRequired(use, 'Custom.ProgramasAgrotrace', program);
  }
  team = await use.result(parseRequiredText(team, 'Custom.Team'));
  program = await use.result(
    parseRequiredText(program, 'Custom.ProgramasAgrotrace'),
  );
  return TestCardSettings(
    areaPath: areaPath,
    assignedTo: assignedTo,
    iterationPath: iterationPath,
    priority: priority,
    team: team,
    program: program,
  );
}

Future<String> _promptOptional(
  EffectContext<AppFailure> use,
  String message,
  String initialValue,
) async {
  final value = use<PromptPort>().text(
    message: '$message (opcional)',
    initialValue: initialValue,
  );
  if (value == null) use.fail(const TestCardFailure('Operação cancelada.'));
  return resolveOptionalText(value, initialValue);
}

Future<String> _promptRequired(
  EffectContext<AppFailure> use,
  String message,
  String initialValue,
) async {
  final value = use<PromptPort>().text(
    message: message,
    initialValue: initialValue,
    validate: (value) => value.trim().isEmpty && initialValue.trim().isEmpty
        ? validateRequiredText(value)
        : null,
  );
  if (value == null) use.fail(const TestCardFailure('Operação cancelada.'));
  return use.result(resolveRequiredText(value, initialValue, message));
}

Future<num> _promptNumber(
  EffectContext<AppFailure> use,
  String message,
  num initialValue,
  PromptValidator validate,
) async {
  final value = use<PromptPort>().text(
    message: message,
    initialValue: '$initialValue',
    validate: validate,
  );
  if (value == null) use.fail(const TestCardFailure('Operação cancelada.'));
  return use.result(parsePositiveDecimal(value, initialValue, message));
}

Future<bool> _confirm(
  EffectContext<AppFailure> use,
  String message, {
  required bool initialValue,
}) async {
  final value = use<PromptPort>().select(
    message: message,
    options: const [
      PromptOption(value: 'yes', label: 'Sim'),
      PromptOption(value: 'no', label: 'Não'),
    ],
    initialValue: initialValue ? 'yes' : 'no',
  );
  return value == 'yes';
}
