import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import '../../features/describe/pull_request_publisher.dart';
import 'client.dart';
import 'execution.dart';
import 'identities.dart';
import 'pull_requests.dart';
import 'types.dart';

final class AzurePullRequestPublisherLive implements PullRequestPublisher {
  const AzurePullRequestPublisherLive();

  @override
  AppEffect<List<PublishedPullRequest>> publish(
    List<String> targets,
    PullRequestDraft draft, {
    String Function(String target)? reviewerForTarget,
  }) => Effect.result((use) async {
    final execution = use<AzureExecutionContext>();
    final remote = execution.change.remote;
    if (remote == null) {
      use.fail(
        const AzureConfigurationError('Remote Azure DevOps não informado.'),
      );
    }
    final pullRequests = use<AzurePullRequestClient>();
    final repository = await use.unwrap(
      pullRequests.getRepository(remote.project, remote.repository),
    );
    if (repository.id.trim().isEmpty) {
      use.fail(
        const AzurePayloadError(
          'Azure DevOps não retornou o ID do repositório.',
        ),
      );
    }

    final results = <PublishedPullRequest>[];
    final resolvedReviewers = <String, AzureIdentity>{};
    for (final target in targets) {
      final reviewer = (reviewerForTarget?.call(target) ?? '').trim();
      AzureIdentity? reviewerIdentity;
      if (reviewer.isNotEmpty) {
        reviewerIdentity = resolvedReviewers[reviewer];
        if (reviewerIdentity == null) {
          final identities = use<AzureIdentityClient>();
          reviewerIdentity = await use.unwrap(identities.resolve(reviewer));
          resolvedReviewers[reviewer] = reviewerIdentity;
        }
      }
      final request = CreatePullRequestInput(
        title: draft.title,
        description: draft.description,
        sourceRefName: 'refs/heads/${execution.change.branch}',
        targetRefName: 'refs/heads/$target',
        reviewers: reviewerIdentity == null ? null : [reviewerIdentity],
        workItemRefs: draft.workItemIds.isEmpty
            ? null
            : draft.workItemIds.map((id) => AzureResourceRef(id: id)).toList(),
      );
      final pullRequest = await use.unwrap(
        pullRequests.create(remote.project, repository.id, request),
      );
      results.add(
        PublishedPullRequest(
          target: target,
          id: pullRequest.pullRequestId,
          url: pullRequest.webUrl ?? pullRequest.url,
        ),
      );
    }
    return results;
  });
}
