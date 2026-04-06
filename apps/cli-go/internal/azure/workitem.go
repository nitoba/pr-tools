package azure

import (
	"context"
	"encoding/json"
	"fmt"
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
