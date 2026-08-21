import 'package:terminice/terminice.dart' as terminice_ui;

import '../../application/terminal/terminal_ports.dart';

final class PromptPortLive implements PromptPort {
  const PromptPortLive();

  terminice_ui.Terminice get _ui => terminice_ui.terminice.autoFallback;

  @override
  String? text({
    required String message,
    String? initialValue,
    String? placeholder,
    PromptValidator? validate,
  }) {
    return _ui.text(
      message,
      placeholder: initialValue ?? placeholder,
      required: false,
      validator: validate == null ? null : (value) => validate(value),
    );
  }

  @override
  String? password({required String message}) =>
      _ui.password(message, required: false);

  @override
  String? select({
    required String message,
    required List<PromptOption> options,
    String? initialValue,
  }) {
    if (options.isEmpty) return null;
    final selected = _ui.searchSelector(
      options: [for (final option in options) option.label],
      prompt: message,
    );
    if (selected.isEmpty) return null;
    final label = selected.first;
    for (final option in options) {
      if (option.label == label) return option.value;
    }
    return null;
  }
}

final class TerminalOutputLive implements TerminalOutput {
  const TerminalOutputLive();

  @override
  void write(String message) =>
      terminice_ui.terminice.autoFallback.log(message);

  @override
  void writeError(String message) =>
      terminice_ui.terminice.autoFallback.error(message);
}

final class TerminiceProgress implements ProgressReporter {
  const TerminiceProgress();

  terminice_ui.Terminice get _ui => terminice_ui.terminice.autoFallback;

  @override
  void error(String message) => _ui.error(message);

  @override
  void message(String message) => _ui.info(message);

  @override
  void start(String message) => _ui.info(message);

  @override
  void stop(String message) => _ui.success(message);
}
