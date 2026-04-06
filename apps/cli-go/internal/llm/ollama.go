package llm

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
)

func init() {
	Register(&OllamaProvider{})
}

type OllamaProvider struct{}

func (p *OllamaProvider) Name() string         { return "ollama" }
func (p *OllamaProvider) DefaultModel() string { return "llama3.2" }

func (p *OllamaProvider) NewClient(apiKey, model string) (LLMClient, error) {
	return &ollamaClient{
		model:      model,
		httpClient: &http.Client{},
	}, nil
}

type ollamaClient struct {
	model      string
	httpClient *http.Client
}

func (c *ollamaClient) Name() string  { return "ollama" }
func (c *ollamaClient) Model() string { return c.model }

func (c *ollamaClient) Chat(ctx context.Context, messages []Message) (string, error) {
	reqBody := map[string]interface{}{
		"model":    c.model,
		"messages": messages,
		"stream":   false,
	}

	reqBytes, err := json.Marshal(reqBody)
	if err != nil {
		return "", err
	}

	req, err := http.NewRequestWithContext(ctx, "POST", "http://localhost:11434/api/chat", bytes.NewReader(reqBytes))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	if resp.StatusCode >= 400 {
		return "", fmt.Errorf("ollama: status %d", resp.StatusCode)
	}

	var response struct {
		Message struct {
			Content string `json:"content"`
		} `json:"message"`
	}

	if err := json.NewDecoder(resp.Body).Decode(&response); err != nil {
		return "", err
	}

	if response.Message.Content == "" {
		return "", fmt.Errorf("ollama: empty response")
	}

	return response.Message.Content, nil
}
