package llm

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
)

func init() {
	Register(&OpenRouterProvider{})
}

type OpenRouterProvider struct{}

func (p *OpenRouterProvider) Name() string         { return "openrouter" }
func (p *OpenRouterProvider) DefaultModel() string { return "meta-llama/llama-3.3-70b-instruct:free" }

func (p *OpenRouterProvider) NewClient(apiKey, model string) (LLMClient, error) {
	if apiKey == "" {
		return nil, fmt.Errorf("openrouter: api key required")
	}
	return &openRouterClient{
		apiKey:     apiKey,
		model:      model,
		httpClient: &http.Client{},
	}, nil
}

type openRouterClient struct {
	apiKey     string
	model      string
	httpClient *http.Client
}

func (c *openRouterClient) Name() string  { return "openrouter" }
func (c *openRouterClient) Model() string { return c.model }

func (c *openRouterClient) Chat(ctx context.Context, messages []Message) (string, error) {
	reqBody := map[string]interface{}{
		"model":    c.model,
		"messages": messages,
	}

	reqBytes, err := json.Marshal(reqBody)
	if err != nil {
		return "", err
	}

	req, err := http.NewRequestWithContext(ctx, "POST", "https://openrouter.ai/api/v1/chat/completions", bytes.NewReader(reqBytes))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+c.apiKey)
	req.Header.Set("HTTP-Referer", "https://prt.dev")
	req.Header.Set("X-Title", "PRT")

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return "", err
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode >= 400 {
		body, _ := io.ReadAll(resp.Body)
		return "", fmt.Errorf("openrouter: status %d: %s", resp.StatusCode, string(body))
	}

	var response struct {
		Choices []struct {
			Message struct {
				Content string `json:"content"`
			} `json:"message"`
		} `json:"choices"`
	}

	if err := json.NewDecoder(resp.Body).Decode(&response); err != nil {
		return "", err
	}

	if len(response.Choices) == 0 {
		return "", fmt.Errorf("openrouter: no choices in response")
	}

	return response.Choices[0].Message.Content, nil
}
