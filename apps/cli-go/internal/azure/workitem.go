package azure

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
)

type WorkItem struct {
	ID     int                    `json:"id"`
	Fields map[string]interface{} `json:"fields"`
}

func (wi *WorkItem) Title() string {
	if v, ok := wi.Fields["System.Title"]; ok {
		if s, ok := v.(string); ok {
			return s
		}
	}
	return ""
}

func (wi *WorkItem) Type() string {
	if v, ok := wi.Fields["System.WorkItemType"]; ok {
		if s, ok := v.(string); ok {
			return s
		}
	}
	return ""
}

func (wi *WorkItem) Description() string {
	if v, ok := wi.Fields["System.Description"]; ok {
		if s, ok := v.(string); ok {
			return s
		}
	}
	return ""
}

func (c *Client) GetWorkItem(ctx context.Context, project string, id int) (*WorkItem, error) {
	url := fmt.Sprintf("%s/%s/_apis/wit/workitems/%d?api-version=7.1",
		c.baseURL(), project, id)
	data, err := c.doRequest(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}
	var wi WorkItem
	if err := json.Unmarshal(data, &wi); err != nil {
		return nil, err
	}
	return &wi, nil
}

// WIQLResult is a work item query result.
type WIQLResult struct {
	WorkItems []struct {
		ID int `json:"id"`
	} `json:"workItems"`
}

// QueryWorkItems runs a WIQL query and returns work item IDs.
func (c *Client) QueryWorkItems(ctx context.Context, project, wiql string) ([]int, error) {
	url := fmt.Sprintf("%s/%s/_apis/wit/wiql?api-version=7.0", c.baseURL(), project)
	body := map[string]string{"query": wiql}
	data, err := c.doRequest(ctx, "POST", url, body)
	if err != nil {
		return nil, err
	}
	var result WIQLResult
	if err := json.Unmarshal(data, &result); err != nil {
		return nil, err
	}
	ids := make([]int, 0, len(result.WorkItems))
	for _, wi := range result.WorkItems {
		ids = append(ids, wi.ID)
	}
	return ids, nil
}

// UpdateWorkItemState updates the state of a work item via JSON Patch.
func (c *Client) UpdateWorkItemState(ctx context.Context, project string, wiID int, state string) error {
	url := fmt.Sprintf("%s/%s/_apis/wit/workitems/%d?api-version=7.0", c.baseURL(), project, wiID)
	ops := []map[string]interface{}{
		{"op": "add", "path": "/fields/System.State", "value": state},
	}
	b, err := json.Marshal(ops)
	if err != nil {
		return err
	}

	req, err := http.NewRequestWithContext(ctx, "PATCH", url, bytes.NewReader(b))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json-patch+json")
	req.Header.Set("Authorization", c.authHeader())

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return err
	}
	defer func() { _ = resp.Body.Close() }()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return err
	}
	if resp.StatusCode >= 400 {
		return fmt.Errorf("azure update work item state: status %d: %s", resp.StatusCode, string(respBody))
	}
	return nil
}

// WorkItemField returns the field value for the given key, or empty string.
func (wi *WorkItem) Field(key string) string {
	if v, ok := wi.Fields[key]; ok {
		if s, ok := v.(string); ok {
			return s
		}
	}
	return ""
}

// Sprint returns the sprint number (e.g. "98") from the iteration path,
// or empty string if not present.
func (wi *WorkItem) Sprint() string {
	path := wi.Field("System.IterationPath")
	if path == "" {
		return ""
	}
	// Iteration paths look like: "Project\Sprint 98" or "Project\Sprint\98"
	// Extract last numeric token
	parts := strings.FieldsFunc(path, func(r rune) bool {
		return r == '\\' || r == '/' || r == ' '
	})
	for i := len(parts) - 1; i >= 0; i-- {
		if isAllDigits(parts[i]) {
			return parts[i]
		}
	}
	return ""
}

func isAllDigits(s string) bool {
	if s == "" {
		return false
	}
	for _, r := range s {
		if r < '0' || r > '9' {
			return false
		}
	}
	return true
}
