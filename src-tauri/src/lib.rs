//! Ethos Connect: WhatsApp, WhatsApp Business e outros serviços em um só lugar.
//!
//! Uma janela ("principal") com:
//! - a barra lateral ("barra"), que é o index.html em src/ e também ocupa a tela toda
//!   quando abre um diálogo, a tela de abertura ou o bloqueio;
//! - um webview por serviço ("svc-<id>"), cada um com seu perfil de login do WebView2.

mod atualizacao;
mod bloqueio;
mod diario;
mod downloads;
mod sistema;

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::webview::{DownloadEvent, WebviewBuilder};
use tauri::window::{Color, WindowBuilder};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize, RunEvent, State,
    Url, Webview, WebviewUrl, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use bloqueio::{Bloqueio, Resposta};
use diario::anotar;
use downloads::Downloads;

/// Precisa bater com --largura-barra no estilo.css
const LARGURA_BARRA: f64 = 72.0;
/// Precisa bater com --largura-faixa no estilo.css
const LARGURA_FAIXA: f64 = 6.0;
const PREFIXO_SERVICO: &str = "svc-";
const AZUL: Color = Color(14, 38, 70, 255);

/// Telas internas que abrem como um serviço (ex.: a calculadora)
const PAGINAS_INTERNAS: &[(&str, &str)] = &[("ethos://calculadora", "calculadora.html")];

const SCRIPT_SERVICOS: &str = include_str!("injetado.js");

// ---------- Estado ----------

#[derive(Default)]
struct Tela {
    /// A barra ocupa a janela toda (abertura, diálogos, bloqueio)
    expandida: bool,
    barra_oculta: bool,
    ativo: Option<String>,
}

struct InfoServico {
    nome: String,
    notificacoes: String,
}

#[derive(Default)]
struct Notificacoes {
    ativas: bool,
    discretas: bool,
}

struct Estado {
    tela: Mutex<Tela>,
    servicos: Mutex<HashMap<String, InfoServico>>,
    notificacoes: Mutex<Notificacoes>,
    bloqueio: Mutex<Bloqueio>,
    pasta_config: PathBuf,
    pasta_local: PathBuf,
    /// Processo do navegador do WebView2, para esperar ele gravar tudo ao fechar
    pid_navegador: Mutex<Option<u32>>,
    encerrado: Mutex<bool>,
}

type Res<T> = Result<T, String>;

