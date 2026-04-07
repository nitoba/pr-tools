package cli

import (
	"bufio"
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/azure"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/clipboard"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/git"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/llm"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/ui"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/wizard"
	"github.com/spf13/cobra"
)

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
	source             string
	targets            []string
	workItem           string
	dryRun             bool
	raw                bool
	debug              bool
	setOpenRouterModel string
	setGroqModel       string
	setGeminiModel     string
	setOllamaModel     string
}

var collectDescGitContext = func(ctx context.Context, source string) (*git.Context, error) {
	gitCtx := git.NewContext(git.ExecRunner{})
	if err := gitCtx.Collect(ctx, source); err != nil {
		return nil, err
	}
	return gitCtx, nil
}

var fetchDescWorkItem = func(ctx context.Context, pat, org, project, workItemID string) (*azure.WorkItem, error) {
	wiID, err := strconv.Atoi(workItemID)
	if err != nil {
		return nil, err
	}
	return azure.NewClient(pat, org).GetWorkItem(ctx, project, wiID)
}

var loadDescTemplateFn = loadDescTemplate
var descInitUI = ui.Init

var runDescLLM = func(ctx context.Context, cfg config.Config, systemPrompt, userPrompt string, onTrying func(string, string), onFailed func(string, error)) (string, string, string, error) {
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
	fallback.OnTrying = onTrying
	fallback.OnFailed = onFailed
	return fallback.Chat(ctx, systemPrompt, userPrompt)
}

var descClipboardWrite = clipboard.Write

var descIsTerminal = func(r io.Reader) bool {
	f, ok := r.(*os.File)
	if !ok {
		return false
	}
	return isTerminal(f)
}

var descWriterIsTerminal = func(w io.Writer) bool {
	f, ok := w.(*os.File)
	if !ok {
		return false
	}
	return isTerminal(f)
}

var createDescPR = func(ctx context.Context, pat, org, project, repo, _ string, req azure.CreatePRRequest) (*azure.PullRequest, error) {
	return azure.NewClient(pat, org).CreatePullRequest(ctx, project, repo, req)
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
	cmd.Flags().StringArrayVar(&flags.targets, "target", nil, "Target branch for PR (can repeat; e.g. --target dev --target sprint)")
	cmd.Flags().StringVar(&flags.workItem, "work-item", "", "Work item ID")
	cmd.Flags().BoolVar(&flags.dryRun, "dry-run", false, "Show prompt without calling LLM")
	cmd.Flags().BoolVar(&flags.raw, "raw", false, "Output without markdown rendering")
	cmd.Flags().BoolVar(&flags.debug, "debug", false, "Show diagnostic details")
	cmd.Flags().StringVar(&flags.setOpenRouterModel, "set-openrouter-model", "", "Save OpenRouter model to config")
	cmd.Flags().StringVar(&flags.setGroqModel, "set-groq-model", "", "Save Groq model to config")
	cmd.Flags().StringVar(&flags.setGeminiModel, "set-gemini-model", "", "Save Gemini model to config")
	cmd.Flags().StringVar(&flags.setOllamaModel, "set-ollama-model", "", "Save Ollama model to config")
	// Keep --create for backward compat but hidden; interactivity is now automatic
	cmd.Flags().Bool("create", false, "Create PR in Azure DevOps (deprecated: now automatic when interactive)")
	_ = cmd.Flags().MarkHidden("create")

	return cmd
}

