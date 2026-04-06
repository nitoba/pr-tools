package llm

import (
	"context"
	"fmt"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// mockLLMClient is a test-only LLM client.
type mockLLMClient struct {
	name     string
	model    string
	response string
	err      error
}

func (m *mockLLMClient) Name() string  { return m.name }
func (m *mockLLMClient) Model() string { return m.model }
func (m *mockLLMClient) Chat(_ context.Context, _ []Message) (string, error) {
	return m.response, m.err
}

// newTestFallbackClient builds a FallbackClient directly from a list of clients,
// bypassing the registry, to allow precise test control.
func newTestFallbackClient(clients ...LLMClient) *FallbackClient {
	return &FallbackClient{clients: clients}
}

func TestFallbackClient_UsesFirstProvider(t *testing.T) {
	client := &mockLLMClient{
		name:     "mock1",
		model:    "mock-model",
		response: "hello from mock1",
		err:      nil,
	}

	fc := newTestFallbackClient(client)

	resp, provider, err := fc.Chat(context.Background(), "system prompt", "user prompt")
	require.NoError(t, err)
	assert.Equal(t, "hello from mock1", resp)
	assert.Equal(t, "mock1", provider)
}

func TestFallbackClient_FallsBackOnError(t *testing.T) {
	failing := &mockLLMClient{
		name:     "mock-fail",
		model:    "mock-model",
		response: "",
		err:      fmt.Errorf("provider error"),
	}
	succeeding := &mockLLMClient{
		name:     "mock-success",
		model:    "mock-model",
		response: "hello from mock-success",
		err:      nil,
	}

	fc := newTestFallbackClient(failing, succeeding)

	resp, provider, err := fc.Chat(context.Background(), "system prompt", "user prompt")
	require.NoError(t, err)
	assert.Equal(t, "hello from mock-success", resp)
	assert.Equal(t, "mock-success", provider)
}

func TestFallbackClient_AllFail(t *testing.T) {
	failing1 := &mockLLMClient{
		name: "mock-fail1",
		err:  fmt.Errorf("provider 1 error"),
	}
	failing2 := &mockLLMClient{
		name: "mock-fail2",
		err:  fmt.Errorf("provider 2 error"),
	}

	fc := newTestFallbackClient(failing1, failing2)

	resp, provider, err := fc.Chat(context.Background(), "system prompt", "user prompt")
	require.Error(t, err)
	assert.Contains(t, err.Error(), "todos os provedores falharam")
	assert.Empty(t, resp)
	assert.Empty(t, provider)
}
