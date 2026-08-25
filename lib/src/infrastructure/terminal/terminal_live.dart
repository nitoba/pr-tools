import 'dart:async';

import 'package:terminice/terminice.dart' as terminice_ui;

import '../../application/terminal/terminal_ports.dart';

final class PromptPortLive implements PromptPort {
  PromptPortLive(this._ui);

  final terminice_ui.Terminice _ui;

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
    final ordered = [
      if (initialValue != null)
        ...options.where((option) => option.value == initialValue),
      ...options.where((option) => option.value != initialValue),
    ];
    final selected = ordered.length <= 4
        ? _ui.choiceSelector(
            message,
            items: [
              for (final option in ordered)
                terminice_ui.ChoiceItem(option.label, subtitle: option.hint),
            ],
            columns: ordered.length == 1 ? 1 : 2,
          )
        : _ui.searchSelector(
            options: [for (final option in ordered) option.label],
            prompt: message,
            showSearch: true,
          );
    if (selected.isEmpty) return null;
    final label = selected.first;
    for (final option in ordered) {
      if (option.label == label) return option.value;
    }
    return null;
  }
}

final class TerminalOutputLive implements TerminalOutput {
  TerminalOutputLive(this._ui);

  final terminice_ui.Terminice _ui;

  @override
  void heading(String title, {String? detail}) {
    _ui.newline();
    _ui.info('prt · $title');
    if (detail != null && detail.isNotEmpty) _ui.detail(detail);
  }

  @override
  void write(String message) => _ui.log(message);

  @override
  void writeError(String message) => _ui.error(message);

  @override
  void info(String message) => _ui.info(message);

  @override
  void success(String message) => _ui.success(message);

  @override
  void warning(String message) => _ui.warn(message);

  @override
  void detail(String message) => _ui.detail(message);
}

final class TerminiceProgress implements ProgressReporter {
  TerminiceProgress(this._ui);

  final terminice_ui.Terminice _ui;
  terminice_ui.InlineSpinner? _spinner;
  Timer? _timer;
  var _frame = 0;

  bool get _usesRichOutput => !_ui.shouldUseFallback;

  @override
  void error(String message) {
    _stopSpinner();
    _ui.error(message);
  }

  @override
  void message(String message) {
    if (!_usesRichOutput) {
      _ui.detail(message);
      return;
    }
    _startSpinner(message);
  }

  @override
  void start(String message) {
    if (!_usesRichOutput) {
      _ui.info(message);
      return;
    }
    _startSpinner(message);
  }

  @override
  void stop(String message) {
    _stopSpinner();
    _ui.success(message);
  }

  void _startSpinner(String message) {
    _stopSpinner();
    _spinner = _ui.inlineSpinner(
      message,
      style: terminice_ui.SpinnerStyle.arcs,
    );
    _spinner!.show(_frame++);
    _timer = Timer.periodic(const Duration(milliseconds: 80), (_) {
      _spinner?.show(_frame++);
    });
  }

  void _stopSpinner() {
    _timer?.cancel();
    _timer = null;
    _spinner?.clear();
    _spinner = null;
  }
}

final prtTerminice = terminice_ui.terminice.ocean.verbose.autoFallback;
