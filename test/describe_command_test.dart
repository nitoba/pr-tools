import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/src/application/ai/description_generator.dart';
import 'package:pr_tools/src/application/ai/description_models.dart';
import 'package:pr_tools/src/application/clipboard/clipboard.dart';
import 'package:pr_tools/src/application/config/config_models.dart';
import 'package:pr_tools/src/application/config/config_service.dart';
import 'package:pr_tools/src/application/terminal/terminal_ports.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/app/app_failure.dart';
import 'package:pr_tools/src/app/cli_options.dart';
import 'package:pr_tools/src/domain/change_context.dart';
import 'package:pr_tools/src/features/describe/describe_command.dart';
import 'package:pr_tools/src/features/describe/describe_models.dart';
import 'package:pr_tools/src/features/describe/describe_presenter.dart';
import 'package:pr_tools/src/features/describe/pull_request_publisher.dart';
import 'package:pr_tools/src/features/describe/describe_service.dart';
import 'package:test/test.dart';

void main() {
  test('dry-run command resolves service and presenter contextually', () async {
    final presenter = _DescribePresenter();
    final result =
        await Module([
          .instance<ConfigRuntime>(_Runtime()),
          .instance<DescribeService>(_DescribeService()),
          .instance<DescribePresenter>(presenter),
          .provide<DescribeCommand>(DescribeCommandLive.new),
        ]).run(
          Effect<int, AppFailure>.result(
            (use) => use.unwrap(use<DescribeCommand>().execute(_options())),
          ),
        );

    expect(result.getOrNull(), 0);
    expect(presenter.dryRun, isTrue);
  });

  test('copies generated content through the contextual clipboard', () async {
    final clipboard = _Clipboard();
    final result =
        await Module([
          .instance<ConfigRuntime>(_Runtime()),
          .instance<DescribeService>(_DescribeService()),
          .instance<DescribePresenter>(_DescribePresenter()),
          .instance<Clipboard>(clipboard),
          .instance<ProgressReporter>(_Progress()),
          .provide<DescribeCommand>(DescribeCommandLive.new),
        ]).run(
          Effect<int, AppFailure>.result(
            (use) => use.unwrap(
              use<DescribeCommand>().execute(
                _options(dryRun: false, copy: true),
              ),
            ),
          ),
        );

    expect(result.getOrNull(), 0);
    expect(clipboard.values, ['body']);
  });
}

CliOptions _options({bool dryRun = true, bool copy = false}) => CliOptions(
  command: Command.desc,
  targets: ['dev'],
  create: false,
  noCreate: false,
  dryRun: dryRun,
  raw: false,
  copy: copy,
);

final class _Runtime implements ConfigRuntime {
  @override
  ConfigPaths get paths => const ConfigPaths(
    directory: '/tmp/pr-tools',
    configFile: '/tmp/pr-tools/config.json',
    envFile: '/tmp/pr-tools/.env',
    templateFile: '/tmp/pr-tools/pr-template.md',
  );

  @override
  Map<String, String> get environment => const {};

  @override
  bool get interactive => false;
}

final class _DescribeService implements DescribeService {
  @override
  AppEffect<DescribePreparation> prepare(
    CliOptions options,
    bool interactive,
  ) => Effect.succeed(
    DescribePreparation(
      config: _config,
      context: _context,
      targets: const ['dev'],
      system: 'system',
      prompt: 'prompt',
      interactive: interactive,
    ),
  );

  @override
  AppEffect<Unit> validateCreation(
    DescribePreparation preparation,
    bool requested,
  ) => Effect.succeed(unit);

  @override
  AppEffect<GeneratedDescription> generate(
    DescribePreparation preparation, {
    DescriptionReporter? report,
  }) => Effect.succeed(
    const GeneratedDescription(
      description: PrDescription(title: 'title', body: 'body'),
      provider: 'codex',
      model: 'model',
    ),
  );
}

final class _DescribePresenter implements DescribePresenter {
  bool dryRun = false;

  @override
  AppEffect<Unit> showDryRun(DescribePreparation preparation) {
    dryRun = true;
    return Effect.succeed(unit);
  }

  @override
  AppEffect<Unit> showDescription(
    DescribePreparation preparation,
    GeneratedDescription generated,
    CliOptions options,
  ) => Effect.succeed(unit);

  @override
  AppEffect<Unit> showPublished(
    DescribePreparation preparation,
    List<PublishedPullRequest> published,
  ) => Effect.succeed(unit);

  @override
  AppEffect<Unit> intro(String branch) => Effect.succeed(unit);

  @override
  AppEffect<Unit> outro(String message) => Effect.succeed(unit);

  @override
  AppEffect<Unit> success(String message) => Effect.succeed(unit);

  @override
  AppEffect<Unit> info(String message) => Effect.succeed(unit);
}

final class _Clipboard implements Clipboard {
  final values = <String>[];

  @override
  AppEffect<bool> copy(String value) {
    values.add(value);
    return Effect.succeed(true);
  }
}

final class _Progress implements ProgressReporter {
  @override
  void error(String message) {}

  @override
  void message(String message) {}

  @override
  void start(String message) {}

  @override
  void stop(String message) {}
}

const _config = Config(
  providers: ['codex'],
  baseUrl: 'https://api.openai.com/v1',
  compatibleModel: 'model',
  compatibleReasoning: 'provider-default',
  codexModel: 'model',
  codexReasoning: 'high',
  opencodeModel: 'model',
  opencodeReasoning: 'provider-default',
  azurePat: '',
  reviewerDev: '',
  reviewerSprint: '',
  testAreaPath: '',
  testAssignedTo: '',
  testTeam: 'DevOps',
  testProgram: 'Agrotrace',
  apiKey: '',
  template: 'template',
);

const _context = ChangeContext(
  branch: 'feature/1',
  sourceRef: 'feature/1',
  baseBranch: 'dev',
  sprintBranch: '',
  diff: 'diff',
  diffOriginalLines: 1,
  log: 'log',
  workItemId: '',
);
