import { GitContextService } from '../infrastructure/git/git-context-service'
import { AiDescriptionGenerator } from '../infrastructure/ai/ai-description-generator'
import { AzureClientFactory } from '../infrastructure/azure/azure-client-factory'
import { AzurePullRequestPublisher } from '../infrastructure/azure/azure-pull-request-publisher'
import { AzureTestCardRepositoryFactory } from '../infrastructure/azure/azure-test-card-repository'
import { NodeClipboard } from '../infrastructure/clipboard/node-clipboard'
import { NodeGitClient } from '../infrastructure/git/git-client'
import { NodeProcessRunner } from '../infrastructure/process/node-process-runner'
import { ConfigService } from '../infrastructure/config/config-service'
import { ConsolePrompts } from '../infrastructure/terminal/console-prompts'
import { ConsoleProgress } from '../infrastructure/terminal/console-progress'
import { NodeTerminalOutput } from '../infrastructure/terminal/node-terminal-output'
import { DescribeCommand, DescribePresenter, DescribeService } from '../features/describe'
import { TestCardCommand, TestCardPresenter, TestCardService } from '../features/test-card'
import { DoctorCommand, DoctorPresenter, DoctorService } from '../features/doctor'
import { InitCommand } from '../features/init'
import { Application } from './application'
import type {
  Clipboard,
  DescriptionGenerator,
  GitContextReader,
  HttpFetcher,
  PullRequestPublisher,
  ProgressReporter,
  PromptPort,
  TerminalOutput,
  TestCardRepositoryFactory
} from './ports'
import type { GitClient } from '../infrastructure/git/git-client'
import type { ProcessRunner } from '../infrastructure/process/process-runner'

export function createApplication(): Application {
  const nodeOutput = new NodeTerminalOutput()
  const output: TerminalOutput = {
    write: (message) => nodeOutput.write(message),
    writeError: (message) => nodeOutput.writeError(message)
  }
  const nodePrompts = new ConsolePrompts()
  const prompts: PromptPort = {
    text: (options) => nodePrompts.text(options),
    password: (options) => nodePrompts.password(options),
    select: (options) => nodePrompts.select(options),
    confirm: (options) => nodePrompts.confirm(options)
  }
  const interactive = Boolean(process.stdin.isTTY && process.stdout.isTTY)
  const nodeDescribeProgress = new ConsoleProgress()
  const describeProgress: ProgressReporter = {
    start: (message) => nodeDescribeProgress.start(message),
    message: (message) => nodeDescribeProgress.message(message),
    stop: (message) => nodeDescribeProgress.stop(message),
    error: (message) => nodeDescribeProgress.error(message)
  }
  const nodeTestCardProgress = new ConsoleProgress()
  const testCardProgress: ProgressReporter = {
    start: (message) => nodeTestCardProgress.start(message),
    message: (message) => nodeTestCardProgress.message(message),
    stop: (message) => nodeTestCardProgress.stop(message),
    error: (message) => nodeTestCardProgress.error(message)
  }
  const nodeGit = new NodeGitClient()
  const gitClient: GitClient = { run: (args) => nodeGit.run(args) }
  const git = new GitContextService(gitClient)
  const gitReader: GitContextReader = { collect: (source) => git.collect(source) }
  const clients = new AzureClientFactory()
  const nodeProcesses = new NodeProcessRunner()
  const processes: ProcessRunner = {
    run: (command, args, timeoutMs) => nodeProcesses.run(command, args, timeoutMs)
  }
  const generator = new AiDescriptionGenerator(processes)
  const descriptionGenerator: DescriptionGenerator = {
    generate: (input) => generator.generate(input)
  }
  const nodeClipboard = new NodeClipboard()
  const clipboard: Clipboard = { copy: (value) => nodeClipboard.copy(value) }
  const config = new ConfigService(undefined, prompts, process.env, interactive)
  const publisher = new AzurePullRequestPublisher(clients)
  const pullRequestPublisher: PullRequestPublisher = {
    publish: (configValue, context, targets, input, reviewer) =>
      publisher.publish(configValue, context, targets, input, reviewer)
  }

  const describeService = new DescribeService(
    config,
    gitReader,
    descriptionGenerator,
    pullRequestPublisher
  )
  const describeCommand = new DescribeCommand(
    describeService,
    new DescribePresenter(output),
    prompts,
    describeProgress,
    clipboard,
    interactive
  )

  const testCardRepositoryFactory = new AzureTestCardRepositoryFactory(clients)
  const repositoryFactory: TestCardRepositoryFactory = {
    create: (configValue, context) => testCardRepositoryFactory.create(configValue, context)
  }
  const testCardService = new TestCardService(
    config,
    gitReader,
    repositoryFactory,
    descriptionGenerator
  )
  const testCardCommand = new TestCardCommand(
    testCardService,
    new TestCardPresenter(output),
    prompts,
    testCardProgress,
    interactive
  )
  const httpFetcher: HttpFetcher = { fetch: (input, init) => fetch(input, init) }
  const doctorService = new DoctorService(config, gitReader, processes, clients, httpFetcher)

  const application = new Application(
    describeCommand,
    testCardCommand,
    new InitCommand(config, output, interactive),
    new DoctorCommand(doctorService, new DoctorPresenter(output))
  )
  return application
}
