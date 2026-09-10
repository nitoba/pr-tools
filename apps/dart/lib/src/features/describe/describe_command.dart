import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../app/cli_options.dart';
import '../../application/clipboard/clipboard.dart';
import '../../application/config/config_service.dart';
import '../../application/terminal/terminal_ports.dart';
import 'describe_presenter.dart';
import 'pull_request_publisher.dart';
import 'describe_service.dart';

abstract interface class DescribeCommand {
  AppEffect<int> execute(CliOptions options);
}

final class DescribeCommandLive implements DescribeCommand {
  const DescribeCommandLive();

  @override
  AppEffect<int> execute(CliOptions options) => Effect.result((use) async {
    final service = use<DescribeService>();
    final presenter = use<DescribePresenter>();
    final interactive = use<ConfigRuntime>().interactive;
    final preparation = await use.unwrap(service.prepare(options, interactive));
    await use.unwrap(service.validateCreation(preparation, options.create));
    if (options.dryRun) {
      await use.unwrap(presenter.showDryRun(preparation));
      return 0;
    }

    await use.unwrap(presenter.intro(preparation.context.branch));
    final progress = use<ProgressReporter>();
    progress.start('Gerando descrição via IA');
    final generated = await use.unwrap(
      service
          .generate(
            preparation,
            report: (provider, model) {
              progress.message('Tentando $provider ($model)');
            },
          )
          .tapError((_) => progress.error('Falha ao gerar descrição')),
    );
    progress.stop(
      'Descrição gerada (${generated.provider}/${generated.model})',
    );
    await use.unwrap(
      presenter.showDescription(preparation, generated, options),
    );
    if (options.copy &&
        await use.unwrap(use<Clipboard>().copy(generated.description.body))) {
      await use.unwrap(
        presenter.success('Descrição copiada para o clipboard.'),
      );
    }

    if (preparation.context.remote != null &&
        preparation.config.azurePat.trim().isNotEmpty &&
        interactive) {
      final shouldCreate = await _confirm(
        use,
        'Criar PR(s) no Azure DevOps?',
        initialValue: options.create,
      );
      if (shouldCreate) {
        final reviewers = <String, String>{};
        final prompts = use<PromptPort>();
        for (final target in preparation.targets) {
          final defaultReviewer = target.contains('sprint')
              ? preparation.config.reviewerSprint.isNotEmpty
                    ? preparation.config.reviewerSprint
                    : preparation.config.reviewerDev
              : preparation.config.reviewerDev;
          final reviewer = prompts.text(
            message: 'Reviewer para $target (opcional; Enter mantém o padrão)',
            initialValue: defaultReviewer,
          );
          if (reviewer == null) return 0;
          final selectedReviewer = reviewer.trim();
          reviewers[target] = selectedReviewer.isEmpty
              ? defaultReviewer.trim()
              : selectedReviewer;
        }
        final reviewerSummary = preparation.targets
            .map(
              (target) =>
                  '$target: ${reviewers[target]!.isEmpty ? 'nenhum' : reviewers[target]}',
            )
            .join('; ');
        final confirmedReviewers = await _confirm(
          use,
          'Criar PR(s) com estes reviewers? $reviewerSummary',
          initialValue: true,
        );
        if (!confirmedReviewers) return 0;
        progress.start('Criando PR(s) no Azure DevOps');
        final published = await use.unwrap(
          use<PullRequestPublisher>()
              .publish(
                preparation.targets,
                PullRequestDraft(
                  title: generated.description.title,
                  description: generated.description.body,
                  workItemIds: preparation.workItemId == null
                      ? const []
                      : [preparation.workItemId!],
                ),
                reviewerForTarget: (target) => reviewers[target] ?? '',
              )
              .tapError((_) => progress.error('Falha ao criar PR(s)')),
        );
        progress.stop('PR(s) criado(s) (${published.length})');
        await use.unwrap(presenter.showPublished(preparation, published));
      }
    }
    await use.unwrap(presenter.outro('Concluído.'));
    return 0;
  });
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
