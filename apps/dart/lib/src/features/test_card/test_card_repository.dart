import '../../app/app_effect.dart';
import 'test_card_models.dart';

abstract interface class TestCardRepository {
  AppEffect<PullRequest> getPullRequest(int id);

  AppEffect<List<int>> getPullRequestWorkItemIds(int id);

  AppEffect<WorkItem> getWorkItem(int id);

  AppEffect<List<Change>> getPullRequestChanges(PullRequest pullRequest);

  AppEffect<List<int>> queryTestCaseIds();

  AppEffect<WorkItem> createTestCase(TestCardDraft input);

  AppEffect<TestCardUpdate> updateWorkItemToTestQa(
    int id, {
    num? effort,
    num? realEffort,
  });
}
