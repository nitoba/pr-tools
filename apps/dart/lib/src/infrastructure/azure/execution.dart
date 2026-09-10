import 'package:better_effect/better_effect.dart';
import 'package:dio/dio.dart';

import '../../application/config/config_models.dart';
import '../../application/change_context/merged_pull_request_lookup.dart';
import '../../features/describe/pull_request_publisher.dart';
import '../../features/test_card/test_card_repository.dart';
import '../../domain/change_context.dart';
import 'client.dart';
import 'identities.dart';
import 'merged_pull_request_lookup.dart';
import 'pull_request_publisher.dart';
import 'pull_requests.dart';
import 'test_card_repository.dart';
import 'work_items.dart';

/// Creates the request-local Azure bindings for one operation.
///
/// The returned module must be installed with `Runtime.runWith` (or
/// `Runtime.executeWith`) around the operation. Keeping the options and HTTP
/// client in the execution scope prevents credentials and clients from being
/// shared across concurrent requests.
Module azureRequestModule(Config config, ChangeContext context) => Module([
  .instance<AzureExecutionContext>(AzureExecutionContext(config, context)),
  .instance<AzureClientOptions>(
    AzureClientOptions(
      pat: config.azurePat,
      organization: context.remote?.organization ?? '',
    ),
  ),
  .resource<Dio>(
    acquire: (_) => Dio(),
    release: (dio, _) => dio.close(force: true),
  ),
  .provide<AzureHttp>(AzureHttpLive.new),
  .provide<AzureDevOpsClient>(AzureDevOpsClientLive.new),
  .provide<AzurePullRequestClient>(AzurePullRequestClientLive.new),
  .provide<AzureIdentityClient>(AzureIdentityClientLive.new),
  .provide<MergedPullRequestLookup>(AzureMergedPullRequestLookup.new),
  .provide<AzureWorkItemClient>(AzureWorkItemClientLive.new),
  .provide<PullRequestPublisher>(AzurePullRequestPublisherLive.new),
  .provide<TestCardRepository>(AzureTestCardRepositoryLive.new),
]);

final class AzureExecutionContext {
  const AzureExecutionContext(this.config, this.change);

  final Config config;
  final ChangeContext change;
}
