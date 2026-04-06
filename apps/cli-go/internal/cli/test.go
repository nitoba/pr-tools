package cli

import (
	"bufio"
	"context"
	"fmt"
	"os"
	"strconv"
	"strings"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/azure"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/git"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/llm"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/ui"
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
	stderr := cmd.ErrOrStderr()
	out := cmd.OutOrStdout()

	ui.Init(stderr)
	ui.Title(stderr, "Gerando card de teste...")

	// Resolve org/project/repo from flags or git remote
	org := flags.org
	project := flags.project
	repo := flags.repo

	if org == "" || project == "" || repo == "" {
		stepCtx := ui.Step(stderr, "Resolvendo contexto Azure DevOps")
		gitCtx := git.NewContext(git.ExecRunner{})
		if err := gitCtx.Collect(ctx, ""); err == nil && gitCtx.IsAzureDevOps {
			if org == "" {
				org = gitCtx.AzureOrg
			}
			if project == "" {
				project = gitCtx.AzureProject
			}
			if repo == "" {
				repo = gitCtx.AzureRepo
			}
			stepCtx(true)
		} else {
			stepCtx(false)
		}
	} else {
		stepRes := ui.Step(stderr, "Resolvendo contexto Azure DevOps")
		stepRes(true)
	}

	// Parse work item ID
	wiID, err := strconv.Atoi(flags.workItem)
	if err != nil {
		return fmt.Errorf("invalid work item ID %q: %w", flags.workItem, err)
	}

	// Get work item details from Azure DevOps
	stepWI := ui.Step(stderr, "Resolvendo work item")
	var wi *azure.WorkItem
	if cfg.AzurePAT != "" && org != "" && project != "" {
		azClient := azure.NewClient(cfg.AzurePAT, org)
		wi, err = azClient.GetWorkItem(ctx, project, wiID)
		if err != nil {
			stepWI(false)
			return fmt.Errorf("get work item: %w", err)
		}
		stepWI(true)
	} else {
		stepWI(false)
	}

	// Fetch PR and changed files
	stepPR := ui.Step(stderr, "Buscando alteracoes do PR")
	var pr *azure.PullRequest
	var changedFiles []azure.PRChange

	if flags.pr > 0 && cfg.AzurePAT != "" && org != "" && project != "" && repo != "" {
		azClient := azure.NewClient(cfg.AzurePAT, org)
		pr, err = azClient.GetPullRequest(ctx, project, repo, flags.pr)
		if err != nil {
			stepPR(false)
			ui.Warn(stderr, fmt.Sprintf("PR não encontrado: %v", err))
		} else {
			iters, itErr := azClient.GetPRIterations(ctx, project, repo, flags.pr)
			if itErr == nil && len(iters) > 0 {
				lastIter := iters[len(iters)-1]
				changedFiles, _ = azClient.GetPRChanges(ctx, project, repo, flags.pr, lastIter.ID)
			}
			stepPR(true)
		}
	} else {
		stepPR(true)
	}

	// Fetch example test cases via WIQL
	var examples []string
	if cfg.AzurePAT != "" && org != "" && project != "" {
		azClient := azure.NewClient(cfg.AzurePAT, org)
		maxEx := flags.examples
		if maxEx > 5 {
			maxEx = 5
		}
		if maxEx > 0 {
			wiql := fmt.Sprintf(
				"SELECT [System.Id],[System.Title] FROM WorkItems WHERE [System.WorkItemType]='Test Case' AND [System.TeamProject]='%s' ORDER BY [System.ChangedDate] DESC",
				project,
			)
			ids, qErr := azClient.QueryWorkItems(ctx, project, wiql)
			if qErr == nil {
				if len(ids) > maxEx {
					ids = ids[:maxEx]
				}
				for _, id := range ids {
					twi, twErr := azClient.GetWorkItem(ctx, project, id)
					if twErr == nil {
						examples = append(examples, fmt.Sprintf("- #%d %s", id, twi.Title()))
					}
				}
			}
		}
	}

	// Build prompt
	userPrompt := buildTestPrompt(wi, wiID, pr, changedFiles, examples, flags)

	if flags.dryRun {
		_, _ = fmt.Fprintln(out, "=== SYSTEM ===")
		_, _ = fmt.Fprintln(out, testSystemPrompt)
		_, _ = fmt.Fprintln(out, "\n=== USER ===")
		_, _ = fmt.Fprintln(out, userPrompt)
		return nil
	}

	// Call LLM
	stepLLM := ui.Step(stderr, "Gerando card via LLM")
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
		stepLLM(false)
		return fmt.Errorf("LLM call failed: %w", err)
	}
	stepLLM(true)
	ui.TitleDone(stderr)

	// Strip <think> blocks
	resp = stripThinkBlocks(resp)

	// Parse response
	title, body := parseTitleAndBody(resp, "")

	// Output
	if flags.raw {
		_, _ = fmt.Fprintln(out, body)
		return nil
	}

	_, _ = fmt.Fprintf(out, "\nTitulo: %s%s%s\n\n", ui.Cyan, title, ui.Reset)
	_, _ = fmt.Fprintf(out, "Test Card:\n%s\n", body)

	// Interactive test case creation
	var createdTCID int
	if !flags.noCreate && isTerminal(os.Stdin) && cfg.AzurePAT != "" && org != "" && project != "" {
		_, _ = fmt.Fprintf(stderr, "\n  %s│%s\n", ui.OrangeDim, ui.Reset)
		_, _ = fmt.Fprintf(stderr, "  Criar este Test Case no Azure DevOps? [y/N]: ")

		scanner := bufio.NewScanner(os.Stdin)
		if scanner.Scan() {
			answer := strings.TrimSpace(strings.ToLower(scanner.Text()))
			if answer == "y" || answer == "yes" {
				azClient := azure.NewClient(cfg.AzurePAT, org)
				tcReq := azure.CreateTestCaseRequest{
					Title:       title,
					Description: body,
					AreaPath:    flags.areaPath,
					AssignedTo:  flags.assignedTo,
					ParentID:    wiID,
				}
				tc, tcErr := azClient.CreateTestCase(ctx, project, tcReq)
				if tcErr != nil {
					ui.Error(stderr, fmt.Sprintf("Erro ao criar test case: %v", tcErr))
				} else {
					createdTCID = tc.ID
					ui.Success(stderr, fmt.Sprintf("Test Case criado: #%d", tc.ID))
				}
			}
		}

		// Ask to update work item state
		if createdTCID > 0 {
			_, _ = fmt.Fprintf(stderr, "  Atualizar o work item #%d para Test QA? [y/N]: ", wiID)
			scanner2 := bufio.NewScanner(os.Stdin)
			if scanner2.Scan() {
				answer := strings.TrimSpace(strings.ToLower(scanner2.Text()))
				if answer == "y" || answer == "yes" {
					azClient := azure.NewClient(cfg.AzurePAT, org)
					if stErr := azClient.UpdateWorkItemState(ctx, project, wiID, "Test QA"); stErr != nil {
						ui.Error(stderr, fmt.Sprintf("Erro ao atualizar work item: %v", stErr))
					} else {
						ui.Success(stderr, fmt.Sprintf("Work item #%d atualizado para Test QA", wiID))
					}
				}
			}
		}
	} else if !flags.noCreate && cfg.AzurePAT != "" && org != "" && project != "" {
		// Non-interactive: create automatically (original behavior)
		azClient := azure.NewClient(cfg.AzurePAT, org)
		tcReq := azure.CreateTestCaseRequest{
			Title:       title,
			Description: body,
			AreaPath:    flags.areaPath,
			AssignedTo:  flags.assignedTo,
			ParentID:    wiID,
		}
		tc, tcErr := azClient.CreateTestCase(ctx, project, tcReq)
		if tcErr != nil {
			_, _ = fmt.Fprintf(out, "\n⚠ Erro ao criar test case: %v\n", tcErr)
		} else {
			_, _ = fmt.Fprintf(out, "\n✓ Test Case criado: #%d\n", tc.ID)
		}
	}

	return nil
}

