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
	Register(&GroqProvider{})
}

type GroqProvider struct{}

func (p *GroqProvider) Name() string         { return "groq" }
func (p *GroqProvider) DefaultModel() string { return "qwen/qwen3-32b" }

func (p *GroqProvider) NewClient(apiKey, model string) (LLMClient, error) {
	if apiKey == "" {
		return nil, fmt.Errorf("groq: api key required")
	}
	return &groqClient{
		apiKey:     apiKey,
		model:      model,
		httpClient: &http.Client{},
	}, nil
}

type groqClient struct {
	apiKey     string
	model      string
	httpClient *http.Client
}

func (c *groqClient) Name() string  { return "groq" }
func (c *groqClient) Model() string { return c.model }

func (c *groqClient) Chat(ctx context.Context, messages []Message) (string, error) {
	reqBody := map[string]interface{}{
		"model":    c.model,
		"messages": messages,
	}

	reqBytes, err := json.Marshal(reqBody)
	if err != nil {
		return "", err
	}

	req, err := http.NewRequestWithContext(ctx, "POST", "https://api.groq.com/openai/v1/chat/completions", bytes.NewReader(reqBytes))
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
		return "", fmt.Errorf("groq: status %d: %s", resp.StatusCode, string(body))
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
		return "", fmt.Errorf("groq: no choices in response")
	}

	return response.Choices[0].Message.Content, nil
}
