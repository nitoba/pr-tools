---
name: dart-result-effect-refactor
description: Refatora profundamente projetos Dart ou Flutter existentes que já usam result_dart e better_effect, elevando o uso de falhas tipadas, composição de Result/Effect, DI via Module/Runtime, scopes, recursos, testabilidade, legibilidade e Dart moderno sem overengineering. Use quando o usuário pedir revisão, modernização, melhoria de DX/arquitetura, redução de boilerplate ou melhor aproveitamento dessas bibliotecas.
---

# Dart Result + Effect Refactor

Refatore código Dart/Flutter existente que já utiliza `result_dart` e `better_effect` para um estado idiomático, legível, seguro e coerente.

O objetivo não é maximizar a quantidade de abstrações ou APIs utilizadas. O objetivo é escolher a abstração correta para cada problema e deixar o código parecer **bom Dart**, não uma tradução de Effect TS.

## Princípios

Priorize, nesta ordem:

1. correção;
2. legibilidade;
3. type safety;
4. simplicidade;
5. coerência arquitetural;
6. boa DX;
7. redução de boilerplate.

Nunca sacrifique legibilidade apenas para usar uma feature moderna ou funcional.

## Fluxo obrigatório

### 1. Entenda o projeto antes de editar

Inspecione, conforme existirem:

- `pubspec.yaml` e `pubspec.lock`;
- versões instaladas de `result_dart` e `better_effect`;
- estrutura de diretórios;
- models, entities e value objects;
- failures/exceptions;
- repositories;
- services;
- use cases;
- view models/controllers/commands;
- clients HTTP, database e integrações externas;
- bootstrap;
- `Module` e `Runtime`;
- recursos gerenciados;
- testes.

Não assuma que a API da versão instalada é igual à versão mais recente. Confirme a versão do projeto e só utilize recursos disponíveis nela.

Para uma refatoração não trivial, leia também:

- `references/refactoring-rules.md`
- `references/transformation-patterns.md`

### 2. Faça um inventário dos padrões problemáticos

Mapeie todas as ocorrências relevantes antes ou durante a implementação. Procure especialmente por:

- `try/catch` manual repetido;
- `Exception` genérica como contrato de domínio;
- `isSuccess()`, `isError()`, `getOrNull()` ou `exceptionOrNull()` controlando fluxo;
- `Result<Result<...>>`;
- `Effect<Result<...>>` redundante;
- conversões repetidas `Future ↔ Result ↔ Effect`;
- `Effect` usado apenas como wrapper de `Future`;
- dependências transportadas por vários constructors sem necessidade estrutural;
- service locator/global singleton;
- Runtime criado repetidamente sem necessidade;
- `try/finally` manual para recursos que possuem ownership claro;
- operações independentes serializadas;
- `dynamic`, casts, `!`, `late` e nullability artificial;
- DTOs/classes temporárias que apenas agrupam alguns valores;
- tipos primitivos representando conceitos de domínio incompatíveis;
- helpers que duplicam APIs já existentes das bibliotecas;
- arquivos genéricos ou grandes demais;
- side effects escondidos em APIs aparentemente puras.

### 3. Defina o boundary correto

Use esta decisão como padrão:

| Necessidade | Abstração preferida |
| --- | --- |
| valor calculado com sucesso ou falha tipada | `ResultDart<A, E>` |
| operação lazy com falha tipada, dependências, async, contexto ou recursos | `Effect<A, E>` |
| async simples em boundary externo sem ganho de falha tipada/Runtime | `Future<A>` |
| operação sem valor de sucesso significativo | `Unit` |
| operação sem falha esperada | `Never` como canal de falha quando aplicável |

Não converta tudo para `Effect`. Não deixe `Future<Result<...>>` espalhado quando `Effect` modela melhor a operação.

### 4. Refatore completamente

Quando identificar um padrão ruim recorrente, corrija **todas as ocorrências relevantes**, não apenas um exemplo.

Não pare com instruções como “os demais arquivos podem seguir o mesmo padrão”. Faça a implementação.

Preserve comportamento, contratos públicos e regras de negócio sempre que possível.

### 5. Valide

Execute o script desta skill quando compatível:

```bash
./scripts/verify.sh
```

Ou execute manualmente os comandos equivalentes do projeto.

Não considere a refatoração concluída enquanto houver erros introduzidos pela alteração.

## Regras essenciais de `result_dart`

- Modele falhas esperadas explicitamente.
- Prefira `map`, `flatMap`, `mapError`, `filter`, `zip`, `flatten`, `recoverWhen`, `tap` e outras operações da versão instalada em vez de branching manual.
- Use `map` para transformação pura e `flatMap` quando a próxima etapa também retorna Result.
- Faça tradução de failures nos boundaries corretos com `mapError` ou equivalente.
- Recovery deve ser o mais localizado possível.
- Não use `tap` para esconder mutação ou side effects importantes.
- Não force Result em código onde um valor simples ou Effect representa melhor a semântica.

