package cli

import (
	"bufio"
	"context"
	"fmt"
	"io"
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

var collectTestGitContext = func(ctx context.Context) (*git.Context, error) {
	gitCtx := git.NewContext(git.ExecRunner{})
	if err := gitCtx.Collect(ctx, ""); err != nil {
		return nil, err
	}
	return gitCtx, nil
}

var fetchTestPullRequest = func(ctx context.Context, pat, org, project, repo string, prID int) (*azure.PullRequest, error) {
	return azure.NewClient(pat, org).GetPullRequest(ctx, project, repo, prID)
}

var fetchTestPullRequestWorkItemIDs = func(ctx context.Context, pat, org, project, repo string, prID int) ([]int, error) {
	return azure.NewClient(pat, org).GetPullRequestWorkItemIDs(ctx, project, repo, prID)
}

var fetchTestWorkItem = func(ctx context.Context, pat, org, project string, wiID int) (*azure.WorkItem, error) {
	return azure.NewClient(pat, org).GetWorkItem(ctx, project, wiID)
}

var fetchTestPRIterations = func(ctx context.Context, pat, org, project, repo string, prID int) ([]azure.PRIteration, error) {
	return azure.NewClient(pat, org).GetPRIterations(ctx, project, repo, prID)
}

var fetchTestPRChanges = func(ctx context.Context, pat, org, project, repo string, prID, iterationID int) ([]azure.PRChange, error) {
	return azure.NewClient(pat, org).GetPRChanges(ctx, project, repo, prID, iterationID)
}

var queryTestExampleWorkItems = func(ctx context.Context, pat, org, project string, maxEx int) ([]string, error) {
	azClient := azure.NewClient(pat, org)
	wiql := fmt.Sprintf(
		"SELECT [System.Id],[System.Title] FROM WorkItems WHERE [System.WorkItemType]='Test Case' AND [System.TeamProject]='%s' ORDER BY [System.ChangedDate] DESC",
		project,
	)
	ids, err := azClient.QueryWorkItems(ctx, project, wiql)
	if err != nil {
		return nil, err
	}
	if len(ids) > maxEx {
		ids = ids[:maxEx]
	}
	examples := make([]string, 0, len(ids))
	for _, id := range ids {
		twi, err := fetchTestWorkItem(ctx, pat, org, project, id)
		if err == nil {
			examples = append(examples, fmt.Sprintf("- #%d %s", id, twi.Title()))
		}
	}
	return examples, nil
}

var runTestLLM = func(ctx context.Context, cfg config.Config, systemPrompt, userPrompt string) (string, string, string, error) {
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
	return fallback.Chat(ctx, systemPrompt, userPrompt)
}

var testIsTerminal = func(r io.Reader) bool {
	f, ok := r.(*os.File)
	if !ok {
		return false
	}
	return isTerminal(f)
}

var createAzureTestCase = func(ctx context.Context, pat, org, project string, req azure.CreateTestCaseRequest) (*azure.WorkItem, error) {
	return azure.NewClient(pat, org).CreateTestCase(ctx, project, req)
}

var updateAzureWorkItemToTestQA = func(ctx context.Context, pat, org, project string, wiID int, effort, realEffort *float64) error {
	return azure.NewClient(pat, org).UpdateWorkItemToTestQA(ctx, project, wiID, effort, realEffort)
}

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
	debug      bool
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

	cmd.Flags().StringVar(&flags.workItem, "work-item", "", "Parent work item ID")
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
	cmd.Flags().BoolVar(&flags.debug, "debug", false, "Show diagnostic details")

	return cmd
}

