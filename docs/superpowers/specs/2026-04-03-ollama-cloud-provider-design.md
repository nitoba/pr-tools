# Design: Ollama Cloud Provider

**Data:** 2026-04-03  
**Status:** Aprovado

## Contexto

O projeto `pr-tools` é um CLI Bash que gera descrições de PR e Test Cards usando IA. Atualmente suporta três providers: OpenRouter, Groq (ambos OpenAI-compatible) e Gemini (API própria). O objetivo é adicionar o **Ollama Cloud** como quarto provider, aproveitando que ele expõe um endpoint OpenAI-compatible (`/v1/chat/completions`).

## Decisões de Design

- **Endpoint:** `https://ollama.com/v1/chat/completions` (OpenAI-compatible, não o nativo `/api/chat`)
- **Modelo padrão:** `qwen3.5:cloud`
- **Streaming:** não incluído nesta iteração
- **Abordagem:** Opção A — tratar Ollama exatamente como Groq/OpenRouter, sem funções dedicadas novas

## Arquitetura

### Arquivos modificados

| Arquivo | Mudanças |
|---------|---------|
| `src/lib/common.sh` | `DEFAULT_OLLAMA_MODEL`, atualizar `DEFAULT_PROVIDERS`, case `ollama` em `test_provider_key()` |
| `src/lib/llm.sh` | Case `ollama` em `get_provider_config()` |
| `src/lib/test-card-llm.sh` | Case `ollama` em `call_with_fallback()` |
| `src/bin/create-pr-description` | Variáveis `OLLAMA_API_KEY`/`OLLAMA_MODEL` na config, Ollama no wizard, template `.env` |
| `src/bin/create-test-card` | Idem |
| `VERSION` | Bump patch: v2.9.2 → v2.9.3 |

### Nenhum arquivo novo criado.

## Configuração

**Novas variáveis de ambiente:**
```bash
OLLAMA_API_KEY="oa-..."        # chave gerada em ollama.com/settings
OLLAMA_MODEL="qwen3.5:cloud"   # modelo padrão
```

**Template `.env` gerado pelo wizard:**
```bash
PR_PROVIDERS="openrouter,groq,ollama"

# OLLAMA_API_KEY="oa-..."
# OLLAMA_MODEL="qwen3.5:cloud"
```

## Fluxo de dados

### Wizard `--init`
1. Lista providers disponíveis incluindo Ollama
2. Usuário digita `OLLAMA_API_KEY`
3. Wizard valida via `test_provider_key "ollama" "$key"` (POST mínimo a `/v1/chat/completions`, `max_tokens: 5`)
4. HTTP 200 ou 429 → válida; qualquer outro → pede nova tentativa
5. Opcionalmente configura modelo customizado (default: `qwen3.5:cloud`)

### Execução normal
```
PR_PROVIDERS="...,ollama"
↓
call_with_fallback()
  → get_provider_config("ollama")
      PROVIDER_URL = "https://ollama.com/v1/chat/completions"
      PROVIDER_KEY = $OLLAMA_API_KEY
      PROVIDER_MODEL = $OLLAMA_MODEL
  → call_llm_api()   ← mesma função usada por Groq e OpenRouter
```

## Tratamento de erros

Reutiliza 100% o tratamento existente em `call_llm_api()`:

| Código HTTP | Comportamento |
|-------------|---------------|
| 200 | Sucesso |
| 429 | Rate limit — `log_warn`, tenta próximo provider |
| 000 | Timeout (120s) — `log_warn`, tenta próximo provider |
| 4xx/5xx | Erro — `log_warn`, tenta próximo provider |
| Resposta vazia | `log_warn`, tenta próximo provider |

Sem retry logic especial (não há equivalente ao `reasoning_format` do Groq).

## O que não está no escopo

- Streaming para Ollama (pode ser adicionado numa iteração futura como os outros providers)
- Endpoint nativo `/api/chat` do Ollama
- Modelos locais (apenas cloud)
