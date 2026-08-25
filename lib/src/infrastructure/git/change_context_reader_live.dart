import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../../application/change_context/change_context_reader.dart';
import '../../application/process/process_runner.dart';
import '../../domain/change_context.dart';

const _maxDiffLines = 8000;

final class ChangeContextReaderLive implements ChangeContextReader {
  const ChangeContextReaderLive();

  @override
  AppEffect<ChangeContext> collect([String? sourceBranch]) => .result((
    use,
  ) async {
    final currentBranch = _requireSuccess(
      await _run(use, ['branch', '--show-current']),
      'Não é um repositório git',
      use,
    ).stdout.trim();
    final branch = sourceBranch ?? currentBranch;
    if (branch.isEmpty) {
      use.fail(
        const GitFailure(
          'Branch não determinada (detached HEAD). Use --source.',
        ),
      );
    }
    if (const {'dev', 'main', 'master'}.contains(branch)) {
      use.fail(GitFailure('A branch de origem ($branch) é uma branch base.'));
    }
    final sourceRef = await use.unwrap(_resolveRef(branch));
    if (sourceRef.isEmpty) {
      use.fail(
        GitFailure("Branch '$branch' não encontrada localmente ou em origin."),
      );
    }

    final sprintBranch = await use.unwrap(_latestSprintBranch());
    final baseBranch = await use.unwrap(_detectBaseBranch(sprintBranch));
    final diff = await use.unwrap(_collectDiff(baseBranch, sourceRef));
    if (diff.isEmpty) {
      use.fail(
        GitFailure('Nenhuma alteração encontrada em relação a $baseBranch.'),
      );
    }
    final diffLines = diff.split('\n');
    final truncatedDiff = diffLines.length > _maxDiffLines
        ? '${diffLines.take(_maxDiffLines).join('\n')}\n\n[diff truncado: ${diffLines.length} -> $_maxDiffLines linhas]'
        : diff;
    final log = await _run(use, [
      'log',
      '$baseBranch...$sourceRef',
      '--oneline',
      '--max-count=50',
    ]);
    final remote = await _run(use, ['remote', 'get-url', 'origin']);
    final remoteContext = _parseRemote(remote.ok ? remote.stdout.trim() : '');
    return ChangeContext(
      branch: branch,
      sourceRef: sourceRef,
      baseBranch: baseBranch,
      sprintBranch: sprintBranch,
      diff: truncatedDiff,
      diffOriginalLines: diffLines.length,
      log: log.ok ? log.stdout : '(log não disponível)',
      workItemId: workItemFromBranch(branch),
      remote: remoteContext,
    );
  });

  AppEffect<String> _latestSprintBranch() => .result((use) async {
    final branches = await _run(use, ['branch', '-r']);
    final sprints =
        branches.stdout
            .split('\n')
            .map((value) => value.trim().replaceFirst(RegExp(r'^origin/'), ''))
            .map(_sprint)
            .whereType<({String branch, int number})>()
            .toList()
          ..sort((left, right) => right.number.compareTo(left.number));
    return sprints.isEmpty ? '' : sprints.first.branch;
  });

  AppEffect<String> _detectBaseBranch(String sprintBranch) =>
      .result((use) async {
        for (final candidate in [sprintBranch, 'dev', 'main', 'master']) {
          if (candidate.isEmpty) {
            continue;
          }
          final resolved = await use.unwrap(_resolveRef(candidate));
          if (resolved.isNotEmpty) {
            return resolved;
          }
        }
        use.fail(
          const GitFailure(
            'Branch base não encontrada. Esperado dev, main, master ou sprint/<número>.',
          ),
        );
      });

  AppEffect<String> _resolveRef(String branch) => .result((use) async {
    for (final candidate in [branch, 'origin/$branch']) {
      final result = await _run(use, ['rev-parse', '--verify', candidate]);
      if (result.ok) return candidate;
    }
    return '';
  });

  AppEffect<String> _collectDiff(String baseBranch, String sourceRef) =>
      .result((use) async {
        for (final arguments in [
          ['diff', '$baseBranch...$sourceRef'],
          ['diff', '$baseBranch..$sourceRef'],
          ['diff', baseBranch, sourceRef],
        ]) {
          final result = await _run(use, arguments);
          if (result.ok && result.stdout.isNotEmpty) return result.stdout;
        }
        return '';
      });
}

Future<ProcessResult> _run(
  EffectContext<AppFailure> use,
  List<String> arguments,
) => use.unwrap(
  use<ProcessRunner>()
      .run('git', arguments)
      .mapError(
        (failure) => GitFailure('Falha ao executar git: ${failure.message}'),
      ),
);

ProcessResult _requireSuccess(
  ProcessResult output,
  String message,
  EffectContext<AppFailure> use,
) {
  if (!output.ok) {
    use.fail(
      GitFailure(
        '$message${output.stderr.isEmpty ? '' : ': ${output.stderr.trim()}'}',
      ),
    );
  }
  return output;
}

({String branch, int number})? _sprint(String branch) {
  final match = RegExp(r'^sprint/(\d+)(?:$|[-/].*)').firstMatch(branch);
  final fullMatch = match?.group(0);
  final number = int.tryParse(match?.group(1) ?? '');
  return fullMatch == null || number == null
      ? null
      : (branch: fullMatch, number: number);
}

RepositoryRemote? _parseRemote(String remote) {
  final normalized = remote.replaceFirst(RegExp(r'\.git$'), '');
  final ssh = RegExp(
    r'(?:^|@)ssh\.dev\.azure\.com:v3/([^/]+)/([^/]+)/([^/]+)',
    caseSensitive: false,
  ).firstMatch(normalized);
  final modern = RegExp(
    r'dev\.azure\.com/([^/]+)/([^/]+)/_git/([^/]+)',
    caseSensitive: false,
  ).firstMatch(normalized);
  final legacy = RegExp(
    r'([^/]+)\.visualstudio\.com/([^/]+)/_git/([^/]+)',
    caseSensitive: false,
  ).firstMatch(normalized);
  final match = ssh ?? modern ?? legacy;
  if (match == null) {
    return null;
  }
  final organization = Uri.decodeComponent(match.group(1)!);
  final project = Uri.decodeComponent(match.group(2)!);
  final repository = Uri.decodeComponent(match.group(3)!);
  return organization.isEmpty || project.isEmpty || repository.isEmpty
      ? null
      : RepositoryRemote(
          organization: organization,
          project: project,
          repository: repository,
        );
}