func runTest(ctx context.Context, cfg *config.Config, flags testFlagSet, cmd *cobra.Command) error {
	stderr := cmd.ErrOrStderr()
	out := cmd.OutOrStdout()
	input := cmd.InOrStdin()
	inputScanner := bufio.NewScanner(input)

	ui.Init(stderr)
	ui.Title(stderr, "Gerando card de teste...")

	if flags.debug {
		ui.Info(stderr, fmt.Sprintf("work-item: %s", flags.workItem))
		ui.Info(stderr, fmt.Sprintf("pr: %d", flags.pr))
		ui.Info(stderr, fmt.Sprintf("org flag: %q  project flag: %q  repo flag: %q", flags.org, flags.project, flags.repo))
		ui.Info(stderr, fmt.Sprintf("providers: %s", cfg.Providers))
		ui.Info(stderr, fmt.Sprintf("azure pat set: %v", cfg.AzurePAT != ""))
	}

	stepDependencies := ui.StepMessage(stderr, "Validando dependencias")
	stepDependencies(true, "Dependencias validadas")

	stepConfig := ui.StepMessage(stderr, "Carregando configuracao")
	stepConfig(true, "Configuracao carregada")

	stepPAT := ui.StepMessage(stderr, "Validando Azure PAT")
	if cfg.AzurePAT == "" {
		stepPAT(false, "Validando Azure PAT")
		return fmt.Errorf("configuracao incompleta: Azure PAT não configurado")
	}
	stepPAT(true, "Azure PAT validado")

	stepKeys := ui.StepMessage(stderr, "Validando API keys")
	if !flags.dryRun && !hasTestAPIKeys(*cfg) {
		stepKeys(false, "Validando API keys")
		return fmt.Errorf("configuracao incompleta: nenhuma API key disponivel")
	}
	stepKeys(true, "API keys validadas")

	stepGit := ui.StepMessage(stderr, "Detectando contexto git")
	gitCtx, err := collectTestGitContext(ctx)
	if err != nil {
		stepGit(false, "Detectando contexto git")
		return fmt.Errorf("git context: %w", err)
	}
	stepGit(true, "Contexto git detectado")

	// Resolve org/project/repo from flags or git remote
	org := flags.org
	project := flags.project
	repo := flags.repo

	stepCtx := ui.StepMessage(stderr, "Resolvendo contexto Azure DevOps")
	if org == "" {
		org = gitCtx.AzureOrg
	}
	if project == "" {
		project = gitCtx.AzureProject
	}
	if repo == "" {
		repo = gitCtx.AzureRepo
	}
	if org == "" || project == "" || repo == "" {
		stepCtx(false, "Resolvendo contexto Azure DevOps")
		return fmt.Errorf("contexto Azure DevOps incompleto")
	}
	stepCtx(true, fmt.Sprintf("Azure DevOps: %s/%s", org, project))

	if flags.debug {
		ui.Info(stderr, fmt.Sprintf("resolved → org: %q  project: %q  repo: %q", org, project, repo))
	}

	stepPR := ui.StepMessage(stderr, "Resolvendo PR")
	if flags.pr <= 0 {
		stepPR(false, "Resolvendo PR")
		return fmt.Errorf("PR não informado. Use --pr explicitamente.")
	}
	pr, err := fetchTestPullRequest(ctx, cfg.AzurePAT, org, project, repo, flags.pr)
	if err != nil {
		stepPR(false, "Resolvendo PR")
		return fmt.Errorf("get pull request: %w", err)
	}
	stepPR(true, fmt.Sprintf("PR: #%d — %s", pr.ID, pr.Title))

	stepWI := ui.StepMessage(stderr, "Resolvendo work item")
	wiID, err := resolveTestWorkItemID(ctx, cfg, flags, org, project, repo)
	if err != nil {
		stepWI(false, "Resolvendo work item")
		return err
	}

	// Get work item details from Azure DevOps
	wi, err := fetchTestWorkItem(ctx, cfg.AzurePAT, org, project, wiID)
	if err != nil {
		stepWI(false, "Resolvendo work item")
		return fmt.Errorf("get work item: %w", err)
	}
	stepWI(true, fmt.Sprintf("Work item: #%d — %s", wiID, wi.Title()))

	stepChanges := ui.StepMessage(stderr, "Buscando alteracoes do PR")
	var changedFiles []azure.PRChange
	iters, err := fetchTestPRIterations(ctx, cfg.AzurePAT, org, project, repo, flags.pr)
	if err == nil && len(iters) > 0 {
		lastIter := iters[len(iters)-1]
		changedFiles, _ = fetchTestPRChanges(ctx, cfg.AzurePAT, org, project, repo, flags.pr, lastIter.ID)
	}
	stepChanges(true, "Alteracoes coletadas")

	// Fetch example test cases via WIQL
	stepExamples := ui.StepMessage(stderr, "Buscando exemplos de test case")
	var examples []string
	maxEx := flags.examples
	if maxEx > 5 {
		maxEx = 5
	}
	if maxEx > 0 {
		examples, err = queryTestExampleWorkItems(ctx, cfg.AzurePAT, org, project, maxEx)
		if err != nil {
			examples = nil
		}
	}
	stepExamples(true, "Exemplos coletados")

	stepFields := ui.StepMessage(stderr, "Preparando campos de criacao")
	tcReq := buildCreateTestCaseRequest(flags, wiID, wi)
	stepFields(true, "Campos resolvidos")

	// Build prompt
	userPrompt := buildTestPrompt(wi, wiID, pr, changedFiles, examples, flags)
	systemPrompt := testSystemPrompt
	configuredProvider, configuredModel := testConfiguredProviderModel(*cfg)

	stepLLM := ui.StepMessage(stderr, "Gerando card via LLM")
	if flags.dryRun {
		stepLLM(true, fmt.Sprintf("Card gerado (%s/%s)", configuredProvider, configuredModel))
		ui.TitleDone(stderr)
		printTestBlockClose(stderr)
		_, _ = fmt.Fprintln(out, "[SYSTEM]")
		_, _ = fmt.Fprintln(out, systemPrompt)
		_, _ = fmt.Fprintln(out)
		_, _ = fmt.Fprintln(out, "[USER]")
		_, _ = fmt.Fprintln(out, userPrompt)
		_, _ = fmt.Fprintln(out)
		_, _ = fmt.Fprintln(out, "[CREATE PREVIEW]")
		printTestCardSummary(out, flags, pr, wi, configuredProvider, configuredModel, "", "")
		return nil
	}

	// Call LLM
	resp, provider, model, err := runTestLLM(ctx, *cfg, systemPrompt, userPrompt)
	if err != nil {
		stepLLM(false, "Gerando card via LLM")
		return fmt.Errorf("LLM call failed: %w", err)
	}
	stepLLM(true, fmt.Sprintf("Card gerado (%s/%s)", provider, model))
	ui.TitleDone(stderr)
	printTestBlockClose(stderr)

	// Strip <think> blocks
	resp = stripThinkBlocks(resp)

	// Parse response
	title, body := parseTitleAndBody(resp, "")

	// Output
	if flags.raw {
		_, _ = fmt.Fprintln(out, body)
		return nil
	}

	printTestCardSummary(out, flags, pr, wi, provider, model, title, body)

	// Interactive test case creation
	if !flags.noCreate {
		publishTestCard(ctx, stderr, inputScanner, *cfg, flags, pr, wi, tcReq, title, body, org, project, testIsTerminal(input))
	}

	return nil
}

