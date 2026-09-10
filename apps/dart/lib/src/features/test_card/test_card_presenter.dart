import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../app/cli_options.dart';
import '../../application/config/config_models.dart';
import '../../application/terminal/terminal_ports.dart';
import 'test_card_models.dart';

abstract interface class TestCardPresenter {
  AppEffect<Unit> intro(String branch);

  AppEffect<Unit> outro(String message);

  AppEffect<Unit> showDryRun(Config config, String prompt);

  AppEffect<Unit> showSummary(
    TestCardPreparation preparation,
    String provider,
    String model,
    String title,
    String body,
    CliOptions options,
  );

  AppEffect<Unit> raw(String body);

  AppEffect<Unit> success(String message);

  AppEffect<Unit> info(String message);
}

final class TestCardPresenterLive implements TestCardPresenter {
  const TestCardPresenterLive();

  @override
  AppEffect<Unit> intro(String branch) => Effect.result((use) {
    use<TerminalOutput>().heading('Test Case', detail: branch);
    return unit;
  });

  @override
  AppEffect<Unit> outro(String message) => _success(message);

  @override
  AppEffect<Unit> showDryRun(Config config, String prompt) =>
      Effect.result((use) {
        final output = use<TerminalOutput>();
        output.heading('Test Case · dry run');
        output.info('Provider/model: ${_models(config)}');
        output.detail('Prompt de sistema');
        output.write(testCardSystemPromptValue);
        output.detail('Prompt de usuário');
        output.write(prompt);
        return unit;
      });

  @override
  AppEffect<Unit> showSummary(
    TestCardPreparation preparation,
    String provider,
    String model,
    String title,
    String body,
    CliOptions options,
  ) => Effect.result((use) {
    final output = use<TerminalOutput>();
    final context = preparation.context;
    output.heading(
      'Test Card${context.pullRequest == null ? '' : ' · PR #${context.pullRequest!.id}'}',
    );
    output.info('Título: $title');
    output.detail('Provider: $provider/$model');
    output.detail(
      'Work Item: #${context.workItem.id} — ${workItemText(context.workItem, 'System.Title')}',
    );
    output.detail(
      'AreaPath: ${(options.areaPath ?? preparation.config.testAreaPath).isEmpty ? '(não configurado)' : options.areaPath ?? preparation.config.testAreaPath}',
    );
    final assigned = options.assignedTo ?? preparation.config.testAssignedTo;
    if (assigned.isNotEmpty) output.detail('Responsável: $assigned');
    output.write('\n$body');
    return unit;
  });

  @override
  AppEffect<Unit> raw(String body) => _write(body);

  @override
  AppEffect<Unit> success(String message) => _success(message);

  @override
  AppEffect<Unit> info(String message) => Effect.result((use) {
    use<TerminalOutput>().info(message);
    return unit;
  });

  AppEffect<Unit> _success(String message) => Effect.result((use) {
    use<TerminalOutput>().success(message);
    return unit;
  });

  AppEffect<Unit> _write(String value) => Effect.result((use) {
    use<TerminalOutput>().write(value);
    return unit;
  });
}

String _models(Config config) => config.providers
    .map((provider) {
      final model = switch (provider) {
        'codex' => config.codexModel,
        'opencode' => config.opencodeModel,
        _ => config.compatibleModel,
      };
      final reasoning = switch (provider) {
        'codex' => config.codexReasoning,
        'opencode' => config.opencodeReasoning,
        _ => config.compatibleReasoning,
      };
      return '$provider/$model (thinking $reasoning)';
    })
    .join(', ');
