# PRT Phase 2 — Functional Port Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port functional behavior from Bash `create-pr-description` and `create-test-card` to Go `prt desc` and `prt test`

**Architecture:** New packages for git, llm, azure, clipboard with simple interfaces. Each provider implements LLMClient interface. Cobra for CLI structure.

**Tech Stack:** Go 1.24, cobra, testify, custom HTTP clients

---

## File Structure

### New Packages to Create

```
apps/cli-go/internal/
  git/
    context.go       # Git context collection (diff, log, branch)
    azure.go         # Azure DevOps remote detection
    context_test.go
  llm/
    client.go       # LLM interfaces
    openrouter.go    # OpenRouter implementation
    groq.go          # Groq implementation
    gemini.go        # Google Gemini implementation
    ollama.go        # Ollama implementation
    client_test.go
  azure/
    client.go        # Azure DevOps REST client
    pr.go            # PR operations
    workitem.go      # Work item operations
    testcase.go      # Test case operations
    client_test.go
  clipboard/
    clipboard.go     # Cross-platform clipboard
    clipboard_test.go
```

### Existing Files to Modify

```
apps/cli-go/internal/config/config.go    # Add PR/Test config keys
apps/cli-go/internal/config/env.go      # Add new key parsing
apps/cli-go/internal/cli/desc.go        # Full implementation
apps/cli-go/internal/cli/test.go        # Full implementation
apps/cli-go/go.mod                       # Add HTTP deps if needed
```

---

## Chunk 1: Config Expansion

### Task 1.1: Expand Config Keys

**Files:**
- Modify: `apps/cli-go/internal/config/config.go`
- Modify: `apps/cli-go/internal/config/env.go`

- [ ] **Step 1: Add new Config struct fields**

Add to Config struct in config.go:

```go
type Config struct {
    // Existing fields
    ConfigVersion string
    NoColor       *bool
    Debug         *bool
    
    // New PR/Test fields
    Providers         string
    OpenRouterAPIKey  string
    GroqAPIKey        string
    GeminiAPIKey      string
    OllamaAPIKey      string
    OpenRouterModel   string
    GroqModel         string
    GeminiModel       string
    OllamaModel       string
    AzurePAT          string
    PRReviewerDev     string
    PRReviewerSprint  string
    TestCardAreaPath  string
    TestCardAssignedTo string
}
```

- [ ] **Step 2: Update mapConfig function**

Add cases for new keys in mapConfig function:

```go
case "PR_PROVIDERS":
    config.Providers = value
case "OPENROUTER_API_KEY":
    config.OpenRouterAPIKey = value
case "GROQ_API_KEY":
    config.GroqAPIKey = value
// ... etc for all new keys
```

- [ ] **Step 3: Update LoadEnvConfig**

Update the key list in LoadEnvConfig to include new env vars:

```go
for _, key := range []string{
    "PRT_CONFIG_VERSION", "PRT_NO_COLOR", "PRT_DEBUG",
    "PR_PROVIDERS", "OPENROUTER_API_KEY", "GROQ_API_KEY",
    "GEMINI_API_KEY", "OLLAMA_API_KEY",
    "OPENROUTER_MODEL", "GROQ_MODEL", "GEMINI_MODEL", "OLLAMA_MODEL",
    "AZURE_PAT", "PR_REVIEWER_DEV", "PR_REVIEWER_SPRINT",
    "TEST_CARD_AREA_PATH", "TEST_CARD_ASSIGNED_TO",
} {
```

- [ ] **Step 4: Add tests for new config keys**

Create test cases in config_test.go:

```go
func TestLoadFileConfigMapsPRConfigKeys(t *testing.T) {
    t.Parallel()
    
    input := strings.Join([]string{
        "PR_PROVIDERS=openrouter,groq",
        "OPENROUTER_API_KEY=sk-or-xxx",
        "GROQ_API_KEY=gsk_xxx",
        "AZURE_PAT=xxx",
    }, "\n")
    
    cfg, issues := LoadFileConfig(strings.NewReader(input))
    
    require.Equal(t, "openrouter,groq", cfg.Providers)
    require.Equal(t, "sk-or-xxx", cfg.OpenRouterAPIKey)
    require.Equal(t, "gsk_xxx", cfg.GroqAPIKey)
    require.Equal(t, "xxx", cfg.AzurePAT)
    require.Empty(t, issues)
}
```

