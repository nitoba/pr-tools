import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../domain/change_context.dart';

final class MergedPullRequest {
  const MergedPullRequest({
    required this.id,
    required this.targetBranch,
    required this.sourceCommit,
    this.closedAt,
  });

  final int id;
  final String targetBranch;
  final String sourceCommit;
  final DateTime? closedAt;
}

final class MergedPullRequestLookupResult {
  const MergedPullRequestLookupResult({this.value});

  final MergedPullRequest? value;
}

abstract interface class MergedPullRequestLookup {
  AppEffect<MergedPullRequestLookupResult> findLatest({
    required RepositoryRemote? remote,
    required String sourceBranch,
    required List<String> targetBranches,
  });
}

final class NoopMergedPullRequestLookup implements MergedPullRequestLookup {
  const NoopMergedPullRequestLookup();

  @override
  AppEffect<MergedPullRequestLookupResult> findLatest({
    required RepositoryRemote? remote,
    required String sourceBranch,
    required List<String> targetBranches,
  }) => Effect.succeed(const MergedPullRequestLookupResult());
}
