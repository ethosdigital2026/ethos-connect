const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// Telas internas do app (abrem como um serviço, sem internet)
const URL_CALCULADORA = 'ethos://calculadora';
const SERVICO_CALCULADORA = {
  id: 'calculadora', nome: 'Calculadora', url: URL_CALCULADORA, tipo: 'calculadora', perfil: 'calculadora', hibernar: true,
};
const ehInterno = (s) => s?.tipo === 'calculadora';

const CONFIG_PADRAO = {
  versao: 1,
  calculadoraAdicionada: true,
  minutosHibernacao: 15,
  barraOculta: false,
  notificacoes: { ativas: true, discretas: false },
  bloqueioMinutos: 0,
  servicos: [
    { id: 'whatsapp', nome: 'WhatsApp', url: 'https://web.whatsapp.com/', tipo: 'whatsapp', perfil: 'whatsapp', hibernar: false },
    { id: 'whatsapp-business', nome: 'WhatsApp Business', url: 'https://web.whatsapp.com/', tipo: 'whatsapp-business', perfil: 'whatsapp-business', hibernar: false },
    { id: 'gmail', nome: 'Gmail', url: 'https://mail.google.com/', tipo: 'site', perfil: 'google', hibernar: true },
    { id: 'agenda', nome: 'Google Agenda', url: 'https://calendar.google.com/', tipo: 'site', perfil: 'google', hibernar: true },
    SERVICO_CALCULADORA,
  ],
};

// Sugestões rápidas no diálogo "Adicionar serviço"
const ATALHOS = [
  { nome: 'WhatsApp (outra conta)', url: 'https://web.whatsapp.com/', tipo: 'whatsapp', perfil: 'proprio', hibernar: false },
  { nome: 'Gmail', url: 'https://mail.google.com/', perfil: 'google', hibernar: true },
  { nome: 'Google Agenda', url: 'https://calendar.google.com/', perfil: 'google', hibernar: true },
  { nome: 'Google Drive', url: 'https://drive.google.com/', perfil: 'google', hibernar: true },
  { nome: 'Instagram', url: 'https://www.instagram.com/direct/inbox/', perfil: 'proprio', hibernar: true },
  { nome: 'Calculadora', url: URL_CALCULADORA, tipo: 'calculadora', perfil: 'calculadora', hibernar: true },
];

const PERFIS_COMPARTILHADOS = ['google', 'geral'];

let config;
let ativo = null;
let bloqueado = false;
let pinDefinido = false;
let servicoInicial = null;
let ultimoTotal = -1;
const estado = {}; // id -> { carregado, ultimoUso, naoLidas }

const $ = (s) => document.querySelector(s);
const est = (id) => (estado[id] ??= { carregado: false, ultimoUso: 0, naoLidas: 0 });
const servico = (id) => config.servicos.find((s) => s.id === id);

async function salvar() {
  await invoke('salvar_config', { conteudo: JSON.stringify(config, null, 2) });
}

// ---------- Ícones ----------
function svgWhatsApp(cor, business) {
  const miolo = business
    ? `<text x="15" y="18.6" text-anchor="middle" font-family="Montserrat,Segoe UI,sans-serif" font-weight="700" font-size="9.5" fill="${cor}">B</text>`
    : `<path d="M12.2 10.6c.3-.7.8-.7 1.1-.7l.7 1.6c.1.3 0 .5-.2.7l-.4.4c.5 1 1.3 1.8 2.3 2.3l.4-.4c.2-.2.5-.3.7-.2l1.6.7c0 .4 0 .8-.7 1.1-.8.4-1.9.2-3.2-.7a8 8 0 0 1-2.6-2.6c-.8-1.3-1-2.4-.6-3.2z" fill="${cor}"/>`;
  return `<svg viewBox="0 0 30 30"><rect width="30" height="30" rx="8" fill="${cor}"/>
    <path d="M15 6.5a8.5 8.5 0 0 0-7.3 12.9L6.6 23.4l4.1-1.1A8.5 8.5 0 1 0 15 6.5z" fill="#fff"/>${miolo}</svg>`;
}

// O serviço de favicons do Google devolve o mesmo "G" para todos os produtos dele
const SVG_GMAIL = `<svg viewBox="0 0 30 30"><rect width="30" height="30" rx="8" fill="#fff"/>
  <path d="M7 10.5v10h3.2v-7.4L15 16.8l4.8-3.7v7.4H23v-10l-1.6-1.2L15 13.6 8.6 9.3z" fill="#EA4335"/>
  <path d="M7 20.5h3.2v-7.4L7 10.5z" fill="#4285F4"/><path d="M19.8 20.5H23v-10l-3.2 2.6z" fill="#34A853"/>
  <path d="M19.8 13.1 23 10.5 21.4 9.3l-1.6 1.2z" fill="#FBBC04"/></svg>`;
const SVG_AGENDA = `<svg viewBox="0 0 30 30"><rect width="30" height="30" rx="8" fill="#fff"/>
  <rect x="6.5" y="6.5" width="17" height="17" rx="2.5" fill="none" stroke="#4285F4" stroke-width="2"/>
  <path d="M6.5 11.5h17" stroke="#4285F4" stroke-width="2"/>
  <text x="15" y="20.8" text-anchor="middle" font-family="Montserrat,Segoe UI,sans-serif" font-weight="700" font-size="8.5" fill="#1A73E8">31</text></svg>`;
