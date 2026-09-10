const data = {
  focus: {
    number: "Versão 01",
    title: "Foco no conteúdo",
    description: "Uma única superfície para ler, ajustar e publicar a descrição gerada.",
  },
  steps: {
    number: "Versão 02",
    title: "Revisão por etapas",
    description: "Um pequeno stepper mantém a pessoa orientada sem competir com o texto.",
  },
  minimal: {
    number: "Versão 03",
    title: "Minimal terminal",
    description: "A opção mais enxuta: contexto mínimo, texto amplo e ações no rodapé.",
  },
};

const finalDescription = {
  title: "Atualiza fluxo de checkout",
  summary: "Atualiza o fluxo de checkout para validar o carrinho antes da confirmação do pedido.",
  changes: [
    "Validação do carrinho antes do pagamento.",
    "Tratamento da resposta de estoque indisponível.",
  ],
  type: "Bug fix",
};

const focusState = {
  phase: "boot",
  title: finalDescription.title,
  progress: 0,
  elapsed: 0,
  stream: "",
  logs: ["inicializando prt…"],
  modal: null,
  paused: false,
  reviewers: [
    { target: "dev", value: "dev@empresa.com" },
    { target: "sprint", value: "sprint@empresa.com" },
  ],
  published: [],
};

const prototype = document.querySelector("#prototype");
const variantNumber = document.querySelector("#variant-number");
const variantTitle = document.querySelector("#variant-title");
const variantDescription = document.querySelector("#variant-description");
const toast = document.querySelector("#toast");
let currentVariant = "focus";
let toastTimer;
let flowTimer;
let flowStartedAt = 0;

const chrome = (command = "prt desc") => `
  <div class="window-chrome">
    <span class="chrome-command">
      <span class="chrome-dots"><i></i><i></i><i></i></span>
      <strong>${command}</strong>
    </span>
    <span>feature/11763-exemplo</span>
  </div>`;

const contextPanel = (id = "context-panel") => `
  <div id="${id}" class="context-panel">
    <div class="context-item"><label>Work Item</label><span>#11763 · User Story</span></div>
    <div class="context-item"><label>Branch</label><span>feature/11763-exemplo</span></div>
    <div class="context-item"><label>Destino</label><span>dev · sprint</span></div>
  </div>`;

const description = () => `
  <div class="description-card">
    <div class="description-toolbar">
      <span class="toolbar-label"><span class="spark">✦</span> Descrição gerada</span>
      <span class="char-count">161 / 4000</span>
    </div>
    <div class="description-content">
      <h4>Atualiza fluxo de checkout</h4>
      <h5>Descrição</h5>
      <p>${finalDescription.summary}</p>
      <h5>Alterações</h5>
      <ul>${finalDescription.changes.map((item) => `<li>${item}</li>`).join("")}</ul>
      <h5>Tipo de mudança</h5>
      <ul><li>${finalDescription.type}</li></ul>
    </div>
  </div>`;

const focusText = () => `# ${focusState.title || finalDescription.title}

## Descrição

${finalDescription.summary}

## Alterações

- ${finalDescription.changes[0]}
- ${finalDescription.changes[1]}

## Tipo de mudança

- ${finalDescription.type}`;

const phaseInfo = {
  boot: { kicker: "Preparação", title: "Inicializando o fluxo", copy: "Preparando o contexto para gerar a descrição.", step: 0 },
  context: { kicker: "Contexto Git", title: "Coletando informações", copy: "Lendo branch, diff, commits e work items relacionados.", step: 0 },
  generating: { kicker: "Geração via IA", title: "Escrevendo a descrição", copy: "O resultado aparece progressivamente enquanto o provider responde.", step: 1 },
  review: { kicker: "Revisão", title: "Descrição pronta para revisão", copy: "Confira o conteúdo antes de publicar no Azure DevOps.", step: 2 },
  publishing: { kicker: "Publicação", title: "Criando Pull Request(s)", copy: "Enviando a descrição e os reviewers selecionados.", step: 3 },
  done: { kicker: "Concluído", title: "Pull Request publicado", copy: "O fluxo terminou e os links já estão disponíveis.", step: 4 },
};

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function focusDemoControls() {
  const playing = !focusState.paused && !["review", "done"].includes(focusState.phase);
  const canSkip = !["review", "publishing", "done"].includes(focusState.phase);
  return `
    <div class="focus-demo-bar">
      <div class="demo-label">
        <span class="eyebrow">Demonstração do fluxo</span>
        <strong>${playing ? "reprodução automática" : focusState.phase === "done" ? "fluxo concluído" : "revisão manual"}</strong>
      </div>
      <div class="demo-actions">
        ${canSkip ? `<button class="button subtle" data-action="skip-review">Pular para revisão</button>` : ""}
        <button class="button subtle" data-action="restart-flow">Reiniciar</button>
        ${playing || focusState.paused ? `<button class="button" data-action="toggle-flow">${focusState.paused ? "Continuar" : "Pausar"}</button>` : ""}
      </div>
    </div>`;
}