func runDesc(ctx context.Context, cfg *config.Config, flags descFlagSet, cmd *cobra.Command) error {
	stderr := cmd.ErrOrStderr()
	stdout := cmd.OutOrStdout()

	descInitUI(stderr)

	// Handle --set-*-model flags: save to .env and exit
	if flags.setOpenRouterModel != "" || flags.setGroqModel != "" ||
		flags.setGeminiModel != "" || flags.setOllamaModel != "" {
		return saveModels(stderr, flags)
	}

	ui.Title(stderr, "Gerando descrição do PR...")

	stepDependencies := ui.StepMessage(stderr, "Validando dependencias")
	stepDependencies(true, "Dependencias validadas")

	stepConfig := ui.StepMessage(stderr, "Carregando configuracao")
	stepConfig(true, "Configuracao carregada")

	stepKeys := ui.StepMessage(stderr, "Validando API keys")
	if !flags.dryRun && !hasDescAPIKeys(*cfg) {
		stepKeys(false, "Validando API keys")
		return fmt.Errorf("configuracao incompleta: nenhuma API key disponivel")
	}
	stepKeys(true, "API keys validadas")

	gitCtx, err := collectDescGitContext(ctx, flags.source)
	stepBranch := ui.StepMessage(stderr, "Validando branch")
	if err != nil {
		stepBranch(false, "Validando branch")
		return fmt.Errorf("git context: %w", err)
	}
	if gitCtx.BranchName == "" {
		stepBranch(false, "Validando branch")
		return fmt.Errorf("branch invalida")
	}
	stepBranch(true, "Branch validada")

	targets := resolveDescTargets(gitCtx, flags.targets)

	stepGit := ui.StepMessage(stderr, "Coletando contexto git")
	stepGit(true, fmt.Sprintf("Contexto git coletado (%s)", gitCtx.BranchName))
	if gitCtx.DiffTruncated {
		ui.Warn(stderr, fmt.Sprintf("Diff truncado: %d linhas -> 8000 linhas", gitCtx.DiffOriginalLines))
	}

	scanner := bufio.NewScanner(cmd.InOrStdin())
	interactive := descIsTerminal(cmd.InOrStdin())
	workItemID := flags.workItem
	stepWorkItem := ui.StepMessage(stderr, "Detectando work item")
	if workItemID == "" {
		workItemID = gitCtx.WorkItemID
	}
	if workItemID == "" && interactive {
		ui.Warn(stderr, fmt.Sprintf("Não foi possivel extrair o work item ID da branch '%s'.", gitCtx.BranchName))
		ui.Info(stderr, "ID do work item (Enter para pular):")
		if scanner.Scan() {
			workItemID = strings.TrimSpace(scanner.Text())
		}
	}
	gitCtx.WorkItemID = workItemID
	if workItemID != "" {
		stepWorkItem(true, fmt.Sprintf("Work item: #%s", workItemID))
	} else {
		stepWorkItem(true, "Sem work item detectado")
	}

	var wi *azure.WorkItem
	if workItemID != "" && cfg.AzurePAT != "" && gitCtx.AzureOrg != "" && gitCtx.AzureProject != "" {
		wi, _ = fetchDescWorkItem(ctx, cfg.AzurePAT, gitCtx.AzureOrg, gitCtx.AzureProject, workItemID)
	}

	stepSprint := ui.StepMessage(stderr, "Detectando sprint")
	sprint := detectDescSprint(gitCtx, wi)
	if sprint != "" {
		stepSprint(true, fmt.Sprintf("Sprint: %s", sprint))
	} else {
		stepSprint(true, "Sem sprint ativo")
	}

	stepRepo := ui.StepMessage(stderr, "Resolvendo repositório Azure DevOps")
	if gitCtx.IsAzureDevOps && gitCtx.AzureOrg != "" && gitCtx.AzureProject != "" && gitCtx.AzureRepo != "" {
		stepRepo(true, fmt.Sprintf("Repositório: %s/%s/%s", gitCtx.AzureOrg, gitCtx.AzureProject, gitCtx.AzureRepo))
	} else {
		stepRepo(true, "Repositório não-Azure (sem links de PR)")
	}

	systemPrompt := loadDescTemplateFn(cfg)
	userPrompt := buildDescPrompt(gitCtx, targets)
	configuredProvider, configuredModel := descConfiguredProviderModel(*cfg)
	debugEnabled := false
	if cmd.Flags().Changed("debug") {
		debugEnabled = flags.debug
	} else if cfg.Debug != nil && *cfg.Debug {
		debugEnabled = true
	}

	stepLLM := ui.StepMessage(stderr, "Gerando descrição via LLM")
	if flags.dryRun {
		stepLLM(true, fmt.Sprintf("Descrição gerada (%s/%s)", configuredProvider, configuredModel))
		ui.TitleDone(stderr)
		printDescBlockClose(stderr)
		_, _ = fmt.Fprintln(stdout, "────────────────────────────────────────")
		_, _ = fmt.Fprintln(stdout, "DRY RUN - Prompt que seria enviado ao LLM")
		_, _ = fmt.Fprintf(stdout, "Provider/Model: %s/%s\n", configuredProvider, configuredModel)
		_, _ = fmt.Fprintln(stdout, "────────────────────────────────────────")
		_, _ = fmt.Fprintln(stdout, "[SYSTEM]")
		_, _ = fmt.Fprintln(stdout, systemPrompt)
		_, _ = fmt.Fprintln(stdout)
		_, _ = fmt.Fprintln(stdout, "[USER]")
		_, _ = fmt.Fprintln(stdout, userPrompt)
		return nil
	}

	resp, provider, model, err := runDescLLM(ctx, *cfg, systemPrompt, userPrompt, func(name, model string) {
		ui.Info(stderr, fmt.Sprintf("Tentando provider: %s (%s)...", name, model))
	}, func(name string, _ error) {
		ui.Warn(stderr, fmt.Sprintf("Provider %s falhou. Tentando próximo...", name))
	})
	if err != nil {
		stepLLM(false, "Gerando descrição via LLM")
		ui.Error(stderr, "Todos os providers falharam")

		if debugEnabled {
			ui.Info(stderr, fmt.Sprintf("provider/model: %s/%s", configuredProvider, configuredModel))
			ui.Info(stderr, fmt.Sprintf("diff lines: %d", gitCtx.DiffOriginalLines))
			ui.Info(stderr, fmt.Sprintf("prompt chars: %d (%d system + %d user)", len(systemPrompt)+len(userPrompt), len(systemPrompt), len(userPrompt)))
			for _, line := range strings.Split(err.Error(), "\n") {
				line = strings.TrimSpace(line)
				if line == "" || strings.EqualFold(line, "todos os provedores falharam:") {
					continue
				}
				ui.Info(stderr, line)
			}
		}

		ui.TitleDone(stderr)
		printDescBlockClose(stderr)
		return fmt.Errorf("LLM call failed")
	}
	stepLLM(true, fmt.Sprintf("Descrição gerada (%s/%s)", provider, model))
	ui.TitleDone(stderr)
	printDescBlockClose(stderr)

	resp = stripThinkBlocks(resp)
	title, body := parseTitleAndBody(resp, gitCtx.BranchName)

	printDescSummary(stderr, gitCtx, targets, workItemID, provider, model)

	if flags.raw {
		_, _ = fmt.Fprintln(stdout, body)
	} else {
		titleColor, titleReset := descStdoutTitleColors(stdout)
		_, _ = fmt.Fprintf(stdout, "\n  Titulo: %s%s%s\n\n", titleColor, title, titleReset)
		_, _ = fmt.Fprintln(stdout, "  Descricao:")
		for _, line := range strings.Split(body, "\n") {
			if line == "" {
				_, _ = fmt.Fprintln(stdout, "  ")
				continue
			}
			_, _ = fmt.Fprintf(stdout, "  %s\n", line)
		}
	}

	if err := descClipboardWrite(body); err == nil {
		ui.Success(stderr, "Descrição copiada para o clipboard")
		ui.Info(stderr, "Título disponível acima para copiar manualmente.")
	} else {
		ui.Warn(stderr, "Clipboard não disponível (pbcopy/xclip/xsel não encontrado)")
	}
	ui.TitleDone(stderr)
	printDescBlockClose(stderr)

	if len(targets) > 0 && interactive && gitCtx.IsAzureDevOps && cfg.AzurePAT != "" {
		publishDescPRs(ctx, stderr, scanner, *cfg, gitCtx, targets, title, body)
	}

	return nil
}

