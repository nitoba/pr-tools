base class AppFailure implements Exception {
  const AppFailure(this.message, this.exitCode);

  final String message;
  final int exitCode;
}

final class CliFailure extends AppFailure {
  const CliFailure(String message) : super(message, 64);
}

final class GitFailure extends AppFailure {
  const GitFailure(String message) : super(message, 1);
}

final class ProcessFailure extends AppFailure {
  const ProcessFailure(String message) : super(message, 1);
}
