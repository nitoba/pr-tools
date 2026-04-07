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
	Title           string
	DescriptionHTML string
	StepsXML        string
	AreaPath        string
	ParentID        int
	IterationPath   string
	Priority        *int
	Team            string
	Program         string
	AssignedTo      string
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
	if req.DescriptionHTML != "" {
		ops = append(ops, patchOp{Op: "add", Path: "/fields/System.Description", Value: req.DescriptionHTML})
	}
	if req.StepsXML != "" {
		ops = append(ops, patchOp{Op: "add", Path: "/fields/Microsoft.VSTS.TCM.Steps", Value: req.StepsXML})
	}
	if req.AreaPath != "" {
		ops = append(ops, patchOp{Op: "add", Path: "/fields/System.AreaPath", Value: req.AreaPath})
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
	if req.IterationPath != "" {
		ops = append(ops, patchOp{Op: "add", Path: "/fields/System.IterationPath", Value: req.IterationPath})
	}
	if req.Priority != nil {
		ops = append(ops, patchOp{Op: "add", Path: "/fields/Microsoft.VSTS.Common.Priority", Value: *req.Priority})
	}
	if req.Team != "" {
		ops = append(ops, patchOp{Op: "add", Path: "/fields/Custom.Team", Value: req.Team})
	}
	if req.Program != "" {
		ops = append(ops, patchOp{Op: "add", Path: "/fields/Custom.ProgramasAgrotrace", Value: req.Program})
	}
	if req.AssignedTo != "" {
		ops = append(ops, patchOp{Op: "add", Path: "/fields/System.AssignedTo", Value: req.AssignedTo})
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
	defer func() { _ = resp.Body.Close() }()

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
