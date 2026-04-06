package cli

import (
	"bufio"
	"context"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/azure"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/clipboard"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/git"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/llm"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/ui"
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
	source   string
	workItem string
	dryRun   bool
	raw      bool
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
	// Keep --create for backward compat but hidden; interactivity is now automatic
	cmd.Flags().Bool("create", false, "Create PR in Azure DevOps (deprecated: now automatic when interactive)")
	_ = cmd.Flags().MarkHidden("create")

	return cmd
}

func runDesc(ctx context.Context, cfg *config.Config, flags descFlagSet, cmd *cobra.Command) error {
	stderr := cmd.ErrOrStderr()
	stdout := cmd.OutOrStdout()

	ui.Init(stderr)

	// Collect git context
	ui.Title(stderr, "Gerando PR description...")
	stepGit := ui.Step(stderr, "Coletando contexto git")

	gitCtx := git.NewContext(git.ExecRunner{})
	if err := gitCtx.Collect(ctx, flags.source); err != nil {
		stepGit(false)
		return fmt.Errorf("git context: %w", err)
	}
	stepGit(true)

	// Load system prompt
	systemPrompt := loadDescTemplate(cfg)

	// Build user prompt
	userPrompt := buildDescPrompt(gitCtx)

	if flags.dryRun {
		_, _ = fmt.Fprintln(stdout, "=== SYSTEM ===")
		_, _ = fmt.Fprintln(stdout, systemPrompt)
		_, _ = fmt.Fprintln(stdout, "\n=== USER ===")
		_, _ = fmt.Fprintln(stdout, userPrompt)
		return nil
	}

	// Call LLM with fallback
	stepLLM := ui.Step(stderr, "Chamando LLM")
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
	resp, provider, err := fallback.Chat(ctx, systemPrompt, userPrompt)
	if err != nil {
		stepLLM(false)
		return fmt.Errorf("LLM call failed: %w", err)
	}
	stepLLM(true)

	ui.TitleDone(stderr)

	// Strip <think> blocks
	resp = stripThinkBlocks(resp)

	// Parse title and body
	title, body := parseTitleAndBody(resp, gitCtx.BranchName)

	// Print summary header to stderr
	_, _ = fmt.Fprintf(stderr, "\n %s%s✦%s %sPR — %s%s\n", ui.Orange, ui.Bold, ui.Reset, ui.OrangeDim, gitCtx.BranchName, ui.Reset)
	_, _ = fmt.Fprintf(stderr, "  %s│%s Provider: %s\n", ui.OrangeDim, ui.Reset, provider)
	if workItemID := gitCtx.WorkItemID; workItemID != "" {
		_, _ = fmt.Fprintf(stderr, "  %s│%s Work Item: #%s\n", ui.OrangeDim, ui.Reset, workItemID)
	}
	_, _ = fmt.Fprintf(stderr, "  %s└%s\n", ui.OrangeDim, ui.Reset)

	// Print result to stdout
	_, _ = fmt.Fprintf(stdout, "\nTitulo: %s%s%s\n\n", ui.Cyan, title, ui.Reset)
	_, _ = fmt.Fprintf(stdout, "Descricao:\n%s\n", body)

	// Copy body to clipboard (best effort)
	if err := clipboard.Write(body); err == nil {
		_, _ = fmt.Fprintf(stderr, "\n%s✓%s Copiado para clipboard\n", ui.Green, ui.Reset)
	}

	// Interactive PR creation
	if isTerminal(os.Stdin) && gitCtx.IsAzureDevOps && cfg.AzurePAT != "" {
		_, _ = fmt.Fprintf(stderr, "\n  %s│%s\n", ui.OrangeDim, ui.Reset)
		_, _ = fmt.Fprintf(stderr, "  Criar PR(s) no Azure DevOps? [y/N]: ")

		scanner := bufio.NewScanner(os.Stdin)
		if scanner.Scan() {
			answer := strings.TrimSpace(strings.ToLower(scanner.Text()))
			if answer == "y" || answer == "yes" {
				azClient := azure.NewClient(cfg.AzurePAT, gitCtx.AzureOrg)

				// Default reviewer
				reviewer := cfg.PRReviewerDev
				_, _ = fmt.Fprintf(stderr, "  Reviewer (email) [%s]: ", reviewer)
				if scanner.Scan() {
					if input := strings.TrimSpace(scanner.Text()); input != "" {
						reviewer = input
					}
				}

				stepPR := ui.Step(stderr, fmt.Sprintf("Criando PR → %s", gitCtx.BaseBranch))
				prReq := azure.CreatePRRequest{
					Title:       title,
					Description: body,
					SourceRef:   "refs/heads/" + gitCtx.SourceBranch,
					TargetRef:   "refs/heads/" + gitCtx.BaseBranch,
				}
				if reviewer != "" {
					prReq.Reviewers = []azure.PRReviewer{{UniqueName: reviewer}}
				}
				pr, prErr := azClient.CreatePullRequest(ctx, gitCtx.AzureProject, gitCtx.AzureRepo, prReq)
				if prErr != nil {
					stepPR(false)
					ui.Error(stderr, fmt.Sprintf("Erro ao criar PR: %v", prErr))
				} else {
					stepPR(true)
					_, _ = fmt.Fprintf(stderr, "  %s│%s   %s\n", ui.OrangeDim, ui.Reset, pr.URL)
				}
			}
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

func buildDescPrompt(gc *git.Context) string {
	var b strings.Builder
	b.WriteString("## Contexto Git\n\n")
	_, _ = fmt.Fprintf(&b, "**Branch:** %s\n", gc.BranchName)
	_, _ = fmt.Fprintf(&b, "**Base:** %s\n", gc.BaseBranch)
	_, _ = fmt.Fprintf(&b, "**Base branches alvo:** %s\n", gc.BaseBranch)
	if gc.WorkItemID != "" {
		_, _ = fmt.Fprintf(&b, "**Work Item:** %s\n", gc.WorkItemID)
	}
	_, _ = fmt.Fprintf(&b, "\n**Diff:**\n```\n%s\n```\n", gc.Diff)
	_, _ = fmt.Fprintf(&b, "\n**Log:**\n```\n%s\n```\n", gc.Log)
	return b.String()
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