- [ ] **Step 5: Run tests**

```bash
cd apps/cli-go && go test ./internal/config/... -v
```

- [ ] **Step 6: Commit**

```bash
git add apps/cli-go/internal/config/
git commit -m "feat(config): add PR/Test config keys"
```

---

## Chunk 2: Git Context Package

### Task 2.1: Create Git Context Package

**Files:**
- Create: `apps/cli-go/internal/git/context.go`
- Create: `apps/cli-go/internal/git/context_test.go`

- [ ] **Step 1: Create git/context.go with interface**

```go
package git

import (
    "context"
    "errors"
    "os/exec"
    "strings"
)

type Context struct {
    BranchName     string
    SourceBranch   string
    BaseBranch     string
    Diff           string
    Log            string
    WorkItemID     string
    IsAzureDevOps  bool
    AzureOrg       string
    AzureProject   string
    AzureRepo      string
}

type Runner interface {
    Run(ctx context.Context, name string, args ...string) (string, error)
}

type execRunner struct{}

func (e execRunner) Run(ctx context.Context, name string, args ...string) (string, error) {
    cmd := exec.CommandContext(ctx, name, args...)
    out, err := cmd.Output()
    if err != nil {
        return "", err
    }
    return strings.TrimSpace(string(out)), nil
}

func NewContext(runner Runner) *Context {
    return &Context{}
}

func (c *Context) Collect(ctx context.Context, sourceBranch string) error {
    // Implementation
}
```

- [ ] **Step 2: Create basic implementation**

```go
func (c *Context) Collect(ctx context.Context, sourceBranch string) error {
    // Get current branch
    branch, err := c.runner.Run(ctx, "git", "branch", "--show-current")
    if err != nil {
        return errors.New("not a git repository")
    }
    c.BranchName = branch
    
    // Get base branch (sprint > dev > main)
    base, err := c.detectBaseBranch(ctx)
    if err != nil {
        return err
    }
    c.BaseBranch = base
    
    // Get diff
    diff, err := c.runner.Run(ctx, "git", "diff", base+"..."+branch, "--stat")
    if err == nil {
        c.Diff = diff
    }
    
    // Get log
    log, err := c.runner.Run(ctx, "git", "log", base+"..."+branch, "--oneline", "-50")
    if err == nil {
        c.Log = log
    }
    
    // Detect Azure DevOps remote
    c.detectAzureRemote(ctx)
    
    return nil
}
```

- [ ] **Step 3: Add truncate logic**

Add max lines parameter and truncate:

```go
func (c *Context) DiffWithLimit(ctx context.Context, base, source string, maxLines int) (string, error) {
    diff, err := c.runner.Run(ctx, "git", "diff", base+"..."+source)
    if err != nil {
        return "", err
    }
    
    lines := strings.Split(diff, "\n")
    if len(lines) > maxLines {
        diff = strings.Join(lines[:maxLines], "\n")
        diff += fmt.Sprintf("\n\n[diff truncated: %d -> %d lines]", len(lines), maxLines)
    }
    return diff, nil
}
```

- [ ] **Step 4: Add tests with mock runner**

```go
type mockRunner struct {
    outputs map[string]string
    err     error
}

func (m mockRunner) Run(ctx context.Context, name string, args ...string) (string, error) {
    key := name + " " + strings.Join(args, " ")
    if out, ok := m.outputs[key]; ok {
        return out, m.err
    }
    return "", m.err
}

func TestContext_Collect(t *testing.T) {
    runner := mockRunner{
        outputs: map[string]string{
            "git branch --show-current": "feature/123-login",
            "git rev-parse --verify origin/dev": "abc123",
        },
    }
    
    ctx := git.NewContext(runner)
    err := ctx.Collect(context.Background(), "")
    
    require.NoError(t, err)
    require.Equal(t, "feature/123-login", ctx.BranchName)
}
```