function flowStepper() {
  const steps = [
    ["context", "Contexto"],
    ["generating", "Geração"],
    ["review", "Revisão"],
    ["publishing", "Publicação"],
  ];
  const current = phaseInfo[focusState.phase].step;
  return `
    <div class="focus-stepper" aria-label="Progresso do fluxo">
      ${steps.map(([key, label], index) => {
        const stepNumber = index;
        const isDone = current > stepNumber || focusState.phase === "done";
        const isCurrent = current === stepNumber;
        return `${index > 0 ? `<span class="focus-step-line ${isDone ? "is-done" : ""}"></span>` : ""}
          <div class="focus-step ${isDone ? "is-done" : ""} ${isCurrent ? "is-current" : ""}">
            <span class="focus-step-dot">${isDone ? "✓" : index + 1}</span><span>${label}</span>
          </div>`;
      }).join("")}
    </div>`;
}

function stateHeader() {
  const info = phaseInfo[focusState.phase];
  const spinner = ["boot", "context", "generating", "publishing"].includes(focusState.phase)
    ? '<span class="state-spinner">◌</span>'
    : focusState.phase === "done"
      ? '<span class="state-check">✓</span>'
      : '<span class="state-dot-solid">●</span>';
  const percent = Math.round(focusState.progress * 100);
  return `
    <div class="focus-state-header">
      <div class="state-copy">
        <div class="surface-kicker">${spinner} ${info.kicker}</div>
        <h3>${info.title}</h3>
        <p>${info.copy}</p>
      </div>
      <div class="state-metrics">
        <span><b>${percent}%</b><small>progresso</small></span>
        <span><b>${Math.floor(focusState.stream.length / 4)}</b><small>tokens</small></span>
        <span><b>${Math.floor(focusState.elapsed / 1000)}s</b><small>decorrido</small></span>
      </div>
    </div>`;
}

function loadingPanel() {
  const rows = [
    ["Branch", focusState.phase === "boot" ? "aguardando…" : "feature/11763-exemplo"],
    ["Work Item", focusState.phase === "boot" ? "aguardando…" : "#11763"],
    ["Diff e commits", focusState.phase === "boot" ? "aguardando…" : "processando…"],
  ];
  return `
    <div class="loading-panel">
      <div class="loading-icon"><span>✦</span></div>
      <div class="loading-copy"><strong>${focusState.phase === "boot" ? "Preparando ambiente" : "Lendo contexto do repositório"}</strong><span>${focusState.phase === "boot" ? "A tarefa começa em um instante…" : "Cada item aparece assim que fica disponível."}</span></div>
      <div class="context-progress-list">
        ${rows.map(([label, value], index) => `<div class="context-progress-row ${value !== "aguardando…" ? "is-ready" : ""}"><span class="progress-check">${value === "aguardando…" ? "○" : "✓"}</span><label>${label}</label><span>${value}</span></div>`).join("")}
      </div>
    </div>`;
}

function streamPanel() {
  const visible = escapeHtml(focusState.stream || "aguardando primeiro token…");
  return `
    <div class="stream-panel">
      <div class="stream-toolbar"><span class="toolbar-label"><span class="spark">✦</span> Resposta do provider</span><span class="stream-live"><i></i> ao vivo</span></div>
      <pre>${visible}<span class="stream-cursor">▊</span></pre>
      <div class="stream-footer"><span>renderizando markdown</span><span>${Math.round(focusState.progress * 100)}%</span></div>
    </div>`;
}

