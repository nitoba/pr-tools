package cli

import (
	"context"
	"fmt"
	"strconv"
	"strings"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/azure"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/llm"
	"github.com/spf13/cobra"
)

type testFlagSet struct {
	workItem   string
	pr         int
	org        string
	project    string
	repo       string
	areaPath   string
	assignedTo string
	examples   int
	noCreate   bool
	dryRun     bool
	raw        bool
}

func NewTestCmd(cfg *config.Config) *cobra.Command {
	var flags testFlagSet

	cmd := &cobra.Command{
		Use:   "test",
		Short: "Generate Azure DevOps test card from Work Item.",
		RunE: func(cmd *cobra.Command, _ []string) error {
			return runTest(cmd.Context(), cfg, flags, cmd)
		},
	}

	cmd.Flags().StringVar(&flags.workItem, "work-item", "", "Parent work item ID (required)")
	cmd.Flags().IntVar(&flags.pr, "pr", 0, "PR ID")
	cmd.Flags().StringVar(&flags.org, "org", "", "Azure organization")
	cmd.Flags().StringVar(&flags.project, "project", "", "Azure project")
	cmd.Flags().StringVar(&flags.repo, "repo", "", "Azure repository")
	cmd.Flags().StringVar(&flags.areaPath, "area-path", cfg.TestCardAreaPath, "Test Case area path")
	cmd.Flags().StringVar(&flags.assignedTo, "assigned-to", cfg.TestCardAssignedTo, "Test Case assignee")
	cmd.Flags().IntVar(&flags.examples, "examples", 2, "Number of examples (0-5)")
	cmd.Flags().BoolVar(&flags.noCreate, "no-create", false, "Generate only, do not create in Azure DevOps")
	cmd.Flags().BoolVar(&flags.dryRun, "dry-run", false, "Show prompt without calling LLM")
	cmd.Flags().BoolVar(&flags.raw, "raw", false, "Output only markdown")

	_ = cmd.MarkFlagRequired("work-item")

	return cmd
}

func runTest(ctx context.Context, cfg *config.Config, flags testFlagSet, cmd *cobra.Command) error {
	out := cmd.OutOrStdout()

	// Resolve org/project from flags or config
	org := flags.org
	project := flags.project

	// Parse work item ID
	wiID, err := strconv.Atoi(flags.workItem)
	if err != nil {
		return fmt.Errorf("invalid work item ID %q: %w", flags.workItem, err)
	}

	// Get work item details from Azure DevOps
	var wi *azure.WorkItem
	if cfg.AzurePAT != "" && org != "" && project != "" {
		azClient := azure.NewClient(cfg.AzurePAT, org)
		wi, err = azClient.GetWorkItem(ctx, project, wiID)
		if err != nil {
			return fmt.Errorf("get work item: %w", err)
		}
	}

	// Build prompt
	userPrompt := buildTestPrompt(wi, wiID, flags)

	if flags.dryRun {
		fmt.Fprintln(out, "=== SYSTEM ===")
		fmt.Fprintln(out, testSystemPrompt)
		fmt.Fprintln(out, "\n=== USER ===")
		fmt.Fprintln(out, userPrompt)
		return nil
	}

	// Call LLM
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
	resp, _, err := fallback.Chat(ctx, testSystemPrompt, userPrompt)
	if err != nil {
		return fmt.Errorf("LLM call failed: %w", err)
	}

	// Parse response
	title, body := parseTitleAndBody(resp)

	// Output
	if flags.raw {
		fmt.Fprintln(out, body)
		return nil
	}

	fmt.Fprintf(out, "\nTitulo: %s\n\n", title)
	fmt.Fprintf(out, "Test Card:\n%s\n", body)

	// Create test case in Azure DevOps unless --no-create
	if !flags.noCreate && cfg.AzurePAT != "" && org != "" && project != "" {
		azClient := azure.NewClient(cfg.AzurePAT, org)
		tcReq := azure.CreateTestCaseRequest{
			Title:       title,
			Description: body,
			AreaPath:    flags.areaPath,
			AssignedTo:  flags.assignedTo,
			ParentID:    wiID,
		}
		tc, err := azClient.CreateTestCase(ctx, project, tcReq)
		if err != nil {
			fmt.Fprintf(out, "\n⚠ Erro ao criar test case: %v\n", err)
		} else {
			fmt.Fprintf(out, "\n✓ Test Case criado: #%d\n", tc.ID)
		}
	}

	return nil
}

const testSystemPrompt = `Voce é um analista de QA tecnico.

Sua tarefa é gerar um card de teste em portugues brasileiro para Azure DevOps com base no work item fornecido.

IMPORTANTE: A PRIMEIRA LINHA da sua resposta DEVE ser exatamente:
TITULO: <titulo curto e objetivo>

Depois disso, responda em Markdown com estas secoes nesta ordem:
## Objetivo
## Cenario base
## Checklist de testes
## Resultado esperado`

func buildTestPrompt(wi *azure.WorkItem, wiID int, flags testFlagSet) string {
	var b strings.Builder
	b.WriteString("## Work Item\n\n")
	fmt.Fprintf(&b, "ID: %d\n", wiID)
	if wi != nil {
		fmt.Fprintf(&b, "Titulo: %s\n", wi.Title())
		fmt.Fprintf(&b, "Tipo: %s\n", wi.Type())
		if desc := wi.Description(); desc != "" {
			fmt.Fprintf(&b, "Descricao: %s\n", desc)
		}
	}
	return b.String()
}
