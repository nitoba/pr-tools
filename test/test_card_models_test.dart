import 'package:pr_tools/src/features/test_card/test_card_models.dart';
import 'package:test/test.dart';

void main() {
  test('converts the checklist and expected results to Azure steps XML', () {
    final xml = buildTestCaseStepsXml('''
## Objetivo
Validar o login.

## Checklist de testes
- Abrir <a tela> & selecionar "Entrar".
- [ ] Informar usuário e senha.

## Resultado esperado
- A tela é exibida corretamente.
- O acesso é concedido.
''');

    expect(xml, '''<steps id="0" last="3">
  <step id="2" type="ValidateStep">
    <parameterizedString isformatted="true">&lt;DIV&gt;&lt;P&gt;Abrir &amp;lt;a tela&amp;gt; &amp;amp; selecionar &quot;Entrar&quot;.&lt;/P&gt;&lt;/DIV&gt;</parameterizedString>
    <parameterizedString isformatted="true">&lt;DIV&gt;&lt;P&gt;A tela é exibida corretamente.&lt;/P&gt;&lt;/DIV&gt;</parameterizedString>
    <description/>
  </step>
  <step id="3" type="ValidateStep">
    <parameterizedString isformatted="true">&lt;DIV&gt;&lt;P&gt;Informar usuário e senha.&lt;/P&gt;&lt;/DIV&gt;</parameterizedString>
    <parameterizedString isformatted="true">&lt;DIV&gt;&lt;P&gt;O acesso é concedido.&lt;/P&gt;&lt;/DIV&gt;</parameterizedString>
    <description/>
  </step>
</steps>''');
  });

  test('puts a shared expected result on the final step', () {
    final xml = buildTestCaseStepsXml('''
## Checklist de testes
- Abrir a tela.
- Confirmar a operação.

## Resultado esperado
A operação é concluída.
''');

    expect(xml, contains('<step id="2" type="ActionStep">'));
    expect(xml, contains('<step id="3" type="ValidateStep">'));
    expect(xml, isNot(contains('<step id="2" type="ValidateStep">')));
    expect(xml, contains('A operação é concluída.'));
  });

  test('includes generated steps in the Test Case creation input', () {
    const settings = TestCardSettings(
      areaPath: r'Project\QA',
      assignedTo: '',
      iterationPath: r'Project\Sprint 98',
      priority: 2,
      team: 'DevOps',
      program: 'Agrotrace',
    );
    const body = '''## Checklist de testes
1. Executar o fluxo principal.

## Resultado esperado
O fluxo termina com sucesso.
''';

    final input = buildCreateTestCaseInput(settings, 42, 'Validar fluxo', body);

    expect(input.descriptionHtml, body);
    expect(input.stepsXml, contains('<steps id="0" last="2">'));
    expect(input.stepsXml, contains('<step id="2" type="ValidateStep">'));
    expect(input.stepsXml, contains('Executar o fluxo principal.'));
    expect(input.stepsXml, contains('O fluxo termina com sucesso.'));
  });

  test('does not send an empty steps field when the checklist is missing', () {
    const settings = TestCardSettings(
      areaPath: '',
      assignedTo: '',
      iterationPath: '',
      priority: 2,
      team: 'DevOps',
      program: 'Agrotrace',
    );

    final input = buildCreateTestCaseInput(
      settings,
      42,
      'Sem checklist',
      '## Objetivo\nApenas descrição.',
    );

    expect(input.stepsXml, isNull);
  });
}