function focusDescriptionCard() {
  return `
    <div class="description-card focus-description-card">
      <div class="description-toolbar">
        <span class="toolbar-label"><span class="spark">✦</span> Descrição gerada</span>
        <span class="char-count">161 / 4000 <b>✓</b></span>
      </div>
      <div class="description-content">
        <h4 class="editable-title" contenteditable="false">${escapeHtml(focusState.title || finalDescription.title)}</h4>
        <h5>Descrição</h5>
        <p>${finalDescription.summary}</p>
        <h5>Alterações</h5>
        <ul>${finalDescription.changes.map((item) => `<li>${item}</li>`).join("")}</ul>
        <h5>Tipo de mudança</h5>
        <ul><li>${finalDescription.type}</li></ul>
      </div>
    </div>`;
}

function reviewPanel() {
  const isPublishing = focusState.phase === "publishing";
  const isDone = focusState.phase === "done";
  return `
    <div class="surface-header focus-review-header">
      <div>
        <div class="surface-kicker">Revisão do Pull Request</div>
        <h3 class="editable-title" contenteditable="false">${escapeHtml(focusState.title || finalDescription.title)}</h3>
        <div class="surface-meta">
          <span class="chip"><b>dev</b></span>
          <span class="chip">Work Item <b>#11763</b></span>
          <span class="chip">161 caracteres <b>✓</b></span>
        </div>
      </div>
      <button class="button" data-action="toggle-context">Ver contexto <span>⌄</span></button>
    </div>
    ${contextPanel()}
    ${focusDescriptionCard()}
    ${isPublishing ? publishProgressPanel() : ""}
    ${isDone ? donePanel() : ""}
    <div class="surface-actions">
      <span class="action-hint">${isDone ? "Fluxo concluído" : "Enter publica · E edita · C copia"}</span>
      <div class="action-group">
        ${isDone ? `<button class="button" data-action="restart-flow">Reproduzir fluxo</button>` : ""}
        ${!isPublishing && !isDone ? `<button class="button subtle" data-action="edit-title">Editar título</button><button class="button" data-action="copy">Copiar body</button><button class="button primary" data-action="publish">Publicar PR <span class="key">↵</span></button>` : ""}
      </div>
    </div>`;
}

function publishProgressPanel() {
  const total = focusState.reviewers.length;
  const done = focusState.published.length;
  const current = focusState.reviewers[Math.min(done, total - 1)]?.target || "dev";
  return `
    <div class="publish-progress-panel">
      <div class="publish-progress-header"><span><span class="state-spinner">◌</span> Publicando PRs</span><b>${done} / ${total}</b></div>
      <div class="progress-track"><span style="width: ${Math.round(focusState.progress * 100)}%"></span></div>
      <div class="publish-progress-meta"><span>${done < total ? `criando PR para ${current}…` : "finalizando…"}</span><span>${Math.round(focusState.progress * 100)}%</span></div>
    </div>`;
}

function donePanel() {
  return `
    <div class="done-panel">
      <span class="done-icon">✓</span>
      <div><strong>Publicação concluída</strong><span>${focusState.published.length} Pull Request(s) criado(s) no Azure DevOps.</span></div>
      <div class="published-links">${focusState.published.map((item) => `<a href="#" data-action="show-toast" data-message="Abrindo ${item.target}">${item.target} ↗</a>`).join("")}</div>
    </div>`;
}

function focusContent() {
  if (focusState.phase === "review" || focusState.phase === "publishing" || focusState.phase === "done") return reviewPanel();
  if (focusState.phase === "generating") return streamPanel();
  return loadingPanel();
}

function activityPanel() {
  const lines = focusState.logs.slice(-3);
  return `
    <div class="focus-activity">
      <div class="activity-heading"><span>Atividade</span><span>${focusState.phase === "review" || focusState.phase === "done" ? "concluída" : "ao vivo"}</span></div>
      ${lines.map((line, index) => `<div class="activity-line ${index === lines.length - 1 ? "is-current" : ""}"><i></i><span>${escapeHtml(line)}</span></div>`).join("")}
    </div>`;
}

function focusView() {
  return `
    ${focusDemoControls()}
    <div class="mock-window focus-window">
      ${chrome(focusState.phase === "review" || focusState.phase === "done" ? "prt desc · revisão" : "prt desc · executando")}
      <div class="mock-body focus-body">
        ${stateHeader()}
        ${flowStepper()}
        <div class="focus-content">${focusContent()}</div>
        ${activityPanel()}
      </div>
    </div>`;
}

