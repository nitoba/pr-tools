package azure

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
)

// Client is an Azure DevOps REST API client.
type Client struct {
	pat             string
	organization    string
	httpClient      *http.Client
	baseURLOverride string // for testing
}

func NewClient(pat, organization string) *Client {
	return &Client{
		pat:          pat,
		organization: organization,
		httpClient:   &http.Client{},
	}
}

// WithBaseURL returns a new client with the base URL overridden (for testing).
func (c *Client) WithBaseURL(url string) *Client {
	copy := *c
	copy.baseURLOverride = url
	return &copy
}

// authHeader returns the Basic auth header value for Azure DevOps PAT.
// Azure DevOps expects: Basic base64(":"+ pat)
func (c *Client) authHeader() string {
	credentials := ":" + c.pat
	encoded := base64.StdEncoding.EncodeToString([]byte(credentials))
	return "Basic " + encoded
}

func (c *Client) baseURL() string {
	if c.baseURLOverride != "" {
		return c.baseURLOverride
	}
	return fmt.Sprintf("https://dev.azure.com/%s", c.organization)
}

func (c *Client) doRequest(ctx context.Context, method, fullURL string, body interface{}) ([]byte, error) {
	var reqBody io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			return nil, err
		}
		reqBody = bytes.NewReader(b)
	}

	req, err := http.NewRequestWithContext(ctx, method, fullURL, reqBody)
	if err != nil {
		return nil, err
	}

	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", c.authHeader())

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("azure: status %d: %s", resp.StatusCode, strings.TrimSpace(string(respBody)))
	}

	return respBody, nil
}