func descStdoutTitleColors(w io.Writer) (string, string) {
	if !descWriterIsTerminal(w) {
		return "", ""
	}
	return ui.Cyan, ui.Reset
}

func hasDescAPIKeys(cfg config.Config) bool {
	return cfg.OpenRouterAPIKey != "" || cfg.GroqAPIKey != "" || cfg.GeminiAPIKey != "" || cfg.OllamaAPIKey != ""
}

func resolveDescTargets(gitCtx *git.Context, targets []string) []string {
	if len(targets) > 0 {
		return append([]string(nil), targets...)
	}

	resolved := make([]string, 0, 2)
	if gitCtx.SprintBranch != "" {
		resolved = append(resolved, gitCtx.SprintBranch)
	}
	if gitCtx.BaseBranch != "" && gitCtx.BaseBranch != gitCtx.SprintBranch {
		resolved = append(resolved, gitCtx.BaseBranch)
	}
	if len(resolved) == 0 && gitCtx.BaseBranch != "" {
		resolved = append(resolved, gitCtx.BaseBranch)
	}
	return resolved
}

func detectDescSprint(gitCtx *git.Context, wi *azure.WorkItem) string {
	if wi != nil {
		if sprint := wi.Sprint(); sprint != "" {
			return sprint
		}
	}
	if strings.HasPrefix(gitCtx.SprintBranch, "sprint/") {
		return strings.TrimPrefix(gitCtx.SprintBranch, "sprint/")
	}
	return ""
}

