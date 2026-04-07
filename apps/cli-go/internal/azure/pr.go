package azure

import (
	"context"
	"encoding/json"
	"fmt"
	"strconv"
)

type PullRequest struct {
	ID          int    `json:"pullRequestId"`
	Title       string `json:"title"`
	Description string `json:"description"`
	SourceRef   string `json:"sourceRefName"`
	TargetRef   string `json:"targetRefName"`
	URL         string `json:"webUrl"`
}

// PRReviewer is a reviewer for a PR.
type PRReviewer struct {
	UniqueName string `json:"uniqueName"`
}

type CreatePRRequest struct {
	Title       string       `json:"title"`
	Description string       `json:"description"`
	SourceRef   string       `json:"sourceRefName"`
	TargetRef   string       `json:"targetRefName"`
	Reviewers   []PRReviewer `json:"reviewers,omitempty"`
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

func (c *Client) GetPullRequestWorkItemIDs(ctx context.Context, project, repo string, prID int) ([]int, error) {
	url := fmt.Sprintf("%s/%s/_apis/git/repositories/%s/pullRequests/%d/workitems?api-version=7.1",
		c.baseURL(), project, repo, prID)
	data, err := c.doRequest(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}

	var result struct {
		Value []struct {
			ID interface{} `json:"id"`
		} `json:"value"`
	}
	if err := json.Unmarshal(data, &result); err != nil {
		return nil, err
	}

	ids := make([]int, 0, len(result.Value))
	for _, item := range result.Value {
		id, err := toWorkItemID(item.ID)
		if err != nil {
			return nil, err
		}
		ids = append(ids, id)
	}

	return ids, nil
}

func toWorkItemID(v interface{}) (int, error) {
	switch id := v.(type) {
	case float64:
		return int(id), nil
	case string:
		return strconv.Atoi(id)
	default:
		return 0, fmt.Errorf("unexpected work item id type %T", v)
	}
}

// PRIteration is a PR iteration (version).
type PRIteration struct {
	ID int `json:"id"`
}

// PRChange is a changed file in a PR.
type PRChange struct {
	ChangeType string `json:"changeType"`
	Item       struct {
		Path string `json:"path"`
	} `json:"item"`
}

// GetPRIterations returns the iterations of a PR.
func (c *Client) GetPRIterations(ctx context.Context, project, repo string, prID int) ([]PRIteration, error) {
	url := fmt.Sprintf("%s/%s/_apis/git/repositories/%s/pullRequests/%d/iterations?api-version=7.0",
		c.baseURL(), project, repo, prID)
	data, err := c.doRequest(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}
	var result struct {
		Value []PRIteration `json:"value"`
	}
	if err := json.Unmarshal(data, &result); err != nil {
		return nil, err
	}
	return result.Value, nil
}

// GetPRChanges returns the changed files in a PR iteration.
func (c *Client) GetPRChanges(ctx context.Context, project, repo string, prID, iterationID int) ([]PRChange, error) {
	url := fmt.Sprintf("%s/%s/_apis/git/repositories/%s/pullRequests/%d/iterations/%d/changes?api-version=7.0&$top=200",
		c.baseURL(), project, repo, prID, iterationID)
	data, err := c.doRequest(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}
	var result struct {
		ChangeEntries []PRChange `json:"changeEntries"`
	}
	if err := json.Unmarshal(data, &result); err != nil {
		return nil, err
	}
	changes := result.ChangeEntries
	if len(changes) > 50 {
		changes = changes[:50]
	}
	return changes, nil
}
