import 'cli_options.dart';

const helpText = '''prt v$version

Gera descrições de PR e Test Cases a partir do contexto Git.

Uso:
  prt desc [opções]
  prt test [opções]
  prt init
  prt doctor

Opções:
  --source <branch>       Branch de origem
  --target <branch>       Target; pode repetir
  --work-item <id>        Work Item
  --provider <nome>       codex, opencode ou openai-compatible
  --model <nome>          Modelo do provider
  --base-url <url>        Endpoint OpenAI-compatible
  --api-key <key>         API key do endpoint
  --create                Confirma a criação após gerar o conteúdo
  --no-create             Apenas gera o Test Case
  --pr <id>               PR Azure DevOps para contexto do card
  --area-path <path>      AreaPath do Test Case
  --assigned-to <valor>   Responsável do Test Case
  --iteration-path <path> IterationPath do Test Case
  --priority <n>          Prioridade do Test Case
  --team <nome>           Campo Custom.Team
  --program <nome>        Campo Custom.ProgramasAgrotrace
  --examples <n>          Exemplos de Test Cases (0-5)
  --dry-run               Mostra o prompt sem chamar o modelo
  --raw                   Imprime somente o Markdown
  --no-copy               Não copia o conteúdo
  --version, -v           Mostra a versão
  --help, -h              Mostra esta ajuda''';

const version = '4.0.7';

const _providers = <String>{'codex', 'opencode', 'openai-compatible'};

const _valueOptions = <String>{
  '--source',
  '--target',
  '--work-item',
  '--provider',
  '--model',
  '--base-url',
  '--api-key',
  '--pr',
  '--area-path',
  '--assigned-to',
  '--iteration-path',
  '--priority',
  '--team',
  '--program',
  '--examples',
};

CliParse parseCli(List<String> arguments) {
  if (arguments.contains('--help') || arguments.contains('-h')) {
    return const HelpRequested();
  }
  if (arguments.contains('--version') || arguments.contains('-v')) {
    return const VersionRequested();
  }

  Command? command;
  final values = <String, String>{};
  final targets = <String>[];
  var create = false;
  var noCreate = false;
  var dryRun = false;
  var raw = false;
  var copy = true;

  for (var index = 0; index < arguments.length; index++) {
    final argument = arguments[index];
    if (!argument.startsWith('-')) {
      final parsedCommand = switch (argument) {
        'desc' => Command.desc,
        'test' => Command.test,
        'init' => Command.init,
        'doctor' => Command.doctor,
        _ => null,
      };
      if (command != null) {
        return InvalidArguments(
          'Argumentos posicionais inesperados: $argument',
        );
      }
      if (parsedCommand == null) {
        return InvalidArguments('Comando desconhecido: $argument');
      }
      command = parsedCommand;
      continue;
    }
    if (argument == '--create') {
      create = true;
      continue;
    }
    if (argument == '--no-create') {
      noCreate = true;
      continue;
    }
    if (argument == '--dry-run') {
      dryRun = true;
      continue;
    }
    if (argument == '--raw') {
      raw = true;
      continue;
    }
    if (argument == '--no-copy') {
      copy = false;
      continue;
    }
    final (name, inlineValue) = _splitOption(argument);
    if (!_valueOptions.contains(name)) {
      return InvalidArguments('Opção desconhecida: $name');
    }
    final value =
        inlineValue ??
        (index + 1 < arguments.length ? arguments[++index] : null);
    if (value == null || value.startsWith('--')) {
      return InvalidArguments('A opção $name requer um valor.');
    }
    if (name == '--target') {
      targets.add(value);
    } else {
      values[name] = value;
    }
  }
  if (create && noCreate) {
    return const InvalidArguments(
      '--create e --no-create não podem ser usados juntos.',
    );
  }
  final provider = values['--provider'];
  if (provider != null && !_providers.contains(provider)) {
    return InvalidArguments(
      'Provider inválido: $provider. Use codex, opencode ou openai-compatible.',
    );
  }
  for (final target in targets) {
    if (target != 'dev' &&
        target != 'sprint' &&
        !target.startsWith('sprint/')) {
      return InvalidArguments(
        'Target inválido: $target. Use dev, sprint ou sprint/<número>.',
      );
    }
  }
  for (final entry in [
    (value: values['--work-item'], label: '--work-item'),
    (value: values['--pr'], label: '--pr'),
  ]) {
    final error = _workItemError(entry.value, entry.label);
    if (error != null) {
      return InvalidArguments(error);
    }
  }
  return ParsedOptions(
    CliOptions(
      command: command ?? Command.desc,
      source: values['--source'],
      targets: List.unmodifiable(targets),
      workItem: _workItem(values['--work-item']),
      provider: values['--provider'],
      model: values['--model'],
      baseUrl: values['--base-url'],
      apiKey: values['--api-key'],
      create: create,
      noCreate: noCreate,
      pr: _workItem(values['--pr']),
      areaPath: values['--area-path'],
      assignedTo: values['--assigned-to'],
      iterationPath: values['--iteration-path'],
      priority: values['--priority'],
      team: values['--team'],
      program: values['--program'],
      examples: values['--examples'],
      dryRun: dryRun,
      raw: raw,
      copy: copy,
    ),
  );
}

(String, String?) _splitOption(String argument) {
  final separator = argument.indexOf('=');
  return separator < 0
      ? (argument, null)
      : (argument.substring(0, separator), argument.substring(separator + 1));
}

WorkItemId? _workItem(String? value) =>
    value == null || value.trim().isEmpty ? null : WorkItemId(value.trim());

String? _workItemError(String? value, String label) {
  final id = value?.trim();
  if (id == null || id.isEmpty) {
    return null;
  }
  if (!RegExp(r'^\d+$').hasMatch(id)) {
    return '$label inválido: Use um ID numérico.';
  }
  final number = int.tryParse(id);
  return number == null || number <= 0
      ? '$label inválido: Use um ID positivo.'
      : null;
}