fn erro(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn rotulo(id: &str) -> String {
    format!("{PREFIXO_SERVICO}{id}")
}

/// Ids e perfis viram nome de webview e de pasta: só letras, números, "-", "_" e "."
fn nome_valido(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

fn principal(app: &AppHandle) -> Res<tauri::Window> {
    app.get_window("principal").ok_or_else(|| "janela principal não encontrada".into())
}

fn webviews_de_servico(app: &AppHandle) -> Vec<Webview> {
    app.webviews().into_iter().filter(|(l, _)| l.starts_with(PREFIXO_SERVICO)).map(|(_, w)| w).collect()
}

// ---------- Layout ----------

/// Posiciona a barra e o serviço aberto conforme o estado da tela
fn organizar(app: &AppHandle) {
    let Ok(janela) = principal(app) else { return };
    let Ok(tamanho) = janela.inner_size() else { return };
    let escala = janela.scale_factor().unwrap_or(1.0);
    let estado = app.state::<Estado>();
    let bloqueado = estado.bloqueio.lock().unwrap().bloqueado;
    let tela = estado.tela.lock().unwrap();
    let lateral = ((if tela.barra_oculta { LARGURA_FAIXA } else { LARGURA_BARRA }) * escala).round() as u32;
    let lateral = lateral.min(tamanho.width);
    let cheia = tela.expandida || bloqueado;

    if let Some(barra) = app.get_webview("barra") {
        let largura = if cheia { tamanho.width } else { lateral };
        let _ = barra.set_position(PhysicalPosition::new(0, 0));
        let _ = barra.set_size(PhysicalSize::new(largura, tamanho.height));
    }
    let alvo = if cheia { None } else { tela.ativo.as_deref().map(rotulo) };
    for w in webviews_de_servico(app) {
        if Some(w.label()) == alvo.as_deref() {
            let _ = w.set_position(PhysicalPosition::new(lateral as i32, 0));
            let _ = w.set_size(PhysicalSize::new(tamanho.width.saturating_sub(lateral).max(1), tamanho.height));
            let _ = w.show();
        } else {
            let _ = w.hide();
        }
    }
}

/// Bloqueia o app (se houver PIN): esconde os serviços e mostra a tela do PIN
pub(crate) fn bloquear(app: &AppHandle, motivo: &str) {
    let estado = app.state::<Estado>();
    {
        let mut b = estado.bloqueio.lock().unwrap();
        if b.pin.is_none() || b.bloqueado {
            return;
        }
        b.bloqueado = true;
    }
    anotar(format!("bloqueado: {motivo}"));
    organizar(app);
    let _ = app.emit_to("barra", "bloqueado", ());
    if let Some(barra) = app.get_webview("barra") {
        let _ = barra.set_focus();
    }
}

/// Fecha os serviços com calma para o WebView2 gravar logins e mensagens no disco
pub(crate) fn encerrar_servicos(app: &AppHandle) {
    let estado = app.state::<Estado>();
    {
        let mut feito = estado.encerrado.lock().unwrap();
        if *feito {
            return;
        }
        *feito = true;
    }
    anotar("encerrando: fechando os serviços");
    for w in webviews_de_servico(app) {
        let _ = w.close();
    }
}

/// Desfaz o encerrar_servicos (ex.: atualização que não instalou)
pub(crate) fn reabrir_servicos(app: &AppHandle) {
    *app.state::<Estado>().encerrado.lock().unwrap() = false;
    if let Some(barra) = app.get_webview("barra") {
        let _ = barra.eval("location.reload()");
    }
}

fn esperar_navegador(app: &AppHandle) {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    let Some(pid) = *app.state::<Estado>().pid_navegador.lock().unwrap() else { return };
    let inicio = Instant::now();
    unsafe {
        let Ok(processo) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else { return };
        let r = WaitForSingleObject(processo, 4000);
        let ms = inicio.elapsed().as_millis();
        if r == WAIT_OBJECT_0 {
            anotar(format!("encerrado: navegador gravou e fechou em {ms} ms"));
        } else {
            anotar(format!("encerrado: TEMPO ESGOTADO esperando o navegador ({ms} ms)"));
        }
        let _ = CloseHandle(processo);
    }
}

// ---------- Configuração ----------

#[tauri::command]
fn carregar_config(estado: State<Estado>) -> Option<String> {
    std::fs::read_to_string(estado.pasta_config.join("servicos.json")).ok()
}

#[tauri::command]
fn salvar_config(estado: State<Estado>, conteudo: String) -> Res<()> {
    serde_json::from_str::<Value>(&conteudo).map_err(erro)?;
    let destino = estado.pasta_config.join("servicos.json");
    let temp = destino.with_extension("json.tmp");
    std::fs::write(&temp, conteudo).map_err(erro)?;
    std::fs::rename(&temp, &destino).map_err(erro)
}

// ---------- Serviços ----------

#[tauri::command]
async fn abrir_servico(
    app: AppHandle,
    estado: State<'_, Estado>,
    id: String,
    url: String,
    nome: String,
    perfil: String,
    visivel: bool,
) -> Res<()> {
    if !nome_valido(&id) {
        return Err("id inválido".into());
    }
    if !nome_valido(&perfil) {
        return Err("perfil inválido".into());
    }
    estado
        .servicos
        .lock()
        .unwrap()
        .entry(id.clone())
        .and_modify(|s| s.nome = nome.clone())
        .or_insert(InfoServico { nome: nome.clone(), notificacoes: "normal".into() });

    let label = rotulo(&id);
    if app.get_webview(&label).is_some() {
        if visivel {
            estado.tela.lock().unwrap().ativo = Some(id);
            organizar(&app);
        }
        return Ok(());
    }

    let endereco = match PAGINAS_INTERNAS.iter().find(|(u, _)| *u == url) {
        Some((_, pagina)) => WebviewUrl::App(PathBuf::from(pagina)),
        None => {
            let u = Url::parse(&url).map_err(|_| "endereço inválido".to_string())?;
            if !matches!(u.scheme(), "https" | "http") {
                return Err("endereço inválido".into());
            }
            WebviewUrl::External(u)
        }
    };

    let id_download = id.clone();
    let construtor = WebviewBuilder::new(&label, endereco)
        .initialization_script(SCRIPT_SERVICOS)
        // "webview2-perfil=<perfil>" vira um perfil do WebView2 (ver patches/tauri-runtime-wry)
        .data_directory(estado.pasta_local.join(format!("webview2-perfil={perfil}")))
        // Sem isso o Tauri segura os arquivos arrastados e o site (ex.: conversa do
        // WhatsApp) nunca recebe. Desligado, o WebView2 entrega o arquivo para a página.
        .disable_drag_drop_handler()
        .zoom_hotkeys_enabled(false)
        .background_color(Color(255, 255, 255, 255))
        .on_download(move |webview, evento| {
            let app = webview.app_handle();
            match evento {
                DownloadEvent::Requested { url, destination } => {
                    downloads::pedido(app, &id_download, &url, destination);
                }
                DownloadEvent::Finished { url, path, success } => {
                    if let Some(d) = downloads::terminou(app, &url, path, success) {
                        if app.state::<Estado>().notificacoes.lock().unwrap().ativas {
                            let _ = app.notification().builder().title("Download concluído").body(d.nome).show();
                        }
                    }
                }
                _ => {}
            }
            true
        });

    let janela = principal(&app)?;
    let webview = janela
        .add_child(construtor, LogicalPosition::new(-10_000.0, 0.0), LogicalSize::new(800.0, 600.0))
        .map_err(erro)?;
    let _ = webview.hide();
    if visivel {
        estado.tela.lock().unwrap().ativo = Some(id);
    }
    organizar(&app);
    Ok(())
}

#[tauri::command]
async fn mostrar_servico(app: AppHandle, estado: State<'_, Estado>, id: String) -> Res<()> {
    if estado.bloqueio.lock().unwrap().bloqueado {
        return Ok(());
    }
    {
        let mut tela = estado.tela.lock().unwrap();
        tela.ativo = Some(id.clone());
        tela.expandida = false;
    }
    organizar(&app);
    if let Some(w) = app.get_webview(&rotulo(&id)) {
        let _ = w.set_focus();
    }
    Ok(())
}

#[tauri::command]
async fn hibernar_servico(app: AppHandle, id: String) -> Res<()> {
    if let Some(w) = app.get_webview(&rotulo(&id)) {
        w.close().map_err(erro)?;
    }
    Ok(())
}

#[tauri::command]
async fn recarregar_servico(app: AppHandle, id: String) -> Res<()> {
    if let Some(w) = app.get_webview(&rotulo(&id)) {
        w.eval("location.reload()").map_err(erro)?;
    }
    Ok(())
}

/// Perfis marcados para apagar: só dá para apagar a pasta antes do WebView2 abrir
fn arquivo_perfis_pendentes(estado: &Estado) -> PathBuf {
    estado.pasta_local.join("apagar-perfis.json")
}

#[tauri::command]
async fn remover_servico(
    app: AppHandle,
    estado: State<'_, Estado>,
    id: String,
    perfil: String,
    apagar_dados: bool,
) -> Res<()> {
    if let Some(w) = app.get_webview(&rotulo(&id)) {
        let _ = w.close();
    }
    estado.servicos.lock().unwrap().remove(&id);
    if apagar_dados && nome_valido(&perfil) {
        let arq = arquivo_perfis_pendentes(&estado);
        let mut lista: Vec<String> =
            std::fs::read_to_string(&arq).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if !lista.contains(&perfil) {
            lista.push(perfil.clone());
        }
        std::fs::write(&arq, serde_json::to_string(&lista).map_err(erro)?).map_err(erro)?;
        anotar(format!("serviço removido; login de \"{perfil}\" será apagado ao reabrir"));
    }
    Ok(())
}

#[tauri::command]
async fn definir_zoom(app: AppHandle, id: String, fator: f64) -> Res<()> {
    if let Some(w) = app.get_webview(&rotulo(&id)) {
        w.set_zoom(fator.clamp(0.25, 5.0)).map_err(erro)?;
    }
    Ok(())
}

#[tauri::command]
async fn ocultar_barra(app: AppHandle, estado: State<'_, Estado>, ocultar: bool) -> Res<()> {
    estado.tela.lock().unwrap().barra_oculta = ocultar;
    organizar(&app);
    Ok(())
}

#[tauri::command]
async fn ui_expandir(app: AppHandle, estado: State<'_, Estado>, expandir: bool) -> Res<()> {
    if !expandir && estado.bloqueio.lock().unwrap().bloqueado {
        return Ok(());
    }
    estado.tela.lock().unwrap().expandida = expandir;
    organizar(&app);
    if expandir {
        if let Some(barra) = app.get_webview("barra") {
            let _ = barra.set_focus();
        }
    }
    Ok(())
}

#[tauri::command]
async fn menu_servico(
    app: AppHandle,
    id: String,
    zoom: u32,
    notificacoes: String,
    interno: Option<bool>,
) -> Res<()> {
    let janela = principal(&app)?;
    let item = |acao: &str, texto: &str| MenuItem::with_id(&app, format!("{acao}:{id}"), texto, true, None::<&str>);
    let marcado = |modo: &str, texto: &str| {
        CheckMenuItem::with_id(&app, format!("notif-{modo}:{id}"), texto, true, notificacoes == modo, None::<&str>)
    };
    let notif = Submenu::with_items(
        &app,
        "Notificações",
        true,
        &[
            &marcado("normal", "Normais (com o texto)").map_err(erro)?,
            &marcado("discreta", "Sem conteúdo (só \"Nova mensagem\")").map_err(erro)?,
            &marcado("desligada", "Desligadas").map_err(erro)?,
        ],
    )
    .map_err(erro)?;
    let zoom_menu = Submenu::with_items(
        &app,
        format!("Zoom ({zoom}%)"),
        true,
        &[
            &item("zoom-mais", "Aumentar zoom (Ctrl +)").map_err(erro)?,
            &item("zoom-menos", "Diminuir zoom (Ctrl −)").map_err(erro)?,
            &item("zoom-normal", &format!("Zoom normal (agora {zoom}%)")).map_err(erro)?,
        ],
    )
    .map_err(erro)?;
    let separador = || PredefinedMenuItem::separator(&app);
    let menu = Menu::new(&app).map_err(erro)?;
    menu.append(&item("recarregar", "Recarregar").map_err(erro)?).map_err(erro)?;
    if !interno.unwrap_or(false) {
        menu.append(&item("navegador", "Abrir no navegador").map_err(erro)?).map_err(erro)?;
        menu.append(&separador().map_err(erro)?).map_err(erro)?;
        menu.append(&notif).map_err(erro)?;
    }
    menu.append(&zoom_menu).map_err(erro)?;
    menu.append(&separador().map_err(erro)?).map_err(erro)?;
    menu.append(&item("hibernar", "Hibernar agora").map_err(erro)?).map_err(erro)?;
    menu.append(&item("editar", "Editar…").map_err(erro)?).map_err(erro)?;
    menu.append(&item("remover", "Remover").map_err(erro)?).map_err(erro)?;
    janela.popup_menu(&menu).map_err(erro)
}

#[tauri::command]
fn abrir_externo(app: AppHandle, url: String) -> Res<()> {
    let u = Url::parse(&url).map_err(|_| "endereço inválido".to_string())?;
    if !matches!(u.scheme(), "https" | "http" | "mailto") {
        return Err("endereço inválido".into());
    }
    app.opener().open_url(u.as_str(), None::<&str>).map_err(erro)
}

// ---------- Contador e notificações ----------

#[tauri::command]
async fn definir_contador(app: AppHandle, total: u32, png: Option<Vec<u8>>) -> Res<()> {
    let janela = principal(&app)?;
    let icone = match &png {
        Some(bytes) if total > 0 => Some(tauri::image::Image::from_bytes(bytes).map_err(erro)?),
        _ => None,
    };
    janela.set_overlay_icon(icone).map_err(erro)?;
    let titulo = if total > 0 { format!("({total}) Ethos Connect") } else { "Ethos Connect".into() };
    janela.set_title(&titulo).map_err(erro)
}

#[tauri::command]
fn definir_notificacoes(estado: State<Estado>, ativas: bool, discretas: bool) {
    *estado.notificacoes.lock().unwrap() = Notificacoes { ativas, discretas };
}

#[tauri::command]
fn definir_notificacoes_servico(estado: State<Estado>, id: String, modo: String) -> Res<()> {
    if !matches!(modo.as_str(), "normal" | "discreta" | "desligada") {
        return Err("modo de notificação inválido".into());
    }
    estado
        .servicos
        .lock()
        .unwrap()
        .entry(id)
        .and_modify(|s| s.notificacoes = modo.clone())
        .or_insert(InfoServico { nome: String::new(), notificacoes: modo });
    Ok(())
}

fn mostrar_notificacao(app: &AppHandle, id: &str, titulo: &str, corpo: &str) {
    let estado = app.state::<Estado>();
    let (ativas, discretas_geral) = {
        let n = estado.notificacoes.lock().unwrap();
        (n.ativas, n.discretas)
    };
    let (nome, modo) = estado
        .servicos
        .lock()
        .unwrap()
        .get(id)
        .map(|s| (s.nome.clone(), s.notificacoes.clone()))
        .unwrap_or_default();
    if !ativas || modo == "desligada" {
        return;
    }
    let bloqueado = estado.bloqueio.lock().unwrap().bloqueado;
    let discreta = discretas_geral || modo == "discreta" || bloqueado;
    let nome = if nome.is_empty() { "Ethos Connect".to_string() } else { nome };
    let (t, c) = if discreta {
        (nome, "Nova mensagem".to_string())
    } else if titulo.is_empty() {
        (nome, corpo.to_string())
    } else {
        (format!("{titulo} · {nome}"), corpo.to_string())
    };
    let _ = app.notification().builder().title(t).body(c).show();
}

/// Eventos que os sites mandam pelo script injetado (injetado.js)
#[tauri::command]
fn servico_evento(app: AppHandle, webview: Webview, tipo: String, dados: Value) -> Res<()> {
    let Some(id) = webview.label().strip_prefix(PREFIXO_SERVICO).map(str::to_string) else {
        return Err("origem inválida".into());
    };
    let texto = |chave: &str| dados.get(chave).and_then(Value::as_str).unwrap_or("").to_string();
    match tipo.as_str() {
        "titulo" => {
            let _ = app.emit_to("barra", "servico-titulo", serde_json::json!({ "id": id, "titulo": texto("titulo") }));
        }
        "notificacao" => {
            let (titulo, corpo) = (texto("titulo"), texto("corpo"));
            mostrar_notificacao(&app, &id, &titulo, &corpo);
            let _ = app.emit_to("barra", "servico-notificacao", serde_json::json!({ "id": id }));
        }
        "abrir" => {
            let _ = abrir_externo(app, texto("url"));
        }
        "atalho" => {
            let _ = app.emit_to("barra", "atalho", serde_json::json!({ "acao": texto("acao") }));
        }
        "atividade" => {
            app.state::<Estado>().bloqueio.lock().unwrap().ultima_atividade = Instant::now();
        }
        "zoom" => {
            let _ = app.emit_to("barra", "zoom", serde_json::json!({ "id": id, "acao": texto("acao") }));
        }
        "diagnostico" => anotar(format!("{id}: {}", texto("texto"))),
        _ => return Err("evento desconhecido".into()),
    }
    Ok(())
}

// ---------- Bloqueio com PIN ----------

#[tauri::command]
fn estado_bloqueio(estado: State<Estado>) -> bloqueio::Estado {
    estado.bloqueio.lock().unwrap().estado()
}

#[tauri::command]
fn definir_bloqueio(estado: State<Estado>, minutos: u32) {
    let mut b = estado.bloqueio.lock().unwrap();
    b.minutos = minutos.min(24 * 60);
    b.ultima_atividade = Instant::now();
}

#[tauri::command]
fn registrar_atividade(estado: State<Estado>) {
    estado.bloqueio.lock().unwrap().ultima_atividade = Instant::now();
}

#[tauri::command]
async fn desbloquear(app: AppHandle, estado: State<'_, Estado>, pin: String) -> Res<Resposta> {
    let r = {
        let mut b = estado.bloqueio.lock().unwrap();
        let r = b.tentar(&pin);
        if r.ok {
            b.bloqueado = false;
            b.ultima_atividade = Instant::now();
        }
        r
    };
    if r.ok {
        anotar("desbloqueado");
        organizar(&app);
    }
    Ok(r)
}

#[tauri::command]
async fn definir_pin(estado: State<'_, Estado>, atual: Option<String>, novo: String) -> Res<Resposta> {
    estado.bloqueio.lock().unwrap().definir(atual.as_deref(), &novo)
}

#[tauri::command]
async fn remover_pin(app: AppHandle, estado: State<'_, Estado>, atual: String) -> Res<Resposta> {
    let r = estado.bloqueio.lock().unwrap().remover(&atual)?;
    organizar(&app);
    Ok(r)
}

#[tauri::command]
fn bloquear_agora(app: AppHandle) {
    bloquear(&app, "botão de cadeado");
}

/// Apaga tudo (logins, configurações e PIN) e fecha. A pasta do WebView2 só pode ser
/// apagada com ele fechado, então deixamos uma marca e apagamos ao abrir de novo.
#[tauri::command]
fn esqueci_pin(app: AppHandle, estado: State<Estado>) -> Res<()> {
    anotar("esqueci o PIN: apagando tudo e fechando");
    std::fs::write(estado.pasta_local.join("apagar-tudo"), b"1").map_err(erro)?;
    let _ = std::fs::remove_file(estado.pasta_config.join("pin.json"));
    encerrar_servicos(&app);
    app.exit(0);
    Ok(())
}

// ---------- Downloads ----------

#[tauri::command]
fn listar_downloads(lista: State<Downloads>) -> Vec<downloads::Download> {
    lista.copia()
}

fn caminho_download(lista: &Downloads, id: u64) -> Res<PathBuf> {
    lista.caminho_de(id).ok_or_else(|| "download não encontrado".into())
}

#[tauri::command]
fn abrir_download(app: AppHandle, lista: State<Downloads>, id: Option<u64>) -> Res<()> {
    let caminho = match id {
        Some(id) => caminho_download(&lista, id)?,
        None => downloads::pasta_downloads(&app),
    };
    if !caminho.exists() {
        return Err("O arquivo não está mais nessa pasta.".into());
    }
    app.opener().open_path(caminho.to_string_lossy(), None::<&str>).map_err(erro)
}

#[tauri::command]
fn mostrar_download(app: AppHandle, lista: State<Downloads>, id: u64) -> Res<()> {
    let caminho = caminho_download(&lista, id)?;
    if caminho.exists() {
        app.opener().reveal_item_in_dir(&caminho).map_err(erro)
    } else {
        let pasta = caminho.parent().map(PathBuf::from).unwrap_or_else(|| downloads::pasta_downloads(&app));
        app.opener().open_path(pasta.to_string_lossy(), None::<&str>).map_err(erro)
    }
}

#[tauri::command]
fn remover_download(app: AppHandle, lista: State<Downloads>, id: u64) {
    lista.remover(id);
    let _ = app.emit_to("barra", "downloads", lista.copia());
}

#[tauri::command]
fn limpar_downloads(app: AppHandle, lista: State<Downloads>) {
    lista.limpar();
    let _ = app.emit_to("barra", "downloads", lista.copia());
}

#[tauri::command]
async fn arrastar_download(app: AppHandle, lista: State<'_, Downloads>, id: u64) -> Res<()> {
    let caminho = caminho_download(&lista, id)?;
    downloads::arrastar(&app, caminho)
}

// ---------- Início ----------

/// Limpezas que só podem acontecer antes de o WebView2 abrir as pastas
fn faxina_inicial(pasta_config: &PathBuf, pasta_local: &PathBuf) {
    let marca = pasta_local.join("apagar-tudo");
    if marca.exists() {
        let _ = std::fs::remove_dir_all(pasta_local.join("EBWebView"));
        for arq in ["servicos.json", "pin.json", "downloads.json"] {
            let _ = std::fs::remove_file(pasta_config.join(arq));
        }
        let _ = std::fs::remove_file(&marca);
        anotar("esqueci o PIN: contas e configurações apagadas");
    }
    let pendentes = pasta_local.join("apagar-perfis.json");
    if let Ok(t) = std::fs::read_to_string(&pendentes) {
        let perfis: Vec<String> = serde_json::from_str(&t).unwrap_or_default();
        for p in perfis.iter().filter(|p| nome_valido(p)) {
            let _ = std::fs::remove_dir_all(pasta_local.join("EBWebView").join(format!("WV2Profile_{p}")));
            let _ = std::fs::remove_dir_all(pasta_local.join(format!("webview2-perfil={p}")));
            anotar(format!("login de \"{p}\" apagado"));
        }
        let _ = std::fs::remove_file(&pendentes);
    }
}

fn montar_janela(app: &AppHandle) -> tauri::Result<()> {
    let janela = WindowBuilder::new(app, "principal")
        .title("Ethos Connect")
        .inner_size(1280.0, 800.0)
        .min_inner_size(760.0, 480.0)
        .background_color(AZUL)
        .center()
        .build()?;

    let tamanho = janela.inner_size()?.to_logical::<f64>(janela.scale_factor()?);
    let barra = janela.add_child(
        WebviewBuilder::new("barra", WebviewUrl::App(PathBuf::from("index.html")))
            .disable_drag_drop_handler()
            .zoom_hotkeys_enabled(false)
            .background_color(AZUL),
        LogicalPosition::new(0.0, 0.0),
        tamanho,
    )?;

    // Guarda o processo do navegador para esperar ele fechar direito na saída
    let app2 = app.clone();
    let _ = barra.with_webview(move |pv| unsafe {
        if let Ok(core) = pv.controller().CoreWebView2() {
            let mut pid = 0u32;
            if core.BrowserProcessId(&mut pid).is_ok() && pid != 0 {
                *app2.state::<Estado>().pid_navegador.lock().unwrap() = Some(pid);
            }
        }
    });

    if let Ok(hwnd) = janela.hwnd() {
        sistema::ouvir_janela(app, windows::Win32::Foundation::HWND(hwnd.0 as _));
    }

    let app3 = app.clone();
    janela.on_window_event(move |evento| match evento {
        WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => organizar(&app3),
        WindowEvent::CloseRequested { .. } => encerrar_servicos(&app3),
        _ => {}
    });
    organizar(app);
    Ok(())
}

fn vigiar_inatividade(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(15));
        let estado = app.state::<Estado>();
        let (venceu, minutos) = {
            let b = estado.bloqueio.lock().unwrap();
            (b.venceu_inatividade(), b.minutos)
        };
        if venceu {
            let a = app.clone();
            let _ = app.run_on_main_thread(move || bloquear(&a, &format!("{minutos} min sem uso")));
        }
    });
}

