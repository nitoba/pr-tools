import 'dart:io';

import 'package:better_effect/testing.dart';
import 'package:pr_tools/src/app/app_failure.dart';
import 'package:pr_tools/src/application/process/process_runner.dart';
import 'package:pr_tools/src/infrastructure/process/process_runner_live.dart';
import 'package:test/test.dart';

void main() {
  test('owns the spawned process for the effect execution', () async {
    final harness = await TestRuntime.start(
      Module([]),
      registerCleanup: (cleanup) => addTearDown(cleanup),
    );

    final completed = harness.observer.next<ExecutionEndEvent>(
      where: (event) => event.context.executionLabel == 'process.version',
    );
    final execution = harness.execute(
      ProcessRunnerLive().run(Platform.resolvedExecutable, ['--version']),
      label: 'process.version',
    );
    final exit = await execution.exit;
    await completed;

    final result = expectExitSuccess<ProcessResult, AppFailure>(exit);
    expect(result.ok, isTrue);
    expect(
      harness.observer.events.whereType<ResourceReleaseEvent>(),
      hasLength(1),
    );
    harness.assertNoActiveExecutions();
  });

  test('forwards optional input through stdin', () async {
    final harness = await TestRuntime.start(
      Module([]),
      registerCleanup: (cleanup) => addTearDown(cleanup),
    );
    final execution = harness.execute(
      ProcessRunnerLive().run(Platform.resolvedExecutable, [
        'run',
        'test/fixtures/read_stdin.dart',
      ], input: 'prompt too large for argv'),
      label: 'process.input',
    );

    final result = expectExitSuccess<ProcessResult, AppFailure>(
      await execution.exit,
    );
    expect(result.stdout, 'prompt too large for argv');
    await harness.close();
  });

  test('stops a running subprocess when its effect is interrupted', () async {
    final harness = await TestRuntime.start(
      Module([]),
      registerCleanup: (cleanup) => addTearDown(cleanup),
    );
    final acquired = harness.observer.next<ServiceAcquireEvent>(
      where: (event) => event.context.executionLabel == 'process.cancel',
    );
    final completed = harness.observer.next<ExecutionEndEvent>(
      where: (event) => event.context.executionLabel == 'process.cancel',
    );
    final execution = harness.execute(
      ProcessRunnerLive().run(Platform.resolvedExecutable, [
        'run',
        'test/fixtures/wait_for_interrupt.dart',
      ]),
      label: 'process.cancel',
    );

    await acquired;
    final shutdown = harness.close(interruptAfterGracePeriod: true);
    expect(
      await execution.exit,
      isA<ExitInterrupted<ProcessResult, AppFailure>>(),
    );
    await completed;
    await shutdown;
  });
}
