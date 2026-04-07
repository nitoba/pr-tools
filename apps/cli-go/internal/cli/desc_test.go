package cli

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"regexp"
	"strings"
	"testing"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/azure"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/git"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/ui"
	"github.com/spf13/cobra"
	"github.com/stretchr/testify/require"
)

func TestExitErrorIsNilSafe(t *testing.T) {
	t.Parallel()

	err := &ExitError{}

	require.Equal(t, "", err.Error())
	require.NoError(t, err.Unwrap())
}

func TestNewDescCmdHasCorrectMetadata(t *testing.T) {
	t.Parallel()

	cfg := &config.Config{}
	cmd := NewDescCmd(cfg)

	require.Equal(t, "desc", cmd.Use)
	require.Equal(t, "Generate PR descriptions.", cmd.Short)
	require.NotNil(t, cmd.Flags().Lookup("source"))
	require.NotNil(t, cmd.Flags().Lookup("dry-run"))
	require.NotNil(t, cmd.Flags().Lookup("create"))
}

func TestParseTitleAndBody_ExtractsTITULO(t *testing.T) {
	t.Parallel()

	resp := "TITULO: My PR Title\n## Descrição\nSome description"
	title, body := parseTitleAndBody(resp, "")

	require.Equal(t, "My PR Title", title)
	require.Contains(t, body, "## Descrição")
}

func TestParseTitleAndBody_FallbackToFirstLine(t *testing.T) {
	t.Parallel()

	resp := "First line\nSecond line"
	title, body := parseTitleAndBody(resp, "")

	require.Equal(t, "First line", title)
	require.Equal(t, "Second line", body)
}

func TestDescConfiguredProviderModel_PicksFirstUsableProvider(t *testing.T) {
	t.Parallel()

	provider, model := descConfiguredProviderModel(config.Config{
		Providers:       "openrouter,groq,gemini",
		GroqAPIKey:      "groq-key",
		GroqModel:       "llama-3.3",
		GeminiAPIKey:    "gemini-key",
		OpenRouterModel: "ignored-openrouter-model",
		GeminiModel:     "gemini-2.5",
	})

	require.Equal(t, "groq", provider)
	require.Equal(t, "llama-3.3", model)
}

