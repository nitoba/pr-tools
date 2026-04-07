package cli

import (
	"bytes"
	"context"
	"errors"
	"io"
	"regexp"
	"strings"
	"testing"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/azure"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/git"
	"github.com/spf13/cobra"
	"github.com/stretchr/testify/require"
)

func TestNewTestCmdHasCorrectMetadata(t *testing.T) {
	t.Parallel()
	cfg := &config.Config{}
	cmd := NewTestCmd(cfg)
	require.Equal(t, "test", cmd.Use)
	require.Equal(t, "Generate Azure DevOps test card from Work Item.", cmd.Short)
	require.NotNil(t, cmd.Flags().Lookup("work-item"))
	require.NotNil(t, cmd.Flags().Lookup("dry-run"))
	require.NotNil(t, cmd.Flags().Lookup("no-create"))
	require.NotNil(t, cmd.Flags().Lookup("raw"))
}

func TestNewTestCmd_DoesNotRequireWorkItemFlag(t *testing.T) {
	t.Parallel()

	cfg := &config.Config{}
	cmd := NewTestCmd(cfg)

	require.NoError(t, cmd.Flags().Lookup("work-item").Value.Set(""))
}

func TestBuildTestPrompt_WithNilWorkItem(t *testing.T) {
	t.Parallel()
	prompt := buildTestPrompt(nil, 42, nil, nil, nil, testFlagSet{})
	require.Contains(t, prompt, "ID: 42")
	require.Contains(t, prompt, "## Contexto do Work Item")
}

func TestTestConfiguredProviderModel_FollowsConfiguredProvidersAndKeys(t *testing.T) {
	t.Parallel()

	provider, model := testConfiguredProviderModel(config.Config{
		Providers:       "groq,openrouter",
		GroqAPIKey:      "groq-key",
		GroqModel:       "llama-fast",
		OpenRouterModel: "ignored",
	})
	require.Equal(t, "groq", provider)
	require.Equal(t, "llama-fast", model)

	provider, model = testConfiguredProviderModel(config.Config{
		GroqAPIKey: "groq-key",
		GroqModel:  "mixtral",
	})
	require.Equal(t, "groq", provider)
	require.Equal(t, "mixtral", model)
}

