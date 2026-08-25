import '../../app/app_effect.dart';
import '../../domain/change_context.dart';

abstract interface class ChangeContextReader {
  AppEffect<ChangeContext> collect([
    String? sourceBranch,
    String? baselineCommit,
  ]);
}