const SVG_SINO_DESLIGADO = '<svg viewBox="0 0 24 24"><path d="M18 8a6 6 0 0 0-9.3-5M6.3 6.3A6 6 0 0 0 6 8c0 7-3 9-3 9h14"/><path d="M13.7 21a2 2 0 0 1-3.4 0"/><path d="M3 3l18 18"/></svg>';
const SVG_OLHO_FECHADO = '<svg viewBox="0 0 24 24"><path d="M17.9 17.9A10 10 0 0 1 12 20c-7 0-11-8-11-8a18 18 0 0 1 5.1-5.9M9.9 4.2A9 9 0 0 1 12 4c7 0 11 8 11 8a18 18 0 0 1-2.2 3.2"/><path d="M1 1l22 22"/></svg>';
const SVG_CALCULADORA = `<svg viewBox="0 0 30 30"><rect width="30" height="30" rx="8" fill="#F5B83D"/>
  <rect x="8.5" y="6" width="13" height="18" rx="2.2" fill="#0E2646"/>
  <rect x="10.5" y="8" width="9" height="3.6" rx="1" fill="#A9B8CF"/>
  <g fill="#fff"><circle cx="11.7" cy="15" r="1.1"/><circle cx="15" cy="15" r="1.1"/><circle cx="18.3" cy="15" r="1.1"/>
  <circle cx="11.7" cy="18.4" r="1.1"/><circle cx="15" cy="18.4" r="1.1"/><circle cx="11.7" cy="21.6" r="1.1"/><circle cx="15" cy="21.6" r="1.1"/></g>
  <rect x="17.2" y="17.4" width="2.2" height="5.3" rx="1.1" fill="#F5B83D"/></svg>`;
const ICONES_POR_SITE = { 'mail.google.com': SVG_GMAIL, 'calendar.google.com': SVG_AGENDA };

function criarIcone(s) {
  const el = document.createElement('span');
  el.className = 'icone';
  if (s.tipo === 'whatsapp') { el.innerHTML = svgWhatsApp('#25D366', false); return el; }
  if (s.tipo === 'whatsapp-business') { el.innerHTML = svgWhatsApp('#128C7E', true); return el; }
  if (s.tipo === 'calculadora') { el.innerHTML = SVG_CALCULADORA; return el; }

  const letra = () => {
    el.textContent = (s.nome || '?').trim().charAt(0).toUpperCase();
    el.classList.add('icone-letra');
  };
  let host;
  try { host = new URL(s.url).hostname; } catch { letra(); return el; }
  if (ICONES_POR_SITE[host]) { el.innerHTML = ICONES_POR_SITE[host]; return el; }
  const img = document.createElement('img');
  img.alt = '';
  img.src = `https://www.google.com/s2/favicons?domain=${encodeURIComponent(host)}&sz=64`;
  img.addEventListener('error', letra, { once: true });
  el.append(img);
  return el;
}

// ---------- Barra lateral ----------
function render() {
  const lista = $('#lista');
  lista.replaceChildren();
  for (const s of config.servicos) {
    const e = est(s.id);
    const btn = document.createElement('button');
    btn.className = 'servico';
    btn.classList.toggle('ativo', s.id === ativo);
    btn.classList.toggle('hibernado', !e.carregado);
    btn.title = s.nome;
    btn.dataset.id = s.id;
    btn.append(criarIcone(s));

    const selo = document.createElement('span');
    selo.className = 'selo';
    btn.append(selo);

    // Marquinha no canto: notificações desligadas ou sem conteúdo
    if (s.notificacoes === 'desligada' || s.notificacoes === 'discreta') {
      const marca = document.createElement('span');
      marca.className = 'marca-notif';
      marca.innerHTML = s.notificacoes === 'desligada' ? SVG_SINO_DESLIGADO : SVG_OLHO_FECHADO;
      btn.append(marca);
      btn.title = `${s.nome} · notificações ${s.notificacoes === 'desligada' ? 'desligadas' : 'sem conteúdo'}`;
    }

    btn.addEventListener('click', () => selecionar(s.id));
    btn.addEventListener('contextmenu', (ev) => {
      ev.preventDefault();
      invoke('menu_servico', { id: s.id, zoom: s.zoom || 100, notificacoes: s.notificacoes || 'normal', interno: ehInterno(s) });
    });
    lista.append(btn);
    atualizarSelo(s.id);
  }
}

function atualizarSelo(id) {
  const selo = document.querySelector(`.servico[data-id="${CSS.escape(id)}"] .selo`);
  if (!selo) return;
  const n = est(id).naoLidas;
  selo.hidden = n === 0;
  selo.textContent = n > 99 ? '99+' : String(n);
}

