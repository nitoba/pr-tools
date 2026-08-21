import 'package:better_effect/better_effect.dart';

import '../../application/ai/description_models.dart';
import '../../app/app_effect.dart';
import '../../app/cli_options.dart';
import '../../application/terminal/terminal_ports.dart';
import 'describe_models.dart';
import 'pull_request_publisher.dart';

abstract interface class DescribePresenter {
  AppEffect<Unit> showDryRun(DescribePreparation preparation);

  AppEffect<Unit> showDescription(
    DescribePreparation preparation,
    GeneratedDescription generated,
    CliOptions options,
  );

  AppEffect<Unit> showPublished(
    DescribePreparation preparation,
    List<PublishedPullRequest> published,
  );

  AppEffect<Unit> intro(String branch);

  AppEffect<Unit> outro(String message);

  AppEffect<Unit> success(String message);

  AppEffect<Unit> info(String message);
}

final class DescribePresenterLive implements DescribePresenter {
  const DescribePresenterLive();

  @override
  AppEffect<Unit> showDryRun(DescribePreparation preparation) =>
      Effect.result((use) async {
        final output = use<TerminalOutput>();
        output.heading('PR · dry run', detail: preparation.context.branch);
        output.info('Provider/model: ${_models(preparation)}');
        output.detail('Prompt de sistema');
        output.write(preparation.system);
        output.detail('Prompt de usuário');
        output.write(preparation.prompt);
        return unit;
      });

  @override
  AppEffect<Unit> showDescription(
    DescribePreparation preparation,
    GeneratedDescription generated,
    CliOptions options,
  ) => Effect.result((use) {
    final output = use<TerminalOutput>();
    final title = generated.description.title;
    final body = generated.description.body;
    if (options.raw) {
      output.write(body);
      return unit;
    }
    output.heading('Pull Request', detail: preparation.context.branch);
    output.info('Título: $title');
    output.detail('Targets: ${preparation.targets.join(', ')}');
    if (preparation.workItemId case final workItemId?) {
      output.detail('Work Item: #$workItemId');
      if (preparation.context.remote != null) {
        output.detail(
          'Work Item URL: ${_workItemUrl(preparation, int.parse(workItemId))}',
        );
      }
    }
    if (preparation.context.remote != null) {
      for (final target in preparation.targets) {
        output.detail('PR $target: ${_pullRequestUrl(preparation, target)}');
      }
    }
    output.write('\n$body');
    return unit;
  });

  @override
  AppEffect<Unit> showPublished(
    DescribePreparation preparation,
    List<PublishedPullRequest> published,
  ) => Effect.result((use) {
    final output = use<TerminalOutput>();
    for (final item in published) {
      final url =
          item.url ??
          (item.id > 0
              ? _pullRequestUrlById(preparation, item.id)
              : _pullRequestUrl(preparation, item.target));
      output.success('PR ${item.target} criado: $url');
    }
    return unit;
  });

  @override
  AppEffect<Unit> intro(String branch) => Effect.result((use) {
    use<TerminalOutput>().heading('Pull Request', detail: branch);
    return unit;
  });

  @override
  AppEffect<Unit> outro(String message) => _success(message);

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
}

String _models(DescribePreparation preparation) => preparation.config.providers
    .map((provider) {
      final model = switch (provider) {
        'codex' => preparation.config.codexModel,
        'opencode' => preparation.config.opencodeModel,
        _ => preparation.config.compatibleModel,
      };
      final reasoning = switch (provider) {
        'codex' => preparation.config.codexReasoning,
        'opencode' => preparation.config.opencodeReasoning,
        _ => preparation.config.compatibleReasoning,
      };
      return '$provider/$model (thinking $reasoning)';
    })
    .join(', ');

String _pullRequestUrl(DescribePreparation preparation, String target) =>
    'https://dev.azure.com/${Uri.encodeComponent(preparation.context.remote!.organization)}/${Uri.encodeComponent(preparation.context.remote!.project)}/_git/${Uri.encodeComponent(preparation.context.remote!.repository)}/pullrequestcreate?sourceRef=${Uri.encodeComponent(preparation.context.branch)}&targetRef=${Uri.encodeComponent(target)}';

String _pullRequestUrlById(DescribePreparation preparation, int id) =>
    'https://dev.azure.com/${Uri.encodeComponent(preparation.context.remote!.organization)}/${Uri.encodeComponent(preparation.context.remote!.project)}/_git/${Uri.encodeComponent(preparation.context.remote!.repository)}/pullrequest/$id';

String _workItemUrl(DescribePreparation preparation, int id) =>
    'https://dev.azure.com/${Uri.encodeComponent(preparation.context.remote!.organization)}/${Uri.encodeComponent(preparation.context.remote!.project)}/_workitems/edit/$id';
