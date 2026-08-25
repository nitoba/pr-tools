import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../app/cli_options.dart';
import '../../application/change_context/change_context_reader.dart';
import '../../application/config/config_models.dart';
import '../../application/config/config_service.dart';
import '../../application/config/config_validation.dart';
import '../../application/process/process_runner.dart';
import '../../domain/change_context.dart';
import 'doctor_models.dart';

const doctorCommandTimeout = Duration(seconds: 5);

void _addProbe(
  List<DoctorCheck> checks,
  String component,
  DoctorProbeResult probe,
  String scope,
) {
  if (probe.ok) {
    checks.add(
      check(
        doctorOk,
        component,
        'PAT consegue consultar o Azure DevOps; escrita não foi testada.',
      ),
    );
    return;
  }
  final detail = probe.status == null
      ? 'Não foi possível consultar o Azure DevOps: ${probe.error ?? 'erro desconhecido.'}'
      : 'Azure DevOps respondeu HTTP ${probe.status}.';
  final fix = probe.status == 401 || probe.status == 403
      ? 'Revise o PAT e conceda o escopo Azure DevOps “$scope”.'
      : 'Confirme a rede, a organização/projeto do remote e repita `prt doctor`.';
  checks.add(check(doctorFailure, component, detail, fix));
}

String _cleanLine(String value) =>
    _cleanOutput(value)
        .split('\n')
        .map((line) => line.trim())
        .firstWhere((line) => line.isNotEmpty, orElse: () => '');

String _cleanOutput(String value) => value
    .replaceAll(RegExp('${String.fromCharCode(27)}\\[[0-?]*[ -/]*[@-~]'), '')
    .replaceAll('\r', '')
    .trim();

Future<bool> _exists(EffectContext<AppFailure> use, String path) async {
  final result = await use.unwrap(
    use<ConfigFileSystem>().exists(path).either(),
  );
  return result.fold((value) => value, (_) => false);
}

Future<ProcessResult> _git(
  EffectContext<AppFailure> use,
  List<String> arguments,
) => _process(use, 'git', arguments);

Future<void> _inspectAzure(
  EffectContext<AppFailure> use,
  Config config,
  RepositoryRemote? remote,
  List<DoctorCheck> checks,
) async {
  if (config.azurePat.trim().isEmpty) {
    checks.add(
      check(
        doctorFailure,
        'Azure DevOps PAT',
        'PAT não configurado; PRs e Test Cases não poderão ser publicados.',
        'Execute `prt init` ou defina AZURE_PAT/AZURE_DEVOPS_PAT.',
      ),
    );
    return;
  }
  checks.add(
    check(doctorOk, 'Azure DevOps PAT', 'PAT configurado (valor oculto).'),
  );
  if (remote == null) {
    checks.add(
      check(
        doctorWarning,
        'Azure DevOps APIs',
        'PAT disponível, mas o remote Azure DevOps não pôde ser identificado; as permissões não foram testadas.',
        'Configure um remote Azure DevOps válido e repita `prt doctor`.',
      ),
    );
    return;
  }
  final result = await use.unwrap(
    use<DoctorAzureProbe>().probe(config, remote).either(),
  );
  result.fold(
    (value) {
      _addProbe(checks, 'Azure Code API', value.repository, 'Code Read');
      _addProbe(
        checks,
        'Azure Work Items API',
        value.workItems,
        'Work Items Read',
      );
    },
    (failure) {
      checks.add(
        check(
          doctorFailure,
          'Azure DevOps APIs',
          failure.message,
          'Confirme a rede, a organização/projeto do remote e repita `prt doctor`.',
        ),
      );
    },
  );
}

Future<bool> _inspectCodex(
  EffectContext<AppFailure> use,
  Config config,
  List<DoctorCheck> checks,
) async {
  final executable = await _process(use, 'codex', ['--version']);
  if (!executable.ok) {
    checks.add(
      check(
        doctorWarning,
        'Codex CLI',
        'O executável `codex` não foi encontrado ou não iniciou.',
        'Instale o Codex CLI e confirme que `codex` está no PATH.',
      ),
    );
    return false;
  }
  checks.add(
    check(
      doctorOk,
      'Codex CLI',
      '${_cleanLine(executable.stdout)} · modelo ${config.codexModel} · thinking ${config.codexReasoning}.',
    ),
  );
  final login = await _process(use, 'codex', ['login', 'status']);
  if (!login.ok) {
    checks.add(
      check(
        doctorWarning,
        'Autenticação Codex',
        _cleanLine(login.stderr).isEmpty
            ? 'O Codex não está autenticado.'
            : _cleanLine(login.stderr),
        'Execute `codex login` e repita `prt doctor`.',
      ),
    );
    return false;
  }
  checks.add(check(doctorOk, 'Autenticação Codex', _cleanLine(login.stdout)));
  return true;
}