func descConfiguredProviderModel(cfg config.Config) (string, string) {
	providers := strings.Split(cfg.Providers, ",")
	for _, provider := range providers {
		provider = strings.TrimSpace(provider)
		if provider == "" {
			continue
		}
		if !descProviderUsable(cfg, provider) {
			continue
		}
		return provider, descConfiguredModel(cfg, provider)
	}

	switch {
	case cfg.OpenRouterAPIKey != "":
		return "openrouter", descConfiguredModel(cfg, "openrouter")
	case cfg.GroqAPIKey != "":
		return "groq", descConfiguredModel(cfg, "groq")
	case cfg.GeminiAPIKey != "":
		return "gemini", descConfiguredModel(cfg, "gemini")
	case cfg.OllamaAPIKey != "":
		return "ollama", descConfiguredModel(cfg, "ollama")
	default:
		return "default", "default"
	}
}

func descProviderUsable(cfg config.Config, provider string) bool {
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

func descConfiguredModel(cfg config.Config, provider string) string {
	switch provider {
	case "openrouter":
		if cfg.OpenRouterModel != "" {
			return cfg.OpenRouterModel
		}
	case "groq":
		if cfg.GroqModel != "" {
			return cfg.GroqModel
		}
	case "gemini":
		if cfg.GeminiModel != "" {
			return cfg.GeminiModel
		}
	case "ollama":
		if cfg.OllamaModel != "" {
			return cfg.OllamaModel
		}
	}
	return "default"
}

func printDescSummary(w io.Writer, gitCtx *git.Context, targets []string, workItemID, provider, model string) {
	ui.Title(w, fmt.Sprintf("PR — %s", gitCtx.BranchName))
	ui.Info(w, fmt.Sprintf("Target: %s", strings.Join(targets, ", ")))
	ui.Info(w, fmt.Sprintf("Provider: %s/%s", provider, model))
	if workItemID != "" {
		ui.Info(w, fmt.Sprintf("Work Item: #%s", workItemID))
		if gitCtx.IsAzureDevOps && gitCtx.AzureOrg != "" && gitCtx.AzureProject != "" {
			ui.Info(w, "Work Item:")
			ui.Info(w, fmt.Sprintf("  https://dev.azure.com/%s/%s/_workitems/edit/%s", gitCtx.AzureOrg, gitCtx.AzureProject, workItemID))
		}
	}
	if len(targets) > 0 && gitCtx.IsAzureDevOps && gitCtx.AzureOrg != "" && gitCtx.AzureProject != "" && gitCtx.AzureRepo != "" {
		ui.Info(w, "Abrir PR:")
		for _, target := range targets {
			ui.Info(w, fmt.Sprintf("  %s", target))
			ui.Info(w, fmt.Sprintf("    %s", descPreviewPRURL(gitCtx, target)))
		}
	}
}

func descPreviewPRURL(gitCtx *git.Context, target string) string {
	return fmt.Sprintf(
		"https://dev.azure.com/%s/%s/_git/%s/pullrequestcreate?sourceRef=refs/heads/%s&targetRef=refs/heads/%s",
		gitCtx.AzureOrg,
		gitCtx.AzureProject,
		gitCtx.AzureRepo,
		gitCtx.SourceBranch,
		target,
	)
}

func publishDescPRs(ctx context.Context, stderr io.Writer, scanner *bufio.Scanner, cfg config.Config, gitCtx *git.Context, targets []string, title, body string) {
	ui.Title(stderr, "Publicar no Azure DevOps")
	ui.Info(stderr, "")
	ui.Info(stderr, "Criar PR(s) no Azure DevOps? [y/N]")
	if !scanner.Scan() {
		ui.Info(stderr, "(cancelado)")
		ui.TitleDone(stderr)
		printDescBlockClose(stderr)
		return
	}
	answer := strings.TrimSpace(strings.ToLower(scanner.Text()))
	if answer != "y" && answer != "yes" && answer != "s" && answer != "sim" {
		ui.Info(stderr, "(cancelado)")
		ui.TitleDone(stderr)
		printDescBlockClose(stderr)
		return
	}

	for _, target := range targets {
		ui.Info(stderr, "")
		ui.Info(stderr, fmt.Sprintf("→ PR para %s", target))
		reviewer := cfg.PRReviewerDev
		if strings.Contains(target, "sprint") && cfg.PRReviewerSprint != "" {
			reviewer = cfg.PRReviewerSprint
		}
		prompt := "Reviewer (email) [Enter para deixar vazio]"
		if reviewer != "" {
			prompt = "Reviewer (email) [Enter para manter atual]"
		}
		ui.Info(stderr, prompt)
		if scanner.Scan() {
			if input := strings.TrimSpace(scanner.Text()); input != "" {
				reviewer = input
			}
		}

		stepPR := ui.StepMessage(stderr, fmt.Sprintf("Criando PR → %s", target))
		prReq := azure.CreatePRRequest{
			Title:       title,
			Description: body,
			SourceRef:   "refs/heads/" + gitCtx.SourceBranch,
			TargetRef:   "refs/heads/" + target,
		}
		if reviewer != "" {
			prReq.Reviewers = []azure.PRReviewer{{UniqueName: reviewer}}
		}
		pr, err := createDescPR(ctx, cfg.AzurePAT, gitCtx.AzureOrg, gitCtx.AzureProject, gitCtx.AzureRepo, target, prReq)
		if err != nil {
			stepPR(false, fmt.Sprintf("Falha ao criar PR → %s", target))
			ui.Info(stderr, err.Error())
			continue
		}
		stepPR(true, fmt.Sprintf("PR criado → %s", target))
		ui.Info(stderr, pr.URL)
	}

	ui.TitleDone(stderr)
	printDescBlockClose(stderr)
}

func printDescBlockClose(w io.Writer) {
	_, _ = fmt.Fprintf(w, "  %s└%s\n", ui.OrangeDim, ui.Reset)
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

// loadDescTemplate reads the PR description template from ~/.config/pr-tools/pr-template.md,
// falling back to the hardcoded constant.
func loadDescTemplate(cfg *config.Config) string {
	home, err := os.UserHomeDir()
	if err != nil {
		return descSystemPrompt
	}
	templatePath := filepath.Join(home, ".config", "pr-tools", "pr-template.md")
	data, err := os.ReadFile(templatePath)
	if err != nil {
		return descSystemPrompt
	}
	s := strings.TrimSpace(string(data))
	if s == "" {
		return descSystemPrompt
	}
	_ = cfg
	return s
}

func buildDescPrompt(gc *git.Context, targets []string) string {
	var b strings.Builder
	b.WriteString("## Contexto Git\n\n")
	_, _ = fmt.Fprintf(&b, "**Branch:** %s\n", gc.BranchName)
	_, _ = fmt.Fprintf(&b, "**Base branches alvo:** %s\n", strings.Join(targets, ", "))
	if gc.WorkItemID != "" {
		_, _ = fmt.Fprintf(&b, "**Work Item:** %s\n", gc.WorkItemID)
	}
	_, _ = fmt.Fprintf(&b, "\n**Diff:**\n```\n%s\n```\n", gc.Diff)
	_, _ = fmt.Fprintf(&b, "\n**Log:**\n```\n%s\n```\n", gc.Log)
	return b.String()
}

// saveModels writes model names to the config .env file and prints confirmation.
func saveModels(w io.Writer, flags descFlagSet) error {
	type modelSave struct {
		key   string
		value string
		label string
	}

	saves := []modelSave{
		{"OPENROUTER_MODEL", flags.setOpenRouterModel, "OpenRouter"},
		{"GROQ_MODEL", flags.setGroqModel, "Groq"},
		{"GEMINI_MODEL", flags.setGeminiModel, "Gemini"},
		{"OLLAMA_MODEL", flags.setOllamaModel, "Ollama"},
	}

	home, err := os.UserHomeDir()
	if err != nil {
		return fmt.Errorf("home dir: %w", err)
	}
	envPath := filepath.Join(home, ".config", "pr-tools", ".env")

	for _, s := range saves {
		if s.value == "" {
			continue
		}
		if err := wizard.SetEnvVar(envPath, s.key, s.value); err != nil {
			_, _ = fmt.Fprintf(w, "Erro ao salvar %s model: %v\n", s.label, err)
			continue
		}
		_, _ = fmt.Fprintf(w, "[OK] %s model salvo: %s\n", s.label, s.value)
	}
	return nil
}

var thinkBlockRe = regexp.MustCompile(`(?s)<think>.*?</think>`)
var thinkTagRe = regexp.MustCompile(`(?i)</?think>`)

// stripThinkBlocks removes <think>...</think> blocks and standalone tags.
func stripThinkBlocks(s string) string {
	s = thinkBlockRe.ReplaceAllString(s, "")
	s = thinkTagRe.ReplaceAllString(s, "")
	// Trim leading blank lines
	lines := strings.Split(s, "\n")
	start := 0
	for start < len(lines) && strings.TrimSpace(lines[start]) == "" {
		start++
	}
	return strings.TrimRight(strings.Join(lines[start:], "\n"), "\n")
}

func parseTitleAndBody(resp, branchName string) (title, body string) {
	lines := strings.Split(resp, "\n")
	for i, line := range lines {
		if strings.HasPrefix(strings.ToUpper(strings.TrimSpace(line)), "TITULO:") {
			idx := strings.Index(strings.ToUpper(line), "TITULO:")
			title = cleanTitle(line[idx+len("TITULO:"):])
			body = cleanBody(strings.Join(lines[i+1:], "\n"))
			return
		}
	}

	// Fallback 1: extract first sentence from ## Descrição section
	if t := extractDescTitle(resp); t != "" {
		title = cleanTitle(t)
		body = cleanBody(resp)
		return
	}

	// Fallback 2: use branch name
	if branchName != "" {
		title = cleanTitle(branchName)
		body = cleanBody(resp)
		return
	}

	// Last resort: first line
	if len(lines) > 0 {
		title = cleanTitle(lines[0])
		body = cleanBody(strings.Join(lines[1:], "\n"))
	}
	return
}

func cleanTitle(s string) string {
	s = strings.TrimSpace(s)
	s = strings.Trim(s, `"'`)
	s = strings.TrimRight(s, ".")
	return s
}

func cleanBody(s string) string {
	lines := strings.Split(s, "\n")
	// Remove leading blank lines
	start := 0
	for start < len(lines) && strings.TrimSpace(lines[start]) == "" {
		start++
	}
	// If first non-empty line is ---, skip it
	if start < len(lines) && strings.TrimSpace(lines[start]) == "---" {
		start++
	}
	// Remove leading blank lines again after ---
	for start < len(lines) && strings.TrimSpace(lines[start]) == "" {
		start++
	}
	return strings.TrimSpace(strings.Join(lines[start:], "\n"))
}

func extractDescTitle(s string) string {
	lines := strings.Split(s, "\n")
	inDesc := false
	for _, line := range lines {
		if strings.HasPrefix(strings.TrimSpace(line), "## Descrição") ||
			strings.HasPrefix(strings.TrimSpace(line), "## Descricao") {
			inDesc = true
			continue
		}
		if inDesc {
			if strings.TrimSpace(line) == "" {
				continue
			}
			if strings.HasPrefix(line, "#") {
				break
			}
			// First sentence (truncate to 80 chars)
			sentence := strings.TrimSpace(line)
			if idx := strings.IndexAny(sentence, ".!?"); idx >= 0 {
				sentence = sentence[:idx+1]
			}
			if len(sentence) > 80 {
				sentence = sentence[:80]
			}
			return sentence
		}
	}
	return ""
}

// isTerminal reports whether the given file is a terminal.
func isTerminal(f *os.File) bool {
	return isTerminalFd(int(f.Fd()))
}