#[derive(Serialize, Clone)]
struct ItemMenu<'a> {
    acao: &'a str,
    id: &'a str,
}

pub fn executar() {
    std::panic::set_hook(Box::new(|info| {
        sistema::erro_fatal(&info.to_string());
    }));

    let resultado = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let pasta_config = app.path().app_config_dir()?;
            let pasta_local = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&pasta_config)?;
            std::fs::create_dir_all(&pasta_local)?;
            diario::iniciar(pasta_local.clone());

            match sistema::versao_webview2() {
                Some(v) => anotar(format!("WebView2 {v}")),
                None => {
                    anotar("WebView2 AUSENTE: pedindo para instalar");
                    sistema::avisar_sem_webview2();
                    std::process::exit(1);
                }
            }
            anotar(format!("iniciado: versão {}", app.package_info().version));
            faxina_inicial(&pasta_config, &pasta_local);

            app.manage(Downloads::carregar(&pasta_config));
            app.manage(Estado {
                tela: Mutex::new(Tela { expandida: true, ..Default::default() }),
                servicos: Mutex::new(HashMap::new()),
                notificacoes: Mutex::new(Notificacoes { ativas: true, discretas: false }),
                bloqueio: Mutex::new(Bloqueio::carregar(&pasta_config)),
                pasta_config,
                pasta_local,
                pid_navegador: Mutex::new(None),
                encerrado: Mutex::new(false),
            });

            let handle = app.handle().clone();
            montar_janela(&handle)?;
            vigiar_inatividade(handle.clone());
            #[cfg(not(debug_assertions))]
            atualizacao::iniciar(handle.clone());
            #[cfg(debug_assertions)]
            let _ = atualizacao::iniciar;
            Ok(())
        })
        .on_menu_event(|app, evento| {
            let texto = evento.id().as_ref().to_string();
            if let Some((acao, id)) = texto.split_once(':') {
                let _ = app.emit_to("barra", "menu-servico", ItemMenu { acao, id });
            }
        })
        .invoke_handler(tauri::generate_handler![
            abrir_externo,
            abrir_servico,
            bloquear_agora,
            carregar_config,
            definir_bloqueio,
            definir_contador,
            definir_notificacoes,
            definir_notificacoes_servico,
            definir_pin,
            definir_zoom,
            desbloquear,
            esqueci_pin,
            estado_bloqueio,
            hibernar_servico,
            menu_servico,
            mostrar_servico,
            ocultar_barra,
            recarregar_servico,
            registrar_atividade,
            remover_pin,
            remover_servico,
            salvar_config,
            servico_evento,
            ui_expandir,
            listar_downloads,
            abrir_download,
            mostrar_download,
            remover_download,
            limpar_downloads,
            arrastar_download,
        ])
        .build(tauri::generate_context!());

    let app = match resultado {
        Ok(app) => app,
        Err(e) => {
            sistema::erro_fatal(&e.to_string());
            std::process::exit(1);
        }
    };
    app.run(|app, evento| {
        if let RunEvent::Exit = evento {
            encerrar_servicos(app);
            esperar_navegador(app);
        }
    });
}