Future<bool> _inspectCompatible(
  EffectContext<AppFailure> use,
  Config config,
  List<DoctorCheck> checks,
) async {
  final base = config.baseUrl
      .trim()
      .replaceFirst(RegExp(r'/+$'), '')
      .replaceFirst(RegExp(r'/chat/completions$'), '');
  final uri = Uri.tryParse(base);
  if (uri == null ||
      !const {'http', 'https'}.contains(uri.scheme) ||
      uri.host.isEmpty) {
    checks.add(
      check(
        doctorWarning,
        'OpenAI-compatible URL',
        'Base URL inválida: ${config.baseUrl}.',
        'Execute `prt init` e informe uma URL HTTP/HTTPS compatível.',
      ),
    );
    return false;
  }
  final hasKey = config.apiKey.trim().isNotEmpty;
  checks.add(
    check(
      hasKey ? doctorOk : doctorWarning,
      'OpenAI-compatible API key',
      hasKey
          ? 'API key configurada (valor oculto).'
          : 'API key não configurada; alguns endpoints locais não exigem uma.',
      hasKey ? null : 'Configure a API key em `prt init` se o endpoint exigir autenticação.',
    ),
  );
  final response = await use.unwrap(
    use<DoctorHttpClient>()
        .get(
          '${uri.toString().replaceFirst(RegExp(r'/$'), '')}/models',
          headers: hasKey
              ? {'Authorization': 'Bearer ${config.apiKey}'}
              : const {},
        )
        .either(),
  );
  return response.fold(
    (value) {
      if (value.status == 401 || value.status == 403) {
        checks.add(
          check(
            doctorWarning,
            'OpenAI-compatible endpoint',
            'O endpoint respondeu HTTP ${value.status}; a autenticação foi rejeitada.',
            'Revise a API key e a Base URL em `prt init`.',
          ),
        );
        return false;
      }
      if (value.status >= 500 || value.status == 0) {
        checks.add(
          check(
            doctorWarning,
            'OpenAI-compatible endpoint',
            'O endpoint está acessível, mas respondeu HTTP ${value.status}.',
            'Verifique se o serviço está ativo e tente novamente.',
          ),
        );
        return false;
      }
      checks.add(
        value.status == 404
            ? check(
                doctorWarning,
                'OpenAI-compatible endpoint',
                'A URL respondeu, mas não expõe /models; a rota de geração pode ainda funcionar.',
                'Confirme se a Base URL termina na raiz compatível, normalmente /v1.',
              )
            : check(
                doctorOk,
                'OpenAI-compatible endpoint',
                'Endpoint acessível (HTTP ${value.status}).',
              ),
      );
      return true;
    },
    (failure) {
      checks.add(
        check(
          doctorWarning,
          'OpenAI-compatible endpoint',
          'Não foi possível alcançar $base. ${failure.message}',
          'Confirme a Base URL, a rede e se o serviço está em execução.',
        ),
      );
      return false;
    },
  );
}

Future<void> _inspectConfiguration(
  EffectContext<AppFailure> use,
  Config config,
  List<DoctorCheck> checks,
) async {
  final runtime = use<ConfigRuntime>();
  final configExists = await _exists(use, runtime.paths.configFile);
  final envExists = await _exists(use, runtime.paths.envFile);
  final templateExists = await _exists(use, runtime.paths.templateFile);
  if (configExists || envExists) {
    checks.add(
      check(
        doctorOk,
        'Configuração local',
        '${configExists ? runtime.paths.configFile : runtime.paths.envFile} carregado.',
      ),
    );
  } else {
    checks.add(
      check(
        doctorWarning,
        'Configuração local',
        'Nenhum arquivo de configuração foi criado; defaults estão sendo usados.',
        'Execute `prt init` para salvar PAT, reviewers, provider e modelos.',
      ),
    );
  }
  checks.add(
    templateExists
        ? check(
            doctorOk,
            'Template de prompt',
            'Template personalizado encontrado.',
          )
        : check(
            doctorWarning,
            'Template de prompt',
            'O template padrão embutido será usado.',
            'Execute `prt init` para criar o template editável.',
          ),
  );
  checks.add(
    config.providers.isEmpty
        ? check(
            doctorFailure,
            'Providers configurados',
            'A lista de providers está vazia.',
            'Execute `prt init` e escolha pelo menos um provider.',
          )
        : check(
            doctorOk,
            'Providers configurados',
            config.providers.join(', '),
          ),
  );
  _inspectEmail('Reviewer de dev', config.reviewerDev, checks);
  _inspectEmail('Reviewer de sprint', config.reviewerSprint, checks);
  _inspectEmail('Reviewer do Test Case', config.testAssignedTo, checks);
  checks.add(
    config.testAreaPath.trim().isEmpty
        ? check(
            doctorWarning,
            'Defaults do Test Case',
            'AreaPath não configurado.',
            'Informe o AreaPath durante a criação ou configure-o em `prt init`.',
          )
        : check(
            doctorOk,
            'Defaults do Test Case',
            'AreaPath: ${config.testAreaPath}.',
          ),
  );
}