func hasTestAPIKeys(cfg config.Config) bool {
	return cfg.OpenRouterAPIKey != "" || cfg.GroqAPIKey != "" || cfg.GeminiAPIKey != "" || cfg.OllamaAPIKey != ""
}

func testConfiguredProviderModel(cfg config.Config) (string, string) {
	providers := strings.Split(cfg.Providers, ",")
	for _, provider := range providers {
		provider = strings.TrimSpace(strings.ToLower(provider))
		if provider == "" || !testProviderAvailable(cfg, provider) {
			continue
		}
		return provider, testConfiguredModel(cfg, provider)
	}

	for _, provider := range []string{"openrouter", "groq", "gemini", "ollama"} {
		if testProviderAvailable(cfg, provider) {
			return provider, testConfiguredModel(cfg, provider)
		}
	}

	return "default", "default"
}

func testProviderAvailable(cfg config.Config, provider string) bool {
	switch provider {
	case "openrouter":
		return cfg.OpenRouterAPIKey != ""
	case "groq":
		return cfg.GroqAPIKey != ""
	case "gemini":
		return cfg.GeminiAPIKey != ""
	case "ollama":
		return cfg.OllamaAPIKey != ""
	default:
		return false
	}
}

func testConfiguredModel(cfg config.Config, provider string) string {
	switch provider {
	case "openrouter":
		return nonEmpty(cfg.OpenRouterModel, "default")
	case "groq":
		return nonEmpty(cfg.GroqModel, "default")
	case "gemini":
		return nonEmpty(cfg.GeminiModel, "default")
	case "ollama":
		return nonEmpty(cfg.OllamaModel, "default")
	default:
		return "default"
	}
}

func nonEmpty(value, fallback string) string {
	if strings.TrimSpace(value) == "" {
		return fallback
	}
	return value
}

func buildCreateTestCaseRequest(flags testFlagSet, wiID int, wi *azure.WorkItem) azure.CreateTestCaseRequest {
	priority := 2
	iterationPath := ""
	if wi != nil {
		iterationPath = wi.Field("System.IterationPath")
	}

	return azure.CreateTestCaseRequest{
		AreaPath:      flags.areaPath,
		AssignedTo:    flags.assignedTo,
		ParentID:      wiID,
		IterationPath: iterationPath,
		Priority:      &priority,
		Team:          "DevOps",
		Program:       "Agrotrace",
	}
}

