// Package wizard implements the interactive setup wizard for "prt init".
package wizard

import (
	"bufio"
	"bytes"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"golang.org/x/term"
)

// ─── colour helpers ──────────────────────────────────────────────────────────

const (
	colorReset = "\033[0m"
	colorGreen = "\033[32m"
	colorRed   = "\033[31m"
	colorCyan  = "\033[36m"
)

func green(s string) string { return colorGreen + s + colorReset }
func red(s string) string   { return colorRed + s + colorReset }
func cyan(s string) string  { return colorCyan + s + colorReset }

// ─── public entry-point ──────────────────────────────────────────────────────

// Run executes the interactive setup wizard.
// stdin / stderr are the streams used for prompts; they may be injected for testing.
// envPath is the absolute path to the .env file that will be read and written.
func Run(stdin io.Reader, stderr io.Writer, envPath string) error {
	fprintln(stderr, cyan("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"))
	fprintln(stderr, cyan(" PRT — Setup Wizard"))
	fprintln(stderr, cyan("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"))

	// Ensure pr-template.md exists in the same directory as the .env file
	ensurePRTemplate(stderr, envPath)

	cfg := loadEnv(envPath)

	everythingSet := cfg["OPENROUTER_API_KEY"] != "" &&
		cfg["GROQ_API_KEY"] != "" &&
		cfg["GEMINI_API_KEY"] != "" &&
		cfg["OLLAMA_API_KEY"] != "" &&
		cfg["AZURE_PAT"] != "" &&
		cfg["PR_REVIEWER_DEV"] != "" &&
		cfg["PR_REVIEWER_SPRINT"] != ""

	if everythingSet {
		fprintln(stderr, "\n[INFO] Configuracao atual:")
		printMaskedSummary(stderr, cfg)
		_, _ = fmt.Fprint(stderr, "\nDeseja alterar alguma configuracao? [y/N]: ")
		answer := readLine(stdin)
		if !isYes(answer) {
			fprintln(stderr, "[OK] Nenhuma alteracao feita.")
			return nil
		}
		// Ask everything
		cfg = map[string]string{}
	} else if hasAnyExisting(cfg) {
		fprintln(stderr, "\n[INFO] Configuracao existente detectada. Apenas itens faltantes serao perguntados.")
	}

	// ── LLM Providers ────────────────────────────────────────────────────────
	anyProviderMissing := cfg["OPENROUTER_API_KEY"] == "" ||
		cfg["GROQ_API_KEY"] == "" ||
		cfg["GEMINI_API_KEY"] == "" ||
		cfg["OLLAMA_API_KEY"] == ""

	if anyProviderMissing {
		fprintln(stderr, "\n[PROVIDERS] Configurar provedores LLM")
		fprintln(stderr, "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")

		if cfg["OPENROUTER_API_KEY"] == "" {
			if key := configureProvider(stdin, stderr, envPath, providerOpenRouter); key != "" {
				cfg["OPENROUTER_API_KEY"] = key
			}
		}
		if cfg["GROQ_API_KEY"] == "" {
			if key := configureProvider(stdin, stderr, envPath, providerGroq); key != "" {
				cfg["GROQ_API_KEY"] = key
			}
		}
		if cfg["GEMINI_API_KEY"] == "" {
			if key := configureProvider(stdin, stderr, envPath, providerGemini); key != "" {
				cfg["GEMINI_API_KEY"] = key
			}
		}
		if cfg["OLLAMA_API_KEY"] == "" {
			if key := configureProvider(stdin, stderr, envPath, providerOllama); key != "" {
				cfg["OLLAMA_API_KEY"] = key
			}
		}

		// Write PR_PROVIDERS if not already set
		if loadEnv(envPath)["PR_PROVIDERS"] == "" {
			_ = SetEnvVar(envPath, "PR_PROVIDERS", "openrouter,groq,gemini,ollama")
		}
	}

	// ── Azure DevOps PAT ─────────────────────────────────────────────────────
	if cfg["AZURE_PAT"] == "" {
		fprintln(stderr, "\n[AZURE] Configurar Azure DevOps")
		fprintln(stderr, "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")
		_, _ = fmt.Fprint(stderr, "Configurar Azure DevOps PAT? [Y/n]: ")
		if answer := readLine(stdin); !isNo(answer) {
			fprintln(stderr, "  Gere seu PAT em: https://dev.azure.com → User Settings → Personal Access Tokens")
			_, _ = fmt.Fprint(stderr, "PAT Token: ")
			pat := readSecret(stdin, stderr)
			if pat != "" {
				_ = SetEnvVar(envPath, "AZURE_PAT", pat)
				fprintln(stderr, green("  [OK] PAT salvo."))
			}
		}
	}

	// ── Reviewers ────────────────────────────────────────────────────────────
	fprintln(stderr, "\n[REVIEWERS] Emails dos reviewers padrao para criacao automatica de PRs.")
	fprintln(stderr, "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")

	configureReviewer(stdin, stderr, envPath, "PR_REVIEWER_DEV",
		"Reviewer para PRs -> dev (email)", cfg["PR_REVIEWER_DEV"])
	configureReviewer(stdin, stderr, envPath, "PR_REVIEWER_SPRINT",
		"Reviewer para PRs -> sprint (email)", cfg["PR_REVIEWER_SPRINT"])

	fprintln(stderr, "\n[OK] Configuracao atualizada em ~/.config/pr-tools/.env")
	fprintln(stderr, "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")
	return nil
}

