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
	Register(&OllamaProvider{})
}

type OllamaProvider struct{}

func (p *OllamaProvider) Name() string         { return "ollama" }
func (p *OllamaProvider) DefaultModel() string { return "llama3.2" }

func (p *OllamaProvider) NewClient(apiKey, model string) (LLMClient, error) {
	if apiKey == "" {
		return nil, fmt.Errorf("ollama: api key required")
	}
	return &ollamaClient{
		apiKey:     apiKey,
		model:      model,
		httpClient: &http.Client{},
	}, nil
}

type ollamaClient struct {
	apiKey     string
	model      string
	httpClient *http.Client
}

func (c *ollamaClient) Name() string  { return "ollama" }
func (c *ollamaClient) Model() string { return c.model }

func (c *ollamaClient) Chat(ctx context.Context, messages []Message) (string, error) {
	reqBody := map[string]interface{}{
		"model":    c.model,
		"messages": messages,
	}

	reqBytes, err := json.Marshal(reqBody)
	if err != nil {
		return "", err
	}

	req, err := http.NewRequestWithContext(ctx, "POST", "https://ollama.com/v1/chat/completions", bytes.NewReader(reqBytes))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+c.apiKey)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return "", err
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode >= 400 {
		body, _ := io.ReadAll(resp.Body)
		return "", fmt.Errorf("ollama: status %d: %s", resp.StatusCode, string(body))
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
		return "", fmt.Errorf("ollama: no choices in response")
	}

	return response.Choices[0].Message.Content, nil
}
