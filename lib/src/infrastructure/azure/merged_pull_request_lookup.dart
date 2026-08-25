import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../application/change_context/merged_pull_request_lookup.dart';
import '../../domain/change_context.dart';
import 'execution.dart';
import 'pull_requests.dart';

final class AzureMergedPullRequestLookup implements MergedPullRequestLookup {
  const AzureMergedPullRequestLookup();

  @override
  AppEffect<MergedPullRequestLookupResult> findLatest({
    required RepositoryRemote? remote,
    required String sourceBranch,
    required List<String> targetBranches,
  }) => Effect.result((use) async {
    final execution = use<AzureExecutionContext>();
    if (remote == null || execution.config.azurePat.trim().isEmpty) {
      return const MergedPullRequestLookupResult();
    }
    final sourceRefName = 'refs/heads/$sourceBranch';
    final requests = use<AzurePullRequestClient>();
    final candidates = <MergedPullRequest>[];
    final seenTargets = <String>{};
    for (final targetBranch in targetBranches) {
      if (!seenTargets.add(targetBranch)) continue;
      final targetRefName = 'refs/heads/$targetBranch';
      final pullRequests = await use.unwrap(
        requests.completed(
          remote.project,
          remote.repository,
          sourceRefName,
          targetRefName,
        ),
      );
      for (final pullRequest in pullRequests) {
        final sourceCommit =
            pullRequest.lastMergeSourceCommit?.commitId.trim() ?? '';
        if (sourceCommit.isEmpty) continue;
        candidates.add(
          MergedPullRequest(
            id: pullRequest.pullRequestId,
            targetBranch: targetBranch,
            sourceCommit: sourceCommit,
            closedAt: pullRequest.closedDate,
          ),
        );
      }
    }
    candidates.sort(_compareByClosedDate);
    return MergedPullRequestLookupResult(value: candidates.firstOrNull);
  });
}

int _compareByClosedDate(MergedPullRequest left, MergedPullRequest right) {
  final leftDate =
      left.closedAt ?? DateTime.fromMillisecondsSinceEpoch(0, isUtc: true);
  final rightDate =
      right.closedAt ?? DateTime.fromMillisecondsSinceEpoch(0, isUtc: true);
  final dateComparison = rightDate.compareTo(leftDate);
  return dateComparison == 0 ? right.id.compareTo(left.id) : dateComparison;
}
