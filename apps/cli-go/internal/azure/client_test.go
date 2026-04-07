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
		_ = json.NewEncoder(w).Encode(map[string]interface{}{
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
		_ = json.NewDecoder(r.Body).Decode(&capturedBody)
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]interface{}{
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

func TestGetPullRequestWorkItemIDs_ReturnsIDs(t *testing.T) {
	var requestPath string

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requestPath = r.URL.Path
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]interface{}{
			"value": []map[string]interface{}{
				{"id": "300"},
				{"id": "11796"},
				{"id": "11820"},
			},
		})
	}))
	defer srv.Close()

	client := azure.NewClient("test-pat", "myorg").WithBaseURL(srv.URL)
	ids, err := client.GetPullRequestWorkItemIDs(context.Background(), "myproject", "myrepo", 77)

	require.NoError(t, err)
	require.Equal(t, []int{300, 11796, 11820}, ids)
	require.Equal(t, "/myproject/_apis/git/repositories/myrepo/pullRequests/77/workitems", requestPath)
}

func TestCreateTestCase_SendsBashParityFields(t *testing.T) {
	var capturedBody []map[string]interface{}

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&capturedBody)
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]interface{}{
			"id": 99,
			"fields": map[string]interface{}{
				"System.Title": "Test Case Title",
			},
		})
	}))
	defer srv.Close()

	priority := 2
	client := azure.NewClient("test-pat", "myorg").WithBaseURL(srv.URL)
	_, err := client.CreateTestCase(context.Background(), "myproject", azure.CreateTestCaseRequest{
		Title:           "Test Case Title",
		DescriptionHTML: "<p>body</p>",
		StepsXML:        "<steps id=\"0\" last=\"2\"></steps>",
		AreaPath:        "AGROTRACE\\Devops",
		ParentID:        123,
		IterationPath:   "AGROTRACE\\Sprint 98",
		Priority:        &priority,
		Team:            "DevOps",
		Program:         "Agrotrace",
		AssignedTo:      "user@example.com",
	})

	require.NoError(t, err)
	require.Equal(t, []map[string]interface{}{
		{"op": "add", "path": "/fields/System.Title", "value": "Test Case Title"},
		{"op": "add", "path": "/fields/System.Description", "value": "<p>body</p>"},
		{"op": "add", "path": "/fields/Microsoft.VSTS.TCM.Steps", "value": "<steps id=\"0\" last=\"2\"></steps>"},
		{"op": "add", "path": "/fields/System.AreaPath", "value": "AGROTRACE\\Devops"},
		{"op": "add", "path": "/relations/-", "value": map[string]interface{}{"rel": "System.LinkTypes.Hierarchy-Reverse", "url": srv.URL + "/_apis/wit/workitems/123"}},
		{"op": "add", "path": "/fields/System.IterationPath", "value": "AGROTRACE\\Sprint 98"},
		{"op": "add", "path": "/fields/Microsoft.VSTS.Common.Priority", "value": float64(2)},
		{"op": "add", "path": "/fields/Custom.Team", "value": "DevOps"},
		{"op": "add", "path": "/fields/Custom.ProgramasAgrotrace", "value": "Agrotrace"},
		{"op": "add", "path": "/fields/System.AssignedTo", "value": "user@example.com"},
	}, capturedBody)
}

func TestGetWorkItem_ReturnsErrorOn404(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
		_, _ = w.Write([]byte(`{"message":"Work item not found"}`))
	}))
	defer srv.Close()

	client := azure.NewClient("test-pat", "myorg").WithBaseURL(srv.URL)
	wi, err := client.GetWorkItem(context.Background(), "myproject", 999)

	require.Error(t, err)
	require.Nil(t, wi)
	require.Contains(t, err.Error(), "404")
}
