# Groq Reasoning Fallback - Design Spec

**Data:** 2026-03-26
**Status:** Draft

## Objetivo

Evitar falha na geracao da descricao de PR quando a API da Groq rejeitar o parametro `reasoning_format` para modelos que nao suportam esse recurso, fazendo uma nova tentativa automatica sem esse campo.

## Problema

Hoje o fluxo de chamadas OpenAI-compatible em `bin/create-pr-description` sempre envia `reasoning_format: "hidden"` para o provider `groq`.

Isso funciona para alguns modelos, mas falha para outros, retornando erro HTTP 400 com corpo semelhante a:

```json
{
  "request_id": "req_01kmnn2paqfd68a20kp3x9d6wy",
  "created_at": "2026-03-26T18:04:04.823Z",
  "error": {
    "message": "reasoning_format is not supported with this model",
    "type": "invalid_request_error",
    "param": "reasoning_format",
    "code": ""
  }
}
```

Na implementacao atual, qualquer HTTP diferente de `200` cai no tratamento generico de falha do provider. Como resultado, o script nao tenta uma segunda chamada sem `reasoning_format`, mesmo quando esse ajuste seria suficiente para o modelo responder corretamente.

## Escopo

Esta mudanca e restrita ao provider `groq`.

Inclui:

- detectar o erro especifico de incompatibilidade com `reasoning_format`
- refazer a requisicao uma unica vez sem `reasoning_format`
- manter o restante do payload identico entre a primeira chamada e o retry

Nao inclui:

- generalizar o comportamento para outros providers
- introduzir mapeamento manual de modelos com ou sem suporte a reasoning
- mudar flags, config ou UX externa da CLI

## Solucao Proposta

### Resumo

O fluxo da Groq continuara tentando primeiro a chamada com `reasoning_format: "hidden"`, preservando o comportamento atual para modelos que suportam esse parametro.

Se a API responder com erro que indique explicitamente que `reasoning_format` nao e suportado pelo modelo, o script fara uma segunda tentativa imediata sem esse campo. Se a segunda chamada funcionar, o resultado segue normalmente no restante do pipeline.

Se o erro for diferente, o comportamento continua igual ao atual: o provider falha e o sistema segue para o fallback configurado de providers.

## Abordagens Consideradas

### 1. Retry condicional com deteccao por erro especifico

Essa e a abordagem escolhida.

Vantagens:

- preserva o comportamento otimizado atual para modelos Groq que suportam `reasoning_format`
- minimiza o escopo da mudanca
- evita manter listas de compatibilidade por modelo

Desvantagens:

- depende da consistencia da mensagem/campos de erro retornados pela Groq

### 2. Remover `reasoning_format` de todas as chamadas Groq

Vantagens:

- simplifica o fluxo
- elimina a necessidade de retry

Desvantagens:

- perde o beneficio atual para modelos que suportam reasoning e cujo output fica melhor com `reasoning_format: "hidden"`
- altera comportamento valido que hoje funciona

### 3. Manter allowlist ou blocklist de modelos Groq

Vantagens:

- evita a primeira chamada falha em modelos ja conhecidos

Desvantagens:

- introduz manutencao manual
- envelhece mal quando a Groq muda suporte ou catalogo de modelos
- aumenta acoplamento entre codigo e nomes de modelos

## Design Tecnico

### Ponto de mudanca

O comportamento atual esta concentrado em `bin/create-pr-description`, principalmente dentro de `call_llm_api`, onde:

- o payload base e montado com `jq`
- `reasoning_format: "hidden"` e adicionado para `groq`
- erros HTTP sao tratados de forma generica

### Ajuste estrutural

Separar a montagem do payload OpenAI-compatible da execucao HTTP, para permitir reaproveitar a mesma logica em dois modos:

- com `reasoning_format`
- sem `reasoning_format`

O objetivo nao e reescrever a integracao, mas reduzir duplicacao suficiente para que o retry altere apenas um aspecto do payload.

Para a implementacao ficar segura no arquivo atual, a spec recomenda uma divisao minima de responsabilidades:

- um helper para montar o payload OpenAI-compatible, com opcao de incluir ou nao `reasoning_format`
- um helper para executar a chamada HTTP e retornar `http_code` + `body`
- um helper para classificar se o erro da Groq permite o retry especial

Essa divisao evita que o primeiro erro HTTP seja descartado antes da classificacao e limita o escopo da mudanca dentro de `call_llm_api`.

### Heuristica de deteccao

O retry deve acontecer apenas quando o erro da Groq indicar de forma clara que o parametro `reasoning_format` nao e suportado.

Sinal esperado, com base no erro observado:

- `error.param == "reasoning_format"`
- `error.message`, apos normalizacao para lowercase, contem `reasoning_format` e `not supported`
- `error.type == "invalid_request_error"` pode ser usado como reforco, nao como gatilho isolado

Regra booleana exata:

- considerar retry especial apenas quando `provider_name == "groq"` **e** `http_code == "400"`
- fazer retry somente se `error.param == "reasoning_format"` **e** `error.message`, apos lowercase, contiver `reasoning_format` **e** `not supported`
- `error.type == "invalid_request_error"` pode ser validado como sinal adicional, mas nao substitui os dois criterios acima
- se qualquer um desses campos estiver ausente, vazio, malformado ou impossivel de parsear, **nao** fazer retry
- nao fazer retry para outros `400`, mesmo que tambem sejam `invalid_request_error`

### Fluxo proposto