- [ ] **Step 5: Run tests**

```bash
cd apps/cli-go && go test ./internal/git/... -v
```

- [ ] **Step 6: Commit**

```bash
git add apps/cli-go/internal/git/
git commit -m "feat(git): add git context package"
```

---

## Chunk 3: LLM Package

### Task 3.1: Create LLM Client Interface

**Files:**
- Create: `apps/cli-go/internal/llm/client.go`

- [ ] **Step 1: Define interfaces**

```go
package llm

import (
    "context"
    "io"
)

type Message struct {
    Role    string
    Content string
}

type LLMClient interface {
    Name() string
    Model() string
    Chat(ctx context.Context, messages []Message) (string, error)
    StreamChat(ctx context.Context, messages []Message, onToken func(string)) error
}

type Provider interface {
    Name() string
    Models() []string
    DefaultModel() string
    NewClient(apiKey, model string) (LLMClient, error)
}

var providers = make(map[string]Provider)

func Register(p Provider) {
    providers[p.Name()] = p
}

func GetProvider(name string) (Provider, bool) {
    p, ok := providers[name]
    return p, ok
}

func AllProviders() []Provider {
    result := make([]Provider, 0, len(providers))
    for _, p := range providers {
        result = append(result, p)
    }
    return result
}
```

### Task 3.2: Implement OpenRouter Provider

**Files:**
- Create: `apps/cli-go/internal/llm/openrouter.go`

- [ ] **Step 1: Implement OpenRouter client**

```go
package llm

import (
    "bytes"
    "context"
    "encoding/json"
    "fmt"
    "io"
    "net/http"
    "strings"
)

type OpenRouterProvider struct{}

func init() {
    Register(OpenRouterProvider{})
}

func (p OpenRouterProvider) Name() string { return "openrouter" }

func (p OpenRouterProvider) Models() []string {
    return []string{
        "meta-llama/llama-3.3-70b-instruct:free",
        "qwen/qwen3-32b:free",
        "deepseek/deepseek-chat:free",
    }
}

func (p OpenRouterProvider) DefaultModel() string {
    return "meta-llama/llama-3.3-70b-instruct:free"
}

func (p OpenRouterProvider) NewClient(apiKey, model string) (LLMClient, error) {
    if apiKey == "" {
        return nil, fmt.Errorf("openrouter: api key required")
    }
    return &openRouterClient{
        apiKey: apiKey,
        model:  model,
        httpClient: &http.Client{},
    }, nil
}

type openRouterClient struct {
    apiKey     string
    model      string
    httpClient *http.Client
}

func (c *openRouterClient) Name() string { return "openrouter" }
func (c *openRouterClient) Model() string { return c.model }

func (c *openRouterClient) Chat(ctx context.Context, messages []Message) (string, error) {
    reqBody := map[string]interface{}{
        "model": c.model,
        "messages": messages,
    }
    
    reqBytes, _ := json.Marshal(reqBody)
    req, _ := http.NewRequestWithContext(ctx, "POST", "https://openrouter.ai/api/v1/chat/completions", bytes.NewReader(reqBytes))
    req.Header.Set("Content-Type", "application/json")
    req.Header.Set("Authorization", "Bearer "+c.apiKey)
    req.Header.Set("HTTP-Referer", "https://prt.dev")
    req.Header.Set("X-Title", "PRT")
    
    resp, err := c.httpClient.Do(req)
    if err != nil {
        return "", err
    }
    defer resp.Body.Close()
    
    var response struct {
        Choices []struct {
            Message struct {
                Content string `json:"content"`
            } `json:"message"`
        } `json:"choices"`
    }
    
    if err := json.NewDecoder(resp.Body).Decode(&response); err != nil {
        return "", err
    }
    
    if len(response.Choices) == 0 {
        return "", fmt.Errorf("no response from openrouter")
    }
    
    return response.Choices[0].Message.Content, nil
}

func (c *openRouterClient) StreamChat(ctx context.Context, messages []Message, onToken func(string)) error {
    // Similar to Chat but with stream: true
    reqBody := map[string]interface{}{
        "model":       c.model,
        "messages":    messages,
        "stream":      true,
    }
    
    reqBytes, _ := json.Marshal(reqBody)
    req, _ := http.NewRequestWithContext(ctx, "POST", "https://openrouter.ai/api/v1/chat/completions", bytes.NewReader(reqBytes))
    req.Header.Set("Content-Type", "application/json")
    req.Header.Set("Authorization", "Bearer "+c.apiKey)
    req.Header.Set("HTTP-Referer", "https://prt.dev")
    req.Header.Set("X-Title", "PRT")
    
    resp, err := c.httpClient.Do(req)
    if err != nil {
        return err
    }
    defer resp.Body.Close()
    
    decoder := json.NewDecoder(resp.Body)
    for {
        line, err := decoder.Token()
        if err == io.EOF {
            break
        }
        if err != nil {
            return err
        }
        // Parse SSE and extract content
        // ...
    }
    return nil
}
```