function seloBarraDeTarefas(total) {
  return new Promise((resolve) => {
    const c = document.createElement('canvas');
    c.width = c.height = 32;
    const ctx = c.getContext('2d');
    ctx.fillStyle = '#E5484D';
    ctx.beginPath();
    ctx.arc(16, 16, 16, 0, Math.PI * 2);
    ctx.fill();
    const texto = total > 99 ? '99+' : String(total);
    ctx.fillStyle = '#fff';
    ctx.font = `700 ${texto.length > 2 ? 13 : texto.length > 1 ? 17 : 21}px Montserrat, Segoe UI, sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(texto, 16, 17);
    c.toBlob(async (blob) => resolve(Array.from(new Uint8Array(await blob.arrayBuffer()))), 'image/png');
  });
}

async function atualizarTotal() {
  const total = config.servicos.reduce((soma, s) => soma + est(s.id).naoLidas, 0);
  if (total === ultimoTotal) return;
  ultimoTotal = total;
  const png = total > 0 ? await seloBarraDeTarefas(total) : null;
  invoke('definir_contador', { total, png }).catch(console.error);
}

// ---------- Barra escondida e notificações ----------
async function ocultarBarra(ocultar) {
  config.barraOculta = ocultar;
  document.body.classList.toggle('barra-oculta', ocultar);
  await invoke('ocultar_barra', { ocultar });
  salvar().catch(console.error);
}

function aplicarNotificacoes() {
  const { ativas, discretas } = config.notificacoes;
  const notif = $('#btn-notif');
  notif.classList.toggle('desligado', !ativas);
  notif.title = ativas ? 'Notificações ligadas (clique para desligar)' : 'Notificações desligadas (clique para ligar)';

  const discreto = $('#btn-discreto');
  discreto.disabled = !ativas;
  discreto.classList.toggle('destaque', discretas);
  discreto.title = discretas
    ? 'Modo discreto: avisa só "nova mensagem", sem o conteúdo (clique para mostrar o conteúdo)'
    : 'Mostrando o conteúdo nas notificações (clique para o modo discreto)';

  invoke('definir_notificacoes', { ativas, discretas }).catch(console.error);
}

// Notificações de um serviço só: "normal", "discreta" (sem conteúdo) ou "desligada"
async function mudarNotificacoesServico(s, modo) {
  s.notificacoes = modo;
  await invoke('definir_notificacoes_servico', { id: s.id, modo });
  await salvar();
  render();
}

async function alterarNotificacoes(mudanca) {
  Object.assign(config.notificacoes, mudanca);
  aplicarNotificacoes();
  await salvar();
}

// ---------- Zoom (por serviço, mesmos níveis do Chrome) ----------
const NIVEIS_ZOOM = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300];
let temporizadorZoom = null;

function proximoZoom(atual, acao) {
  if (acao === 'normal') return 100;
  if (acao === 'mais') return NIVEIS_ZOOM.find((n) => n > atual) ?? NIVEIS_ZOOM.at(-1);
  return [...NIVEIS_ZOOM].reverse().find((n) => n < atual) ?? NIVEIS_ZOOM[0];
}

async function mudarZoom(id, acao) {
  const s = servico(id);
  if (!s) return;
  const novo = proximoZoom(s.zoom || 100, acao);
  s.zoom = novo;
  await invoke('definir_zoom', { id, fator: novo / 100 });
  salvar().catch(console.error);
  if (id === ativo) {
    atualizarZoomBarra();
    // Destaca o número rapidinho para mostrar que mudou
    const nivel = $('#zoom-nivel');
    nivel.classList.add('mudou');
    clearTimeout(temporizadorZoom);
    temporizadorZoom = setTimeout(() => nivel.classList.remove('mudou'), 900);
  }
}

// Mostra na barra o zoom do serviço aberto
function atualizarZoomBarra() {
  const s = ativo && servico(ativo);
  const zoom = s ? s.zoom || 100 : 100;
  $('#zoom-nivel').textContent = `${zoom}%`;
  $('#btn-zoom-menos').disabled = !s || zoom <= NIVEIS_ZOOM[0];
  $('#btn-zoom-mais').disabled = !s || zoom >= NIVEIS_ZOOM.at(-1);
  $('#zoom-nivel').disabled = !s;
}

// ---------- Serviços ----------
async function carregar(s, visivel) {
  await invoke('definir_notificacoes_servico', { id: s.id, modo: s.notificacoes || 'normal' });
  await invoke('abrir_servico', {
    id: s.id, url: s.url, nome: s.nome, perfil: s.perfil, visivel,
  });
  est(s.id).carregado = true;
  if (s.zoom && s.zoom !== 100) await invoke('definir_zoom', { id: s.id, fator: s.zoom / 100 });
}

async function selecionar(id) {
  const s = servico(id);
  if (!s || bloqueado) return;
  if (ativo) est(ativo).ultimoUso = Date.now();
  ativo = id;
  est(id).ultimoUso = Date.now();
  try { localStorage.setItem('ethos-ativo', id); } catch {}

  if (!est(id).carregado) await carregar(s, false);
  if (document.body.classList.contains('expandida')) await recolher(false);
  await invoke('mostrar_servico', { id });
  render();
  atualizarZoomBarra();
}

async function hibernar(id) {
  await invoke('hibernar_servico', { id });
  const e = est(id);
  e.carregado = false;
  e.naoLidas = 0;
  render();
  atualizarTotal();
  if (id === ativo) mostrarAbertura('Serviço hibernado. Clique no ícone para abrir de novo.');
}

function verificarHibernacao() {
  const limite = (config.minutosHibernacao || 15) * 60_000;
  const agora = Date.now();
  for (const s of config.servicos) {
    const e = est(s.id);
    if (s.hibernar && s.id !== ativo && e.carregado && agora - e.ultimoUso > limite) {
      hibernar(s.id).catch(console.error);
    }
  }
}

// ---------- Tela expandida (abertura e diálogos) ----------
async function expandir() {
  document.body.classList.add('expandida');
  await invoke('ui_expandir', { expandir: true });
}

async function recolher(mostrarAtivo = true) {
  if (bloqueado) return;
  $('#dialogo').hidden = true;
  $('#dialogo').classList.remove('dialogo-largo');
  dialogoDownloadsAberto = false;
  document.body.classList.remove('com-dialogo');
  if (!ativo || !servico(ativo) || !est(ativo).carregado) {
    mostrarAbertura(config.servicos.length ? 'Escolha um serviço na barra ao lado.' : 'Clique em + para adicionar um serviço.');
    return;
  }
  await invoke('ui_expandir', { expandir: false });
  document.body.classList.remove('expandida');
  if (mostrarAtivo) await invoke('mostrar_servico', { id: ativo });
}

function mostrarAbertura(msg) {
  $('#abertura-msg').textContent = msg;
  $('#dialogo').hidden = true;
  document.body.classList.remove('com-dialogo');
  expandir();
}

function abrirDialogo(html) {
  const form = $('#dialogo');
  form.innerHTML = html;
  form.classList.remove('dialogo-largo');
  dialogoDownloadsAberto = false;
  form.hidden = false;
  form.onsubmit = null;
  document.body.classList.add('com-dialogo');
  expandir();
  form.querySelector('[data-acao="cancelar"]')?.addEventListener('click', () => recolher());
  setTimeout(() => form.querySelector('input, select, button')?.focus(), 50);
  return form;
}

function dialogoServico(s = null) {
  const novo = !s;
  const form = abrirDialogo(`
    <h2>${novo ? 'Adicionar serviço' : 'Editar serviço'}</h2>
    ${novo ? `<div class="atalhos">${ATALHOS.map((a, i) => `<button type="button" data-i="${i}"></button>`).join('')}</div>` : ''}
    <div class="campo"><label for="f-nome">Nome</label><input id="f-nome" type="text" maxlength="40" required></div>
    <div class="campo" id="campo-url"><label for="f-url">Endereço do site</label><input id="f-url" type="text" placeholder="https://..." required></div>
    <div class="campo">
      <label for="f-perfil">Login</label>
      <select id="f-perfil">
        <option value="proprio">Separado (só deste serviço)</option>
        <option value="google">Conta Google compartilhada (Gmail, Agenda, Drive)</option>
        <option value="geral">Compartilhado com outros sites</option>
      </select>
      <div class="ajuda">Serviços com o mesmo login ficam conectados juntos. Para outra conta de WhatsApp, use "Separado".</div>
    </div>
    <div class="campo">
      <label for="f-notif">Notificações</label>
      <select id="f-notif">
        <option value="normal">Normais (com quem mandou e o texto)</option>
        <option value="discreta">Sem conteúdo (só "Nova mensagem")</option>
        <option value="desligada">Desligadas (o contador no ícone continua)</option>
      </select>
    </div>
    <div class="campo"><label class="marcar"><input id="f-hib" type="checkbox">
      <span>Hibernar quando ficar sem uso<br><span class="ajuda">Economiza memória, mas enquanto hiberna não chegam notificações.</span></span></label></div>
    <div class="erro" id="f-erro"></div>
    <div class="acoes">
      <button type="button" class="btn-sec" data-acao="cancelar">Cancelar</button>
      <button type="submit" class="btn-pri">${novo ? 'Adicionar' : 'Salvar'}</button>
    </div>`);

  const nome = form.querySelector('#f-nome');
  const url = form.querySelector('#f-url');
  const perfil = form.querySelector('#f-perfil');
  const hib = form.querySelector('#f-hib');
  const notif = form.querySelector('#f-notif');
  let tipoAtalho = null;
  // A calculadora não tem endereço nem login: só dá para mudar o nome
  const ajustarInterno = (interno) => {
    form.querySelector('#campo-url').hidden = interno;
    url.required = !interno;
    for (const sel of ['#f-perfil', '#f-notif']) form.querySelector(sel).closest('.campo').hidden = interno;
  };

  nome.value = s?.nome ?? '';
  url.value = s?.url ?? '';
  perfil.value = s && PERFIS_COMPARTILHADOS.includes(s.perfil) ? s.perfil : 'proprio';
  hib.checked = s ? !!s.hibernar : true;
  notif.value = s?.notificacoes || 'normal';
  ajustarInterno(ehInterno(s));

  form.querySelectorAll('.atalhos button').forEach((b) => {
    const a = ATALHOS[+b.dataset.i];
    b.textContent = a.nome;
    b.addEventListener('click', () => {
      nome.value = a.nome;
      url.value = a.url;
      perfil.value = a.perfil;
      hib.checked = a.hibernar;
      tipoAtalho = a.tipo ?? null;
      ajustarInterno(tipoAtalho === 'calculadora');
    });
  });

  form.onsubmit = async (ev) => {
    ev.preventDefault();
    const erro = form.querySelector('#f-erro');
    const interno = (s ? s.tipo : tipoAtalho) === 'calculadora';
    let endereco = interno ? 'https://calculadora.invalid/' : url.value.trim();
    if (!/^https?:\/\//i.test(endereco)) endereco = 'https://' + endereco;
    let u;
    try { u = new URL(endereco); } catch { erro.textContent = 'Endereço inválido.'; return; }
    if (!nome.value.trim()) { erro.textContent = 'Dê um nome ao serviço.'; return; }

    const id = s?.id ?? 's' + Date.now().toString(36);
    const perfilFinal = perfil.value !== 'proprio'
      ? perfil.value
      : (s && !PERFIS_COMPARTILHADOS.includes(s.perfil) ? s.perfil : id);
    const tipo = s?.tipo?.startsWith('whatsapp') || interno ? (s?.tipo ?? tipoAtalho)
      : tipoAtalho ?? (u.hostname === 'web.whatsapp.com' ? 'whatsapp' : 'site');
    const dados = {
      id, nome: nome.value.trim(), url: interno ? URL_CALCULADORA : u.href, tipo,
      perfil: interno ? 'calculadora' : perfilFinal, hibernar: hib.checked, notificacoes: notif.value,
    };
    await invoke('definir_notificacoes_servico', { id, modo: notif.value });

    if (novo) {
      config.servicos.push(dados);
    } else {
      const mudouCarregamento = s.url !== dados.url || s.perfil !== dados.perfil;
      Object.assign(s, dados);
      if (mudouCarregamento && est(id).carregado) await hibernar(id);
    }
    await salvar();
    render();
    await selecionar(id);
  };
}

function dialogoRemover(s) {
  const compartilhado = config.servicos.some((o) => o.id !== s.id && o.perfil === s.perfil);
  const form = abrirDialogo(`
    <h2>Remover serviço</h2>
    <p>Remover <strong id="r-nome"></strong> da barra?</p>
    ${compartilhado
      ? '<p class="ajuda">O login deste serviço é compartilhado com outros, então ele continua salvo.</p>'
      : '<label class="marcar"><input id="r-apagar" type="checkbox" checked> Apagar também o login e os dados salvos</label>'}
    <div class="acoes">
      <button type="button" class="btn-sec" data-acao="cancelar">Cancelar</button>
      <button type="submit" class="btn-perigo">Remover</button>
    </div>`);
  form.querySelector('#r-nome').textContent = s.nome;

  form.onsubmit = async (ev) => {
    ev.preventDefault();
    const apagarDados = !compartilhado && form.querySelector('#r-apagar').checked;
    await invoke('remover_servico', { id: s.id, perfil: s.perfil, apagarDados });
    config.servicos = config.servicos.filter((o) => o.id !== s.id);
    delete estado[s.id];
    await salvar();
    if (ativo === s.id) ativo = null;
    render();
    atualizarTotal();
    const proximo = config.servicos[0];
    if (proximo) await selecionar(proximo.id);
    else recolher();
  };
}

const OPCOES_BLOQUEIO = [0, 1, 5, 10, 15, 30, 60];

function dialogoConfig() {
  const form = abrirDialogo(`
    <h2>Configurações</h2>
    <div class="campo">
      <label for="c-min">Hibernar serviços sem uso depois de (minutos)</label>
      <input id="c-min" type="number" min="1" max="480" required>
      <div class="ajuda">Vale só para os serviços marcados para hibernar. Os WhatsApps ficam sempre ativos para receber mensagens.</div>
    </div>

    <div class="secao">
      <h3>Bloqueio com PIN</h3>
      <div id="c-pin-campos"></div>
      <div class="campo">
        <label for="c-bloq">Bloquear depois de</label>
        <select id="c-bloq">${OPCOES_BLOQUEIO.map((m) => `<option value="${m}">${m === 0 ? 'Nunca (só nas outras situações)' : `${m} min sem usar o app`}</option>`).join('')}</select>
        <div class="ajuda">Com PIN, o app também bloqueia ao abrir, quando o computador volta da suspensão e quando o Windows é bloqueado (Win+L). Bloqueado, os WhatsApps continuam conectados e as notificações chegam só como "Nova mensagem".</div>
      </div>
    </div>

    <p class="ajuda">Clique com o botão direito num ícone para recarregar, abrir no navegador, editar ou remover.</p>
    <p class="ajuda" id="c-versao">Ethos Connect · Ethos Digital</p>
    <div class="acoes">
      <button type="button" class="btn-sec" data-acao="cancelar">Cancelar</button>
      <button type="submit" class="btn-pri">Salvar</button>
    </div>`);

  const min = form.querySelector('#c-min');
  const bloq = form.querySelector('#c-bloq');
  min.value = config.minutosHibernacao;
  bloq.value = String(config.bloqueioMinutos);
  window.__TAURI__.app.getVersion().then((v) => {
    form.querySelector('#c-versao').textContent = `Ethos Connect ${v} · Ethos Digital · atualiza sozinho`;
  }).catch(() => {});
  montarCamposPin(form);

  form.onsubmit = async (ev) => {
    ev.preventDefault();
    config.minutosHibernacao = Math.min(480, Math.max(1, Math.round(+min.value || 15)));
    config.bloqueioMinutos = +bloq.value;
    await invoke('definir_bloqueio', { minutos: config.bloqueioMinutos });
    await salvar();
    recolher();
  };
}

// Campos de criar, trocar ou remover o PIN (as ações valem na hora, sem o "Salvar")
function montarCamposPin(form) {
  const area = form.querySelector('#c-pin-campos');
  const campoSenha = (id, rotulo) => `<div class="campo"><label for="${id}">${rotulo}</label>
    <input id="${id}" type="password" inputmode="numeric" maxlength="8" autocomplete="off"></div>`;
  area.innerHTML = pinDefinido
    ? `${campoSenha('p-atual', 'PIN atual')}
       <div class="linha">${campoSenha('p-novo', 'Novo PIN')}${campoSenha('p-conf', 'Repita o novo PIN')}</div>
       <div class="linha" style="justify-content: space-between; align-items: center">
         <button type="button" class="btn-sec" id="p-trocar" style="border:0;border-radius:9px;padding:9px 14px;font:inherit;font-size:13px;font-weight:700;cursor:pointer">Trocar PIN</button>
         <button type="button" class="btn-texto" id="p-remover">Remover PIN</button>
       </div>`
    : `<div class="linha">${campoSenha('p-novo', 'Crie um PIN (4 a 8 números)')}${campoSenha('p-conf', 'Repita o PIN')}</div>
       <button type="button" class="btn-sec" id="p-criar" style="border:0;border-radius:9px;padding:9px 14px;font:inherit;font-size:13px;font-weight:700;cursor:pointer">Criar PIN</button>`;
  area.insertAdjacentHTML('beforeend', '<div class="erro" id="p-erro"></div><div class="ok" id="p-ok"></div>');

  const valor = (id) => area.querySelector(`#${id}`)?.value.trim() ?? '';
  const erro = (msg) => { area.querySelector('#p-erro').textContent = msg; area.querySelector('#p-ok').textContent = ''; };
  const ok = (msg) => { area.querySelector('#p-ok').textContent = msg; area.querySelector('#p-erro').textContent = ''; };
  area.querySelectorAll('input').forEach((i) => i.addEventListener('input', () => { i.value = i.value.replace(/\D/g, ''); }));

  const mensagemFalha = (r) => (r.esperaSegundos > 0
    ? `PIN atual incorreto. Aguarde ${formatarTempo(r.esperaSegundos)} para tentar de novo.`
    : 'PIN atual incorreto.');

  const novoValido = () => {
    const novo = valor('p-novo');
    if (!/^\d{4,8}$/.test(novo)) { erro('O PIN precisa ter de 4 a 8 números.'); return null; }
    if (novo !== valor('p-conf')) { erro('Os dois PINs não são iguais.'); return null; }
    return novo;
  };

  const definir = async (atual) => {
    const novo = novoValido();
    if (!novo) return;
    try {
      const r = await invoke('definir_pin', { atual, novo });
      if (!r.ok) { erro(mensagemFalha(r)); return; }
      const primeiraVez = !pinDefinido;
      pinDefinido = true;
      $('#btn-cadeado').hidden = false;
      if (primeiraVez && config.bloqueioMinutos === 0) {
        config.bloqueioMinutos = 10;
        form.querySelector('#c-bloq').value = '10';
        await invoke('definir_bloqueio', { minutos: 10 });
        await salvar();
      }
      montarCamposPin(form);
      form.querySelector('#p-ok').textContent = primeiraVez ? 'PIN criado. O app já bloqueia nas situações abaixo.' : 'PIN trocado.';
    } catch (e) { erro(String(e)); }
  };

  area.querySelector('#p-criar')?.addEventListener('click', () => definir(null));
  area.querySelector('#p-trocar')?.addEventListener('click', () => definir(valor('p-atual')));
  area.querySelector('#p-remover')?.addEventListener('click', async () => {
    if (!valor('p-atual')) { erro('Digite o PIN atual para remover.'); return; }
    try {
      const r = await invoke('remover_pin', { atual: valor('p-atual') });
      if (!r.ok) { erro(mensagemFalha(r)); return; }
      pinDefinido = false;
      $('#btn-cadeado').hidden = true;
      montarCamposPin(form);
      ok('PIN removido. O app não bloqueia mais.');
    } catch (e) { erro(String(e)); }
  });
}

// ---------- Downloads ----------
// O que os serviços baixam aparece aqui: abrir, mostrar na pasta ou arrastar para fora.
let downloads = [];
let novosDownloads = 0;
let dialogoDownloadsAberto = false;

const TIPOS_ARQUIVO = [
  [/\.(jpe?g|png|gif|webp|bmp|heic|svg)$/i, '#3A9BDC', 'IMG'],
  [/\.(mp4|mov|avi|mkv|webm|3gp)$/i, '#8E5BD8', 'VÍD'],
  [/\.(mp3|ogg|opus|wav|m4a|aac)$/i, '#E07A2E', 'ÁUD'],
  [/\.pdf$/i, '#E5484D', 'PDF'],
  [/\.(docx?|odt|rtf)$/i, '#2B6CD8', 'DOC'],
  [/\.(xlsx?|ods|csv)$/i, '#1B8A4A', 'XLS'],
  [/\.(pptx?|odp)$/i, '#D2562E', 'PPT'],
  [/\.(zip|rar|7z|tar|gz)$/i, '#7A6A55', 'ZIP'],
];

function iconeArquivo(nome) {
  const tipo = TIPOS_ARQUIVO.find(([re]) => re.test(nome));
  const el = document.createElement('span');
  el.className = 'dl-icone';
  el.style.background = tipo ? tipo[1] : '#5B6B82';
  el.textContent = tipo ? tipo[2] : (nome.includes('.') ? nome.split('.').pop().slice(0, 3).toUpperCase() : '?');
  return el;
}

function formatarTamanho(b) {
  if (!b) return '';
  const u = ['B', 'KB', 'MB', 'GB'];
  let i = 0;
  while (b >= 1024 && i < u.length - 1) { b /= 1024; i++; }
  return `${b.toLocaleString('pt-BR', { maximumFractionDigits: i ? 1 : 0 })} ${u[i]}`;
}

function formatarQuando(ms) {
  const d = new Date(ms);
  const hora = d.toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' });
  if (d.toDateString() === new Date().toDateString()) return `hoje, ${hora}`;
  if (d.toDateString() === new Date(Date.now() - 86_400_000).toDateString()) return `ontem, ${hora}`;
  return `${d.toLocaleDateString('pt-BR')}, ${hora}`;
}

const telaDownloadsVisivel = () => dialogoDownloadsAberto && !$('#dialogo').hidden;

function atualizarSeloDownloads() {
  const selo = $('#selo-downloads');
  selo.hidden = novosDownloads === 0;
  selo.textContent = novosDownloads > 9 ? '9+' : String(novosDownloads);
  const baixando = downloads.some((d) => d.estado === 'baixando');
  $('#btn-downloads').classList.toggle('baixando', baixando);
  $('#btn-downloads').title = baixando ? 'Downloads (baixando…)' : 'Downloads';
}

function dialogoDownloads() {
  novosDownloads = 0;
  atualizarSeloDownloads();
  const form = abrirDialogo(`
    <h2>Downloads</h2>
    <p class="ajuda dl-dica">Arraste um arquivo daqui para a Área de Trabalho, uma pasta ou outro programa.
      Para mandar um arquivo no WhatsApp, arraste do computador direto para a conversa.</p>
    <div id="dl-lista" class="dl-lista"></div>
    <div class="acoes dl-acoes">
      <button type="button" class="btn-texto" id="dl-limpar" title="Tira os itens da lista; os arquivos continuam no computador">Limpar lista</button>
      <span class="espaco"></span>
      <button type="button" class="btn-sec" id="dl-pasta">Abrir pasta Downloads</button>
      <button type="button" class="btn-pri" data-acao="cancelar">Fechar</button>
    </div>`);
  form.classList.add('dialogo-largo');
  dialogoDownloadsAberto = true;
  form.querySelector('#dl-pasta').addEventListener('click', () => invoke('abrir_download', { id: null }).catch(console.error));
  form.querySelector('#dl-limpar').addEventListener('click', () => invoke('limpar_downloads').catch(console.error));
  desenharDownloads();
  invoke('listar_downloads').then((l) => { downloads = l; desenharDownloads(); }).catch(console.error);
}

function avisarErro(e) {
  const lista = document.querySelector('#dl-lista');
  if (!lista) return;
  let aviso = lista.parentElement.querySelector('.erro.dl-erro');
  if (!aviso) {
    aviso = document.createElement('div');
    aviso.className = 'erro dl-erro';
    lista.after(aviso);
  }
  aviso.textContent = String(e);
}

// Arrastar para fora: começa o arraste do Windows quando o mouse anda alguns pixels
function permitirArrastar(item, id) {
  item.addEventListener('mousedown', (ev) => {
    if (ev.button !== 0 || ev.target.closest('button')) return;
    ev.preventDefault();
    const x0 = ev.clientX;
    const y0 = ev.clientY;
    const parar = () => {
      window.removeEventListener('mousemove', mover);
      window.removeEventListener('mouseup', parar);
    };
    const mover = (m) => {
      if (Math.abs(m.clientX - x0) + Math.abs(m.clientY - y0) < 5) return;
      parar();
      item.classList.add('arrastando');
      invoke('arrastar_download', { id })
        .catch(avisarErro)
        .finally(() => item.classList.remove('arrastando'));
    };
    window.addEventListener('mousemove', mover);
    window.addEventListener('mouseup', parar);
  });
}

function desenharDownloads() {
  const lista = document.querySelector('#dl-lista');
  if (!lista) return;
  lista.replaceChildren();
  if (!downloads.length) {
    const vazio = document.createElement('p');
    vazio.className = 'dl-vazio';
    vazio.textContent = 'Nada baixado ainda. O que você baixar no WhatsApp, no Gmail e nos outros serviços aparece aqui.';
    lista.append(vazio);
    return;
  }
  const SITUACAO = { baixando: 'Baixando…', falhou: 'Não terminou', sumiu: 'Apagado ou movido da pasta' };
  for (const d of downloads) {
    const item = document.createElement('div');
    item.className = `dl-item dl-${d.estado}`;
    item.title = d.caminho;

    const info = document.createElement('div');
    info.className = 'dl-info';
    const nome = document.createElement('div');
    nome.className = 'dl-nome';
    nome.textContent = d.nome;
    const detalhe = document.createElement('div');
    detalhe.className = 'dl-detalhe';
    detalhe.textContent = [SITUACAO[d.estado], formatarTamanho(d.tamanho), formatarQuando(d.quando), servico(d.servico)?.nome]
      .filter(Boolean).join(' · ');
    info.append(nome, detalhe);

    const botoes = document.createElement('div');
    botoes.className = 'dl-botoes';
    const botao = (texto, titulo, acao, classe = '') => {
      const b = document.createElement('button');
      b.type = 'button';
      b.textContent = texto;
      b.title = titulo;
      if (classe) b.className = classe;
      b.addEventListener('click', () => acao().catch(avisarErro));
      botoes.append(b);
    };
    const concluido = d.estado === 'concluido';
    if (concluido) botao('Abrir', 'Abrir o arquivo', () => invoke('abrir_download', { id: d.id }));
    if (d.estado !== 'baixando') {
      botao('Pasta', 'Mostrar na pasta', () => invoke('mostrar_download', { id: d.id }));
      botao('×', 'Tirar da lista (o arquivo continua no computador)', () => invoke('remover_download', { id: d.id }), 'dl-x');
    }

    item.append(iconeArquivo(d.nome), info, botoes);
    if (concluido) {
      item.classList.add('arrastavel');
      item.addEventListener('dblclick', (ev) => {
        if (!ev.target.closest('button')) invoke('abrir_download', { id: d.id }).catch(avisarErro);
      });
      permitirArrastar(item, d.id);
    }
    lista.append(item);
  }
}

listen('downloads', ({ payload }) => {
  downloads = payload;
  atualizarSeloDownloads();
  if (telaDownloadsVisivel()) desenharDownloads();
});

listen('download-concluido', () => {
  if (!telaDownloadsVisivel()) novosDownloads += 1;
  atualizarSeloDownloads();
});


// ---------- Bloqueio com PIN ----------
function formatarTempo(segundos) {
  if (segundos < 60) return `${segundos} s`;
  const m = Math.floor(segundos / 60);
  const s = segundos % 60;
  return s ? `${m} min ${s} s` : `${m} min`;
}

let contagem = null;

function mostrarBloqueio() {
  bloqueado = true;
  document.body.classList.add('bloqueado', 'expandida');
  document.body.classList.remove('com-dialogo');
  $('#dialogo').hidden = true;
  $('#form-pin').hidden = false;
  $('#painel-esqueci').hidden = true;
  $('#campo-pin').value = '';
  if (!contagem) $('#pin-erro').textContent = '';
  setTimeout(() => $('#campo-pin').focus(), 50);
}

async function aposDesbloquear() {
  bloqueado = false;
  document.body.classList.remove('bloqueado');
  const id = ativo && servico(ativo) ? ativo : servicoInicial ?? config.servicos[0]?.id;
  if (id) await selecionar(id);
  else mostrarAbertura('Clique em + para adicionar um serviço.');
}

function aguardar(segundos) {
  const campo = $('#campo-pin');
  const botao = $('#form-pin .btn-pin');
  campo.disabled = botao.disabled = true;
  clearInterval(contagem);
  let resta = segundos;
  const passo = () => {
    if (resta <= 0) {
      clearInterval(contagem);
      contagem = null;
      campo.disabled = botao.disabled = false;
      $('#pin-erro').textContent = 'Pode tentar de novo.';
      campo.focus();
      return;
    }
    $('#pin-erro').textContent = `Muitas tentativas erradas. Aguarde ${formatarTempo(resta)}.`;
    resta -= 1;
  };
  passo();
  contagem = setInterval(passo, 1000);
}

$('#campo-pin').addEventListener('input', (ev) => {
  ev.target.value = ev.target.value.replace(/\D/g, '');
});

$('#form-pin').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const campo = $('#campo-pin');
  const botao = $('#form-pin .btn-pin');
  const pin = campo.value.trim();
  if (!pin || campo.disabled) return;
  botao.disabled = true;
  try {
    const r = await invoke('desbloquear', { pin });
    campo.value = '';
    if (r.ok) {
      $('#pin-erro').textContent = '';
      await aposDesbloquear();
    } else if (r.esperaSegundos > 0) {
      aguardar(r.esperaSegundos);
    } else {
      $('#pin-erro').textContent = r.restantes > 0
        ? `PIN incorreto. Mais ${r.restantes} ${r.restantes === 1 ? 'tentativa' : 'tentativas'} antes de ter que esperar.`
        : 'PIN incorreto.';
      campo.focus();
    }
  } catch (e) {
    $('#pin-erro').textContent = String(e);
  } finally {
    if (!campo.disabled) botao.disabled = false;
  }
});

