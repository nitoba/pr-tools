package cli

import (
	"context"
	"fmt"
	"strings"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/azure"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/clipboard"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/git"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/llm"
	"github.com/spf13/cobra"
)

const approvedSpecPath = "docs/superpowers/specs/2026-04-06-prt-go-foundation-design.md"

type ExitError struct {
	Code int
	Err  error
}

func (e *ExitError) Error() string {
	if e == nil || e.Err == nil {
		return ""
	}

	return e.Err.Error()
}

func (e *ExitError) Unwrap() error {
	if e == nil {
		return nil
	}

	return e.Err
}

type descFlagSet struct {
	source   string
	workItem string
	dryRun   bool
	raw      bool
	createPR bool
}

func NewDescCmd(cfg *config.Config) *cobra.Command {
	var flags descFlagSet

	cmd := &cobra.Command{
		Use:   "desc",
		Short: "Generate PR descriptions.",
		RunE: func(cmd *cobra.Command, _ []string) error {
			return runDesc(cmd.Context(), cfg, flags, cmd)
		},
	}

	cmd.Flags().StringVar(&flags.source, "source", "", "Source branch (defaults to current branch)")
	cmd.Flags().StringVar(&flags.workItem, "work-item", "", "Work item ID")
	cmd.Flags().BoolVar(&flags.dryRun, "dry-run", false, "Show prompt without calling LLM")
	cmd.Flags().BoolVar(&flags.raw, "raw", false, "Output without markdown rendering")
	cmd.Flags().BoolVar(&flags.createPR, "create", false, "Create PR in Azure DevOps")

	return cmd
}

func runDesc(ctx context.Context, cfg *config.Config, flags descFlagSet, cmd *cobra.Command) error {
	// Collect git context
	gitCtx := git.NewContext(git.ExecRunner{})
	if err := gitCtx.Collect(ctx, flags.source); err != nil {
		return fmt.Errorf("git context: %w", err)
	}

	// Build user prompt
	userPrompt := buildDescPrompt(gitCtx)

	if flags.dryRun {
		fmt.Fprintln(cmd.OutOrStdout(), "=== SYSTEM ===")
		fmt.Fprintln(cmd.OutOrStdout(), descSystemPrompt)
		fmt.Fprintln(cmd.OutOrStdout(), "\n=== USER ===")
		fmt.Fprintln(cmd.OutOrStdout(), userPrompt)
		return nil
	}

	// Call LLM with fallback
	llmCfg := llm.Config{
		Providers:        cfg.Providers,
		OpenRouterAPIKey: cfg.OpenRouterAPIKey,
		GroqAPIKey:       cfg.GroqAPIKey,
		GeminiAPIKey:     cfg.GeminiAPIKey,
		OllamaAPIKey:     cfg.OllamaAPIKey,
		OpenRouterModel:  cfg.OpenRouterModel,
		GroqModel:        cfg.GroqModel,
		GeminiModel:      cfg.GeminiModel,
		OllamaModel:      cfg.OllamaModel,
	}
	fallback := llm.NewFallbackClient(llmCfg)
	resp, provider, err := fallback.Chat(ctx, descSystemPrompt, userPrompt)
	if err != nil {
		return fmt.Errorf("LLM call failed: %w", err)
	}
	_ = provider

	// Parse title and body
	title, body := parseTitleAndBody(resp)

	// Output
	out := cmd.OutOrStdout()
	fmt.Fprintf(out, "\nTitulo: %s\n\n", title)
	fmt.Fprintf(out, "Descricao:\n%s\n", body)

	// Copy body to clipboard (best effort)
	if err := clipboard.Write(body); err == nil {
		fmt.Fprintln(out, "\n✓ Copiado para clipboard")
	}

	// Create PR in Azure DevOps if requested
	if flags.createPR && cfg.AzurePAT != "" && gitCtx.IsAzureDevOps {
		azClient := azure.NewClient(cfg.AzurePAT, gitCtx.AzureOrg)
		prReq := azure.CreatePRRequest{
			Title:       title,
			Description: body,
			SourceRef:   "refs/heads/" + gitCtx.SourceBranch,
			TargetRef:   "refs/heads/" + gitCtx.BaseBranch,
		}
		pr, err := azClient.CreatePullRequest(ctx, gitCtx.AzureProject, gitCtx.AzureRepo, prReq)
		if err != nil {
			fmt.Fprintf(out, "\n⚠ Erro ao criar PR: %v\n", err)
		} else {
			fmt.Fprintf(out, "\n✓ PR criado: %s\n", pr.URL)
		}
	}

	return nil
}

const descSystemPrompt = `Analise o diff e log do git fornecidos e gere um TITULO e uma DESCRIÇÃO de PR em portugues brasileiro.

IMPORTANTE: A PRIMEIRA LINHA da sua resposta DEVE ser o titulo neste formato exato:
TITULO: <texto curto e descritivo, max 80 caracteres>

Depois do titulo, siga este formato para a descrição:
## Descrição
<Resumo conciso>

## Alterações
<Lista de componentes alterados>

## Tipo de mudança
- [ ] Bug fix
- [ ] Nova feature
- [ ] Breaking change
- [ ] Refactoring`

func buildDescPrompt(gc *git.Context) string {
	var b strings.Builder
	b.WriteString("## Contexto Git\n\n")
	fmt.Fprintf(&b, "**Branch:** %s\n", gc.BranchName)
	fmt.Fprintf(&b, "**Base:** %s\n", gc.BaseBranch)
	if gc.WorkItemID != "" {
		fmt.Fprintf(&b, "**Work Item:** %s\n", gc.WorkItemID)
	}
	fmt.Fprintf(&b, "\n**Diff:**\n```\n%s\n```\n", gc.Diff)
	fmt.Fprintf(&b, "\n**Log:**\n```\n%s\n```\n", gc.Log)
	return b.String()
}

func parseTitleAndBody(resp string) (title, body string) {
	lines := strings.Split(resp, "\n")
	for i, line := range lines {
		if strings.HasPrefix(strings.ToUpper(strings.TrimSpace(line)), "TITULO:") {
			idx := strings.Index(strings.ToUpper(line), "TITULO:")
			title = strings.TrimSpace(line[idx+len("TITULO:"):])
			body = strings.TrimSpace(strings.Join(lines[i+1:], "\n"))
			return
		}
	}
	// Fallback: first line is title
	if len(lines) > 0 {
		title = strings.TrimSpace(lines[0])
		body = strings.TrimSpace(strings.Join(lines[1:], "\n"))
	}
	return
}
