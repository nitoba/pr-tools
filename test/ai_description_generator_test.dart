import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/src/application/ai/description_generator.dart';
import 'package:pr_tools/src/application/ai/description_models.dart';
import 'package:pr_tools/src/application/config/config_models.dart';
import 'package:pr_tools/src/application/process/process_runner.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/app/app_failure.dart';
import 'package:pr_tools/src/infrastructure/ai/ai_description_generator.dart';
import 'package:test/test.dart';

const config = Config(
  providers: ['codex', 'opencode'],
  baseUrl: 'https://api.openai.com/v1',
  compatibleModel: 'compatible-model',
  compatibleReasoning: 'provider-default',
  codexModel: 'codex-model',
  codexReasoning: 'high',
  opencodeModel: 'openai/gpt-5.5',
  opencodeReasoning: 'medium',
  azurePat: '',
  reviewerDev: '',
  reviewerSprint: '',
  testAreaPath: '',
  testAssignedTo: '',
  testTeam: 'DevOps',
  testProgram: 'Agrotrace',
  apiKey: '',
  template: 'system',
);

void main() {
  test(
    'tries providers in order and returns the first successful result',
    () async {
      final processes = FakeProcessRunner([
        const ProcessResult(exitCode: 1, stdout: '', stderr: 'denied'),
        const ProcessResult(
          exitCode: 0,
          stdout: '{"title":"Ajusta login","body":"## Descrição"}',
          stderr: '',
        ),
      ]);
      final reports = <String>[];
      final module = Module([
        .instance<ProcessRunner>(processes),
        .provide<DescriptionGenerator>(DescriptionGeneratorLive.new),
      ]);

      final result = await module.run(
        Effect<GeneratedDescription, AppFailure>.result((use) async {
          final generator = use<DescriptionGenerator>();
          return use.unwrap(
            generator.generate(
              config: config,
              system: 'system',
              prompt: 'prompt',
              branch: 'feature/1-login',
              report: (provider, model) => reports.add('$provider/$model'),
            ),
          );
        }),
      );
      GeneratedDescription? generated;
      AppFailure? failure;
      result.fold<void>(
        (value) => generated = value,
        (error) => failure = error,
      );

      expect(failure, isNull);
      expect(
        generated?.description,
        const PrDescription(title: 'Ajusta login', body: '## Descrição'),
      );
      expect(generated?.provider, 'opencode');
      expect(generated?.model, 'openai/gpt-5.5');
      expect(reports, ['codex/codex-model', 'opencode/openai/gpt-5.5']);
      expect(processes.commands.map((value) => value.command), [
        'codex',
        'opencode',
      ]);
    },
  );

  test(
    'resolves the compatible provider through its abstract boundary',
    () async {
      final compatible = FakeCompatibleDescriptionGenerator();
      final module = Module([
        .instance<CompatibleDescriptionGenerator>(compatible),
        .provide<DescriptionGenerator>(DescriptionGeneratorLive.new),
      ]);
      const compatibleConfig = Config(
        providers: ['openai-compatible'],
        baseUrl: 'http://localhost:1234/v1',
        compatibleModel: 'local-model',
        compatibleReasoning: 'provider-default',
        codexModel: 'codex-model',
        codexReasoning: 'high',
        opencodeModel: 'openai/gpt-5.5',
        opencodeReasoning: 'provider-default',
        azurePat: '',
        reviewerDev: '',
        reviewerSprint: '',
        testAreaPath: '',
        testAssignedTo: '',
        testTeam: 'DevOps',
        testProgram: 'Agrotrace',
        apiKey: '',
        template: 'system',
      );

      final result = await module.run(
        Effect.result((use) async {
          return use.unwrap(
            use<DescriptionGenerator>().generate(
              config: compatibleConfig,
              system: 'system',
              prompt: 'prompt',
              branch: 'feature/1-login',
            ),
          );
        }),
      );

      expect(result.getOrNull()?.description.title, 'Compatible');
      expect(compatible.calls, 1);
    },
  );
}

final class FakeProcessRunner implements ProcessRunner {
  FakeProcessRunner(this.results);

  final List<ProcessResult> results;
  final commands = <({String command, List<String> arguments})>[];

  @override
  AppEffect<ProcessResult> run(
    String command,
    List<String> arguments, {
    Duration? timeout,
  }) {
    commands.add((command: command, arguments: List.unmodifiable(arguments)));
    return Effect.succeed(results.removeAt(0));
  }
}

final class FakeCompatibleDescriptionGenerator
    implements CompatibleDescriptionGenerator {
  var calls = 0;

  @override
  AppEffect<PrDescription> generate({
    required Config config,
    required String system,
    required String prompt,
    required String branch,
  }) {
    calls += 1;
    return Effect.succeed(
      const PrDescription(title: 'Compatible', body: 'Body'),
    );
  }
}