const testSystemPrompt = `Voce é um analista de QA tecnico.

Sua tarefa é gerar um card de teste em portugues brasileiro para Azure DevOps com base em:
1. Work item pai
2. Pull request relacionado
3. Arquivos alterados e resumo tecnico do PR
4. Exemplos de test cases existentes

IMPORTANTE: A PRIMEIRA LINHA da sua resposta DEVE ser exatamente:
TITULO: <titulo curto e objetivo>

Depois disso, responda em Markdown com estas secoes nesta ordem:

## Objetivo
## Cenario base
## Checklist de testes
## Resultado esperado

Regras:
- Escreva em portugues brasileiro
- Não invente comportamento que não esteja sustentado pelo contexto
- Foque em cobertura funcional, validacoes e regressao quando fizer sentido
- Seja especifico e tecnico, sem ser prolixo
- O titulo deve ser curto e claro
- O checklist deve ser acionavel para QA
- Se houver limites, formatos aceitos, obrigatoriedade, bloqueios ou reaproveitamento de comportamento, inclua isso quando sustentado pelo contexto
- Não mencione detalhes tecnicos de implementacao na resposta final
- Não cite nomes de arquivos, componentes, classes, funcoes, migrations, APIs internas, queries ou trechos de codigo
- Não descreva a solucao como tarefa de desenvolvimento; descreva apenas cenarios observaveis e validaveis por QA
- Transforme pistas tecnicas em comportamento funcional testavel pelo usuario final ou pelo analista de QA`

