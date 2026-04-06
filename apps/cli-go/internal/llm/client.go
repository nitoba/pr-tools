package llm

import (
	"context"
	"fmt"
	"strings"
)

type Message struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

// LLMClient is the interface for calling an LLM.
type LLMClient interface {
	Name() string
	Model() string
	Chat(ctx context.Context, messages []Message) (string, error)
}

// Provider knows how to create LLMClient instances.
type Provider interface {
	Name() string
	DefaultModel() string
	NewClient(apiKey, model string) (LLMClient, error)
}

var registry = make(map[string]Provider)

func Register(p Provider) {
	registry[p.Name()] = p
}

func GetProvider(name string) (Provider, bool) {
	p, ok := registry[name]
	return p, ok
}

// Config holds API keys and models per provider.
type Config struct {
	Providers        string
	OpenRouterAPIKey string
	GroqAPIKey       string
	GeminiAPIKey     string
	OllamaAPIKey     string
	OpenRouterModel  string
	GroqModel        string
	GeminiModel      string
	OllamaModel      string
}

// FallbackClient tries providers in order, returning the first success.
type FallbackClient struct {
	clients []LLMClient
}

func NewFallbackClient(cfg Config) *FallbackClient {
	fc := &FallbackClient{}

	providerNames := strings.Split(cfg.Providers, ",")
	for _, name := range providerNames {
		name = strings.TrimSpace(name)
		if name == "" {
			continue
		}
		p, ok := GetProvider(name)
		if !ok {
			continue
		}

		var apiKey, model string
		switch name {
		case "openrouter":
			apiKey = cfg.OpenRouterAPIKey
			model = cfg.OpenRouterModel
		case "groq":
			apiKey = cfg.GroqAPIKey
			model = cfg.GroqModel
		case "gemini":
			apiKey = cfg.GeminiAPIKey
			model = cfg.GeminiModel
		case "ollama":
			apiKey = cfg.OllamaAPIKey
			model = cfg.OllamaModel
		}

		if model == "" {
			model = p.DefaultModel()
		}

		client, err := p.NewClient(apiKey, model)
		if err != nil {
			continue
		}
		fc.clients = append(fc.clients, client)
	}

	return fc
}

// Chat tries each provider in order, returning the first success.
// Returns (response, providerName, error).
func (fc *FallbackClient) Chat(ctx context.Context, system, user string) (string, string, error) {
	messages := []Message{
		{Role: "system", Content: system},
		{Role: "user", Content: user},
	}

	for _, client := range fc.clients {
		resp, err := client.Chat(ctx, messages)
		if err == nil {
			return resp, client.Name(), nil
		}
	}
	return "", "", fmt.Errorf("all providers failed")
}