const stepsView = () => `
  <div class="mock-window">
    ${chrome("prt desc · revisão")}
    <div class="steps-body">
      <div class="stepper" aria-label="Etapas do fluxo">
        <div class="step is-done"><span class="step-number">✓</span><span>Contexto</span></div>
        <span class="step-line is-done"></span>
        <div class="step is-current"><span class="step-number">2</span><span>Revisão</span></div>
        <span class="step-line"></span>
        <div class="step"><span class="step-number">3</span><span>Publicação</span></div>
      </div>
      <div class="steps-grid">
        <aside class="summary-rail">
          <h4>Resumo</h4>
          <div class="summary-row"><label>Pull Request</label><span>feature/11763-exemplo</span></div>
          <div class="summary-row"><label>Work Item</label><span>#11763</span></div>
          <div class="summary-divider"></div>
          <div class="summary-row"><label>Destino</label><span>dev</span></div>
          <div class="summary-row"><label>Limite</label><span class="accent-text">161 / 4000</span></div>
          <button class="button" data-action="toggle-context">Mais contexto</button>
        </aside>
        <div class="steps-description">
          ${contextPanel("steps-context")}
          ${description()}
          <div class="steps-footer">
            <button class="button" data-action="copy">Copiar</button>
            <button class="button primary" data-action="publish">Continuar para publicar <span class="key">↵</span></button>
          </div>
        </div>
      </div>
    </div>
  </div>`;

const minimalView = () => `
  <div class="mock-window minimal-window">
    ${chrome("prt desc")}
    <div class="minimal-body">
      <div class="minimal-topline"><span>REVISÃO · 01/02</span><span>dev · #11763</span></div>
      <div class="minimal-title"><span class="prompt">›</span><h3 class="editable-title">${finalDescription.title}</h3></div>
      <p class="minimal-subtitle">descrição pronta para revisão</p>
      <div class="minimal-copy">${description()}</div>
      <div class="minimal-actions">
        <span class="action-hint">[e] editar · [c] copiar · [q] sair</span>
        <div class="action-group">
          <button class="button subtle" data-action="edit-title">Editar</button>
          <button class="button primary" data-action="publish">Publicar <span class="key">↵</span></button>
        </div>
      </div>
    </div>
  </div>`;

function renderVariant(name) {
  currentVariant = name;
  closeModal();
  const variant = data[name];
  variantNumber.textContent = variant.number;
  variantTitle.textContent = variant.title;
  variantDescription.textContent = variant.description;
  prototype.innerHTML = name === "focus" ? focusView() : name === "steps" ? stepsView() : minimalView();
  document.querySelectorAll(".variant-tab").forEach((tab) => {
    tab.classList.toggle("is-active", tab.dataset.variant === name);
    tab.setAttribute("aria-selected", tab.dataset.variant === name ? "true" : "false");
  });
}

function renderFocus() {
  if (currentVariant === "focus") prototype.innerHTML = focusView();
}

function showToast(message) {
  toast.textContent = message;
  toast.classList.add("is-visible");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove("is-visible"), 2200);
}

function pushLog(message) {
  if (focusState.logs.at(-1) !== message) focusState.logs.push(message);
}

function setPhase(phase) {
  if (focusState.phase === phase) return;
  focusState.phase = phase;
  const labels = {
    context: "coletando contexto Git…",
    generating: "gerando descrição…",
    review: "descrição pronta — revise, copie ou publique",
    publishing: "publicando…",
    done: "publicado",
  };
  if (labels[phase]) pushLog(labels[phase]);
}

