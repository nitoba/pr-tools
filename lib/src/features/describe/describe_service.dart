import 'package:better_effect/better_effect.dart';

import '../../application/ai/description_generator.dart';
import '../../application/ai/description_models.dart';
import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../app/cli_options.dart';
import '../../application/config/config_service.dart';
import '../../application/change_context/change_context_reader.dart';
import '../../application/change_context/merged_pull_request_lookup.dart';
import '../../domain/change_context.dart';
import 'describe_models.dart';
import 'describe_prompt.dart';

abstract interface class DescribeService {
  AppEffect<DescribePreparation> prepare(CliOptions options, bool interactive);

  AppEffect<Unit> validateCreation(
    DescribePreparation preparation,
    bool requested,
  );

  AppEffect<GeneratedDescription> generate(
    DescribePreparation preparation, {
    DescriptionReporter? report,
  });
}

final class DescribeFailure extends AppFailure {
  const DescribeFailure(String message) : super(message, 1);
}

final class DescribeServiceLive implements DescribeService {
  const DescribeServiceLive();

  @override
  AppEffect<DescribePreparation> prepare(
    CliOptions options,
    bool interactive,
  ) => .result((use) async {
    final config = await use.unwrap(use<ConfigService>().load(options));
    final initialContext = await use.unwrap(
      use<ChangeContextReader>().collect(options.source),
    );
    final usesDefaultTargets = options.targets.isEmpty;
    if ((usesDefaultTargets || options.targets.contains('sprint')) &&
        initialContext.sprintBranch.isEmpty) {
      use.fail(
        const DescribeFailure(
          'Target sprint solicitado, mas nenhuma branch sprint/<número> foi encontrada.',
        ),
      );
    }
    final targets = resolveTargets(initialContext, options.targets);
    if (targets.isEmpty) {
      use.fail(const DescribeFailure('Nenhum target disponível.'));
    }
    final mergedResult = await use.unwrap(
      use<MergedPullRequestLookup>().findLatest(
        remote: initialContext.remote,
        sourceBranch: initialContext.branch,
        targetBranches: targets,
      ),
    );
    final mergedPullRequest = mergedResult.value;
    final context = mergedPullRequest == null
        ? initialContext
        : await use.unwrap(
            use<ChangeContextReader>().collect(
              options.source,
              mergedPullRequest.sourceCommit,
            ),
          );
    final workItemId =
        options.workItem?.value ??
        (context.workItemId.isEmpty ? null : context.workItemId);
    if (workItemId != null) {
      await use.result(_validateWorkItem(workItemId, 'Work Item'));
    }
    return DescribePreparation(
      config: config,
      context: context,
      targets: targets,
      workItemId: workItemId,
      system: config.template,
      prompt: buildPrompt(context, targets, workItemId ?? ''),
      interactive: interactive,
    );
  });

  @override
  AppEffect<Unit> validateCreation(
    DescribePreparation preparation,
    bool requested,
  ) => .result((use) {
    if (requested && !preparation.interactive) {
      use.fail(
        const DescribeFailure(
          '--create requer terminal interativo para confirmar a descrição e os revisores.',
        ),
      );
    }
    if (requested && preparation.context.remote == null) {
      use.fail(
        const DescribeFailure('--create requer um remote Git do Azure DevOps.'),
      );
    }
    if (requested && preparation.config.azurePat.trim().isEmpty) {
      use.fail(
        const DescribeFailure(
          '--create requer AZURE_PAT ou AZURE_DEVOPS_PAT configurado.',
        ),
      );
    }
    return unit;
  });

  @override
  AppEffect<GeneratedDescription> generate(
    DescribePreparation preparation, {
    DescriptionReporter? report,
  }) => .result((use) async {
    final generator = use<DescriptionGenerator>();
    return use.unwrap(
      generator.generate(
        config: preparation.config,
        system: preparation.system,
        prompt: preparation.prompt,
        branch: preparation.context.branch,
        report: report,
      ),
    );
  });
}

ResultDart<String, DescribeFailure> _validateWorkItem(
  String value,
  String label,
) {
  final id = value.trim();
  if (!RegExp(r'^\d+$').hasMatch(id)) {
    return Failure<String, DescribeFailure>(
      DescribeFailure('$label inválido: Use um ID numérico.'),
    );
  }
  final number = int.tryParse(id);
  if (number == null || number <= 0) {
    return Failure<String, DescribeFailure>(
      DescribeFailure('$label inválido: Use um ID positivo.'),
    );
  }
  return Success<String, DescribeFailure>(id);
}
