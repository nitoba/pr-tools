typedef ProviderName = String;
typedef ReasoningLevel = String;

final class Config {
  const Config({
    required this.providers,
    required this.baseUrl,
    required this.compatibleModel,
    required this.compatibleReasoning,
    required this.codexModel,
    required this.codexReasoning,
    required this.opencodeModel,
    required this.opencodeReasoning,
    required this.azurePat,
    required this.reviewerDev,
    required this.reviewerSprint,
    required this.testAreaPath,
    required this.testAssignedTo,
    required this.testTeam,
    required this.testProgram,
    required this.apiKey,
    required this.template,
  });

  final List<ProviderName> providers;
  final String baseUrl;
  final String compatibleModel;
  final ReasoningLevel compatibleReasoning;
  final String codexModel;
  final ReasoningLevel codexReasoning;
  final String opencodeModel;
  final ReasoningLevel opencodeReasoning;
  final String azurePat;
  final String reviewerDev;
  final String reviewerSprint;
  final String testAreaPath;
  final String testAssignedTo;
  final String testTeam;
  final String testProgram;
  final String apiKey;
  final String template;
}

final class ConfigPaths {
  const ConfigPaths({
    required this.directory,
    required this.configFile,
    required this.envFile,
    required this.templateFile,
  });

  final String directory;
  final String configFile;
  final String envFile;
  final String templateFile;
}

final class ConfigInitialization {
  const ConfigInitialization({
    required this.paths,
    required this.interactive,
    required this.azurePatConfigured,
    this.cancelled = false,
  });

  const ConfigInitialization.cancelled({
    required this.paths,
    required this.interactive,
  }) : azurePatConfigured = false,
       cancelled = true;

  final ConfigPaths paths;
  final bool interactive;
  final bool azurePatConfigured;
  final bool cancelled;
}