func printTestCardSummary(w io.Writer, flags testFlagSet, pr *azure.PullRequest, wi *azure.WorkItem, provider, model, title, body string) {
	prID := 0
	if pr != nil {
		prID = pr.ID
	}
	_, _ = fmt.Fprintf(w, "Test Card — PR #%d\n", prID)
	_, _ = fmt.Fprintf(w, "Provider: %s/%s\n", provider, model)
	if wi != nil {
		_, _ = fmt.Fprintf(w, "Work Item: #%d — %s\n", wi.ID, wi.Title())
	}
	_, _ = fmt.Fprintf(w, "AreaPath: %s\n", flags.areaPath)
	if flags.assignedTo != "" {
		_, _ = fmt.Fprintf(w, "Responsável: %s\n", flags.assignedTo)
	}
	if title != "" {
		_, _ = fmt.Fprintf(w, "Título: %s\n", title)
	}
	_, _ = fmt.Fprintln(w)
	if body != "" {
		_, _ = fmt.Fprintf(w, "%s\n", body)
	}
}

func publishTestCard(ctx context.Context, stderr io.Writer, scanner *bufio.Scanner, cfg config.Config, flags testFlagSet, pr *azure.PullRequest, wi *azure.WorkItem, req azure.CreateTestCaseRequest, title, body, org, project string, interactive bool) {
	if pr == nil || wi == nil || cfg.AzurePAT == "" || org == "" || project == "" {
		return
	}

	ui.Title(stderr, "Publicar no Azure DevOps")
	defer func() {
		ui.TitleDone(stderr)
		printTestBlockClose(stderr)
	}()

	ui.Info(stderr, "")
	if !interactive {
		ui.Info(stderr, "Ambiente não interativo; pulando criacao automatica do Test Case. Rode interativamente para confirmar a criacao.")
		return
	}

	ui.Info(stderr, "Criar este Test Case no Azure DevOps?")
	if !scanner.Scan() || !isAffirmative(scanner.Text()) {
		ui.Info(stderr, "(cancelado)")
		return
	}

	req.Title = title
	req.DescriptionHTML = body
	stepCreate := ui.StepMessage(stderr, "Criando test case no Azure DevOps")
	tc, err := createAzureTestCase(ctx, cfg.AzurePAT, org, project, req)
	if err != nil {
		stepCreate(false, "Falha ao criar test case")
		printTestCaseCreateFallback(stderr, err, req)
		return
	}
	stepCreate(true, fmt.Sprintf("Test case criado: #%d", tc.ID))
	ui.Info(stderr, fmt.Sprintf("https://dev.azure.com/%s/%s/_workitems/edit/%d", org, project, tc.ID))

	ui.Info(stderr, "")
	ui.Info(stderr, fmt.Sprintf("Atualizar o work item #%d para Test QA?", wi.ID))
	if !scanner.Scan() || !isAffirmative(scanner.Text()) {
		ui.Info(stderr, "(cancelado)")
		return
	}

	currentWI, err := fetchTestWorkItem(ctx, cfg.AzurePAT, org, project, wi.ID)
	if err != nil {
		currentWI = wi
	}

	missingEffort, missingRealEffort, realEffortDefault := missingTestQAEffortFields(currentWI)
	var effort *float64
	var realEffort *float64
	if missingEffort {
		effortValue := promptFloat(scanner, stderr, "Effort (horas decimais, ex: 0.5) [0.5]:", 0.5)
		effort = &effortValue
		realEffortDefault = effortValue
	}
	if missingRealEffort {
		realEffortValue := promptFloat(scanner, stderr, fmt.Sprintf("Real Effort (horas decimais) [%s]:", formatDecimal(realEffortDefault)), realEffortDefault)
		realEffort = &realEffortValue
	}

	if err := updateAzureWorkItemToTestQA(ctx, cfg.AzurePAT, org, project, wi.ID, effort, realEffort); err != nil {
		ui.Error(stderr, "Falha ao atualizar work item")
		return
	}
	ui.Success(stderr, fmt.Sprintf("Work item #%d atualizado para Test QA", wi.ID))
}

func missingTestQAEffortFields(wi *azure.WorkItem) (bool, bool, float64) {
	if wi == nil {
		return false, false, 0.5
	}
	effortValue, hasEffort := workItemFloatField(wi, "Microsoft.VSTS.Scheduling.Effort")
	_, hasRealEffort := workItemFloatField(wi, "Custom.RealEffort")
	if !hasEffort {
		effortValue = 0.5
	}
	return !hasEffort, !hasRealEffort, effortValue
}