void _inspectEmail(String component, String value, List<DoctorCheck> checks) {
  if (value.trim().isEmpty) {
    checks.add(
      check(
        doctorWarning,
        component,
        'Email não configurado; a confirmação solicitará o reviewer/responsável.',
        'Execute `prt init` ou informe o email durante o fluxo interativo.',
      ),
    );
  } else if (validateOptionalEmail(value) != null) {
    checks.add(
      check(
        doctorWarning,
        component,
        'O valor configurado não parece ser um email válido.',
        'Execute `prt init` e informe um email Azure DevOps válido.',
      ),
    );
  } else {
    checks.add(check(doctorOk, component, 'Email configurado.'));
  }
}

Future<RepositoryRemote?> _inspectGit(
  EffectContext<AppFailure> use,
  String? sourceBranch,
  List<DoctorCheck> checks,
) async {
  final version = await _git(use, ['--version']);
  if (!version.ok) {
    checks.add(
      check(
        doctorFailure,
        'Git',
        'O executável git não está disponível.',
        'Instale o Git e abra um novo terminal antes de executar `prt doctor`.',
      ),
    );
    return null;
  }
  checks.add(check(doctorOk, 'Git', _cleanLine(version.stdout)));

  final root = await _git(use, ['rev-parse', '--show-toplevel']);
  if (!root.ok) {
    checks.add(
      check(
        doctorFailure,
        'Repositório Git',
        'O diretório atual não está dentro de um repositório Git.',
        'Entre no clone do projeto usado para gerar o contexto.',
      ),
    );
    return null;
  }
  checks.add(
    check(
      doctorOk,
      'Repositório Git',
      'Projeto detectado em ${root.stdout.trim()}.',
    ),
  );

  final branch = await _git(use, ['branch', '--show-current']);
  final branchName = branch.stdout.trim();
  if (branchName.isEmpty) {
    checks.add(
      check(
        doctorWarning,
        'Branch de trabalho',
        'O Git está em detached HEAD.',
        'Mude para uma branch de trabalho ou use `--source <branch>`.',
      ),
    );
  } else if (const {'dev', 'main', 'master'}.contains(branchName)) {
    checks.add(
      check(
        doctorWarning,
        'Branch de trabalho',
        'A branch atual ($branchName) é uma branch base.',
        'Mude para a branch da alteração antes de gerar o contexto.',
      ),
    );
  } else {
    checks.add(check(doctorOk, 'Branch de trabalho', branchName));
  }

  final origin = await _git(use, ['remote', 'get-url', 'origin']);
  RepositoryRemote? remote;
  if (!origin.ok || origin.stdout.trim().isEmpty) {
    checks.add(
      check(
        doctorFailure,
        'Remote origin',
        'Nenhum remote origin foi encontrado.',
        'Configure o remote Azure DevOps com `git remote add origin <url>`.',
      ),
    );
  } else {
    checks.add(check(doctorOk, 'Remote origin', origin.stdout.trim()));
  }

  final context = await use.unwrap(
    use<ChangeContextReader>().collect(sourceBranch).either(),
  );
  context.fold(
    (value) {
      remote = value.remote;
      checks.add(
        remote == null
            ? check(
                doctorFailure,
                'Remote Azure DevOps',
                'O origin atual não é Azure DevOps (${origin.stdout.trim()}).',
                'Aponte o origin para o repositório Azure DevOps.',
              )
            : check(doctorOk, 'Remote Azure DevOps', remoteLabel(remote!)),
      );
      checks.add(
        check(
          doctorOk,
          'Contexto de PR',
          '${value.diffOriginalLines} linhas de diff contra ${value.baseBranch}.',
        ),
      );
      checks.add(
        value.workItemId.isEmpty
            ? check(
                doctorWarning,
                'Work Item da branch',
                'Nenhum ID numérico foi encontrado no nome da branch.',
                'Use `--work-item <id>` ao gerar o PR ou Test Case.',
              )
            : check(doctorOk, 'Work Item da branch', '#${value.workItemId}.'),
      );
    },
    (failure) => checks.add(
      check(
        doctorFailure,
        'Contexto de PR/Test Case',
        failure.message,
        'Entre na branch da alteração ou use `--source <branch>`.',
      ),
    ),
  );
  return remote;
}