$('#btn-esqueci').addEventListener('click', () => {
  $('#form-pin').hidden = true;
  $('#painel-esqueci').hidden = false;
});
$('#btn-voltar-pin').addEventListener('click', () => mostrarBloqueio());
$('#btn-apagar-tudo').addEventListener('click', () => {
  $('#btn-apagar-tudo').disabled = true;
  invoke('esqueci_pin').catch((e) => { $('#btn-apagar-tudo').disabled = false; console.error(e); });
});
$('#btn-cadeado').addEventListener('click', () => invoke('bloquear_agora'));

listen('bloqueado', () => mostrarBloqueio());

// Uso da barra conta como atividade (os sites avisam pelo script deles)
let ultimoUsoBarra = 0;
for (const tipo of ['pointerdown', 'keydown', 'wheel', 'mousemove']) {
  window.addEventListener(tipo, () => {
    const agora = Date.now();
    if (!bloqueado && agora - ultimoUsoBarra > 20000) {
      ultimoUsoBarra = agora;
      invoke('registrar_atividade').catch(() => {});
    }
  }, { capture: true, passive: true });
}

// ---------- Eventos ----------
listen('servico-titulo', ({ payload }) => {
  const m = String(payload.titulo ?? '').match(/\((\d+)\)/);
  est(payload.id).naoLidas = m ? +m[1] : 0;
  atualizarSelo(payload.id);
  atualizarTotal();
});