## Regras essenciais de `better_effect`

- Use `Effect` como descrição de execução, não apenas como wrapper de `Future<Result>`.
- Em fluxos imperativos naturais, prefira `Effect.result((use) async { ... })` quando disponível na versão instalada.
- Use os mecanismos nativos da biblioteca para compor/desembrulhar Results e Effects, como `use.unwrap`, `use.result` e `use.fail`, quando disponíveis.
- Separe falhas esperadas de defects/erros de programação.
- Traduza exceptions externas na fronteira da infraestrutura, não em todas as camadas.
- Use `Module` e `Runtime` com lifecycle explícito.
- Resolva dependências pelo contexto do Effect quando forem dependências da operação; mantenha constructor injection quando a dependência for estrutural ao objeto.
- Use Scope/resource management para ownership e cleanup determinísticos.
- Use contexto por execução apenas para dados realmente contextuais, nunca para esconder parâmetros de domínio.
- Considere composição paralela somente para operações realmente independentes.
- Use overrides/test runtime da biblioteca quando melhorarem testes sem criar infraestrutura desnecessária.

## Dart moderno

Use recursos modernos somente quando aumentarem clareza ou segurança:

- `sealed class` para famílias fechadas;
- `final class` para implementações não extensíveis;
- `abstract interface class` para contratos;
- pattern matching e exhaustive `switch`;
- switch expressions;
- Records e destructuring;
- constructor tear-offs;
- dot shorthands quando claros e suportados pelo SDK do projeto;
- extension types para conceitos primitivos que precisam de identidade distinta;
- collection `if`, `for` e spread;
- expression-bodied members quando realmente mais claros;
- `const` e boa inferência de tipos;
- `Never` para caminhos que não retornam.

Evite “syntax golf”. Código curto não é automaticamente código bom.

## Modelo de falhas

Prefira famílias de failures semanticamente relevantes:

```dart
sealed class AuthenticationFailure implements Exception {
  const AuthenticationFailure();
}

final class InvalidCredentials extends AuthenticationFailure {
  const InvalidCredentials();
}

final class UserBlocked extends AuthenticationFailure {
  const UserBlocked();
}
```

Evite usar uma única `AppException(String message)` para representar todos os problemas.

Diferencie sempre:

1. falha esperada de domínio/aplicação;
2. falha de infraestrutura traduzível;
3. defect inesperado.

## Dependency Injection

Não imponha a regra “tudo pelo constructor” nem a regra “nada pelo constructor”.

Use este critério:

- dependência estrutural e estável, necessária para a validade do objeto → constructor;
- dependência contextual à execução de uma operação → contexto do Effect/Runtime.

Não transforme o Runtime em service locator global.

## Organização

O filesystem deve explicar o sistema.

Evite arquivos genéricos como `models.dart`, `services.dart`, `repositories.dart`, `utils.dart` ou `exceptions.dart` contendo conceitos não relacionados.

Separe por responsabilidade e feature quando isso melhora navegação, mas não crie um arquivo para cada função trivial.

Prefira nomes que descrevam papel real. Evite `Manager`, `Helper`, `Utils`, `Processor` e `CommonService` quando forem apenas nomes genéricos.

## Proibições

Não:

- reescreva o projeto do zero sem necessidade;
- introduza Clean Architecture/DDD/CQRS ou outra arquitetura apenas por padrão;
- crie abstrações sem problema concreto;
- replique Effect TS literalmente em Dart;
- transforme todo `Future` em `Effect`;
- transforme todos os primitivos em extension types;
- use todos os recursos do `better_effect` apenas porque existem;
- esconda side effects em getters, constructors, `map` ou APIs aparentemente puras;
- deixe refatorações equivalentes pendentes depois de identificar o padrão;
- declare sucesso sem formatar, analisar e testar o código quando as ferramentas estiverem disponíveis.

## Critério de conclusão

A tarefa está concluída somente quando:

- os padrões problemáticos mapeados foram tratados em todas as ocorrências relevantes;
- o código compila;
- formatter/analyzer não apresentam regressões introduzidas;
- testes passam ou falhas preexistentes estão claramente identificadas;
- Result, Effect e Future possuem responsabilidades compreensíveis;
- failures esperadas estão explicitamente modeladas onde isso importa;
- DI e Runtime possuem lifecycle coerente;
- recursos possuem ownership claro;
- o código ficou mais fácil de compreender, não apenas mais sofisticado.

## Entrega

Ao finalizar, forneça um relatório objetivo com:

1. principais problemas encontrados;
2. refatorações realizadas;
3. melhorias no uso de `result_dart`;
4. melhorias no uso de `better_effect`;
5. recursos modernos de Dart utilizados e motivo;
6. simplificações e boilerplate removido;
7. validações/comandos executados e resultados;
8. problemas preexistentes que impediram alguma validação, se houver.

A métrica final é:

> legibilidade + correção + type safety + simplicidade + coerência arquitetural + aproveitamento real de `result_dart` e `better_effect`.
