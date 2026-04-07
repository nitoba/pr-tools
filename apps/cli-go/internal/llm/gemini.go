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
	Register(&GeminiProvider{})
}

type GeminiProvider struct{}

func (p *GeminiProvider) Name() string         { return "gemini" }
func (p *GeminiProvider) DefaultModel() string { return "gemini-3.1-flash-lite-preview" }

func (p *GeminiProvider) NewClient(apiKey, model string) (LLMClient, error) {
	if apiKey == "" {
		return nil, fmt.Errorf("gemini: api key required")
	}
	return &geminiClient{
		apiKey:     apiKey,
		model:      model,
		httpClient: &http.Client{},
	}, nil
}

type geminiClient struct {
	apiKey     string
	model      string
	httpClient *http.Client
}

func (c *geminiClient) Name() string  { return "gemini" }
func (c *geminiClient) Model() string { return c.model }

func (c *geminiClient) Chat(ctx context.Context, messages []Message) (string, error) {
	// Combine system and user messages into a single text field
	var systemText, userText string
	for _, msg := range messages {
		switch msg.Role {
		case "system":
			systemText = msg.Content
		case "user":
			userText = msg.Content
		}
	}

	combinedText := fmt.Sprintf("<system>\n%s\n</system>\n<user>\n%s\n</user>", systemText, userText)

	reqBody := map[string]interface{}{
		"contents": []map[string]interface{}{
			{
				"role": "user",
				"parts": []map[string]interface{}{
					{"text": combinedText},
				},
			},
		},
	}

	reqBytes, err := json.Marshal(reqBody)
	if err != nil {
		return "", err
	}

	url := fmt.Sprintf("https://generativelanguage.googleapis.com/v1beta/models/%s:generateContent?key=%s", c.model, c.apiKey)
	req, err := http.NewRequestWithContext(ctx, "POST", url, bytes.NewReader(reqBytes))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return "", err
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode >= 400 {
		body, _ := io.ReadAll(resp.Body)
		return "", fmt.Errorf("gemini: status %d: %s", resp.StatusCode, string(body))
	}

	var response struct {
		Candidates []struct {
			Content struct {
				Parts []struct {
					Text string `json:"text"`
				} `json:"parts"`
			} `json:"content"`
		} `json:"candidates"`
	}

	if err := json.NewDecoder(resp.Body).Decode(&response); err != nil {
		return "", err
	}

	if len(response.Candidates) == 0 || len(response.Candidates[0].Content.Parts) == 0 {
		return "", fmt.Errorf("gemini: no candidates in response")
	}

	return response.Candidates[0].Content.Parts[0].Text, nil
}
