import '../../app/app_effect.dart';
import '../../app/app_failure.dart';
import '../config/config_models.dart';
import 'description_models.dart';

typedef DescriptionReporter = void Function(
  ProviderName provider,
  String model,
);

abstract interface class DescriptionGenerator {
  AppEffect<GeneratedDescription> generate({
    required Config config,
    required String system,
    required String prompt,
    required String branch,
    DescriptionReporter? report,
  });
}

abstract interface class CompatibleDescriptionGenerator {
  AppEffect<PrDescription> generate({
    required Config config,
    required String system,
    required String prompt,
    required String branch,
  });
}

final class AiFailure extends AppFailure {
  const AiFailure(String message) : super(message, 1);
}