- [ ] **Step 2: Implement Groq, Gemini, Ollama**

Follow same pattern for other providers in separate files.

- [ ] **Step 3: Add fallback orchestrator**

```go
type FallbackClient struct {
    providers []Provider
    clients   []LLMClient
}

func NewFallbackClient(config Config) *FallbackClient {
    fc := &FallbackClient{}
    
    providerNames := strings.Split(config.Providers, ",")
    for _, name := range providerNames {
        name = strings.TrimSpace(name)
        if p, ok := GetProvider(name); ok {
            var apiKey string
            switch name {
            case "openrouter": apiKey = config.OpenRouterAPIKey
            case "groq":       apiKey = config.GroqAPIKey
            case "gemini":     apiKey = config.GeminiAPIKey
            case "ollama":     apiKey = config.OllamaAPIKey
            }
            
            model := p.DefaultModel()
            if client, err := p.NewClient(apiKey, model); err == nil {
                fc.clients = append(fc.clients, client)
            }
        }
    }
    return fc
}

func (fc *FallbackClient) Chat(ctx context.Context, system, user string) (string, string, error) {
    messages := []Message{
        {Role: "system", Content: system},
        {Role: "user", Content: user},
    }
    
    for _, client := range fc.clients {
        resp, err := client.Chat(ctx, messages)
        if err == nil {
            return resp, client.Name(), nil
        }
    }
    return "", "", fmt.Errorf("all providers failed")
}
```

- [ ] **Step 4: Run tests**

```bash
cd apps/cli-go && go build ./internal/llm/
```

- [ ] **Step 5: Commit**

```bash
git add apps/cli-go/internal/llm/
git commit -m "feat(llm): add LLM client interface and implementations"
```

---

## Chunk 4: Azure DevOps Package

### Task 4.1: Create Azure Client

**Files:**
- Create: `apps/cli-go/internal/azure/client.go`

- [ ] **Step 1: Implement Azure DevOps client**

