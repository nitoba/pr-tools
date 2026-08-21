enum Command { desc, test, init, doctor }

extension type WorkItemId(String value) {}

final class CliOptions {
  const CliOptions({
    required this.command,
    required this.targets,
    required this.create,
    required this.noCreate,
    required this.dryRun,
    required this.raw,
    required this.copy,
    this.source,
    this.workItem,
    this.provider,
    this.model,
    this.baseUrl,
    this.apiKey,
    this.pr,
    this.areaPath,
    this.assignedTo,
    this.iterationPath,
    this.priority,
    this.team,
    this.program,
    this.examples,
  });

  final Command command;
  final String? source;
  final List<String> targets;
  final WorkItemId? workItem;
  final String? provider;
  final String? model;
  final String? baseUrl;
  final String? apiKey;
  final bool create;
  final bool noCreate;
  final WorkItemId? pr;
  final String? areaPath;
  final String? assignedTo;
  final String? iterationPath;
  final String? priority;
  final String? team;
  final String? program;
  final String? examples;
  final bool dryRun;
  final bool raw;
  final bool copy;
}

sealed class CliParse {
  const CliParse();
}

final class ParsedOptions extends CliParse {
  const ParsedOptions(this.options);

  final CliOptions options;
}

final class HelpRequested extends CliParse {
  const HelpRequested();
}

final class VersionRequested extends CliParse {
  const VersionRequested();
}

final class InvalidArguments extends CliParse {
  const InvalidArguments(this.message);

  final String message;
}
