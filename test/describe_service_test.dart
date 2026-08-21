import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/src/application/ai/description_generator.dart';
import 'package:pr_tools/src/application/ai/description_models.dart';
import 'package:pr_tools/src/application/config/config_models.dart';
import 'package:pr_tools/src/application/config/config_service.dart';
import 'package:pr_tools/src/application/change_context/change_context_reader.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/app/app_failure.dart';
import 'package:pr_tools/src/app/cli_options.dart';
import 'package:pr_tools/src/domain/change_context.dart';
import 'package:pr_tools/src/features/describe/describe_models.dart';
import 'package:pr_tools/src/features/describe/describe_service.dart';
import 'package:test/test.dart';

const config = Config(
  providers: ['codex'],
  baseUrl: 'https://api.openai.com/v1',
  compatibleModel: 'compatible-model',
  compatibleReasoning: 'provider-default',
  codexModel: 'codex-model',
  codexReasoning: 'high',
  opencodeModel: 'openai/gpt-5.5',
  opencodeReasoning: 'provider-default',
  azurePat: 'pat',
  reviewerDev: '',
  reviewerSprint: '',
  testAreaPath: '',
  testAssignedTo: '',
  testTeam: 'DevOps',
  testProgram: 'Agrotrace',
  apiKey: '',
  template: 'system template',
);

const context = ChangeContext(
  branch: 'feature/42-login',
  sourceRef: 'feature/42-login',
  baseBranch: 'dev',
  sprintBranch: 'sprint/98',
  diff: 'diff',
  diffOriginalLines: 1,
  log: '42 Ajusta login',
  workItemId: '42',
  remote: RepositoryRemote(
    organization: 'acme',
    project: 'project',
    repository: 'repo',
  ),
);

void main() {
  test(
    'prepares context and delegates generation through contextual services',
    () async {
      final module = Module([
        .instance<ConfigService>(FakeConfigService()),
        .instance<ChangeContextReader>(FakeChangeContextReader()),
        .instance<DescriptionGenerator>(FakeDescriptionGenerator()),
        .provide<DescribeService>(DescribeServiceLive.new),
      ]);
      const options = CliOptions(
        command: Command.desc,
        targets: ['dev'],
        create: false,
        noCreate: false,
        dryRun: false,
        raw: false,
        copy: false,
      );

      final result = await module.run(
        Effect<(DescribePreparation, GeneratedDescription), AppFailure>.result((
          use,
        ) async {
          final service = use<DescribeService>();
          final preparation = await use.unwrap(service.prepare(options, true));
          final generated = await use.unwrap(service.generate(preparation));
          return (preparation, generated);
        }),
      );

      (DescribePreparation, GeneratedDescription)? success;
      AppFailure? failure;
      result.fold<void>((value) => success = value, (error) => failure = error);
      expect(failure, isNull);
      expect(success?.$1.prompt, contains('**Work Item:** #42'));
      expect(success?.$1.system, 'system template');
      expect(success?.$2.description.title, 'Generated');
    },
  );

  test(
    'rejects requested creation outside an interactive Azure context',
    () async {
      final module = Module([
        .instance<ConfigService>(FakeConfigService()),
        .instance<ChangeContextReader>(FakeChangeContextReader()),
        .provide<DescribeService>(DescribeServiceLive.new),
      ]);
      const options = CliOptions(
        command: Command.desc,
        targets: ['dev'],
        create: true,
        noCreate: false,
        dryRun: false,
        raw: false,
        copy: false,
      );
      final result = await module.run(
        Effect<Unit, AppFailure>.result((use) async {
          final service = use<DescribeService>();
          final preparation = await use.unwrap(service.prepare(options, false));
          return use.unwrap(service.validateCreation(preparation, true));
        }),
      );

      expect(result.getOrNull(), isNull);
      expect(
        result.exceptionOrNull()?.message,
        contains('terminal interativo'),
      );
    },
  );
}

final class FakeConfigService implements ConfigService {
  @override
  AppEffect<Config> load(CliOptions options) => Effect.succeed(config);

  @override
  AppEffect<ConfigInitialization> initialize() => Effect.succeed(
    const ConfigInitialization(
      paths: ConfigPaths(
        directory: '',
        configFile: '',
        envFile: '',
        templateFile: '',
      ),
      interactive: false,
      azurePatConfigured: false,
    ),
  );
}

final class FakeChangeContextReader implements ChangeContextReader {
  @override
  AppEffect<ChangeContext> collect([String? sourceBranch]) =>
      Effect.succeed(context);
}

final class FakeDescriptionGenerator implements DescriptionGenerator {
  @override
  AppEffect<GeneratedDescription> generate({
    required Config config,
    required String system,
    required String prompt,
    required String branch,
    DescriptionReporter? report,
  }) => Effect.succeed(
    const GeneratedDescription(
      description: PrDescription(title: 'Generated', body: 'Body'),
      provider: 'codex',
      model: 'codex-model',
    ),
  );
}
