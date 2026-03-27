#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

# shellcheck source=/dev/null
source "$REPO_ROOT/bin/create-test-card"

markdown=$(cat <<'EOF'
## Objetivo
Validar exclusao de imagem unica.

## Checklist de testes
- [ ] Adicionar 1 imagem e remover
- [ ] Validar erro quando obrigatoria sem imagens

## Resultado esperado
Exclusao deve funcionar.
EOF
)

steps=$(markdown_to_azure_steps "$markdown")

[[ "$steps" == *'<steps id="0"'* ]]
[[ "$steps" == *'Adicionar 1 imagem e remover'* ]]
[[ "$steps" == *'Validar erro quando obrigatoria sem imagens'* ]]
[[ "$steps" != *'[ ] Adicionar 1 imagem e remover'* ]]
[[ "$steps" != *'[ ] Validar erro quando obrigatoria sem imagens'* ]]
[[ "$steps" == *'<step id="2" type="ActionStep">'* ]]
[[ "$steps" == *'<step id="3" type="ActionStep">'* ]]

echo "ok"