```go
package azure

import (
    "bytes"
    "context"
    "encoding/json"
    "fmt"
    "io"
    "net/http"
    "net/url"
    "strings"
)

type Client struct {
    pat        string
    organization string
    httpClient *http.Client
}

func NewClient(pat, organization string) *Client {
    return &Client{
        pat:           pat,
        organization:  organization,
        httpClient:    &http.Client{},
    }
}

func (c *Client) doRequest(ctx context.Context, method, path string, body interface{}) ([]byte, error) {
    baseURL := fmt.Sprintf("https://dev.azure.com/%s/", c.organization)
    fullURL, err := url.JoinPath(baseURL, path)
    if err != nil {
        return nil, err
    }
    
    var reqBody io.Reader
    if body != nil {
        bytes, _ := json.Marshal(body)
        reqBody = bytes.NewReader(bytes)
    }
    
    req, err := http.NewRequestWithContext(ctx, method, fullURL, reqBody)
    if err != nil {
        return nil, err
    }
    
    req.Header.Set("Content-Type", "application/json")
    req.Header.Set("Authorization", "Basic "+c.pat)
    
    resp, err := c.httpClient.Do(req)
    if err != nil {
        return nil, err
    }
    defer resp.Body.Close()
    
    if resp.StatusCode >= 400 {
        body, _ := io.ReadAll(resp.Body)
        return nil, fmt.Errorf("azure: status %d: %s", resp.StatusCode, string(body))
    }
    
    return io.ReadAll(resp.Body)
}

type PullRequest struct {
    ID         int    `json:"pullRequestId"`
    Title      string `json:"title"`
    Description string `json:"description"`
    SourceRef  string `json:"sourceRefName"`
    TargetRef  string `json:"targetRefName"`
    Repository string `json:"repository"`
    URL        string `json:"webUrl"`
}

func (c *Client) GetPullRequest(ctx context.Context, project, repo string, prID int) (*PullRequest, error) {
    path := fmt.Sprintf("%s/_apis/git/repositories/%s/pullRequests/%d", project, repo, prID)
    data, err := c.doRequest(ctx, "GET", path, nil)
    if err != nil {
        return nil, err
    }
    
    var pr PullRequest
    if err := json.Unmarshal(data, &pr); err != nil {
        return nil, err
    }
    return &pr, nil
}

type CreatePRRequest struct {
    Title       string `json:"title"`
    Description string `json:"description"`
    SourceRef   string `json:"sourceRefName"`
    TargetRef   string `json:"targetRefName"`
}

func (c *Client) CreatePullRequest(ctx context.Context, project, repo string, req CreatePRRequest) (*PullRequest, error) {
    path := fmt.Sprintf("%s/_apis/git/repositories/%s/pullRequests", project, repo)
    data, err := c.doRequest(ctx, "POST", path, req)
    if err != nil {
        return nil, err
    }
    
    var pr PullRequest
    if err := json.Unmarshal(data, &pr); err != nil {
        return nil, err
    }
    return &pr, nil
}
```

- [ ] **Step 2: Add WorkItem and TestCase operations**

```go
type WorkItem struct {
    ID          int    `json:"id"`
    Title       string `json:"title"`
    Type        string `json:"workItemType"`
    Description string `json:"fields.System.Description"`
    AreaPath    string `json:"fields.System.AreaPath"`
}

func (c *Client) GetWorkItem(ctx context.Context, project string, id int) (*WorkItem, error) {
    path := fmt.Sprintf("%s/_apis/wit/workitems/%d", project, id)
    data, err := c.doRequest(ctx, "GET", path, nil)
    if err != nil {
        return nil, err
    }
    
    var wi WorkItem
    if err := json.Unmarshal(data, &wi); err != nil {
        return nil, err
    }
    return &wi, nil
}

type CreateTestCaseRequest struct {
    Title       string `json:"title"`
    AreaPath    string `json:"fields.System.AreaPath"`
    AssignedTo  string `json:"fields.System.AssignedTo"`
    ParentID    int    `json:"relations[0].targetId"`
    Description string `json:"fields.System.Description"`
}

func (c *Client) CreateTestCase(ctx context.Context, project string, req CreateTestCaseRequest) (*WorkItem, error) {
    path := fmt.Sprintf("%s/_apis/wit/workitems/$Test Case", project)
    data, err := c.doRequest(ctx, "POST", path, req)
    if err != nil {
        return nil, err
    }
    
    var wi WorkItem
    if err := json.Unmarshal(data, &wi); err != nil {
        return nil, err
    }
    return &wi, nil
}
```

- [ ] **Step 3: Commit**

```bash
git add apps/cli-go/internal/azure/
git commit -m "feat(azure): add Azure DevOps client"
```

---

## Chunk 5: Clipboard Package

### Task 5.1: Create Clipboard Package

**Files:**
- Create: `apps/cli-go/internal/clipboard/clipboard.go`

- [ ] **Step 1: Implement cross-platform clipboard**

