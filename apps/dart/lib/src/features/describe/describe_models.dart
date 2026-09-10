import '../../application/config/config_models.dart';
import '../../domain/change_context.dart';

final class DescribePrompt {
  const DescribePrompt({
    required this.context,
    required this.targets,
    this.workItemId = '',
  });

  final ChangeContext context;
  final List<String> targets;
  final String workItemId;
}

final class DescribePreparation {
  const DescribePreparation({
    required this.config,
    required this.context,
    required this.targets,
    required this.system,
    required this.prompt,
    required this.interactive,
    this.workItemId,
  });

  final Config config;
  final ChangeContext context;
  final List<String> targets;
  final String? workItemId;
  final String system;
  final String prompt;
  final bool interactive;
}
