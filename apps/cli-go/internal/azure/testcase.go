package azure

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
)

type CreateTestCaseRequest struct {
	Title       string
	AreaPath    string
	AssignedTo  string
	ParentID    int
	Description string
}

type patchOp struct {
	Op    string      `json:"op"`
	Path  string      `json:"path"`
	Value interface{} `json:"value"`
}

// CreateTestCase creates a new Azure DevOps Test Case work item using JSON Patch format.
func (c *Client) CreateTestCase(ctx context.Context, project string, req CreateTestCaseRequest) (*WorkItem, error) {
	url := fmt.Sprintf("%s/%s/_apis/wit/workitems/$Test%%20Case?api-version=7.1",
		c.baseURL(), project)

	ops := []patchOp{
		{Op: "add", Path: "/fields/System.Title", Value: req.Title},
	}
	if req.AreaPath != "" {
		ops = append(ops, patchOp{Op: "add", Path: "/fields/System.AreaPath", Value: req.AreaPath})
	}
	if req.AssignedTo != "" {
		ops = append(ops, patchOp{Op: "add", Path: "/fields/System.AssignedTo", Value: req.AssignedTo})
	}
	if req.Description != "" {
		ops = append(ops, patchOp{Op: "add", Path: "/fields/System.Description", Value: req.Description})
	}
	if req.ParentID > 0 {
		ops = append(ops, patchOp{
			Op:   "add",
			Path: "/relations/-",
			Value: map[string]interface{}{
				"rel": "System.LinkTypes.Hierarchy-Reverse",
				"url": fmt.Sprintf("%s/_apis/wit/workitems/%d", c.baseURL(), req.ParentID),
			},
		})
	}

	b, err := json.Marshal(ops)
	if err != nil {
		return nil, err
	}

	httpReq, err := http.NewRequestWithContext(ctx, "PATCH", url, bytes.NewReader(b))
	if err != nil {
		return nil, err
	}
	httpReq.Header.Set("Content-Type", "application/json-patch+json")
	httpReq.Header.Set("Authorization", c.authHeader())

	resp, err := c.httpClient.Do(httpReq)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("azure create test case: status %d: %s", resp.StatusCode, string(respBody))
	}

	var wi WorkItem
	if err := json.Unmarshal(respBody, &wi); err != nil {
		return nil, err
	}
	return &wi, nil
}
