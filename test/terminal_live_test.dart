import 'package:pr_tools/src/infrastructure/terminal/terminal_live.dart';
import 'package:terminice/testing.dart';
import 'package:test/test.dart';

void main() {
  test('renders terminal states through Terminice fallback', () async {
    final tester = TerminiceTester.nonInteractive();

    await tester.runAsync((_) async {
      final output = TerminalOutputLive(tester.terminice);
      output.heading('Doctor');
      output.info('Verificando configuração');
      output.success('Configuração pronta');
      output.warning('Token expira em breve');
      output.writeError('Remote indisponível');
      output.card('Descrição do PR', 'Linha 1\nLinha 2');

      final progress = TerminiceProgress(tester.terminice);
      progress.start('Gerando descrição');
      progress.stop('Descrição gerada');
    });

    final output = tester.output.plainText;
    expect(output, contains('prt · Doctor'));
    expect(output, contains('Verificando configuração'));
    expect(output, contains('Configuração pronta'));
    expect(output, contains('Token expira em breve'));
    expect(output, contains('Remote indisponível'));
    expect(output, contains('+- Descrição do PR -+'));
    expect(output, contains('| Linha 1 |'));
    expect(output, contains('Descrição gerada'));
  });

  test('renders the description card with the active rich theme', () async {
    final tester = TerminiceTester.interactive(base: prtTerminice);

    await tester.runAsync((_) async {
      TerminalOutputLive(tester.terminice).card('Descrição do PR', 'Linha 1');
    });

    final output = tester.output.plainText;
    expect(output, contains('╭─ Descrição do PR ─╮'));
    expect(output, contains('┊ Linha 1'));
    expect(output, contains('┊\n'));
  });
}