// ─── provider definitions ────────────────────────────────────────────────────

type providerDef struct {
	name    string
	envKey  string
	infoURL string
	test    func(string) bool
}

var (
	providerOpenRouter = providerDef{
		name:    "OpenRouter",
		envKey:  "OPENROUTER_API_KEY",
		infoURL: "https://openrouter.ai/keys",
		test:    testOpenRouter,
	}
	providerGroq = providerDef{
		name:    "Groq",
		envKey:  "GROQ_API_KEY",
		infoURL: "https://console.groq.com/keys",
		test:    testGroq,
	}
	providerGemini = providerDef{
		name:    "Google Gemini",
		envKey:  "GEMINI_API_KEY",
		infoURL: "https://aistudio.google.com/app/apikey",
		test:    testGemini,
	}
	providerOllama = providerDef{
		name:    "Ollama Cloud",
		envKey:  "OLLAMA_API_KEY",
		infoURL: "https://ollama.com/settings/tokens",
		test:    testOllama,
	}
)

// ─── helper functions ────────────────────────────────────────────────────────

func configureProvider(stdin io.Reader, stderr io.Writer, envPath string, p providerDef) string {
	_, _ = fmt.Fprintf(stderr, "Configurar %s? [Y/n]: ", p.name)
	if isNo(readLine(stdin)) {
		return ""
	}
	_, _ = fmt.Fprintf(stderr, "  Obtenha sua chave em: %s\n", p.infoURL)
	_, _ = fmt.Fprintf(stderr, "API Key (%s): ", p.name)
	key := readSecret(stdin, stderr)
	if key == "" {
		return ""
	}
	testAndSave(stderr, envPath, p.envKey, key, p.test)
	return key
}

func configureReviewer(stdin io.Reader, stderr io.Writer, envPath, envKey, label, current string) {
	if current != "" {
		_, _ = fmt.Fprintf(stderr, "%s [atual: %s] Alterar? [y/N]: ", label, current)
		if !isYes(readLine(stdin)) {
			return
		}
	}
	_, _ = fmt.Fprintf(stderr, "%s: ", label)
	email := readLine(stdin)
	if email != "" {
		_ = SetEnvVar(envPath, envKey, email)
	}
}

// ensurePRTemplate creates pr-template.md in the config dir if it does not already exist.
func ensurePRTemplate(stderr io.Writer, envPath string) {
	templatePath := filepath.Join(filepath.Dir(envPath), "pr-template.md")
	if _, err := os.Stat(templatePath); err == nil {
		return // already exists
	}
	if err := os.WriteFile(templatePath, []byte(defaultPRTemplate), 0o644); err != nil {
		_, _ = fmt.Fprintf(stderr, "[AVISO] Nao foi possivel criar pr-template.md: %v\n", err)
		return
	}
	_, _ = fmt.Fprintf(stderr, "[INFO] Template criado em %s\n", templatePath)
}

