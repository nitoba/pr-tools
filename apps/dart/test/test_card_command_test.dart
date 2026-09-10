import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/src/application/ai/description_generator.dart';
import 'package:pr_tools/src/application/ai/description_models.dart';
import 'package:pr_tools/src/application/config/config_models.dart';
import 'package:pr_tools/src/application/config/config_service.dart';
import 'package:pr_tools/src/application/terminal/terminal_ports.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/app/app_failure.dart';
import 'package:pr_tools/src/app/cli_options.dart';
import 'package:pr_tools/src/domain/change_context.dart';
import 'package:pr_tools/src/features/test_card/test_card_command.dart';
import 'package:pr_tools/src/features/test_card/test_card_models.dart';
import 'package:pr_tools/src/features/test_card/test_card_presenter.dart';
import 'package:pr_tools/src/features/test_card/test_card_service.dart';
import 'package:test/test.dart';

void main() {
  test(
    'dry-run test card command resolves dependencies contextually',
    () async {
      final presenter = _Presenter();
      final result =
          await Module([
            .instance<ConfigRuntime>(_Runtime()),
            .instance<TestCardService>(_Service()),
            .instance<TestCardPresenter>(presenter),
            .provide<TestCardCommand>(TestCardCommandLive.new),
          ]).run(
            Effect<int, AppFailure>.result(
              (use) => use.unwrap(use<TestCardCommand>().execute(_options())),
            ),
          );

      expect(result.getOrNull(), 0);
      expect(presenter.dryRun, isTrue);
    },
  );

  test('uses the Work Item effort when Enter confirms the default', () async {
    final prompts = _PromptScript(selects: ['yes', 'yes']);
    final service = await _executeInteractive(
      WorkItem(id: 42, fields: const {'Microsoft.VSTS.Scheduling.Effort': 2.5}),
      prompts,
    );

    expect(service.effort, 2.5);
    expect(service.realEffort, 2.5);
    expect(
      prompts.messages,
      contains('Effort (horas decimais; Enter usa 2.5)'),
    );
  });

  test('uses one hour when the Work Item has no effort', () async {
    final prompts = _PromptScript(selects: ['yes', 'yes']);
    final service = await _executeInteractive(
      const WorkItem(id: 42, fields: {}),
      prompts,
    );

    expect(service.effort, 1);
    expect(service.realEffort, 1);
    expect(prompts.messages, contains('Effort (horas decimais; Enter usa 1)'));
  });
}

Future<_InteractiveService> _executeInteractive(
  WorkItem parent,
  _PromptScript prompts,
) async {
  final service = _InteractiveService(parent);
  final result =
      await Module([
        .instance<ConfigRuntime>(_Runtime(interactiveValue: true)),
        .instance<TestCardService>(service),
        .instance<TestCardPresenter>(_Presenter()),
        .instance<PromptPort>(prompts),
        .instance<ProgressReporter>(_Progress()),
        .provide<TestCardCommand>(TestCardCommandLive.new),
      ]).run(
        Effect<int, AppFailure>.result(
          (use) => use.unwrap(
            use<TestCardCommand>().execute(_options(dryRun: false)),
          ),
        ),
      );

  expect(result.getOrNull(), 0, reason: result.exceptionOrNull()?.message);
  return service;
}

CliOptions _options({bool dryRun = true}) => CliOptions(
  command: Command.test,
  targets: [],
  create: false,
  noCreate: false,
  dryRun: dryRun,
  raw: false,
  copy: false,
);

final class _Runtime implements ConfigRuntime {
  _Runtime({this.interactiveValue = false});

  final bool interactiveValue;

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
  bool get interactive => interactiveValue;
}

final class _PromptScript implements PromptPort {
  _PromptScript({required List<String?> selects})
    : _selects = [...selects],
      _texts = List<String?>.filled(8, '', growable: true);

  final List<String?> _selects;
  final List<String?> _texts;
  final messages = <String>[];

