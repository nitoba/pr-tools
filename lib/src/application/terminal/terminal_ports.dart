typedef PromptValidator = String? Function(String value);

final class PromptOption {
  const PromptOption({required this.value, required this.label, this.hint});

  final String value;
  final String label;
  final String? hint;
}

abstract interface class PromptPort {
  String? text({
    required String message,
    String? initialValue,
    String? placeholder,
    PromptValidator? validate,
  });

  String? password({required String message});

  String? select({
    required String message,
    required List<PromptOption> options,
    String? initialValue,
  });
}

abstract interface class TerminalOutput {
  void write(String message);

  void writeError(String message);
}

abstract interface class ProgressReporter {
  void start(String message);

  void message(String message);

  void stop(String message);

  void error(String message);
}