func buildTestPrompt(wi *azure.WorkItem, wiID int, pr *azure.PullRequest, changedFiles []azure.PRChange, examples []string, flags testFlagSet) string {
	var b strings.Builder

	// ## Contexto do Work Item
	b.WriteString("## Contexto do Work Item\n\n")
	_, _ = fmt.Fprintf(&b, "ID: %d\n", wiID)
	if wi != nil {
		_, _ = fmt.Fprintf(&b, "Título: %s\n", wi.Title())
		_, _ = fmt.Fprintf(&b, "Tipo: %s\n", wi.Type())
		if area := wi.Field("System.AreaPath"); area != "" {
			_, _ = fmt.Fprintf(&b, "Área: %s\n", area)
		}
		if desc := wi.Description(); desc != "" {
			_, _ = fmt.Fprintf(&b, "Descrição: %s\n", desc)
		}
	}

	// ## Contexto do PR
	if pr != nil {
		b.WriteString("\n## Contexto do PR\n\n")
		_, _ = fmt.Fprintf(&b, "PR ID: %d\n", pr.ID)
		_, _ = fmt.Fprintf(&b, "Título: %s\n", pr.Title)
		_, _ = fmt.Fprintf(&b, "Branch origem: %s\n", pr.SourceRef)
		_, _ = fmt.Fprintf(&b, "Branch destino: %s\n", pr.TargetRef)
		if pr.Description != "" {
			_, _ = fmt.Fprintf(&b, "Descrição: %s\n", pr.Description)
		}
	}

	// ## Arquivos alterados e resumo técnico
	if len(changedFiles) > 0 {
		b.WriteString("\n## Arquivos alterados e resumo técnico\n\n")
		for _, f := range changedFiles {
			_, _ = fmt.Fprintf(&b, "- [%s] %s\n", f.ChangeType, f.Item.Path)
		}
	}

	// ## Exemplos de Test Case
	if len(examples) > 0 {
		b.WriteString("\n## Exemplos de Test Case\n\n")
		for _, ex := range examples {
			b.WriteString(ex + "\n")
		}
	}

	// ## Instruções finais
	b.WriteString("\n## Instruções finais\n\n")
	b.WriteString("Gere um card de teste em Markdown seguindo as seções e regras definidas no system prompt.\n")

	return b.String()
}