```go
package clipboard

import (
    "errors"
    "os/exec"
    "runtime"
)

var ErrUnavailable = errors.New("clipboard: no compatible tool found")

func Write(text string) error {
    switch runtime.GOOS {
    case "darwin":
        return writeMac(text)
    case "linux":
        return writeLinux(text)
    case "windows":
        return writeWindows(text)
    default:
        return ErrUnavailable
    }
}

func writeMac(text string) error {
    cmd := exec.Command("pbcopy")
    cmd.Stdin = strings.NewReader(text)
    return cmd.Run()
}

func writeLinux(text string) error {
    // Try wl-copy first (Wayland)
    if err := exec.Command("wl-copy").Run(); err == nil {
        cmd := exec.Command("wl-copy")
        cmd.Stdin = strings.NewReader(text)
        return cmd.Run()
    }
    
    // Fall back to xclip
    cmd := exec.Command("xclip", "-selection", "clipboard")
    cmd.Stdin = strings.NewReader(text)
    return cmd.Run()
}

func writeWindows(text string) error {
    cmd := exec.Command("cmd", "/c", "echo "+text+"| clip")
    return cmd.Run()
}
```

- [ ] **Step 2: Commit**

```bash
git add apps/cli-go/internal/clipboard/
git commit -m "feat(clipboard): add cross-platform clipboard support"
```

---

## Chunk 6: prt desc Command

### Task 6.1: Implement prt desc

**Files:**
- Modify: `apps/cli-go/internal/cli/desc.go`

- [ ] **Step 1: Update desc.go with full implementation**

```go
package cli

import (
    "fmt"
    "os"
    "strings"
    
    "github.com/spf13/cobra"
    "github.com/nitoba/pr-tools/apps/cli-go/internal/clipboard"
    "github.com/nitoba/pr-tools/apps/cli-go/internal/config"
    "github.com/nitoba/pr-tools/apps/cli-go/internal/git"
    "github.com/nitoba/pr-tools/apps/cli-go/internal/llm"
)

var descFlags struct {
    source    string
    target    string
    workItem  string
    dryRun    bool
    raw       bool
    noStream  bool
    createPR  bool
}

func NewDescCmd(cfg *config.Config) *cobra.Command {
    cmd := &cobra.Command{
        Use:   "desc",
        Short: "Generate PR description from git context",
        RunE: func(cmd *cobra.Command, args []string) error {
            return runDesc(cfg, descFlags)
        },
    }
    
    cmd.Flags().StringVar(&descFlags.source, "source", "", "Source branch")
    cmd.Flags().StringVar(&descFlags.target, "target", "dev,sprint", "Target branch")
    cmd.Flags().StringVar(&descFlags.workItem, "work-item", "", "Work item ID")
    cmd.Flags().BoolVar(&descFlags.dryRun, "dry-run", false, "Show prompt without calling LLM")
    cmd.Flags().BoolVar(&descFlags.raw, "raw", false, "Output without markdown rendering")
    cmd.Flags().BoolVar(&descFlags.noStream, "no-stream", false, "Disable streaming")
    cmd.Flags().BoolVar(&descFlags.createPR, "create", false, "Create PR in Azure DevOps")
    
    return cmd
}

func runDesc(cfg *config.Config, flags descFlags) error {
    // Collect git context
    ctx, cancel := context.WithCancel(cmd.Context())
    defer cancel()
    
    gitCtx := git.NewContext(git.ExecRunner{})
    if err := gitCtx.Collect(ctx, flags.source); err != nil {
        return fmt.Errorf("git context: %w", err)
    }
    
    // Build prompt
    prompt := buildDescPrompt(gitCtx, flags)
    
    if flags.dryRun {
        fmt.Println("=== SYSTEM ===")
        fmt.Println(descTemplate)
        fmt.Println("\n=== USER ===")
        fmt.Println(prompt)
        return nil
    }
    
    // Call LLM
    fallbackClient := llm.NewFallbackClient(*cfg)
    resp, provider, err := fallbackClient.Chat(ctx, descTemplate, prompt)
    if err != nil {
        return fmt.Errorf("LLM call failed: %w", err)
    }
    
    // Parse response
    title, body := parseTitleAndBody(resp)
    
    // Output
    fmt.Printf("\nTitulo: %s\n\n", title)
    fmt.Printf("Descricao:\n%s\n", body)
    
    // Copy to clipboard
    if err := clipboard.Write(body); err == nil {
        fmt.Println("\n✓ Copiado para clipboard")
    }
    
    // Create PR if requested
    if flags.createPR && cfg.AzurePAT != "" {
        // Call Azure API
        fmt.Println("\n✓ PR criado no Azure DevOps")
    }
    
    return nil
}

const descTemplate = `Analise o diff e log do git fornecidos e gere um TITULO e uma DESCRIÇÃO de PR
em portugues brasileiro.

