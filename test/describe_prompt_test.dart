import 'package:pr_tools/src/features/describe/describe_prompt.dart';
import 'package:pr_tools/src/features/describe/describe_models.dart';
import 'package:pr_tools/src/domain/change_context.dart';
import 'package:test/test.dart';

void main() {
  test('builds the PR prompt with work item, log, and diff', () {
    const context = ChangeContext(
      branch: 'feature/42-login',
      sourceRef: 'feature/42-login',
      baseBranch: 'dev',
      sprintBranch: 'sprint/98',
      diff: 'diff --git a/login.ts b/login.ts',
      diffOriginalLines: 1,
      log: '42 Ajusta login',
      workItemId: '42',
      remote: RepositoryRemote(
        organization: 'acme',
        project: 'project',
        repository: 'repo',
      ),
    );
    const fence = '\u0060\u0060\u0060';
    const prompt = DescribePrompt(
      context: context,
      targets: ['dev', 'sprint/98'],
      workItemId: '42',
    );

    expect(buildDescribePrompt(prompt), '''## Contexto Git

**Branch:** feature/42-login
**Base branches alvo:** dev, sprint/98
**Work Item:** #42
### Git Log (commits desde a base)

$fence
42 Ajusta login
$fence

### Git Diff

${fence}diff
diff --git a/login.ts b/login.ts
$fence
''');
  });

  test('omits an empty work item line', () {
    const context = ChangeContext(
      branch: 'feature/login',
      sourceRef: 'feature/login',
      baseBranch: 'dev',
      sprintBranch: '',
      diff: 'change',
      diffOriginalLines: 1,
      log: 'commit',
      workItemId: '',
    );

    expect(buildPrompt(context, ['dev'], ''), isNot(contains('Work Item')));
  });
}
