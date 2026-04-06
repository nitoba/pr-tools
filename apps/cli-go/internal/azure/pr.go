package azure

import (
	"context"
	"encoding/json"
	"fmt"
)

type PullRequest struct {
	ID          int    `json:"pullRequestId"`
	Title       string `json:"title"`
	Description string `json:"description"`
	SourceRef   string `json:"sourceRefName"`
	TargetRef   string `json:"targetRefName"`
	URL         string `json:"webUrl"`
}

type CreatePRRequest struct {
	Title       string `json:"title"`
	Description string `json:"description"`
	SourceRef   string `json:"sourceRefName"`
	TargetRef   string `json:"targetRefName"`
}

func (c *Client) GetPullRequest(ctx context.Context, project, repo string, prID int) (*PullRequest, error) {
	url := fmt.Sprintf("%s/%s/_apis/git/repositories/%s/pullRequests/%d?api-version=7.1",
		c.baseURL(), project, repo, prID)
	data, err := c.doRequest(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}
	var pr PullRequest
	if err := json.Unmarshal(data, &pr); err != nil {
		return nil, err
	}
	return &pr, nil
}

func (c *Client) CreatePullRequest(ctx context.Context, project, repo string, req CreatePRRequest) (*PullRequest, error) {
	url := fmt.Sprintf("%s/%s/_apis/git/repositories/%s/pullRequests?api-version=7.1",
		c.baseURL(), project, repo)
	data, err := c.doRequest(ctx, "POST", url, req)
	if err != nil {
		return nil, err
	}
	var pr PullRequest
	if err := json.Unmarshal(data, &pr); err != nil {
		return nil, err
	}
	return &pr, nil
}
