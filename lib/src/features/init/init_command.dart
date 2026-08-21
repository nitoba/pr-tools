import '../../app/app_effect.dart';
import '../../app/cli_options.dart';
import '../../application/config/config_service.dart';
import '../../application/terminal/terminal_ports.dart';

abstract interface class InitCommand {
  AppEffect<int> execute(CliOptions options);
}

final class InitCommandLive implements InitCommand {
  const InitCommandLive();

  @override
  AppEffect<int> execute(CliOptions options) => .result((use) async {
    final output = use<TerminalOutput>();
    final config = use<ConfigService>();
    final runtime = use<ConfigRuntime>();
    if (runtime.interactive) output.write('┌ prt · configuração\n│');

    final result = await use.unwrap(config.initialize());
    if (result.cancelled) {
      output.write('✗ Operação cancelada.');
      return 0;
    }
    if (result.interactive) {
      final patStatus = result.azurePatConfigured
          ? 'PAT Azure salvo em ${result.paths.envFile} (AZURE_PAT)'
          : 'PAT Azure não configurado';
      output.write(
        '◆ prt\n'
        'Configuração salva em ${result.paths.configFile}\n'
        '$patStatus\n'
        'Template salvo em ${result.paths.templateFile}',
      );
      output.write('└ Pronto. Execute `prt desc --dry-run`.');
    } else {
      output.write('Configuração salva em ${result.paths.configFile}');
    }
    return 0;
  });
}
