package azure_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/azure"
	"github.com/stretchr/testify/require"
)

func TestGetWorkItem_ParsesResponse(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]interface{}{
			"id": 42,
			"fields": map[string]interface{}{
				"System.Title":        "My Work Item",
				"System.WorkItemType": "User Story",
				"System.Description":  "Some description",
			},
		})
	}))
	defer srv.Close()

	client := azure.NewClient("test-pat", "myorg").WithBaseURL(srv.URL)
	wi, err := client.GetWorkItem(context.Background(), "myproject", 42)

	require.NoError(t, err)
	require.NotNil(t, wi)
	require.Equal(t, 42, wi.ID)
	require.Equal(t, "My Work Item", wi.Title())
	require.Equal(t, "User Story", wi.Type())
	require.Equal(t, "Some description", wi.Description())
}

func TestCreateTestCase_SendsCorrectPatchOps(t *testing.T) {
	var capturedBody []map[string]interface{}
	var capturedContentType string

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		capturedContentType = r.Header.Get("Content-Type")
		json.NewDecoder(r.Body).Decode(&capturedBody)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]interface{}{
			"id": 99,
			"fields": map[string]interface{}{
				"System.Title": "Test Case Title",
			},
		})
	}))
	defer srv.Close()

	client := azure.NewClient("test-pat", "myorg").WithBaseURL(srv.URL)
	req := azure.CreateTestCaseRequest{
		Title:      "Test Case Title",
		AreaPath:   "MyProject\\MyArea",
		AssignedTo: "user@example.com",
		ParentID:   123,
	}
	wi, err := client.CreateTestCase(context.Background(), "myproject", req)

	require.NoError(t, err)
	require.NotNil(t, wi)
	require.Equal(t, 99, wi.ID)
	require.Equal(t, "Test Case Title", wi.Title())

	// Verify the correct Content-Type was sent for JSON Patch
	require.Equal(t, "application/json-patch+json", capturedContentType)

	// Verify patch ops structure
	require.GreaterOrEqual(t, len(capturedBody), 3, "expected at least 3 patch ops")

	// First op must set the title
	require.Equal(t, "add", capturedBody[0]["op"])
	require.Equal(t, "/fields/System.Title", capturedBody[0]["path"])
	require.Equal(t, "Test Case Title", capturedBody[0]["value"])

	// Find the AreaPath op
	found := false
	for _, op := range capturedBody {
		if op["path"] == "/fields/System.AreaPath" {
			require.Equal(t, "add", op["op"])
			require.Equal(t, "MyProject\\MyArea", op["value"])
			found = true
		}
	}
	require.True(t, found, "expected AreaPath patch op")
}

func TestGetWorkItem_ReturnsErrorOn404(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
		w.Write([]byte(`{"message":"Work item not found"}`))
	}))
	defer srv.Close()

	client := azure.NewClient("test-pat", "myorg").WithBaseURL(srv.URL)
	wi, err := client.GetWorkItem(context.Background(), "myproject", 999)

	require.Error(t, err)
	require.Nil(t, wi)
	require.Contains(t, err.Error(), "404")
}