function tickFlow() {
  if (focusState.paused || ["review", "done"].includes(focusState.phase)) return;
  focusState.elapsed = Math.max(0, performance.now() - flowStartedAt);
  const elapsed = focusState.elapsed;
  if (elapsed < 900) {
    setPhase("boot");
    focusState.progress = ease(elapsed / 900) * 0.08;
  } else if (elapsed < 2400) {
    setPhase("context");
    const ratio = (elapsed - 900) / 1500;
    focusState.progress = 0.08 + ease(ratio) * 0.22;
    if (ratio > 0.12) pushLog("branch feature/11763-exemplo encontrada");
    if (ratio > 0.42) pushLog("work item #11763 carregado");
    if (ratio > 0.72) pushLog("diff: 48 linhas · log com 3 commits");
  } else if (elapsed < 7600) {
    setPhase("generating");
    const ratio = (elapsed - 2400) / 5200;
    focusState.progress = 0.3 + ease(ratio) * 0.7;
    focusState.stream = focusText().slice(0, Math.max(1, Math.floor(focusText().length * ratio)));
    if (ratio > 0.16) pushLog("provider codex respondeu");
    if (ratio > 0.58) pushLog("renderizando markdown…");
    if (ratio > 0.88) pushLog("validando limite de 4000 caracteres");
  } else {
    setPhase("review");
    focusState.progress = 1;
    focusState.stream = focusText();
    pushLog("descrição pronta — revise, copie ou publique");
    stopFlowTimer();
  }
  renderFocus();
}

function ease(value) {
  const clamped = Math.max(0, Math.min(1, value));
  return clamped * clamped * (3 - 2 * clamped);
}

function stopFlowTimer() {
  if (flowTimer) {
    clearInterval(flowTimer);
    flowTimer = null;
  }
}

function startFlow() {
  stopFlowTimer();
  flowStartedAt = performance.now();
  focusState.paused = false;
  flowTimer = setInterval(tickFlow, 80);
  tickFlow();
}

function resetFlow() {
  closeModal();
  focusState.phase = "boot";
  focusState.title = finalDescription.title;
  focusState.progress = 0;
  focusState.elapsed = 0;
  focusState.stream = "";
  focusState.logs = ["inicializando prt…"];
  focusState.paused = false;
  focusState.reviewers = [
    { target: "dev", value: "dev@empresa.com" },
    { target: "sprint", value: "sprint@empresa.com" },
  ];
  focusState.published = [];
  renderFocus();
  startFlow();
}

function skipToReview() {
  stopFlowTimer();
  focusState.phase = "review";
  focusState.progress = 1;
  focusState.elapsed = 7600;
  focusState.stream = focusText();
  focusState.logs = ["contexto pronto", "provider codex respondeu", "descrição pronta — revise, copie ou publique"];
  renderFocus();
}

function toggleFlow() {
  if (["review", "done"].includes(focusState.phase)) return;
  if (focusState.paused) {
    focusState.paused = false;
    flowStartedAt = performance.now() - focusState.elapsed;
    flowTimer = setInterval(tickFlow, 80);
  } else {
    focusState.elapsed = performance.now() - flowStartedAt;
    focusState.paused = true;
    stopFlowTimer();
  }
  renderFocus();
}

