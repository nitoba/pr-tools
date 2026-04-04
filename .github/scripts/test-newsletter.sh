#!/usr/bin/env bash
set -euo pipefail

TAG="${1:-v0.0.0-test}"
RESEND_API_KEY="${RESEND_API_KEY:?Defina a variável RESEND_API_KEY}"
RESEND_AUDIENCE_ID="${RESEND_AUDIENCE_ID:?Defina a variável RESEND_AUDIENCE_ID}"
FROM="pr-tools <newsletter@nitodev.com.br>"
SUBJECT="pr-tools ${TAG} — o que há de novo"

CONTENT_FILE="/tmp/test-newsletter-content.md"
HTML_FILE="/tmp/test-newsletter.html"

# Conteúdo de teste
cat > "$CONTENT_FILE" <<MD
## O que há de novo em ${TAG}
Este é um envio de teste do fluxo de newsletter do pr-tools.

## Destaques
- Geração de descrições de PR com IA
- Suporte a OpenRouter, Groq, Gemini e Ollama
- Cards de teste automáticos no Azure DevOps

## Como atualizar
\`\`\`bash
create-pr-description --update
create-test-card --update
\`\`\`

## Notas
Teste local do workflow de newsletter. Veja a [documentação completa](https://pr-tools.dev/docs) para mais detalhes.
MD

echo "→ Gerando HTML..."
node "$(dirname "$0")/generate-newsletter-email.mjs" "$TAG" "$CONTENT_FILE" "$HTML_FILE"
echo "  ✓ HTML gerado em $HTML_FILE"

echo "→ Criando broadcast no Resend..."
PAYLOAD=$(jq -n \
  --arg name "release-${TAG}-$(date +%s)" \
  --arg audience_id "$RESEND_AUDIENCE_ID" \
  --arg from "$FROM" \
  --arg subject "$SUBJECT" \
  --rawfile html "$HTML_FILE" \
  '{name: $name, audience_id: $audience_id, from: $from, subject: $subject, html: $html}')

BROADCAST=$(curl -s -X POST https://api.resend.com/broadcasts \
  -H "Authorization: Bearer $RESEND_API_KEY" \
  -H "Content-Type: application/json" \
  -d "$PAYLOAD")

BROADCAST_ID=$(echo "$BROADCAST" | jq -r '.id')

if [ -z "$BROADCAST_ID" ] || [ "$BROADCAST_ID" = "null" ]; then
  echo "  ✗ Falha ao criar broadcast:"
  echo "$BROADCAST" | jq .
  exit 1
fi

echo "  ✓ Broadcast criado: $BROADCAST_ID"

echo "→ Enviando..."
SEND=$(curl -sf -X POST "https://api.resend.com/broadcasts/$BROADCAST_ID/send" \
  -H "Authorization: Bearer $RESEND_API_KEY")

echo "  ✓ Enviado: $(echo "$SEND" | jq -r '.id')"
echo ""
echo "Verifique sua caixa de entrada."