const defaultPRTemplate = `Analise o diff e log do git fornecidos e gere um TITULO e uma DESCRIÇÃO de PR em portugues brasileiro.

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

func testAndSave(stderr io.Writer, envPath, key, value string, test func(string) bool) {
	_, _ = fmt.Fprint(stderr, "  Testando credencial... ")
	if test(value) {
		fprintln(stderr, green("valida!"))
	} else {
		fprintln(stderr, red("falhou"))
		fprintln(stderr, "  [AVISO] A credencial pode estar errada ou expirada. Salvando mesmo assim.")
	}
	_ = SetEnvVar(envPath, key, value)
}

// ─── terminal I/O ────────────────────────────────────────────────────────────

// IsTerminal returns true when fd is a real TTY.
func IsTerminal(fd uintptr) bool {
	return term.IsTerminal(int(fd))
}

// readSecret reads a password without echoing it to the terminal.
// Falls back to plain readLine when stdin is not a real TTY (e.g. in tests).
func readSecret(stdin io.Reader, stderr io.Writer) string {
	if f, ok := stdin.(*os.File); ok && term.IsTerminal(int(f.Fd())) {
		b, err := term.ReadPassword(int(f.Fd()))
		fprintln(stderr, "") // newline after hidden input
		if err != nil {
			return ""
		}
		return strings.TrimSpace(string(b))
	}
	return readLine(stdin)
}

func readLine(r io.Reader) string {
	scanner := bufio.NewScanner(r)
	if scanner.Scan() {
		return strings.TrimSpace(scanner.Text())
	}
	return ""
}

func fprintln(w io.Writer, s string) {
	_, _ = fmt.Fprintln(w, s)
}

func isYes(s string) bool {
	s = strings.ToLower(strings.TrimSpace(s))
	return s == "y" || s == "yes"
}

func isNo(s string) bool {
	s = strings.ToLower(strings.TrimSpace(s))
	return s == "n" || s == "no"
}

// ─── config loading ──────────────────────────────────────────────────────────

func loadEnv(path string) map[string]string {
	data, err := os.ReadFile(path)
	if err != nil {
		return map[string]string{}
	}
	result := map[string]string{}
	scanner := bufio.NewScanner(bytes.NewReader(data))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		bare := stripExportPrefix(line)
		parts := strings.SplitN(bare, "=", 2)
		if len(parts) != 2 {
			continue
		}
		k := strings.TrimSpace(parts[0])
		v := strings.TrimSpace(parts[1])
		if len(v) >= 2 && ((v[0] == '"' && v[len(v)-1] == '"') || (v[0] == '\'' && v[len(v)-1] == '\'')) {
			v = v[1 : len(v)-1]
		}
		result[k] = v
	}
	return result
}

func hasAnyExisting(cfg map[string]string) bool {
	for _, v := range cfg {
		if v != "" {
			return true
		}
	}
	return false
}

func printMaskedSummary(w io.Writer, cfg map[string]string) {
	keys := []struct{ label, key string }{
		{"OPENROUTER_API_KEY", "OPENROUTER_API_KEY"},
		{"GROQ_API_KEY", "GROQ_API_KEY"},
		{"GEMINI_API_KEY", "GEMINI_API_KEY"},
		{"OLLAMA_API_KEY", "OLLAMA_API_KEY"},
		{"AZURE_PAT", "AZURE_PAT"},
		{"PR_REVIEWER_DEV", "PR_REVIEWER_DEV"},
		{"PR_REVIEWER_SPRINT", "PR_REVIEWER_SPRINT"},
	}
	for _, kv := range keys {
		v := cfg[kv.key]
		if v == "" {
			_, _ = fmt.Fprintf(w, "  %-22s (nao configurado)\n", kv.label)
		} else {
			_, _ = fmt.Fprintf(w, "  %-22s %s\n", kv.label, mask(v))
		}
	}
}

func mask(s string) string {
	if len(s) <= 4 {
		return "****"
	}
	return s[:4] + strings.Repeat("*", len(s)-4)
}

// ─── API testers ─────────────────────────────────────────────────────────────

var httpClient = &http.Client{Timeout: 10 * time.Second}

func testOpenRouter(key string) bool {
	// Use lightweight models list endpoint instead of chat completion
	req, err := http.NewRequest(http.MethodGet, "https://openrouter.ai/api/v1/models", nil)
	if err != nil {
		return false
	}
	req.Header.Set("Authorization", "Bearer "+key)
	resp, err := httpClient.Do(req)
	if err != nil {
		return false
	}
	defer func() { _ = resp.Body.Close() }()
	return resp.StatusCode == http.StatusOK
}

func testGroq(key string) bool {
	// Use lightweight models list endpoint instead of chat completion
	req, err := http.NewRequest(http.MethodGet, "https://api.groq.com/openai/v1/models", nil)
	if err != nil {
		return false
	}
	req.Header.Set("Authorization", "Bearer "+key)
	resp, err := httpClient.Do(req)
	if err != nil {
		return false
	}
	defer func() { _ = resp.Body.Close() }()
	return resp.StatusCode == http.StatusOK
}

func testGemini(key string) bool {
	// Use lightweight models list endpoint instead of generateContent
	url := "https://generativelanguage.googleapis.com/v1beta/models?key=" + key
	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return false
	}
	resp, err := httpClient.Do(req)
	if err != nil {
		return false
	}
	defer func() { _ = resp.Body.Close() }()
	return resp.StatusCode == http.StatusOK
}

func testOllama(key string) bool {
	// Use lightweight models list endpoint
	req, err := http.NewRequest(http.MethodGet, "https://ollama.com/v1/models", nil)
	if err != nil {
		return false
	}
	req.Header.Set("Authorization", "Bearer "+key)
	resp, err := httpClient.Do(req)
	if err != nil {
		return false
	}
	defer func() { _ = resp.Body.Close() }()
	return resp.StatusCode == http.StatusOK
}