function focusModalMarkup() {
  if (focusState.modal === "create") {
    return `
      <div class="modal-backdrop" data-action="close-modal">
        <div class="modal focus-modal" role="dialog" aria-modal="true" aria-labelledby="focus-modal-title">
          <div class="modal-header"><div><p class="modal-kicker">Próximo passo</p><h3 id="focus-modal-title">Criar PR(s) no Azure DevOps?</h3><p>A descrição está pronta. Antes de publicar, você poderá revisar os reviewers de cada destino.</p></div><button class="modal-close" data-action="close-modal" aria-label="Fechar">×</button></div>
          <div class="target-selection"><span class="selection-label">Targets selecionados</span><div>${["dev", "sprint"].map((target) => `<span class="target-pill"><i></i>${target}</span>`).join("")}</div></div>
          <div class="modal-actions"><button class="button" data-action="close-modal">Não, voltar</button><button class="button primary" data-action="open-reviewers">Sim, continuar <span class="key">↵</span></button></div>
          <div class="modal-hint">Enter confirma · Esc volta</div>
        </div>
      </div>`;
  }
  if (focusState.modal === "reviewers") {
    return `
      <div class="modal-backdrop" data-action="close-modal">
        <div class="modal reviewers-modal" role="dialog" aria-modal="true" aria-labelledby="reviewers-modal-title">
          <div class="modal-header"><div><p class="modal-kicker">Configurar publicação</p><h3 id="reviewers-modal-title">Revisar reviewers</h3><p>Os valores abaixo vêm da configuração padrão e podem ser alterados antes do envio.</p></div><button class="modal-close" data-action="close-modal" aria-label="Fechar">×</button></div>
          <div class="reviewer-list">
            ${focusState.reviewers.map((reviewer, index) => `<label class="reviewer-row ${index === 0 ? "is-focus" : ""}"><span class="reviewer-row-heading"><b>${reviewer.target}</b><small>${reviewer.target === "sprint" ? "reviewer do sprint" : "reviewer de desenvolvimento"}</small></span><input class="reviewer-input" data-reviewer-index="${index}" type="email" value="${escapeHtml(reviewer.value)}" placeholder="email@empresa.com" /><span class="reviewer-default">padrão configurado</span></label>`).join("")}
          </div>
          <div class="modal-actions"><button class="button" data-action="close-modal">Voltar</button><button class="button primary" data-action="confirm-reviewers">Continuar <span class="key">↵</span></button></div>
          <div class="modal-hint">Vazio mantém o padrão · Tab troca o campo · Enter avança</div>
        </div>
      </div>`;
  }
  return `
    <div class="modal-backdrop" data-action="close-modal">
      <div class="modal confirm-reviewers-modal" role="dialog" aria-modal="true" aria-labelledby="confirm-reviewers-title">
        <div class="modal-header"><div><p class="modal-kicker">Última confirmação</p><h3 id="confirm-reviewers-title">Criar PR(s) com estes reviewers?</h3><p>Confira os destinatários antes de iniciar a publicação.</p></div><button class="modal-close" data-action="close-modal" aria-label="Fechar">×</button></div>
        <div class="reviewer-summary-list">${focusState.reviewers.map((reviewer) => `<div><span>${reviewer.target}</span><strong>${escapeHtml(reviewer.value || "nenhum")}</strong></div>`).join("")}</div>
        <div class="modal-actions"><button class="button" data-action="open-reviewers">Editar reviewers</button><button class="button primary" data-action="start-publish">Publicar PR <span class="key">↵</span></button></div>
        <div class="modal-hint">Esc volta para a revisão dos reviewers</div>
      </div>
    </div>`;
}

function renderModal() {
  document.querySelector("#modal-root").innerHTML = currentVariant === "focus" && focusState.modal ? focusModalMarkup() : "";
  if (focusState.modal === "reviewers") document.querySelector(".reviewer-input")?.focus();
}

function closeModal() {
  focusState.modal = null;
  const root = document.querySelector("#modal-root");
  if (root) root.innerHTML = "";
}

function openFocusPublish() {
  if (focusState.phase !== "review") return;
  focusState.modal = "create";
  renderModal();
}

function openReviewers() {
  focusState.modal = "reviewers";
  renderModal();
}

function confirmReviewers() {
  focusState.reviewers = focusState.reviewers.map((reviewer) => ({
    ...reviewer,
    value: reviewer.value.trim() || `${reviewer.target}@empresa.com`,
  }));
  focusState.modal = "confirm";
  renderModal();
}

function startPublishing() {
  focusState.modal = null;
  focusState.phase = "publishing";
  focusState.progress = 0.05;
  focusState.elapsed = 0;
  focusState.published = [];
  focusState.logs = ["reviewers confirmados", "publicando…"];
  renderModal();
  renderFocus();
  flowStartedAt = performance.now();
  flowTimer = setInterval(tickPublishing, 120);
  tickPublishing();
}

function tickPublishing() {
  const elapsed = performance.now() - flowStartedAt;
  const duration = 3600;
  const ratio = Math.min(1, elapsed / duration);
  focusState.elapsed = elapsed;
  focusState.progress = 0.05 + ratio * 0.95;
  if (ratio > 0.2 && focusState.published.length === 0) {
    focusState.published.push({ target: "dev", url: "https://dev.azure.com/empresa/proj/_git/app/pullrequest/481" });
    pushLog("PR dev criado: #481");
  }
  if (ratio > 0.62 && focusState.published.length === 1) {
    focusState.published.push({ target: "sprint", url: "https://dev.azure.com/empresa/proj/_git/app/pullrequest/482" });
    pushLog("PR sprint criado: #482");
  }
  if (ratio >= 1) {
    focusState.phase = "done";
    focusState.progress = 1;
    pushLog("publicado — 2 PR(s) criado(s)");
    stopFlowTimer();
  }
  renderFocus();
}

