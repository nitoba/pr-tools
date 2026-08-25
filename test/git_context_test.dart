import 'package:better_effect/better_effect.dart';
import 'package:pr_tools/src/application/change_context/change_context_reader.dart';
import 'package:pr_tools/src/application/process/process_runner.dart';
import 'package:pr_tools/src/app/app_effect.dart';
import 'package:pr_tools/src/domain/change_context.dart';
import 'package:pr_tools/src/infrastructure/git/change_context_reader_live.dart';
import 'package:test/test.dart';

void main() {
  test('resolves sprint targets and extracts work item IDs', () {
    const context = ChangeContext(
      branch: 'feature/11763-description',
      sourceRef: 'feature/11763-description',
      baseBranch: 'sprint/42',
      sprintBranch: 'sprint/42',
      diff: '',
      diffOriginalLines: 0,
      log: '',
      workItemId: '11763',
      remote: RepositoryRemote(
        organization: 'org',
        project: 'project',
        repository: 'repo',
      ),
    );

    expect(resolveTargets(context, ['sprint', 'dev']), ['sprint/42', 'dev']);
    expect(resolveTargets(context, []), ['sprint/42', 'dev']);
    expect(workItemFromBranch(context.branch), '11763');
  });

  test('resolves the Git service from its module binding', () async {
    final module = Module([
      .instance<ProcessRunner>(_ProcessRunnerFake()),
      .provide<ChangeContextReader>(ChangeContextReaderLive.new),
    ]);
    final result = await module.run(_collectContext());
    final context = result.fold(
      (value) => value,
      (failure) => fail(failure.message),
    );

    expect(context.branch, 'feature/11763-description');
    expect(context.baseBranch, 'sprint/42');
    expect(context.remote?.repository, 'repo');
  });
}

AppEffect<ChangeContext> _collectContext() => .result((use) async {
  final reader = use<ChangeContextReader>();
  return use.unwrap(reader.collect());
});

final class _ProcessRunnerFake implements ProcessRunner {
  final _outputs = <String, ProcessResult>{
    'branch --show-current': const ProcessResult(
      exitCode: 0,
      stdout: 'feature/11763-description\n',
      stderr: '',
    ),
    'rev-parse --verify feature/11763-description': const ProcessResult(
      exitCode: 0,
      stdout: '',
      stderr: '',
    ),
    'branch -r': const ProcessResult(
      exitCode: 0,
      stdout: '  origin/sprint/42\n',
      stderr: '',
    ),
    'rev-parse --verify sprint/42': const ProcessResult(
      exitCode: 0,
      stdout: '',
      stderr: '',
    ),
    'diff sprint/42...feature/11763-description': const ProcessResult(
      exitCode: 0,
      stdout: 'diff --git a/a b/a\n',
      stderr: '',
    ),
    'log sprint/42...feature/11763-description --oneline --max-count=50':
        const ProcessResult(exitCode: 0, stdout: 'abc change\n', stderr: ''),
    'remote get-url origin': const ProcessResult(
      exitCode: 0,
      stdout: 'https://dev.azure.com/org/project/_git/repo\n',
      stderr: '',
    ),
  };

  @override
  AppEffect<ProcessResult> run(
    String command,
    List<String> arguments, {
    Duration? timeout,
    String? input,
  }) => Effect.succeed(
    _outputs[arguments.join(' ')] ??
        const ProcessResult(
          exitCode: 1,
          stdout: '',
          stderr: 'unexpected git command',
        ),
  );
}