Future<bool> _inspectOpenCode(
  EffectContext<AppFailure> use,
  Config config,
  List<DoctorCheck> checks,
) async {
  final executable = await _process(use, 'opencode', ['--version']);
  if (!executable.ok) {
    checks.add(
      check(
        doctorWarning,
        'OpenCode CLI',
        'O executável `opencode` não foi encontrado ou não iniciou.',
        'Instale o OpenCode CLI e confirme que `opencode` está no PATH.',
      ),
    );
    return false;
  }
  checks.add(
    check(
      doctorOk,
      'OpenCode CLI',
      '${_cleanLine(executable.stdout)} · modelo ${config.opencodeModel} · thinking ${config.opencodeReasoning}.',
    ),
  );
  final auth = await _process(use, 'opencode', ['auth', 'list']);
  final output = _cleanOutput(auth.stdout).toLowerCase();
  final provider = config.opencodeModel.split('/').first.trim().toLowerCase();
  if (!auth.ok ||
      output.isEmpty ||
      output.contains('0 credentials') ||
      output.contains('no credentials')) {
    checks.add(
      check(
        doctorWarning,
        'Autenticação OpenCode',
        'Nenhuma credencial utilizável foi encontrada.',
        'Execute `opencode auth login` para o provider usado pelo modelo.',
      ),
    );
    return false;
  }
  if (provider.isNotEmpty && !output.contains(provider)) {
    checks.add(
      check(
        doctorWarning,
        'Autenticação OpenCode',
        'Não foi encontrada uma credencial clara para $provider.',
        'Execute `opencode auth login $provider` ou escolha outro modelo em `prt init`.',
      ),
    );
    return false;
  }
  checks.add(
    check(
      doctorOk,
      'Autenticação OpenCode',
      'Credencial compatível encontrada.',
    ),
  );
  return true;
}

Future<int> _inspectProviders(
  EffectContext<AppFailure> use,
  Config config,
  List<DoctorCheck> checks,
) async {
  var ready = 0;
  for (final provider in config.providers) {
    if (provider == 'codex' && await _inspectCodex(use, config, checks)) {
      ready++;
    }
    if (provider == 'opencode' && await _inspectOpenCode(use, config, checks)) {
      ready++;
    }
    if (provider == 'openai-compatible' &&
        await _inspectCompatible(use, config, checks)) {
      ready++;
    }
  }
  return ready;
}

Future<ProcessResult> _process(
  EffectContext<AppFailure> use,
  String command,
  List<String> arguments,
) async {
  final result = await use.unwrap(
    use<ProcessRunner>()
        .run(command, arguments, timeout: doctorCommandTimeout)
        .either(),
  );
  return result.fold(
    (value) => value,
    (failure) => ProcessResult(
      exitCode: 1,
      stdout: '',
      stderr: failure.message,
      error: failure.message,
    ),
  );
}

abstract interface class DoctorAzureProbe {
  AppEffect<DoctorAzureReport> probe(Config config, RepositoryRemote remote);
}

abstract interface class DoctorHttpClient {
  AppEffect<DoctorHttpResponse> get(
    String url, {
    required Map<String, String> headers,
    Duration timeout,
  });
}

final class DoctorHttpResponse {
  final int status;

  final String body;
  const DoctorHttpResponse({required this.status, required this.body});
}

abstract interface class DoctorService {
  AppEffect<DoctorReport> inspect(CliOptions options);
}

final class DoctorServiceLive implements DoctorService {
  const DoctorServiceLive();

  @override
  AppEffect<DoctorReport> inspect(CliOptions options) => Effect.result((
    use,
  ) async {
    final checks = <DoctorCheck>[];
    final remote = await _inspectGit(use, options.source, checks);
    final configResult = await use.unwrap(
      use<ConfigService>().load(options).either(),
    );
    final config = configResult.fold((value) => value, (failure) {
      checks.add(
        check(
          doctorFailure,
          'Configuração',
          failure.message,
          'Execute `prt init` ou corrija o arquivo de configuração indicado.',
        ),
      );
      return null;
    });

    if (config != null) {
      await _inspectConfiguration(use, config, checks);
      final ready = await _inspectProviders(use, config, checks);
      if (ready == 0) {
        checks.add(
          check(
            doctorFailure,
            'Providers de IA',
            'Nenhum provider configurado está pronto para gerar conteúdo.',
            'Instale/autentique um provider e execute `prt init` para selecioná-lo.',
          ),
        );
      } else {
        checks.add(
          check(
            doctorOk,
            'Providers de IA',
            '$ready provider(s) pronto(s) para geração.',
          ),
        );
      }
      await _inspectAzure(use, config, remote, checks);
    }
    return DoctorReport(checks);
  });
}
