import '../../app/app_effect.dart';

abstract interface class PullRequestPublisher {
  AppEffect<List<PublishedPullRequest>> publish(
    List<String> targets,
    PullRequestDraft draft, {
    String Function(String target)? reviewerForTarget,
  });
}

final class PullRequestDraft {
  const PullRequestDraft({
    required this.title,
    required this.description,
    this.workItemIds = const [],
  });

  final String title;
  final String description;
  final List<String> workItemIds;
}

final class PublishedPullRequest {
  const PublishedPullRequest({
    required this.target,
    required this.id,
    this.url,
  });

  final String target;
  final int id;
  final String? url;
}