1. Montar o payload Groq com `reasoning_format: "hidden"`
2. Executar a chamada HTTP
3. Se HTTP `200`, seguir normalmente
4. Se HTTP nao for `200`, inspecionar o corpo
5. Somente se `provider == groq` e `HTTP == 400`, classificar se o erro e o caso especifico de `reasoning_format` nao suportado
6. Se o erro for o caso especifico:
   - logar aviso diagnostico curto
   - remontar o payload sem `reasoning_format`
   - repetir a chamada uma unica vez
7. Se a segunda tentativa retornar `200`, seguir normalmente
8. Se a segunda tentativa falhar:
   - logar a falha final da segunda tentativa como erro relevante do provider
   - nao repetir novamente
   - seguir com o fallback existente para o proximo provider
9. Se o primeiro erro nao for o caso esperado, manter a falha do provider e o fallback existente sem retry especial

### Requisitos de comportamento

- o retry vale somente para `groq`
- o retry acontece no maximo uma vez por chamada
- apenas `reasoning_format` pode ser removido no retry
- `model`, `messages`, `temperature` e demais campos do JSON devem permanecer identicos
- headers, timeout, URL, metodo HTTP e demais argumentos de `curl` tambem devem permanecer identicos entre as duas tentativas
- timeout, rate limit, auth error e outros erros nao relacionados continuam sem retry especial

## Observabilidade

Adicionar um aviso enxuto quando o retry especial ocorrer, para facilitar suporte e diagnostico. Exemplo de intencao:

```text
[AVISO] Groq rejeitou reasoning_format para este modelo; tentando novamente sem esse parametro
```

Esse log deve aparecer apenas no caso de fallback especifico, sem expor payloads completos nem poluir a saida normal.

Em caso de falha tambem na segunda tentativa, a saida deve priorizar o resultado final do retry como a falha relevante do provider. O objetivo e evitar diagnosticos duplicados ou confusos para o usuario.

Comportamento esperado de logs quando o retry especial dispara:

- nao emitir primeiro o aviso HTTP generico da primeira resposta `400`
- emitir apenas o aviso especifico informando que a Groq rejeitou `reasoning_format` e que a chamada sera refeita sem o parametro
- se a segunda tentativa falhar, emitir o log normal de erro do provider com base nessa segunda resposta
- no caminho com retry, a primeira tentativa deve ficar silenciosa exceto pelo aviso especifico de retry

## Impacto em compatibilidade

- nenhuma mudanca de interface CLI
- nenhuma mudanca de configuracao em `.env`
- nenhum impacto esperado para OpenRouter ou Gemini
- para Groq, modelos compativeis continuam recebendo `reasoning_format: "hidden"`
- para modelos incompativeis, a execucao deixa de falhar desnecessariamente

## Riscos

### Variacao da mensagem de erro

A Groq pode variar o texto do erro no futuro. Para reduzir fragilidade:

- priorizar `error.param == "reasoning_format"`
- usar a mensagem como confirmacao adicional
- manter a regra conservadora para evitar retries indevidos

### Acoplamento excessivo dentro de `call_llm_api`

Se o retry for implementado diretamente no fluxo atual sem pequenas extracoes, o codigo pode ficar mais dificil de manter. A spec recomenda uma separacao minima entre:

- montagem de payload
- execucao HTTP
- classificacao do erro

## Verificacao

### Verificacoes minimas

- `bash -n bin/create-pr-description`
- `bin/create-pr-description --help`
- `bin/create-pr-description --dry-run`

### Verificacoes de comportamento

- confirmar que uma chamada Groq bem-sucedida com `reasoning_format` continua funcionando
- confirmar que o erro especifico abaixo dispara retry sem `reasoning_format`
- confirmar que, no retry bem-sucedido, a descricao do PR e produzida normalmente
- confirmar que um erro Groq diferente nao dispara o retry especial
- confirmar que corpo de erro nao-JSON, JSON malformado ou sem `error.param`/`error.message` nao dispara retry

### Estrategia de verificacao no repositório

Como este repositório nao possui harness automatizado formal para APIs externas, a verificacao deve ser desenhada de forma segura e local. A spec recomenda que a implementacao permita testar a classificacao e o comportamento do retry sem depender de uma chamada real para a Groq.

Caminhos aceitaveis:

- extrair a classificacao do erro para uma funcao testavel com fixtures JSON inline
- simular respostas HTTP com corpo e status controlados em uma verificacao shell local
- complementar com validacao manual real apenas se houver credenciais e modelo adequados

O importante e que exista pelo menos uma forma repetivel de validar:

- erro-alvo gera retry
- erro diferente nao gera retry
- parse invalido nao gera retry

Mecanismo concreto recomendado para este repositório:

- extrair a classificacao do erro para um helper que receba `http_code`, `provider_name` e `body`
- validar esse helper com um script shell local temporario ou funcao de verificacao alimentada por tres fixtures inline: erro-alvo, erro diferente e corpo invalido
- usar essa verificacao local como prova principal do predicado de retry, deixando a chamada real para Groq como validacao complementar quando disponivel

Erro-alvo para o fallback:

```json
{
  "error": {
    "message": "reasoning_format is not supported with this model",
    "type": "invalid_request_error",
    "param": "reasoning_format"
  }
}
```

## Criterios de Aceitacao

- Dado o provider `groq` e um modelo que rejeita `reasoning_format`, o script detecta esse erro especifico e refaz a chamada sem o campo
- O retry reutiliza o mesmo payload da primeira tentativa, removendo apenas `reasoning_format`
- Se a segunda chamada funcionar, a descricao do PR e gerada normalmente
- Se o erro nao estiver ligado a `reasoning_format`, o comportamento de falha atual e preservado
- Nenhum outro provider passa a usar essa logica especial