func TestRunDescDryRunUsesBashTranscript(t *testing.T) {
	restore := stubDescDeps(descTestDeps{
		gitCtx: &git.Context{
			BranchName:    "feature/123-login",
			SourceBranch:  "feature/123-login",
			BaseBranch:    "dev",
			SprintBranch:  "sprint/98",
			Diff:          "diff --git a/file b/file",
			Log:           "abc123 feat: login",
			WorkItemID:    "123",
			IsAzureDevOps: true,
			AzureOrg:      "org",
			AzureProject:  "project",
			AzureRepo:     "repo",
		},
		workItem:     &azure.WorkItem{ID: 123, Fields: map[string]interface{}{"System.IterationPath": "Project\\Sprint 98"}},
		systemPrompt: "system prompt",
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newDescTestCommand(stdout, stderr, "")

	err := runDesc(context.Background(), &config.Config{OpenRouterAPIKey: "key"}, descFlagSet{dryRun: true}, cmd)
	require.NoError(t, err)

	out := stdout.String()
	require.Contains(t, out, "────────────────────────────────────────")
	require.Contains(t, out, "DRY RUN - Prompt que seria enviado ao LLM")
	require.Contains(t, out, "[SYSTEM]")
	require.Contains(t, out, "system prompt")
	require.Contains(t, out, "[USER]")
	require.Contains(t, out, "Provider/Model:")

	errOut := stderr.String()
	require.NotContains(t, errOut, "✓ Validando dependencias")
	require.Contains(t, errOut, "✓ Dependencias validadas")
	require.NotContains(t, errOut, "✓ Gerando descrição via LLM")
	require.Contains(t, errOut, "✓ Descrição gerada (openrouter/default)")
	require.NotContains(t, errOut, "Criar PR(s) no Azure DevOps?")
}

func TestRunDescPromptsForWorkItemWhenNotDerived(t *testing.T) {
	restore := stubDescDeps(descTestDeps{
		gitCtx: &git.Context{
			BranchName:    "feature/login-improvements",
			SourceBranch:  "feature/login-improvements",
			BaseBranch:    "dev",
			Diff:          "diff",
			Log:           "log",
			IsAzureDevOps: false,
		},
		systemPrompt: "system prompt",
		llmResp:      "TITULO: Login\n## Descrição\nBody",
		llmProvider:  "openrouter",
		llmModel:     "model-x",
		interactive:  true,
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newDescTestCommand(stdout, stderr, "456\n")

	err := runDesc(context.Background(), &config.Config{OpenRouterAPIKey: "key", Providers: "openrouter"}, descFlagSet{}, cmd)
	require.NoError(t, err)

	errOut := stderr.String()
	require.Contains(t, errOut, "Não foi possivel extrair o work item ID da branch 'feature/login-improvements'.")
	require.Contains(t, errOut, "ID do work item (Enter para pular):")
	require.NotContains(t, errOut, "✓ Detectando work item")
	require.Contains(t, errOut, "✓ Work item: #456")
}

func TestRunDescSummaryBlockShowsBashRows(t *testing.T) {
	restore := stubDescDeps(descTestDeps{
		gitCtx: &git.Context{
			BranchName:    "feature/123-login",
			SourceBranch:  "feature/123-login",
			BaseBranch:    "dev",
			Diff:          "diff",
			Log:           "log",
			WorkItemID:    "123",
			IsAzureDevOps: true,
			AzureOrg:      "org",
			AzureProject:  "project",
			AzureRepo:     "repo",
		},
		systemPrompt: "system prompt",
		llmResp:      "TITULO: Melhorar login\n## Descrição\nBody",
		llmProvider:  "openrouter",
		llmModel:     "model-x",
		clipboardErr: nil,
		prLinks: map[string]*azure.PullRequest{
			"dev": {URL: "https://dev.azure.com/org/project/_git/repo/pullrequest/1"},
		},
		interactive: true,
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newDescTestCommand(stdout, stderr, "n\n")

	err := runDesc(context.Background(), &config.Config{OpenRouterAPIKey: "key", AzurePAT: "pat", Providers: "openrouter"}, descFlagSet{targets: []string{"dev"}}, cmd)
	require.NoError(t, err)

	errOut := stderr.String()
	require.Contains(t, errOut, "PR — feature/123-login")
	require.Contains(t, errOut, "Target: dev")
	require.Contains(t, errOut, "Provider: openrouter/model-x")
	require.Contains(t, errOut, "│ Work Item: #123")
	require.Contains(t, errOut, "│ Work Item:")
	require.Contains(t, errOut, "│   https://dev.azure.com/org/project/_workitems/edit/123")
	require.Contains(t, errOut, "│ Abrir PR:")
	require.Contains(t, errOut, "│   dev")
	require.Contains(t, errOut, "│     https://dev.azure.com/org/project/_git/repo/pullrequestcreate?sourceRef=refs/heads/feature/123-login&targetRef=refs/heads/dev")
	require.Contains(t, errOut, "Descrição copiada para o clipboard")
	require.Contains(t, errOut, "Título disponível acima para copiar manualmente.")
	require.Contains(t, errOut, "│ ✓ Descrição copiada para o clipboard")
	require.Contains(t, errOut, "│ Título disponível acima para copiar manualmente.")
	require.Less(t, strings.Index(errOut, "Descrição copiada para o clipboard"), strings.Index(errOut, "  └\n ✦ Publicar no Azure DevOps"))
}

func TestRunDescWarnsWhenClipboardUnavailable(t *testing.T) {
	restore := stubDescDeps(descTestDeps{
		gitCtx:       &git.Context{BranchName: "feature/123-login", SourceBranch: "feature/123-login", BaseBranch: "dev", Diff: "diff", Log: "log", WorkItemID: "123"},
		systemPrompt: "system prompt",
		llmResp:      "TITULO: Melhorar login\n## Descrição\nBody",
		llmProvider:  "openrouter",
		llmModel:     "model-x",
		clipboardErr: errors.New("clipboard unavailable"),
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newDescTestCommand(stdout, stderr, "")
	origCyan, origReset := ui.Cyan, ui.Reset
	descInitUI = func(io.Writer) {
		ui.Cyan = "\x1b[36m"
		ui.Reset = "\x1b[0m"
	}
	defer func() {
		descInitUI = ui.Init
		ui.Cyan = origCyan
		ui.Reset = origReset
	}()

	err := runDesc(context.Background(), &config.Config{OpenRouterAPIKey: "key", Providers: "openrouter"}, descFlagSet{}, cmd)
	require.NoError(t, err)

	errOut := stderr.String()
	require.Contains(t, errOut, "│ ⚠ Clipboard não disponível (pbcopy/xclip/xsel não encontrado)")
	require.Less(t, strings.Index(errOut, "Clipboard não disponível (pbcopy/xclip/xsel não encontrado)"), strings.LastIndex(errOut, "  └"))
}

func TestRunDescStdoutIsPlainWhenOnlyStderrIsInteractive(t *testing.T) {
	restore := stubDescDeps(descTestDeps{
		gitCtx:            &git.Context{BranchName: "feature/123-login", SourceBranch: "feature/123-login", BaseBranch: "dev", Diff: "diff", Log: "log", WorkItemID: "123"},
		systemPrompt:      "system prompt",
		llmResp:           "TITULO: Melhorar login\n## Descrição\nBody",
		llmProvider:       "openrouter",
		llmModel:          "model-x",
		clipboardErr:      nil,
		interactive:       false,
		stdoutInteractive: false,
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newDescTestCommand(stdout, stderr, "")

	err := runDesc(context.Background(), &config.Config{OpenRouterAPIKey: "key", Providers: "openrouter"}, descFlagSet{}, cmd)
	require.NoError(t, err)

	out := stdout.String()
	require.Contains(t, out, "  Titulo: Melhorar login\n\n  Descricao:\n  ## Descrição\n  Body\n")
	require.NotContains(t, out, "\x1b[")
	require.False(t, regexp.MustCompile(`\x1b\[[0-9;]*m`).MatchString(out))
}

func TestRunDescSkipsPublishBlockWhenTargetsResolveEmpty(t *testing.T) {
	restore := stubDescDeps(descTestDeps{
		gitCtx: &git.Context{
			BranchName:    "feature/123-login",
			SourceBranch:  "feature/123-login",
			Diff:          "diff",
			Log:           "log",
			WorkItemID:    "123",
			IsAzureDevOps: true,
			AzureOrg:      "org",
			AzureProject:  "project",
			AzureRepo:     "repo",
		},
		systemPrompt: "system prompt",
		llmResp:      "TITULO: Melhorar login\n## Descrição\nBody",
		llmProvider:  "openrouter",
		llmModel:     "model-x",
		interactive:  true,
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newDescTestCommand(stdout, stderr, "y\n")

	err := runDesc(context.Background(), &config.Config{OpenRouterAPIKey: "key", AzurePAT: "pat", Providers: "openrouter"}, descFlagSet{}, cmd)
	require.NoError(t, err)

	errOut := stderr.String()
	require.NotContains(t, errOut, "Publicar no Azure DevOps")
	require.NotContains(t, errOut, "Criar PR(s) no Azure DevOps?")
}

func TestRunDescPublishTranscriptCancelSuccessAndFailure(t *testing.T) {
	restore := stubDescDeps(descTestDeps{
		gitCtx: &git.Context{
			BranchName:    "feature/123-login",
			SourceBranch:  "feature/123-login",
			BaseBranch:    "dev",
			Diff:          "diff",
			Log:           "log",
			WorkItemID:    "123",
			IsAzureDevOps: true,
			AzureOrg:      "org",
			AzureProject:  "project",
			AzureRepo:     "repo",
		},
		systemPrompt: "system prompt",
		llmResp:      "TITULO: Melhorar login\n## Descrição\nBody",
		llmProvider:  "openrouter",
		llmModel:     "model-x",
		prLinks: map[string]*azure.PullRequest{
			"dev":    {URL: "https://example/dev"},
			"sprint": nil,
		},
		prErrs: map[string]error{
			"sprint": errors.New("azure failed"),
		},
		interactive: true,
	})
	defer restore()

	t.Run("cancel", func(t *testing.T) {
		stdout := new(bytes.Buffer)
		stderr := new(bytes.Buffer)
		cmd := newDescTestCommand(stdout, stderr, "n\n")

		err := runDesc(context.Background(), &config.Config{OpenRouterAPIKey: "key", AzurePAT: "pat", Providers: "openrouter"}, descFlagSet{targets: []string{"dev"}}, cmd)
		require.NoError(t, err)
		require.Contains(t, stderr.String(), "Publicar no Azure DevOps")
		require.Contains(t, stderr.String(), "│ Criar PR(s) no Azure DevOps? [y/N]")
		require.Contains(t, stderr.String(), "(cancelado)")
	})

	t.Run("success-and-failure", func(t *testing.T) {
		stdout := new(bytes.Buffer)
		stderr := new(bytes.Buffer)
		cmd := newDescTestCommand(stdout, stderr, "y\nreviewer@example.com\n")

		err := runDesc(context.Background(), &config.Config{OpenRouterAPIKey: "key", AzurePAT: "pat", Providers: "openrouter", PRReviewerDev: "dev@example.com"}, descFlagSet{targets: []string{"dev", "sprint"}}, cmd)
		require.NoError(t, err)

		errOut := stderr.String()
		require.Contains(t, errOut, "→ PR para dev")
		require.Contains(t, errOut, "│ Criar PR(s) no Azure DevOps? [y/N]")
		require.Contains(t, errOut, "│ Reviewer (email) [Enter para manter atual]")
		require.NotContains(t, errOut, "Reviewer (email) [dev@example.com]:")
		require.NotContains(t, errOut, "✓ Criando PR → dev")
		require.Contains(t, errOut, "✓ PR criado → dev")
		require.Equal(t, 1, strings.Count(errOut, "PR criado → dev"))
		require.Contains(t, errOut, "https://example/dev")
		require.Contains(t, errOut, "→ PR para sprint")
		require.NotContains(t, errOut, "✗ Criando PR → sprint")
		require.Contains(t, errOut, "✗ Falha ao criar PR → sprint")
		require.Equal(t, 1, strings.Count(errOut, "Falha ao criar PR → sprint"))
		require.Contains(t, errOut, "azure failed")
	})

	t.Run("without-default-reviewer", func(t *testing.T) {
		stdout := new(bytes.Buffer)
		stderr := new(bytes.Buffer)
		cmd := newDescTestCommand(stdout, stderr, "y\n\n")

		err := runDesc(context.Background(), &config.Config{OpenRouterAPIKey: "key", AzurePAT: "pat", Providers: "openrouter"}, descFlagSet{targets: []string{"dev"}}, cmd)
		require.NoError(t, err)

		errOut := stderr.String()
		require.Contains(t, errOut, "│ Criar PR(s) no Azure DevOps? [y/N]")
		require.Contains(t, errOut, "│ Reviewer (email) [Enter para deixar vazio]")
	})
}

func TestRunDescRawPreservesTranscriptTree(t *testing.T) {
	restore := stubDescDeps(descTestDeps{
		gitCtx:       &git.Context{BranchName: "feature/123-login", SourceBranch: "feature/123-login", BaseBranch: "dev", Diff: "diff", Log: "log", WorkItemID: "123"},
		systemPrompt: "system prompt",
		llmResp:      "TITULO: Melhorar login\n## Descrição\nBody",
		llmProvider:  "openrouter",
		llmModel:     "model-x",
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newDescTestCommand(stdout, stderr, "")

	err := runDesc(context.Background(), &config.Config{OpenRouterAPIKey: "key", Providers: "openrouter"}, descFlagSet{raw: true}, cmd)
	require.NoError(t, err)

	require.Equal(t, "## Descrição\nBody\n", stdout.String())
	require.Contains(t, stderr.String(), "Tentando provider: openrouter (model-x)...")
	require.NotContains(t, stderr.String(), "✓ Gerando descrição via LLM")
	require.Contains(t, stderr.String(), "✓ Descrição gerada (openrouter/model-x)")
	require.Contains(t, stderr.String(), "PR — feature/123-login")
}

type descTestDeps struct {
	gitCtx            *git.Context
	gitErr            error
	workItem          *azure.WorkItem
	workItemErr       error
	systemPrompt      string
	llmResp           string
	llmProvider       string
	llmModel          string
	llmErr            error
	clipboardErr      error
	interactive       bool
	stdoutInteractive bool
	prLinks           map[string]*azure.PullRequest
	prErrs            map[string]error
}

func stubDescDeps(deps descTestDeps) func() {
	origCollect := collectDescGitContext
	origWorkItem := fetchDescWorkItem
	origPrompt := loadDescTemplateFn
	origInitUI := descInitUI
	origLLM := runDescLLM
	origClipboard := descClipboardWrite
	origTerminal := descIsTerminal
	origStdoutTerminal := descWriterIsTerminal
	origCreatePR := createDescPR

	collectDescGitContext = func(context.Context, string) (*git.Context, error) {
		if deps.gitCtx == nil {
			return nil, deps.gitErr
		}
		clone := *deps.gitCtx
		return &clone, deps.gitErr
	}
	fetchDescWorkItem = func(context.Context, string, string, string, string) (*azure.WorkItem, error) {
		return deps.workItem, deps.workItemErr
	}
	loadDescTemplateFn = func(*config.Config) string {
		if deps.systemPrompt == "" {
			return descSystemPrompt
		}
		return deps.systemPrompt
	}
	runDescLLM = func(_ context.Context, _ config.Config, _, _ string, onTrying func(string, string), _ func(string, error)) (string, string, string, error) {
		if onTrying != nil && deps.llmErr == nil && deps.llmProvider != "" && deps.llmModel != "" {
			onTrying(deps.llmProvider, deps.llmModel)
		}
		return deps.llmResp, deps.llmProvider, deps.llmModel, deps.llmErr
	}
	descClipboardWrite = func(string) error {
		return deps.clipboardErr
	}
	descIsTerminal = func(io.Reader) bool {
		return deps.interactive
	}
	descWriterIsTerminal = func(io.Writer) bool {
		return deps.stdoutInteractive
	}
	createDescPR = func(_ context.Context, _, _, _, _, target string, _ azure.CreatePRRequest) (*azure.PullRequest, error) {
		if err := deps.prErrs[target]; err != nil {
			return nil, err
		}
		if pr, ok := deps.prLinks[target]; ok {
			return pr, nil
		}
		return &azure.PullRequest{URL: fmt.Sprintf("https://example/%s", target)}, nil
	}

	return func() {
		collectDescGitContext = origCollect
		fetchDescWorkItem = origWorkItem
		loadDescTemplateFn = origPrompt
		descInitUI = origInitUI
		runDescLLM = origLLM
		descClipboardWrite = origClipboard
		descIsTerminal = origTerminal
		descWriterIsTerminal = origStdoutTerminal
		createDescPR = origCreatePR
	}
}

func newDescTestCommand(stdout, stderr *bytes.Buffer, input string) *cobra.Command {
	cmd := &cobra.Command{}
	cmd.SetOut(stdout)
	cmd.SetErr(stderr)
	cmd.SetIn(strings.NewReader(input))
	return cmd
}
