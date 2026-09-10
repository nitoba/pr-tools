import '../../app/app_effect.dart';

final class ProcessResult {
  const ProcessResult({
    required this.exitCode,
    required this.stdout,
    required this.stderr,
    this.error,
  });

  final int exitCode;
  final String stdout;
  final String stderr;
  final String? error;

  bool get ok => exitCode == 0 && error == null;
}

abstract interface class ProcessRunner {
  AppEffect<ProcessResult> run(
    String command,
    List<String> arguments, {
    Duration? timeout,
    String? input,
  });
}
