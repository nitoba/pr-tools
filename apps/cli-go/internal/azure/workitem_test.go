package azure

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestUpdateWorkItemToTestQA_SendsEffortAndRealEffort(t *testing.T) {
	var capturedBody []map[string]interface{}

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&capturedBody)
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"id":42}`))
	}))
	defer srv.Close()

	effort := 0.5
	realEffort := 1.25
	client := NewClient("test-pat", "myorg").WithBaseURL(srv.URL)
	err := client.UpdateWorkItemToTestQA(context.Background(), "myproject", 42, &effort, &realEffort)

	require.NoError(t, err)
	require.Equal(t, []map[string]interface{}{
		{"op": "add", "path": "/fields/System.State", "value": "Test QA"},
		{"op": "add", "path": "/fields/Microsoft.VSTS.Scheduling.Effort", "value": 0.5},
		{"op": "add", "path": "/fields/Custom.RealEffort", "value": 1.25},
	}, capturedBody)
}

func TestUpdateWorkItemToTestQA_LeavesOptionalFieldsOutWhenNil(t *testing.T) {
	var capturedBody []map[string]interface{}

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&capturedBody)
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"id":42}`))
	}))
	defer srv.Close()

	client := NewClient("test-pat", "myorg").WithBaseURL(srv.URL)
	err := client.UpdateWorkItemToTestQA(context.Background(), "myproject", 42, nil, nil)

	require.NoError(t, err)
	require.Equal(t, []map[string]interface{}{
		{"op": "add", "path": "/fields/System.State", "value": "Test QA"},
	}, capturedBody)
}