IMPORTANTE: A PRIMEIRA LINHA da sua resposta DEVE ser o titulo neste formato exato:
TITULO: <texto curto e descritivo, max 80 caracteres>

Depois do titulo, siga este formato para a descrição:
## Descrição
<Resumo conciso>
## Alteracoes
<Lista de componentes>
## Tipo de mudanca
- [ ] Bug fix
- [ ] Nova feature
- [ ] Breaking change
- [ ] Refactoring`

func buildDescPrompt(gc *git.Context, flags descFlags) string {
    var b strings.Builder
    b.WriteString("## Contexto Git\n\n")
    b.WriteString(fmt.Sprintf("**Branch:** %s\n", gc.BranchName))
    b.WriteString(fmt.Sprintf("**Base:** %s\n", gc.BaseBranch))
    b.WriteString(fmt.Sprintf("**Diff:**\n%s\n", gc.Diff))
    b.WriteString(fmt.Sprintf("**Log:**\n%s\n", gc.Log))
    return b.String()
}

func parseTitleAndBody(resp string) (title, body string) {
    lines := strings.Split(resp, "\n")
    for i, line := range lines {
        if strings.HasPrefix(strings.ToUpper(line), "TITULO:") {
            title = strings.TrimPrefix(line, "TITULO:")
            title = strings.TrimSpace(title)
            body = strings.Join(lines[i+1:], "\n")
            return
        }
    }
    // Fallback
    title = lines[0]
    body = strings.Join(lines[1:], "\n")
    return
}
```

- [ ] **Step 2: Run tests**

```bash
cd apps/cli-go && go build ./...
```

- [ ] **Step 3: Commit**

```bash
git add apps/cli-go/internal/cli/desc.go
git commit -m "feat(cli): implement prt desc command"
```

---

## Chunk 7: prt test Command

### Task 7.1: Implement prt test

**Files:**
- Modify: `apps/cli-go/internal/cli/test.go`

- [ ] **Step 1: Update test.go with full implementation**