function openSimplePublishModal() {
  document.querySelector("#modal-root").innerHTML = `
    <div class="modal-backdrop" data-action="close-modal">
      <div class="modal" role="dialog" aria-modal="true" aria-labelledby="modal-title">
        <div class="modal-header"><div><h3 id="modal-title">Publicar Pull Request?</h3><p>Uma última confirmação antes de enviar para o Azure DevOps.</p></div><button class="modal-close" data-action="close-modal" aria-label="Fechar">×</button></div>
        <div class="modal-list"><div><span>Título</span><strong>${finalDescription.title}</strong></div><div><span>Destino</span><strong>dev</strong></div><div><span>Reviewer</span><strong>dev@empresa.com</strong></div></div>
        <div class="modal-actions"><button class="button" data-action="close-modal">Voltar</button><button class="button primary" data-action="confirm-simple-publish">Publicar PR</button></div>
      </div>
    </div>`;
}

function handleAction(action, target) {
  if (action === "toggle-context") {
    const panel = target.closest(".mock-window")?.querySelector(".context-panel");
    if (panel) panel.classList.toggle("is-open");
    target.innerHTML = panel?.classList.contains("is-open") ? "Ocultar contexto <span>⌃</span>" : "Ver contexto <span>⌄</span>";
  }
  if (action === "copy") showToast("Body copiado para o clipboard");
  if (action === "publish") currentVariant === "focus" ? openFocusPublish() : openSimplePublishModal();
  if (action === "open-reviewers") openReviewers();
  if (action === "confirm-reviewers") confirmReviewers();
  if (action === "start-publish") startPublishing();
  if (action === "confirm-simple-publish") {
    closeModal();
    showToast("PR enviado · fluxo concluído");
  }
  if (action === "close-modal") closeModal();
  if (action === "restart-flow") resetFlow();
  if (action === "skip-review") skipToReview();
  if (action === "toggle-flow") toggleFlow();
  if (action === "show-toast") showToast(target.dataset.message || "Abrindo Pull Request");
  if (action === "edit-title") {
    const window = target.closest(".mock-window");
    const titles = [...(window?.querySelectorAll(".editable-title") || [])];
    const editing = titles[0]?.getAttribute("contenteditable") === "true";
    if (!titles.length) return;
    if (editing) {
      const value = titles[0].textContent.trim() || finalDescription.title;
      focusState.title = value;
      titles.forEach((title) => title.setAttribute("contenteditable", "false"));
      renderFocus();
      showToast("Título atualizado");
    } else {
      titles.forEach((title) => {
        title.setAttribute("contenteditable", "true");
        title.classList.add("is-editing");
      });
      titles[0].focus();
      target.textContent = "Salvar título";
    }
  }
}

document.addEventListener("click", (event) => {
  const tab = event.target.closest("[data-variant]");
  if (tab) renderVariant(tab.dataset.variant);
  const action = event.target.closest("[data-action]");
  if (action) handleAction(action.dataset.action, action);
});

document.addEventListener("input", (event) => {
  const input = event.target.closest("[data-reviewer-index]");
  if (input) focusState.reviewers[Number(input.dataset.reviewerIndex)].value = input.value;
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    if (focusState.modal) closeModal();
    else document.querySelector("#modal-root").innerHTML = "";
  }
  if (event.key === "Enter" && event.target.matches("[contenteditable='true']")) {
    event.preventDefault();
    document.querySelector("#prototype [data-action='edit-title']")?.click();
    return;
  }
  if (event.key === "Enter" && focusState.modal === "reviewers" && event.target.matches(".reviewer-input")) {
    event.preventDefault();
    confirmReviewers();
    return;
  }
  if (event.key.toLowerCase() === "c" && !event.target.matches("input, [contenteditable='true']")) showToast("Body copiado para o clipboard");
  if (event.key.toLowerCase() === "e" && currentVariant === "focus" && !event.target.matches("input, [contenteditable='true']")) document.querySelector("#prototype [data-action='edit-title']")?.click();
  if (event.key === " " && currentVariant === "focus" && !event.target.matches("input, [contenteditable='true']")) {
    event.preventDefault();
    toggleFlow();
  }
  if (event.key === "Enter" && !focusState.modal && !event.target.matches("input, [contenteditable='true']") && currentVariant === "focus" && focusState.phase === "review") openFocusPublish();
});

renderVariant("focus");
startFlow();
