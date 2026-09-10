import '../../app/app_effect.dart';
import '../config/config_models.dart';
import 'description_generator.dart';
import 'description_models.dart';

abstract interface class DescriptionRewriter {
  AppEffect<GeneratedDescription> rewrite({
    required Config config,
    required String system,
    required String branch,
    required PrDescription description,
    DescriptionReporter? report,
  });
}
