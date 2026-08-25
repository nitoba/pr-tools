final class ChangeContext {
  const ChangeContext({
    required this.branch,
    required this.sourceRef,
    required this.baseBranch,
    required this.sprintBranch,
    required this.diff,
    required this.diffOriginalLines,
    required this.log,
    required this.workItemId,
    this.remote,
  });

  final String branch;
  final String sourceRef;
  final String baseBranch;
  final String sprintBranch;
  final String diff;
  final int diffOriginalLines;
  final String log;
  final String workItemId;
  final RepositoryRemote? remote;
}

final class RepositoryRemote {
  const RepositoryRemote({
    required this.organization,
    required this.project,
    required this.repository,
  });
  final String organization;
  final String project;
  final String repository;
}

String workItemFromBranch(String branch) {
  return RegExp(r'(?:^|[/_-])(\d+)(?:$|[/_-])').firstMatch(branch)?.group(1) ??
      '';
}

List<String> resolveTargets(ChangeContext context, List<String> requested) {
  if (requested.isNotEmpty) {
    return requested
        .map((target) => target == 'sprint' ? context.sprintBranch : target)
        .where((target) => target.isNotEmpty)
        .toList();
  }
  return [
    context.sprintBranch,
    'dev',
  ].where((target) => target.isNotEmpty).toList();
}
