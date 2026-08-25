# Padrões de transformação

Use estes exemplos como direção de design, não como substituições mecânicas. Confirme sempre a API exata das versões instaladas.

## Result manual -> composição

### Antes

```dart
final result = validate(input);
if (result.isError()) {
  return Failure(result.exceptionOrNull()!);
}

final value = result.getOrNull()!;
return Success(normalize(value));
```

### Direção preferida

```dart
return validate(input).map(normalize);
```

---

## Result dentro de Effect -> canal de failure do Effect

### Antes

```dart
Effect<ResultDart<User, UserFailure>, UserFailure> loadUser(UserId id) {
  // ...
}
```

### Direção preferida

```dart
Effect<User, UserFailure> loadUser(UserId id) {
  // ...
}
```

Integre o Result no Effect usando o adaptador/desembrulhamento nativo da versão instalada.

---

## Orquestração manual -> `Effect.result`

### Antes

```dart
Future<ResultDart<User, AuthenticationFailure>> login(
  Credentials credentials,
) async {
  final result = await repository.login(credentials);
  if (result.isError()) {
    return Failure(result.exceptionOrNull()!);
  }

  final user = result.getOrNull()!;
  final saveResult = await sessionStorage.save(user.session);
  if (saveResult.isError()) {
    return Failure(saveResult.exceptionOrNull()!);
  }

  return Success(user);
}
```

### Direção preferida

Quando a versão instalada suportar as APIs abaixo:

```dart
Effect<User, AuthenticationFailure> login(
  Credentials credentials,
) =>
    Effect.result((use) async {
      final repository = use<AuthenticationRepository>();
      final sessionStorage = use<SessionStorage>();

      final user = await use.unwrap(
        repository.login(credentials),
      );

      await use.unwrap(
        sessionStorage.save(user.session),
      );

      return user;
    });
```

---

## Exception externa -> failure tipada no boundary

### Antes

```dart
Future<ResultDart<User, UserFailure>> load() async {
  try {
    final response = await client.get('/user');
    return Success(User.fromJson(response.data));
  } catch (error) {
    return Failure(UserFailure(error.toString()));
  }
}
```

### Direção preferida

Mantenha captura/tradução na camada que conhece a infraestrutura e preserve a causa original:

```dart
Effect<User, UserFailure> load() => Effect.tryAsync(
  () async {
    final response = await client.get('/user');
    return User.fromJson(response.data);
  },
  onError: (error, stackTrace) => UserRemoteUnavailable(
    cause: error,
  ),
);
```

Se parsing possuir failure esperada própria, componha-a separadamente em vez de capturá-la como exception genérica.

---

## Família de erros genérica -> failures seladas

### Antes

```dart
final class AppException implements Exception {
  const AppException(this.message);
  final String message;
}
```

### Direção preferida

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

final class AuthenticationUnavailable extends AuthenticationFailure {
  const AuthenticationUnavailable({required this.cause});

  final Object cause;
}
```

---

## `if (failure is ...)` -> exhaustive switch

### Antes

```dart
if (failure is InvalidCredentials) {
  return 'Credenciais inválidas';
} else if (failure is UserBlocked) {
  return 'Usuário bloqueado';
}

return 'Erro inesperado';
```

### Direção preferida

```dart
String messageFor(AuthenticationFailure failure) => switch (failure) {
  InvalidCredentials() => 'Credenciais inválidas',
  UserBlocked() => 'Usuário bloqueado',
  AuthenticationUnavailable() => 'Serviço indisponível',
};
```

---

## DTO temporário -> Record

### Antes

```dart
final class UserAndOrders {
  const UserAndOrders(this.user, this.orders);

  final User user;
  final List<Order> orders;
}
```

Se o tipo só existe localmente para transportar dois valores:

```dart
final (user, orders) = await loadUserAndOrders();
```

Mantenha uma classe se houver identidade, invariantes, API pública ou uso recorrente.

---

## IDs homogêneos -> extension types

### Antes

```dart
Future<void> moveOrder(String orderId, String userId)
```

### Direção preferida

```dart
extension type OrderId(String value) {}
extension type UserId(String value) {}

Future<void> moveOrder(OrderId orderId, UserId userId)
```

Use apenas quando misturar os valores for um risco real.

---

## Constructor plumbing -> dependência contextual

### Antes

```dart
final class A {
  A(this.repository);
  final UserRepository repository;
}

final class B {
  B(this.a, this.repository);
  final A a;
  final UserRepository repository;
}
```

Se `B` só transporta `repository` para uma operação contextual, considere resolvê-la no Effect:

```dart
Effect<User, UserFailure> execute(UserId id) =>
    Effect.result((use) async {
      final repository = use<UserRepository>();
      return use.unwrap(repository.find(id));
    });
```

Não remova constructor injection quando a dependência for estrutural ao objeto.

---

## Runtime por operação -> lifecycle de aplicação

### Sinal de problema

```dart
await module.run(operationA);
await module.run(operationB);
await module.run(operationC);
```

Se as operações compartilham o mesmo lifecycle e os mesmos recursos, avalie iniciar o Runtime uma vez e fechá-lo no boundary da aplicação.

Não aplique esta transformação a jobs one-shot genuinamente isolados.

---

## `try/finally` de recurso -> Scope/resource binding

### Antes

```dart
final connection = await database.open();
try {
  return await work(connection);
} finally {
  await connection.close();
}
```

### Direção preferida

- recurso da aplicação: binding de resource no Module;
- recurso de uma execução: aquisição scoped dentro do Effect.

Use a API exata da versão instalada.

---

## Estado opcional mal modelado

### Antes

```dart
User? user;
String? error;
bool isLoading = false;
```

Se os estados forem mutuamente exclusivos, considere uma hierarquia selada:

```dart
sealed class UserState {
  const UserState();
}

final class UserIdle extends UserState {
  const UserIdle();
}

final class UserLoading extends UserState {
  const UserLoading();
}

final class UserLoaded extends UserState {
  const UserLoaded(this.user);
  final User user;
}

final class UserFailed extends UserState {
  const UserFailed(this.failure);
  final UserFailure failure;
}
```

Não aplique isso automaticamente a todo estado simples.

---

## Anti-pattern: pipeline só por estética

Não transforme:

```dart
final user = await use.unwrap(repository.find(id));
return user;
```

em uma cadeia complexa se a versão linear é mais clara.

A skill favorece programação funcional pragmática, não maximalista.
