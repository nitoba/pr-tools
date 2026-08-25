import 'package:better_effect/better_effect.dart';
import 'package:terminice/terminice.dart' show Terminice;

import '../application/ai/description_generator.dart';
import '../application/clipboard/clipboard.dart';
import '../application/config/config_service.dart';
import '../application/change_context/change_context_reader.dart';
import '../application/process/process_runner.dart';
import '../application/terminal/terminal_ports.dart';
import '../features/describe/describe_service.dart';
import '../features/describe/describe_command.dart';
import '../features/describe/describe_presenter.dart';
import '../features/doctor/doctor_command.dart';
import '../features/doctor/doctor_presenter.dart';
import '../features/doctor/doctor_service.dart';
import '../features/test_card/test_card_service.dart';
import '../features/test_card/test_card_command.dart';
import '../features/test_card/test_card_presenter.dart';
import '../features/init/init_command.dart';
import '../infrastructure/ai/ai_description_generator.dart';
import '../infrastructure/ai/genkit_compatible_generator.dart';
import '../infrastructure/config/config_service_live.dart';
import '../infrastructure/doctor/execution.dart';
import '../infrastructure/git/change_context_reader_live.dart';
import '../infrastructure/clipboard/clipboard.dart';
import '../infrastructure/azure/execution.dart';
import '../infrastructure/process/process_runner_live.dart';
import '../infrastructure/terminal/terminal_live.dart';
import 'app_effect.dart';
import 'application.dart';
import 'cli_options.dart';

final appModule = Module.complete([
  .instance<Terminice>(prtTerminice),
  .provide<PromptPort>(PromptPortLive.new),
  .provide<TerminalOutput>(TerminalOutputLive.new),
  .provide<ProgressReporter>(TerminiceProgress.new),
  .provide<ProcessRunner>(ProcessRunnerLive.new),
  .provide<Clipboard>(NativeClipboard.new),
  .provide<CliApplication>(CliApplicationLive.new),
  .provide<ConfigRuntime>(ConfigRuntimeLive.new),
  .provide<ConfigFileSystem>(ConfigFileSystemLive.new),
  .provide<ConfigService>(ConfigServiceLive.new),
  .provide<ChangeContextReader>(ChangeContextReaderLive.new),
  .provide<CompatibleDescriptionGenerator>(
    GenkitCompatibleDescriptionGenerator.new,
  ),
  .provide<DescriptionGenerator>(DescriptionGeneratorLive.new),
  .provide<DescribeService>(DescribeServiceLive.new),
  .provide<DescribePresenter>(DescribePresenterLive.new),
  .provide<DescribeCommand>(DescribeCommandLive.new),
  .provide<TestCardService>(TestCardServiceLive.new),
  .provide<TestCardPresenter>(TestCardPresenterLive.new),
  .provide<TestCardCommand>(TestCardCommandLive.new),
  .provide<InitCommand>(InitCommandLive.new),
  .provide<DoctorService>(DoctorServiceLive.new),
  .provide<DoctorPresenter>(DoctorPresenterLive.new),
  .provide<DoctorCommand>(DoctorCommandLive.new),
]);

Module doctorExecutionModule() => doctorRequestModule();

AppEffect<int> runCli(List<String> arguments) => .result((use) {
  use.cancellation.throwIfCancelled();
  final application = use<CliApplication>();
  return use.unwrap(application.run(arguments));
});

AppEffect<Module> azureExecutionModule(CliOptions options) =>
    .result((use) async {
      final config = await use.unwrap(use<ConfigService>().load(options));
      final context = await use.unwrap(
        use<ChangeContextReader>().collect(options.source),
      );
      use.cancellation.throwIfCancelled();
      return azureRequestModule(config, context);
    });
