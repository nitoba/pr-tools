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
  AppEffect<ChangeContext> collect([
    String? sourceBranch,
    String? baselineCommit,
  ]) => .result((use) async {
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
    final comparison = baselineCommit == null
        ? await _resolveComparison(use, baseBranch, sourceRef)
        : await _resolveBaseline(use, baselineCommit, sourceRef);
    final diff = await _collectDiff(use, comparison);
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
      comparison.logRange,
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

  Future<_GitComparison> _resolveComparison(
    EffectContext<AppFailure> use,
    String baseBranch,
    String sourceRef,
  ) async {
    final cherry = await _run(use, ['cherry', baseBranch, sourceRef]);
    if (!cherry.ok) {
      return _GitComparison.mergeBase(baseBranch, sourceRef);
    }
    final lines = cherry.stdout
        .split('\n')
        .map((line) => line.trim())
        .where((line) => line.isNotEmpty)
        .toList();
    if (!lines.any((line) => line.startsWith('- '))) {
      return _GitComparison.mergeBase(baseBranch, sourceRef);
    }
    final firstUnmerged = lines
        .map(_cherryCommit)
        .whereType<String>()
        .firstOrNull;
    if (firstUnmerged == null) {
      return _GitComparison.tree(sourceRef, sourceRef);
    }
    return _GitComparison.tree('$firstUnmerged^', sourceRef);
  }

  Future<_GitComparison> _resolveBaseline(
    EffectContext<AppFailure> use,
    String baselineCommit,
    String sourceRef,
  ) async {
    final resolved = await _run(use, [
      'rev-parse',
      '--verify',
      '$baselineCommit^{commit}',
    ]);
    if (!resolved.ok || resolved.stdout.trim().isEmpty) {
      use.fail(
        GitFailure(
          'O commit do PR anterior ($baselineCommit) não está disponível localmente.',
        ),
      );
    }
    final commit = resolved.stdout.trim();
    final ancestor = await _run(use, [
      'merge-base',
      '--is-ancestor',
      commit,
      sourceRef,
    ]);
    if (!ancestor.ok) {
      use.fail(
        GitFailure(
          'O commit do PR anterior ($commit) não pertence ao histórico da branch $sourceRef. Faça fetch ou verifique se a branch foi recriada.',
        ),
      );
    }
    return _GitComparison.tree(commit, sourceRef);
  }

  Future<String> _collectDiff(
    EffectContext<AppFailure> use,
    _GitComparison comparison,
  ) async {
    final result = await _run(use, comparison.diffArguments);
    return result.ok ? result.stdout : '';
  }
}

final class _GitComparison {
  const _GitComparison.mergeBase(this.baseBranch, this.sourceRef)
    : _treeComparison = false;

  const _GitComparison.tree(this.baseBranch, this.sourceRef)
    : _treeComparison = true;

  final String baseBranch;
  final String sourceRef;
  final bool _treeComparison;

  List<String> get diffArguments => _treeComparison
      ? ['diff', baseBranch, sourceRef]
      : ['diff', '$baseBranch...$sourceRef'];

  String get logRange =>
      _treeComparison ? '$baseBranch..$sourceRef' : '$baseBranch...$sourceRef';
}

String? _cherryCommit(String line) {
  if (!line.startsWith('+ ')) return null;
  return line.substring(2).trim().split(RegExp(r'\s+')).firstOrNull;
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