listen('menu-servico', async ({ payload: { acao, id } }) => {
  const s = servico(id);
  if (!s) return;
  if (acao === 'recarregar') {
    if (est(id).carregado) await invoke('recarregar_servico', { id });
    else await selecionar(id);
  } else if (acao === 'navegador') {
    await invoke('abrir_externo', { url: s.url });
  } else if (acao === 'hibernar') {
    if (est(id).carregado) await hibernar(id);
  } else if (acao.startsWith('notif-')) {
    await mudarNotificacoesServico(s, acao.slice(6));
  } else if (acao.startsWith('zoom-')) {
    await mudarZoom(id, acao.slice(5));
  } else if (acao === 'editar') {
    dialogoServico(s);
  } else if (acao === 'remover') {
    dialogoRemover(s);
  }
});

listen('zoom', ({ payload }) => mudarZoom(payload.id, payload.acao));

listen('atalho', ({ payload }) => {
  if (payload.acao === 'barra') ocultarBarra(!config.barraOculta);
});

$('#btn-adicionar').addEventListener('click', () => dialogoServico());
$('#btn-downloads').addEventListener('click', () => dialogoDownloads());
$('#btn-config').addEventListener('click', () => dialogoConfig());
$('#btn-ocultar').addEventListener('click', () => ocultarBarra(true));
$('#btn-zoom-mais').addEventListener('click', () => ativo && mudarZoom(ativo, 'mais'));
$('#btn-zoom-menos').addEventListener('click', () => ativo && mudarZoom(ativo, 'menos'));
$('#zoom-nivel').addEventListener('click', () => ativo && mudarZoom(ativo, 'normal'));
$('#faixa').addEventListener('click', () => ocultarBarra(false));
$('#btn-notif').addEventListener('click', () => alterarNotificacoes({ ativas: !config.notificacoes.ativas }));
$('#btn-discreto').addEventListener('click', () => alterarNotificacoes({ discretas: !config.notificacoes.discretas }));
document.addEventListener('keydown', (ev) => {
  if (ev.key === 'Escape' && !$('#dialogo').hidden) recolher();
  if (ev.ctrlKey && ev.altKey && !ev.shiftKey && ev.code === 'KeyB') {
    ev.preventDefault();
    ocultarBarra(!config.barraOculta);
  }
});
document.addEventListener('contextmenu', (ev) => {
  if (!ev.target.closest('input, textarea')) ev.preventDefault();
});

