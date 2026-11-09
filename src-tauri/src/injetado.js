// Injetado em cada serviço (WhatsApp, Gmail...). Repassa ao Ethos Connect:
// título da aba (contador de não lidas), notificações e links externos.
(function () {
  if (window.top !== window || window.__ethosConnect) return;
  window.__ethosConnect = true;

  const avisar = (tipo, dados) => {
    try {
      window.__TAURI_INTERNALS__.invoke('servico_evento', { tipo, dados });
    } catch (_) {}
  };

  // Notificações: o site acha que tem permissão e nós mostramos o aviso do Windows
  class NotificacaoEthos extends EventTarget {
    constructor(titulo, opcoes = {}) {
      super();
      this.title = String(titulo ?? '');
      this.body = String(opcoes.body ?? '');
      this.onclick = null;
      this.onclose = null;
      this.onerror = null;
      this.onshow = null;
      avisar('notificacao', { titulo: this.title, corpo: this.body });
    }
    close() {}
    static get permission() { return 'granted'; }
    static requestPermission(cb) {
      if (typeof cb === 'function') cb('granted');
      return Promise.resolve('granted');
    }
  }
  Object.defineProperty(window, 'Notification', { value: NotificacaoEthos, writable: true, configurable: true });

  if (window.ServiceWorkerRegistration) {
    ServiceWorkerRegistration.prototype.showNotification = function (titulo, opcoes = {}) {
      avisar('notificacao', { titulo: String(titulo ?? ''), corpo: String(opcoes.body ?? '') });
      return Promise.resolve();
    };
  }

  // Título da aba, ex.: "(3) WhatsApp" ou "Caixa de entrada (12) - Gmail"
  let ultimoTitulo = null;
  setInterval(() => {
    const t = document.title;
    if (t !== ultimoTitulo) {
      ultimoTitulo = t;
      avisar('titulo', { titulo: t });
    }
  }, 1500);

  // Links que abririam nova aba vão para o navegador padrão, exceto os do mesmo
  // serviço (ex.: "Fazer login" do Gmail abre accounts.google.com), que ficam no app
  // para o login acontecer no perfil certo.
  const ehWeb = (u) => /^https?:/i.test(u);
  const PAGINAS_DE_LOGIN = ['accounts.google.com', 'login.microsoftonline.com', 'login.live.com'];
  const familia = (host) => {
    const partes = host.split('.');
    // "algo.com.br" tem 3 partes no domínio principal; "algo.com" tem 2
    const n = partes.length > 2 && partes[partes.length - 2].length <= 3 && partes[partes.length - 1].length === 2 ? 3 : 2;
    return partes.slice(-n).join('.');
  };
  const ficaNoApp = (url) => {
    if (location.hostname === 'web.whatsapp.com') return false;
    const destino = new URL(url).hostname;
    return PAGINAS_DE_LOGIN.includes(destino) || familia(destino) === familia(location.hostname);
  };

  document.addEventListener('click', (e) => {
    const a = e.target && e.target.closest && e.target.closest('a[target="_blank"]');
    if (a && ehWeb(a.href)) {
      e.preventDefault();
      e.stopPropagation();
      if (ficaNoApp(a.href)) location.href = a.href;
      else avisar('abrir', { url: a.href });
    }
  }, true);

  // Uso do app (para o bloqueio por inatividade); avisa no máximo a cada 20 s
  let ultimoUso = 0;
  const usou = () => {
    const agora = Date.now();
    if (agora - ultimoUso > 20000) {
      ultimoUso = agora;
      avisar('atividade', {});
    }
  };
  for (const tipo of ['pointerdown', 'keydown', 'wheel', 'mousemove']) {
    window.addEventListener(tipo, usou, { capture: true, passive: true });
  }

  // Zoom igual ao Chrome: Ctrl + rolar, Ctrl +, Ctrl − e Ctrl 0. O app aplica e
  // guarda o nível de cada serviço.
  let ultimoZoomRoda = 0;
  window.addEventListener('wheel', (e) => {
    if (!e.ctrlKey || e.deltaY === 0) return;
    e.preventDefault();
    const agora = Date.now();
    if (agora - ultimoZoomRoda < 120) return; // um passo por "clique" da roda
    ultimoZoomRoda = agora;
    avisar('zoom', { acao: e.deltaY < 0 ? 'mais' : 'menos' });
  }, { capture: true, passive: false });

  window.addEventListener('keydown', (e) => {
    if (!e.ctrlKey || e.altKey || e.metaKey) return;
    let acao = null;
    if (e.key === '+' || e.key === '=' || e.code === 'NumpadAdd') acao = 'mais';
    else if (e.key === '-' || e.code === 'NumpadSubtract') acao = 'menos';
    else if (e.key === '0' || e.code === 'Numpad0') acao = 'normal';
    if (!acao) return;
    e.preventDefault();
    e.stopPropagation();
    avisar('zoom', { acao });
  }, true);

  // Ctrl+Alt+B esconde/mostra a barra do Ethos Connect, mesmo com o foco no site
  window.addEventListener('keydown', (e) => {
    if (e.ctrlKey && e.altKey && !e.shiftKey && e.code === 'KeyB') {
      e.preventDefault();
      e.stopPropagation();
      avisar('atalho', { acao: 'barra' });
    }
  }, true);

  if (location.hostname === 'web.whatsapp.com') {
    divisoriaWhatsApp();
    // Diagnóstico: anota se a sessão voltou logada ou caiu no QR code
    setTimeout(() => {
      const logado = !!document.getElementById('side');
      const qr = !!document.querySelector('[data-ref], canvas[aria-label]');
      avisar('diagnostico', {
        texto: `ao abrir: ${logado ? 'LOGADO' : qr ? 'PEDINDO QR CODE' : 'indefinido'} (localStorage com ${localStorage.length} chaves)`,
      });
    }, 40000);
  }

  // Divisória arrastável entre a lista de conversas e o chat.
  // O WhatsApp não tem isso; achamos a coluna da lista pela posição de #side.
  function divisoriaWhatsApp() {
    const CHAVE = 'ethos-largura-lista';
    const MINIMO = 260;
    let largura = parseInt(localStorage.getItem(CHAVE), 10) || 0;
    let puxador = null;
    let coluna = null;

    // Sobe a partir de #side até o elemento que é a coluna inteira da lista
    const acharColuna = () => {
      const side = document.getElementById('side');
      if (!side) return null;
      let el = side;
      while (el.parentElement && el.parentElement !== document.body) {
        const pai = el.parentElement;
        if (pai.getBoundingClientRect().width - el.getBoundingClientRect().width > 50) return el;
        el = pai;
      }
      return null;
    };

    const limitar = (w) => Math.max(MINIMO, Math.min(w, window.innerWidth - 380));

    const fixarLargura = (el, w) => {
      el.style.flex = `0 0 ${w}px`;
      el.style.width = `${w}px`;
      el.style.maxWidth = 'none';
      el.style.minWidth = '0';
    };

    // O WhatsApp tem camadas com a mesma largura da lista (onde abrem perfil,
    // configurações, nova conversa...). Elas têm borda própria e, se não forem
    // ajustadas junto, fica uma linha cortando o chat na largura antiga.
    let camadas = [];
    let ultimaBusca = 0;
    const buscarCamadas = () => {
      const pai = coluna.parentElement;
      const ref = coluna.getBoundingClientRect();
      const achadas = [];
      const visitar = (el, nivel) => {
        for (const filho of el.children) {
          if (filho === coluna || filho.id === 'main' || filho === puxador) continue;
          const r = filho.getBoundingClientRect();
          const mesmaColuna = Math.abs(r.left - ref.left) < 2
            && r.width > 150 && r.width < window.innerWidth * 0.75
            && r.height > ref.height * 0.6;
          if (mesmaColuna && !filho.contains(coluna)) achadas.push(filho);
          else if (nivel < 6) visitar(filho, nivel + 1);
        }
      };
      visitar(pai, 0);
      camadas = achadas;
      ultimaBusca = Date.now();
    };

    const aplicar = () => {
      if (!coluna || !largura) return;
      const w = limitar(largura);
      const pai = coluna.parentElement;
      if (getComputedStyle(pai).display.includes('grid')) {
        const itens = [...pai.children].filter((c) => getComputedStyle(c).display !== 'none');
        const i = itens.indexOf(coluna);
        const trilhas = getComputedStyle(pai).gridTemplateColumns.split(' ');
        if (i >= 0 && trilhas.length === itens.length) {
          trilhas[i] = `${w}px`;
          trilhas[trilhas.length - 1] = 'minmax(0, 1fr)';
          pai.style.gridTemplateColumns = trilhas.join(' ');
        }
      } else {
        fixarLargura(coluna, w);
      }
      if (Date.now() - ultimaBusca > 3000 || camadas.some((c) => !c.isConnected)) buscarCamadas();
      for (const c of camadas) fixarLargura(c, w);
    };

    const posicionar = () => {
      if (!coluna || !puxador) return;
      const r = coluna.getBoundingClientRect();
      puxador.style.left = `${r.right - 3}px`;
      puxador.style.top = `${r.top}px`;
      puxador.style.height = `${r.height}px`;
    };

    const criarPuxador = () => {
      puxador = document.createElement('div');
      puxador.title = 'Arraste para ajustar a largura da lista de conversas';
      Object.assign(puxador.style, {
        position: 'fixed', width: '6px', cursor: 'col-resize', zIndex: '2147483647',
        background: 'transparent', transition: 'background .15s',
      });
      puxador.addEventListener('mouseenter', () => { puxador.style.background = 'rgba(37, 211, 102, .45)'; });
      puxador.addEventListener('mouseleave', () => { puxador.style.background = 'transparent'; });
      puxador.addEventListener('dblclick', () => {
        // Duplo clique volta ao tamanho original do WhatsApp
        largura = 0;
        localStorage.removeItem(CHAVE);
        location.reload();
      });
      puxador.addEventListener('mousedown', (e) => {
        e.preventDefault();
        const inicio = coluna.getBoundingClientRect().left;
        document.documentElement.style.cursor = 'col-resize';
        document.body.style.userSelect = 'none';
        const mover = (ev) => {
          largura = limitar(ev.clientX - inicio);
          aplicar();
          posicionar();
        };
        const soltar = () => {
          buscarCamadas();
          aplicar();
          window.removeEventListener('mousemove', mover, true);
          window.removeEventListener('mouseup', soltar, true);
          document.documentElement.style.cursor = '';
          document.body.style.userSelect = '';
          localStorage.setItem(CHAVE, String(largura));
        };
        window.addEventListener('mousemove', mover, true);
        window.addEventListener('mouseup', soltar, true);
      });
      document.documentElement.append(puxador);
    };

    // O WhatsApp redesenha a tela o tempo todo; conferimos a cada segundo
    setInterval(() => {
      if (!document.body) return;
      coluna = acharColuna();
      if (!coluna) {
        if (puxador) puxador.style.display = 'none';
        return;
      }
      if (!puxador || !puxador.isConnected) criarPuxador();
      puxador.style.display = '';
      aplicar();
      posicionar();
    }, 1000);
    window.addEventListener('resize', () => { aplicar(); posicionar(); });
  }

  const abrirOriginal = window.open;
  window.open = function (url, ...resto) {
    if (url) {
      const absoluta = new URL(String(url), location.href).href;
      // Janelas do próprio serviço (ex.: pop-up de login) abrem normalmente no app
      if (ehWeb(absoluta) && !ficaNoApp(absoluta)) {
        avisar('abrir', { url: absoluta });
        return null;
      }
    }
    return abrirOriginal.call(window, url, ...resto);
  };
})();