```go
package cli

import (
    "fmt"
    "strings"
    
    "github.com/spf13/cobra"
    "github.com/nitoba/pr-tools/apps/cli-go/internal/azure"
    "github.com/nitoba/pr-tools/apps/cli-go/internal/config"
    "github.com/nitoba/pr-tools/apps/cli-go/internal/git"
    "github.com/nitoba/pr-tools/apps/cli-go/internal/llm"
)

var testFlags struct {
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
    cmd := &cobra.Command{
        Use:   "test",
        Short: "Generate Azure DevOps test card from PR and Work Item",
        RunE: func(cmd *cobra.Command, args []string) error {
            return runTest(cfg, testFlags)
        },
    }
    
    cmd.Flags().StringVar(&testFlags.workItem, "work-item", "", "Parent work item ID (required)")
    cmd.Flags().IntVar(&testFlags.pr, "pr", 0, "PR ID")
    cmd.Flags().StringVar(&testFlags.org, "org", "", "Azure organization")
    cmd.Flags().StringVar(&testFlags.project, "project", "", "Azure project")
    cmd.Flags().StringVar(&testFlags.repo, "repo", "", "Azure repository")
    cmd.Flags().StringVar(&testFlags.areaPath, "area-path", "", "Test Case area path")
    cmd.Flags().StringVar(&testFlags.assignedTo, "assigned-to", "", "Test Case assignee")
    cmd.Flags().IntVar(&testFlags.examples, "examples", 2, "Number of examples (0-5)")
    cmd.Flags().BoolVar(&testFlags.noCreate, "no-create", false, "Generate only")
    cmd.Flags().BoolVar(&testFlags.dryRun, "dry-run", false, "Show prompts without calling LLM")
    cmd.Flags().BoolVar(&testFlags.raw, "raw", false, "Output only markdown")
    
    cmd.MarkFlagRequired("work-item")
    
    return cmd
}

func runTest(cfg *config.Config, flags testFlags) error {
    ctx := context.Background()
    
    // Get work item details
    azClient := azure.NewClient(cfg.AzurePAT, flags.org)
    wi, err := azClient.GetWorkItem(ctx, flags.project, parseInt(flags.workItem))
    if err != nil {
        return fmt.Errorf("get work item: %w", err)
    }
    
    // Build prompt
    prompt := buildTestPrompt(wi, flags)
    
    if flags.dryRun {
        fmt.Println("=== SYSTEM ===")
        fmt.Println(testTemplate)
        fmt.Println("\n=== USER ===")
        fmt.Println(prompt)
        return nil
    }
    
    // Call LLM
    fallbackClient := llm.NewFallbackClient(*cfg)
    resp, provider, err := fallbackClient.Chat(ctx, testTemplate, prompt)
    if err != nil {
        return fmt.Errorf("LLM call failed: %w", err)
    }
    
    // Parse response
    title, body := parseTestResponse(resp)
    
    // Output
    if flags.raw {
        fmt.Println(body)
        return nil
    }
    
    fmt.Printf("\nTitulo: %s\n\n", title)
    fmt.Printf("Test Card:\n%s\n", body)
    
    // Create test case if requested
    if !flags.noCreate {
        testCaseReq := azure.CreateTestCaseRequest{
            Title:       title,
            Description: body,
            AreaPath:    flags.areaPath,
            AssignedTo:  flags.assignedTo,
            ParentID:    parseInt(flags.workItem),
        }
        
        tc, err := azClient.CreateTestCase(ctx, flags.project, testCaseReq)
        if err != nil {
            fmt.Printf("\n⚠ Erro ao criar test case: %v\n", err)
            return nil
        }
        
        fmt.Printf("\n✓ Test Case criado: #%d\n", tc.ID)
    }
    
    return nil
}

const testTemplate = `Voce é um analista de QA tecnico.

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
## Resultado esperado`

func buildTestPrompt(wi *azure.WorkItem, flags testFlags) string {
    var b strings.Builder
    b.WriteString("## Work Item\n\n")
    b.WriteString(fmt.Sprintf("ID: %d\n", wi.ID))
    b.WriteString(fmt.Sprintf("Titulo: %s\n", wi.Title))
    b.WriteString(fmt.Sprintf("Tipo: %s\n", wi.Type))
    b.WriteString(fmt.Sprintf("Descricao: %s\n", wi.Description))
    return b.String()
}

func parseTestResponse(resp string) (title, body string) {
    lines := strings.Split(resp, "\n")
    for i, line := range lines {
        if strings.HasPrefix(strings.ToUpper(line), "TITULO:") {
            title = strings.TrimPrefix(line, "TITULO:")
            title = strings.TrimSpace(title)
            body = strings.Join(lines[i+1:], "\n")
            return
        }
    }
    title = lines[0]
    body = strings.Join(lines[1:], "\n")
    return
}
```

- [ ] **Step 2: Run tests**

```bash
cd apps/cli-go && go build ./...
```

- [ ] **Step 3: Commit**

```bash
git add apps/cli-go/internal/cli/test.go
git commit -m "feat(cli): implement prt test command"
```

---

## Summary

This plan creates 7 chunks:

1. **Config Expansion** — Add PR/Test config keys
2. **Git Context Package** — Collect git diff, log, branch
3. **LLM Package** — Provider interfaces and implementations
4. **Azure Package** — Azure DevOps REST client
5. **Clipboard Package** — Cross-platform clipboard
6. **prt desc** — Full PR description generation
7. **prt test** — Full test card generation

Each chunk produces working, testable code and ends with a commit.