func workItemFloatField(wi *azure.WorkItem, key string) (float64, bool) {
	value, ok := wi.Fields[key]
	if !ok {
		return 0, false
	}
	switch typed := value.(type) {
	case float64:
		return typed, true
	case float32:
		return float64(typed), true
	case int:
		return float64(typed), true
	case int32:
		return float64(typed), true
	case int64:
		return float64(typed), true
	default:
		return 0, false
	}
}

func promptFloat(scanner *bufio.Scanner, stderr io.Writer, prompt string, fallback float64) float64 {
	ui.Info(stderr, prompt)
	if !scanner.Scan() {
		return fallback
	}
	value := strings.TrimSpace(scanner.Text())
	if value == "" {
		return fallback
	}
	parsed, err := strconv.ParseFloat(value, 64)
	if err != nil {
		return fallback
	}
	return parsed
}

func formatDecimal(value float64) string {
	return strconv.FormatFloat(value, 'f', -1, 64)
}

func printTestCaseCreateFallback(stderr io.Writer, err error, req azure.CreateTestCaseRequest) {
	ui.Warn(stderr, "Não foi possivel criar o Test Case automaticamente")
	ui.Info(stderr, err.Error())
	ui.Info(stderr, "Campos tentados na criacao:")
	ui.Info(stderr, fmt.Sprintf("AreaPath: %s", req.AreaPath))
	ui.Info(stderr, fmt.Sprintf("IterationPath: %s", req.IterationPath))
	if req.Priority != nil {
		ui.Info(stderr, fmt.Sprintf("Priority: %d", *req.Priority))
	} else {
		ui.Info(stderr, "Priority:")
	}
	ui.Info(stderr, fmt.Sprintf("Custom.Team: %s", req.Team))
	ui.Info(stderr, fmt.Sprintf("Custom.ProgramasAgrotrace: %s", req.Program))
	ui.Info(stderr, fmt.Sprintf("AssignedTo: %s", req.AssignedTo))
	ui.Info(stderr, fmt.Sprintf("Parent: %d", req.ParentID))
	ui.Info(stderr, "Use o Markdown acima para criar o card manualmente no Azure DevOps.")
}

func printTestBlockClose(w io.Writer) {
	_, _ = fmt.Fprintf(w, "  %s└%s\n", ui.OrangeDim, ui.Reset)
}

func isAffirmative(input string) bool {
	answer := strings.TrimSpace(strings.ToLower(input))
	return answer == "y" || answer == "yes" || answer == "s" || answer == "sim"
}

func resolveTestWorkItemID(ctx context.Context, cfg *config.Config, flags testFlagSet, org, project, repo string) (int, error) {
	if flags.workItem != "" {
		wiID, err := strconv.Atoi(flags.workItem)
		if err != nil {
			return 0, fmt.Errorf("invalid work item ID %q: %w", flags.workItem, err)
		}
		return wiID, nil
	}

	if flags.pr > 0 && cfg.AzurePAT != "" && org != "" && project != "" && repo != "" {
		ids, err := fetchTestPullRequestWorkItemIDs(ctx, cfg.AzurePAT, org, project, repo, flags.pr)
		if err != nil {
			return 0, err
		}
		if wiID, ok := selectParentWorkItemID(ctx, cfg, org, project, ids); ok {
			return wiID, nil
		}
	}

	return 0, fmt.Errorf("Não foi possível resolver o work item pai. Use --work-item explicitamente.")
}

func selectParentWorkItemID(ctx context.Context, cfg *config.Config, org, project string, ids []int) (int, bool) {
	bestAny := 0
	bestNonTestCase := 0
	for _, id := range ids {
		wi, err := fetchTestWorkItem(ctx, cfg.AzurePAT, org, project, id)
		if err != nil || wi == nil {
			continue
		}
		if bestAny == 0 || wi.ID < bestAny {
			bestAny = wi.ID
		}
		if wi.Type() == "Test Case" {
			continue
		}
		if bestNonTestCase == 0 || wi.ID < bestNonTestCase {
			bestNonTestCase = wi.ID
		}
	}
	if bestNonTestCase != 0 {
		return bestNonTestCase, true
	}
	if bestAny != 0 {
		return bestAny, true
	}
	return 0, false
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