func TestRunTest_ExplicitWorkItemWinsOverPRLinks(t *testing.T) {
	prLinkedLookups := 0
	restore := stubTestDeps(testDeps{
		gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
		workItems: map[int]*azure.WorkItem{
			42: {ID: 42, Fields: map[string]interface{}{"System.Title": "Explicit Parent", "System.WorkItemType": "User Story"}},
		},
		pr:                  &azure.PullRequest{ID: 99, Title: "PR title"},
		prIDs:               []int{300, 11796, 11820},
		prWorkItemLookupHit: &prLinkedLookups,
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newTestCommand(stdout, stderr, "")

	err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key"}, testFlagSet{workItem: "42", pr: 99, dryRun: true}, cmd)
	require.NoError(t, err)
	require.Equal(t, 0, prLinkedLookups)
	require.Contains(t, stderr.String(), "Work item: #42")
}

func TestRunTest_ResolvesWorkItemFromPRWhenFlagIsMissing(t *testing.T) {
	restore := stubTestDeps(testDeps{
		gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
		workItems: map[int]*azure.WorkItem{
			300:   {ID: 300, Fields: map[string]interface{}{"System.Title": "Linked Test Case", "System.WorkItemType": "Test Case"}},
			11796: {ID: 11796, Fields: map[string]interface{}{"System.Title": "Parent Story", "System.WorkItemType": "User Story"}},
			11820: {ID: 11820, Fields: map[string]interface{}{"System.Title": "Parent Bug", "System.WorkItemType": "Bug"}},
		},
		pr:    &azure.PullRequest{ID: 99, Title: "PR title"},
		prIDs: []int{300, 11796, 11820},
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newTestCommand(stdout, stderr, "")

	err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key"}, testFlagSet{pr: 99, dryRun: true}, cmd)
	require.NoError(t, err)
	require.Contains(t, stderr.String(), "Work item: #11796")
	require.Contains(t, stdout.String(), "ID: 11796")
}

func TestRunTest_AllLinkedTestCasesFallsBackToLowestID(t *testing.T) {
	restore := stubTestDeps(testDeps{
		gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
		workItems: map[int]*azure.WorkItem{
			300: {ID: 300, Fields: map[string]interface{}{"System.Title": "Case B", "System.WorkItemType": "Test Case"}},
			120: {ID: 120, Fields: map[string]interface{}{"System.Title": "Case A", "System.WorkItemType": "Test Case"}},
		},
		pr:    &azure.PullRequest{ID: 99, Title: "PR title"},
		prIDs: []int{300, 120},
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newTestCommand(stdout, stderr, "")

	err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key"}, testFlagSet{pr: 99, dryRun: true}, cmd)
	require.NoError(t, err)
	require.Contains(t, stderr.String(), "Work item: #120")
	require.Contains(t, stdout.String(), "ID: 120")
}

func TestRunTest_NoLinkedItemsFailsWithBashMessage(t *testing.T) {
	restore := stubTestDeps(testDeps{
		gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
		pr:     &azure.PullRequest{ID: 99, Title: "PR title"},
		prIDs:  nil,
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newTestCommand(stdout, stderr, "")

	err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key"}, testFlagSet{pr: 99, dryRun: true}, cmd)
	require.EqualError(t, err, "não foi possível resolver o work item pai; use --work-item explicitamente")
	require.NotContains(t, stdout.String(), "=== SYSTEM ===")
	require.NotContains(t, stderr.String(), "Work item: #")
}

func TestSelectParentWorkItemID_PicksLowestNonTestCase(t *testing.T) {
	restore := stubTestDeps(testDeps{
		workItems: map[int]*azure.WorkItem{
			300:   {ID: 300, Fields: map[string]interface{}{"System.WorkItemType": "Test Case"}},
			11796: {ID: 11796, Fields: map[string]interface{}{"System.WorkItemType": "User Story"}},
			11820: {ID: 11820, Fields: map[string]interface{}{"System.WorkItemType": "Bug"}},
		},
	})
	defer restore()

	wiID, ok := selectParentWorkItemID(context.Background(), &config.Config{AzurePAT: "pat"}, "org", "project", []int{300, 11796, 11820})

	require.True(t, ok)
	require.Equal(t, 11796, wiID)
}

func TestSelectParentWorkItemID_FallsBackToLowestIDWhenAllAreTestCases(t *testing.T) {
	restore := stubTestDeps(testDeps{
		workItems: map[int]*azure.WorkItem{
			300: {ID: 300, Fields: map[string]interface{}{"System.WorkItemType": "Test Case"}},
			120: {ID: 120, Fields: map[string]interface{}{"System.WorkItemType": "Test Case"}},
		},
	})
	defer restore()

	wiID, ok := selectParentWorkItemID(context.Background(), &config.Config{AzurePAT: "pat"}, "org", "project", []int{300, 120})

	require.True(t, ok)
	require.Equal(t, 120, wiID)
}

func TestRunTest_InteractiveCreateAndUpdateUseCommandInput(t *testing.T) {
	created := 0
	updated := 0
	var gotEffort *float64
	var gotRealEffort *float64
	restore := stubTestDeps(testDeps{
		gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
		workItems: map[int]*azure.WorkItem{
			42: {ID: 42, Fields: map[string]interface{}{"System.Title": "Explicit Parent", "System.WorkItemType": "User Story"}},
		},
		pr:                  &azure.PullRequest{ID: 99, Title: "PR title"},
		llmResp:             "TITULO: Teste\n## Objetivo\nBody",
		interactive:         true,
		createTestCaseCalls: &created,
		updateTestQACalls:   &updated,
		updateEffort:        &gotEffort,
		updateRealEffort:    &gotRealEffort,
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newTestCommand(stdout, stderr, "y\ny\n")

	err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key", Providers: "openrouter"}, testFlagSet{workItem: "42", pr: 99}, cmd)
	require.NoError(t, err)
	require.Equal(t, 1, created)
	require.Equal(t, 1, updated)
	require.Contains(t, stderr.String(), "Atualizar o work item #42 para Test QA?")
	require.NotNil(t, gotEffort)
	require.NotNil(t, gotRealEffort)
}

func TestRunTest_RendersBashGenerationTranscript(t *testing.T) {
	restore := stubTestDeps(testDeps{
		gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
		workItems: map[int]*azure.WorkItem{
			42: {ID: 42, Fields: map[string]interface{}{
				"System.Title":         "Parent Story",
				"System.WorkItemType":  "User Story",
				"System.IterationPath": "AGROTRACE\\Sprint 98",
			}},
		},
		pr:       &azure.PullRequest{ID: 99, Title: "Adicionar transcript"},
		examples: []string{"- #1 Exemplo"},
		llmResp:  "TITULO: Testar transcript\n## Objetivo\nBody markdown",
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newTestCommand(stdout, stderr, "")

	err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key", Providers: "openrouter"}, testFlagSet{workItem: "42", pr: 99, areaPath: "AGROTRACE\\Devops", assignedTo: "qa@empresa.com", noCreate: true}, cmd)
	require.NoError(t, err)

	requireSubstringsInOrder(t, stderr.String(),
		"Gerando card de teste...",
		"Dependencias validadas",
		"Configuracao carregada",
		"Azure PAT validado",
		"API keys validadas",
		"Contexto git detectado",
		"Azure DevOps: org/project",
		"PR: #99 — Adicionar transcript",
		"Work item: #42 — Parent Story",
		"Alteracoes coletadas",
		"Exemplos coletados",
		"Campos resolvidos",
		"Card gerado (openrouter/default)",
	)

	require.Contains(t, stdout.String(), "Test Card — PR #99")
	require.Contains(t, stdout.String(), "Provider: openrouter/default")
	require.Contains(t, stdout.String(), "Work Item: #42 — Parent Story")
	require.Contains(t, stdout.String(), "AreaPath: AGROTRACE\\Devops")
	require.Contains(t, stdout.String(), "Responsável: qa@empresa.com")
	require.Contains(t, stdout.String(), "Título: Testar transcript")
	require.Contains(t, stdout.String(), "## Objetivo\nBody markdown")
}

func TestRunTest_PublishCancelAndNonInteractiveMatchBash(t *testing.T) {
	t.Run("interactive cancel", func(t *testing.T) {
		created := 0
		restore := stubTestDeps(testDeps{
			gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
			workItems: map[int]*azure.WorkItem{
				42: {ID: 42, Fields: map[string]interface{}{"System.Title": "Parent Story", "System.WorkItemType": "User Story"}},
			},
			pr:                  &azure.PullRequest{ID: 99, Title: "Adicionar transcript"},
			llmResp:             "TITULO: Testar publish\n## Objetivo\nBody",
			interactive:         true,
			createTestCaseCalls: &created,
		})
		defer restore()

		stdout := new(bytes.Buffer)
		stderr := new(bytes.Buffer)
		cmd := newTestCommand(stdout, stderr, "n\n")

		err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key", Providers: "openrouter"}, testFlagSet{workItem: "42", pr: 99, areaPath: "AGROTRACE\\Devops"}, cmd)
		require.NoError(t, err)
		require.Equal(t, 0, created)
		require.Contains(t, stderr.String(), "Publicar no Azure DevOps")
		require.Contains(t, stderr.String(), "Criar este Test Case no Azure DevOps?")
		require.Contains(t, stderr.String(), "(cancelado)")
	})

	t.Run("non interactive warning", func(t *testing.T) {
		created := 0
		restore := stubTestDeps(testDeps{
			gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
			workItems: map[int]*azure.WorkItem{
				42: {ID: 42, Fields: map[string]interface{}{"System.Title": "Parent Story", "System.WorkItemType": "User Story"}},
			},
			pr:                  &azure.PullRequest{ID: 99, Title: "Adicionar transcript"},
			llmResp:             "TITULO: Testar publish\n## Objetivo\nBody",
			interactive:         false,
			createTestCaseCalls: &created,
		})
		defer restore()

		stdout := new(bytes.Buffer)
		stderr := new(bytes.Buffer)
		cmd := newTestCommand(stdout, stderr, "")

		err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key", Providers: "openrouter"}, testFlagSet{workItem: "42", pr: 99, areaPath: "AGROTRACE\\Devops"}, cmd)
		require.NoError(t, err)
		require.Equal(t, 0, created)
		require.Contains(t, stderr.String(), "Publicar no Azure DevOps")
		require.Contains(t, stderr.String(), "Ambiente não interativo; pulando criacao automatica do Test Case. Rode interativamente para confirmar a criacao.")
		require.NotContains(t, stderr.String(), "Criar este Test Case no Azure DevOps?")
	})
}

func TestRunTest_PublishSuccessPromptsForEffortAndRealEffort(t *testing.T) {
	created := 0
	updated := 0
	fetches := 0
	var gotEffort *float64
	var gotRealEffort *float64
	restore := stubTestDeps(testDeps{
		gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
		workItemSequence: map[int][]*azure.WorkItem{
			42: {
				{ID: 42, Fields: map[string]interface{}{
					"System.Title":                     "Parent Story",
					"System.WorkItemType":              "User Story",
					"Microsoft.VSTS.Scheduling.Effort": 1.25,
					"Custom.RealEffort":                1.25,
				}},
				{ID: 42, Fields: map[string]interface{}{
					"System.Title":        "Parent Story",
					"System.WorkItemType": "User Story",
				}},
			},
		},
		workItemFetchCalls:  &fetches,
		pr:                  &azure.PullRequest{ID: 99, Title: "Adicionar transcript"},
		llmResp:             "TITULO: Testar publish\n## Objetivo\nBody",
		interactive:         true,
		createdTestCaseID:   88,
		createTestCaseCalls: &created,
		updateTestQACalls:   &updated,
		updateEffort:        &gotEffort,
		updateRealEffort:    &gotRealEffort,
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newTestCommand(stdout, stderr, "y\ny\n0.5\n\n")

	err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key", Providers: "openrouter"}, testFlagSet{workItem: "42", pr: 99, areaPath: "AGROTRACE\\Devops", assignedTo: "qa@empresa.com"}, cmd)
	require.NoError(t, err)
	require.Equal(t, 2, fetches)
	require.Equal(t, 1, created)
	require.Equal(t, 1, updated)
	require.NotNil(t, gotEffort)
	require.NotNil(t, gotRealEffort)
	require.InDelta(t, 0.5, *gotEffort, 0.0001)
	require.InDelta(t, 0.5, *gotRealEffort, 0.0001)
	require.Contains(t, stderr.String(), "Test case criado: #88")
	require.Contains(t, stderr.String(), "https://dev.azure.com/org/project/_workitems/edit/88")
	require.Contains(t, stderr.String(), "Atualizar o work item #42 para Test QA?")
	require.Contains(t, stderr.String(), "Effort (horas decimais, ex: 0.5) [0.5]:")
	require.Contains(t, stderr.String(), "Real Effort (horas decimais) [0.5]:")
	require.Contains(t, stderr.String(), "Work item #42 atualizado para Test QA")
}

func TestRunTest_PublishSuccessPromptsOnlyForMissingValues(t *testing.T) {
	t.Run("only real effort missing", func(t *testing.T) {
		created := 0
		updated := 0
		var gotEffort *float64
		var gotRealEffort *float64
		restore := stubTestDeps(testDeps{
			gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
			workItemSequence: map[int][]*azure.WorkItem{
				42: {
					{ID: 42, Fields: map[string]interface{}{"System.Title": "Parent Story", "System.WorkItemType": "User Story"}},
					{ID: 42, Fields: map[string]interface{}{
						"System.Title":                     "Parent Story",
						"System.WorkItemType":              "User Story",
						"Microsoft.VSTS.Scheduling.Effort": 0.75,
					}},
				},
			},
			pr:                  &azure.PullRequest{ID: 99, Title: "Adicionar transcript"},
			llmResp:             "TITULO: Testar publish\n## Objetivo\nBody",
			interactive:         true,
			createdTestCaseID:   88,
			createTestCaseCalls: &created,
			updateTestQACalls:   &updated,
			updateEffort:        &gotEffort,
			updateRealEffort:    &gotRealEffort,
		})
		defer restore()

		stdout := new(bytes.Buffer)
		stderr := new(bytes.Buffer)
		cmd := newTestCommand(stdout, stderr, "y\ny\n\n")

		err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key", Providers: "openrouter"}, testFlagSet{workItem: "42", pr: 99, areaPath: "AGROTRACE\\Devops", assignedTo: "qa@empresa.com"}, cmd)
		require.NoError(t, err)
		require.Equal(t, 1, created)
		require.Equal(t, 1, updated)
		require.Nil(t, gotEffort)
		require.NotNil(t, gotRealEffort)
		require.InDelta(t, 0.75, *gotRealEffort, 0.0001)
		require.NotContains(t, stderr.String(), "Effort (horas decimais, ex: 0.5) [0.5]:")
		require.Contains(t, stderr.String(), "Real Effort (horas decimais) [0.75]:")
	})

	t.Run("only effort missing", func(t *testing.T) {
		created := 0
		updated := 0
		var gotEffort *float64
		var gotRealEffort *float64
		restore := stubTestDeps(testDeps{
			gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
			workItemSequence: map[int][]*azure.WorkItem{
				42: {
					{ID: 42, Fields: map[string]interface{}{"System.Title": "Parent Story", "System.WorkItemType": "User Story"}},
					{ID: 42, Fields: map[string]interface{}{
						"System.Title":        "Parent Story",
						"System.WorkItemType": "User Story",
						"Custom.RealEffort":   1.5,
					}},
				},
			},
			pr:                  &azure.PullRequest{ID: 99, Title: "Adicionar transcript"},
			llmResp:             "TITULO: Testar publish\n## Objetivo\nBody",
			interactive:         true,
			createdTestCaseID:   88,
			createTestCaseCalls: &created,
			updateTestQACalls:   &updated,
			updateEffort:        &gotEffort,
			updateRealEffort:    &gotRealEffort,
		})
		defer restore()

		stdout := new(bytes.Buffer)
		stderr := new(bytes.Buffer)
		cmd := newTestCommand(stdout, stderr, "y\ny\n0.5\n")

		err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key", Providers: "openrouter"}, testFlagSet{workItem: "42", pr: 99, areaPath: "AGROTRACE\\Devops", assignedTo: "qa@empresa.com"}, cmd)
		require.NoError(t, err)
		require.Equal(t, 1, created)
		require.Equal(t, 1, updated)
		require.NotNil(t, gotEffort)
		require.Nil(t, gotRealEffort)
		require.InDelta(t, 0.5, *gotEffort, 0.0001)
		require.Contains(t, stderr.String(), "Effort (horas decimais, ex: 0.5) [0.5]:")
		require.NotContains(t, stderr.String(), "Real Effort (horas decimais)")
	})
}

func TestPrintTestCardSummary_WritesPlainStdoutFriendlyOutput(t *testing.T) {
	t.Parallel()

	var out bytes.Buffer
	printTestCardSummary(&out, testFlagSet{areaPath: "AGROTRACE\\Devops", assignedTo: "qa@empresa.com"}, &azure.PullRequest{ID: 99}, &azure.WorkItem{ID: 42, Fields: map[string]interface{}{"System.Title": "Parent Story"}}, "groq", "mixtral", "Titulo de teste", "## Objetivo\nBody")

	rendered := out.String()
	require.Contains(t, rendered, "Test Card — PR #99")
	require.Contains(t, rendered, "Provider: groq/mixtral")
	require.Contains(t, rendered, "Work Item: #42 — Parent Story")
	require.Contains(t, rendered, "AreaPath: AGROTRACE\\Devops")
	require.Contains(t, rendered, "Responsável: qa@empresa.com")
	require.Contains(t, rendered, "Título: Titulo de teste")
	require.Contains(t, rendered, "## Objetivo\nBody")
	require.NotContains(t, rendered, "│")
	require.NotContains(t, rendered, "✦")
	require.False(t, regexp.MustCompile(`\x1b\[[0-9;]*m`).MatchString(rendered), rendered)
}

func TestRunTest_CreateFailurePrintsFullBashFallbackBlock(t *testing.T) {
	created := 0
	restore := stubTestDeps(testDeps{
		gitCtx: &git.Context{IsAzureDevOps: true, AzureOrg: "org", AzureProject: "project", AzureRepo: "repo"},
		workItems: map[int]*azure.WorkItem{
			42: {ID: 42, Fields: map[string]interface{}{
				"System.Title":         "Parent Story",
				"System.WorkItemType":  "User Story",
				"System.IterationPath": "AGROTRACE\\Sprint 98",
			}},
		},
		pr:                  &azure.PullRequest{ID: 99, Title: "Adicionar transcript"},
		llmResp:             "TITULO: Testar publish\n## Objetivo\nBody",
		interactive:         true,
		createTestCaseCalls: &created,
		createTestCaseErr:   errors.New("azure create test case: status 400: regra de processo"),
	})
	defer restore()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)
	cmd := newTestCommand(stdout, stderr, "y\n")

	err := runTest(context.Background(), &config.Config{AzurePAT: "pat", OpenRouterAPIKey: "key", Providers: "openrouter"}, testFlagSet{workItem: "42", pr: 99, areaPath: "AGROTRACE\\Devops", assignedTo: "qa@empresa.com"}, cmd)
	require.NoError(t, err)
	require.Equal(t, 1, created)
	requireSubstringsInOrder(t, stderr.String(),
		"Falha ao criar test case",
		"⚠ Não foi possivel criar o Test Case automaticamente",
		"azure create test case: status 400: regra de processo",
		"Campos tentados na criacao:",
		"AreaPath:",
		"IterationPath:",
		"Priority:",
		"Custom.Team:",
		"Custom.ProgramasAgrotrace:",
		"AssignedTo:",
		"Parent:",
		"Use o Markdown acima para criar o card manualmente no Azure DevOps.",
	)
}

type testDeps struct {
	gitCtx              *git.Context
	gitErr              error
	pr                  *azure.PullRequest
	prErr               error
	prIDs               []int
	prIDsErr            error
	workItems           map[int]*azure.WorkItem
	workItemSequence    map[int][]*azure.WorkItem
	workItemFetchCalls  *int
	prWorkItemLookupHit *int
	llmResp             string
	llmErr              error
	interactive         bool
	examples            []string
	createTestCaseCalls *int
	createdTestCaseID   int
	createTestCaseErr   error
	updateTestQACalls   *int
	updateEffort        **float64
	updateRealEffort    **float64
}

func stubTestDeps(deps testDeps) func() {
	origCollect := collectTestGitContext
	origGetPR := fetchTestPullRequest
	origGetPRWorkItemIDs := fetchTestPullRequestWorkItemIDs
	origGetWorkItem := fetchTestWorkItem
	origGetPRIterations := fetchTestPRIterations
	origGetPRChanges := fetchTestPRChanges
	origQueryExamples := queryTestExampleWorkItems
	origRunLLM := runTestLLM
	origTerminal := testIsTerminal
	origCreateTestCase := createAzureTestCase
	origUpdateTestQA := updateAzureWorkItemToTestQA

	collectTestGitContext = func(context.Context) (*git.Context, error) {
		if deps.gitCtx == nil {
			return nil, deps.gitErr
		}
		clone := *deps.gitCtx
		return &clone, deps.gitErr
	}
	fetchTestPullRequest = func(context.Context, string, string, string, string, int) (*azure.PullRequest, error) {
		return deps.pr, deps.prErr
	}
	fetchTestPullRequestWorkItemIDs = func(context.Context, string, string, string, string, int) ([]int, error) {
		if deps.prWorkItemLookupHit != nil {
			*deps.prWorkItemLookupHit = *deps.prWorkItemLookupHit + 1
		}
		return append([]int(nil), deps.prIDs...), deps.prIDsErr
	}
	fetchTestWorkItem = func(_ context.Context, _, _, _ string, wiID int) (*azure.WorkItem, error) {
		if deps.workItemFetchCalls != nil {
			*deps.workItemFetchCalls = *deps.workItemFetchCalls + 1
		}
		if seq, ok := deps.workItemSequence[wiID]; ok {
			if len(seq) == 0 {
				return nil, errors.New("missing work item")
			}
			wi := seq[0]
			deps.workItemSequence[wiID] = seq[1:]
			return wi, nil
		}
		if wi, ok := deps.workItems[wiID]; ok {
			return wi, nil
		}
		return nil, errors.New("missing work item")
	}
	fetchTestPRIterations = func(context.Context, string, string, string, string, int) ([]azure.PRIteration, error) {
		return nil, nil
	}
	fetchTestPRChanges = func(context.Context, string, string, string, string, int, int) ([]azure.PRChange, error) {
		return nil, nil
	}
	queryTestExampleWorkItems = func(context.Context, string, string, string, int) ([]string, error) {
		return append([]string(nil), deps.examples...), nil
	}
	runTestLLM = func(context.Context, config.Config, string, string) (string, string, string, error) {
		return deps.llmResp, "openrouter", "default", deps.llmErr
	}
	testIsTerminal = func(io.Reader) bool {
		return deps.interactive
	}
	createAzureTestCase = func(context.Context, string, string, string, azure.CreateTestCaseRequest) (*azure.WorkItem, error) {
		if deps.createTestCaseCalls != nil {
			*deps.createTestCaseCalls = *deps.createTestCaseCalls + 1
		}
		if deps.createTestCaseErr != nil {
			return nil, deps.createTestCaseErr
		}
		id := deps.createdTestCaseID
		if id == 0 {
			id = 77
		}
		return &azure.WorkItem{ID: id}, nil
	}
	updateAzureWorkItemToTestQA = func(_ context.Context, _, _, _ string, _ int, effort, realEffort *float64) error {
		if deps.updateTestQACalls != nil {
			*deps.updateTestQACalls = *deps.updateTestQACalls + 1
		}
		if deps.updateEffort != nil {
			if effort == nil {
				*deps.updateEffort = nil
			} else {
				v := *effort
				*deps.updateEffort = &v
			}
		}
		if deps.updateRealEffort != nil {
			if realEffort == nil {
				*deps.updateRealEffort = nil
			} else {
				v := *realEffort
				*deps.updateRealEffort = &v
			}
		}
		return nil
	}

	return func() {
		collectTestGitContext = origCollect
		fetchTestPullRequest = origGetPR
		fetchTestPullRequestWorkItemIDs = origGetPRWorkItemIDs
		fetchTestWorkItem = origGetWorkItem
		fetchTestPRIterations = origGetPRIterations
		fetchTestPRChanges = origGetPRChanges
		queryTestExampleWorkItems = origQueryExamples
		runTestLLM = origRunLLM
		testIsTerminal = origTerminal
		createAzureTestCase = origCreateTestCase
		updateAzureWorkItemToTestQA = origUpdateTestQA
	}
}

func newTestCommand(stdout, stderr *bytes.Buffer, input string) *cobra.Command {
	cmd := &cobra.Command{}
	cmd.SetOut(stdout)
	cmd.SetErr(stderr)
	cmd.SetIn(strings.NewReader(input))
	return cmd
}

func requireSubstringsInOrder(t *testing.T, got string, want ...string) {
	t.Helper()
	pos := 0
	for _, item := range want {
		idx := strings.Index(got[pos:], item)
		if idx < 0 {
			require.Failf(t, "missing substring", "did not find %q after byte %d in:\n%s", item, pos, got)
		}
		pos += idx + len(item)
	}
}