// ---------- Início ----------
async function iniciar() {
  try {
    const texto = await invoke('carregar_config');
    config = texto ? JSON.parse(texto) : null;
  } catch (e) {
    console.error('config inválida, usando padrão', e);
    config = null;
  }
  if (!config || !Array.isArray(config.servicos)) {
    config = structuredClone(CONFIG_PADRAO);
    await salvar();
  }
  // Configs salvas por versões anteriores não têm esses campos
  config.notificacoes = { ...CONFIG_PADRAO.notificacoes, ...config.notificacoes };
  config.barraOculta = !!config.barraOculta;
  config.bloqueioMinutos = OPCOES_BLOQUEIO.includes(config.bloqueioMinutos) ? config.bloqueioMinutos : 0;
  // A calculadora entra uma vez como app padrão; se a pessoa remover, não volta sozinha
  if (!config.calculadoraAdicionada) {
    if (!config.servicos.some(ehInterno)) config.servicos.push(structuredClone(SERVICO_CALCULADORA));
    config.calculadoraAdicionada = true;
    await salvar();
  }
  aplicarNotificacoes();
  if (config.barraOculta) await ocultarBarra(true);
  render();

  // Com PIN, o app já abre bloqueado (a parte nativa decide); os serviços carregam
  // por trás para as mensagens continuarem chegando
  const estadoBloqueio = await invoke('estado_bloqueio');
  pinDefinido = estadoBloqueio.pinDefinido;
  $('#btn-cadeado').hidden = !pinDefinido;
  await invoke('definir_bloqueio', { minutos: config.bloqueioMinutos });
  if (estadoBloqueio.bloqueado) mostrarBloqueio();

  // Os que não hibernam já abrem em segundo plano, para receber mensagens
  for (const s of config.servicos.filter((s) => !s.hibernar)) {
    await carregar(s, false).catch(console.error);
  }
  render();

  let inicial = null;
  try { inicial = localStorage.getItem('ethos-ativo'); } catch {}
  if (!servico(inicial)) inicial = config.servicos[0]?.id;
  servicoInicial = inicial;

  await new Promise((r) => setTimeout(r, 900));
  if (bloqueado) {
    // Fica na tela de PIN; o serviço inicial abre depois de desbloquear
  } else if (inicial) await selecionar(inicial);
  else mostrarAbertura('Clique em + para adicionar um serviço.');

  setInterval(verificarHibernacao, 30_000);
  invoke('listar_downloads').then((l) => { downloads = l; atualizarSeloDownloads(); }).catch(console.error);
}

iniciar();