  @override
  String? text({
    required String message,
    String? initialValue,
    String? placeholder,
    PromptValidator? validate,
  }) {
    messages.add(message);
    return _texts.removeAt(0);
  }

  @override
  String? password({required String message}) => null;

  @override
  String? select({
    required String message,
    required List<PromptOption> options,
    String? initialValue,
  }) {
    messages.add(message);
    return _selects.removeAt(0);
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

final class _Service implements TestCardService {
  @override
  AppEffect<TestCardPreparation> prepare(
    CliOptions options,
    bool interactive,
  ) => Effect.succeed(
    TestCardPreparation(
      config: _config,
      context: const TestCardContext(
        change: _context,
        workItem: WorkItem(id: 1, fields: {}),
        changes: [],
        examples: [],
      ),
      prompt: 'prompt',
    ),
  );

  @override
  AppEffect<GeneratedDescription> generate(
    TestCardPreparation preparation, {
    DescriptionReporter? report,
  }) => Effect.succeed(
    const GeneratedDescription(
      description: PrDescription(title: 'title', body: 'body'),
      provider: 'codex',
      model: 'model',
    ),
  );

  @override
  AppEffect<WorkItem> create(
    TestCardPreparation preparation,
    TestCardDraft input,
  ) => Effect.fail(const TestCardFailure('not used'));

  @override
  AppEffect<TestCardUpdate> updateParent(
    TestCardPreparation preparation, {
    num? effort,
    num? realEffort,
  }) => Effect.fail(const TestCardFailure('not used'));
}

final class _InteractiveService implements TestCardService {
  _InteractiveService(this.parent);

  final WorkItem parent;
  num? effort;
  num? realEffort;

  @override
  AppEffect<TestCardPreparation> prepare(
    CliOptions options,
    bool interactive,
  ) => Effect.succeed(
    TestCardPreparation(
      config: _config,
      context: TestCardContext(
        change: _context,
        workItem: parent,
        changes: const [],
        examples: const [],
      ),
      prompt: 'prompt',
    ),
  );

  @override
  AppEffect<GeneratedDescription> generate(
    TestCardPreparation preparation, {
    DescriptionReporter? report,
  }) => Effect.succeed(
    const GeneratedDescription(
      description: PrDescription(title: 'title', body: 'body'),
      provider: 'codex',
      model: 'model',
    ),
  );

  @override
  AppEffect<WorkItem> create(
    TestCardPreparation preparation,
    TestCardDraft input,
  ) => Effect.succeed(const WorkItem(id: 99, fields: {}));

  @override
  AppEffect<TestCardUpdate> updateParent(
    TestCardPreparation preparation, {
    num? effort,
    num? realEffort,
  }) {
    this.effort = effort;
    this.realEffort = realEffort;
    return Effect.succeed(const TestCardUpdate());
  }
}

final class _Presenter implements TestCardPresenter {
  bool dryRun = false;

  @override
  AppEffect<Unit> showDryRun(Config config, String prompt) {
    dryRun = true;
    return Effect.succeed(unit);
  }

  @override
  AppEffect<Unit> intro(String branch) => Effect.succeed(unit);

  @override
  AppEffect<Unit> outro(String message) => Effect.succeed(unit);

  @override
  AppEffect<Unit> showSummary(
    TestCardPreparation preparation,
    String provider,
    String model,
    String title,
    String body,
    CliOptions options,
  ) => Effect.succeed(unit);

  @override
  AppEffect<Unit> raw(String body) => Effect.succeed(unit);

  @override
  AppEffect<Unit> success(String message) => Effect.succeed(unit);

  @override
  AppEffect<Unit> info(String message) => Effect.succeed(unit);
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
  azurePat: 'pat',
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
  workItemId: '1',
  remote: RepositoryRemote(
    organization: 'org',
    project: 'project',
    repository: 'repo',
  ),
);
