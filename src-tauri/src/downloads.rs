//! Área de downloads: guarda o que foi baixado pelos serviços (WhatsApp, Gmail...)
//! para abrir, mostrar na pasta ou arrastar para fora sem procurar no computador.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Url};

use crate::diario::anotar;

const MAXIMO: usize = 300;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub id: u64,
    pub nome: String,
    pub caminho: String,
    pub url: String,
    /// "baixando", "concluido" ou "falhou"
    pub estado: String,
    pub tamanho: u64,
    /// milissegundos desde 1970
    pub quando: i64,
    /// id do serviço que baixou
    pub servico: String,
}

pub struct Downloads {
    caminho: PathBuf,
    pub lista: Mutex<Vec<Download>>,
}

impl Downloads {
    pub fn carregar(pasta_config: &Path) -> Self {
        let caminho = pasta_config.join("downloads.json");
        let mut lista: Vec<Download> = std::fs::read_to_string(&caminho)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        // O que estava baixando quando o app fechou não terminou
        for d in lista.iter_mut().filter(|d| d.estado == "baixando") {
            d.estado = "falhou".into();
        }
        Downloads { caminho, lista: Mutex::new(lista) }
    }

    fn salvar(&self, lista: &[Download]) {
        if let Ok(t) = serde_json::to_string_pretty(lista) {
            let _ = std::fs::write(&self.caminho, t);
        }
    }

    pub fn copia(&self) -> Vec<Download> {
        let mut v = self.lista.lock().unwrap().clone();
        // Arquivo apagado ou movido fora do app
        for d in v.iter_mut() {
            if d.estado == "concluido" && !Path::new(&d.caminho).exists() {
                d.estado = "sumiu".into();
            }
        }
        v
    }

    pub fn caminho_de(&self, id: u64) -> Option<PathBuf> {
        self.lista.lock().unwrap().iter().find(|d| d.id == id).map(|d| PathBuf::from(&d.caminho))
    }

    pub fn remover(&self, id: u64) {
        let mut l = self.lista.lock().unwrap();
        l.retain(|d| d.id != id);
        self.salvar(&l);
    }

    /// Tira da lista o que não está baixando (os arquivos continuam no computador)
    pub fn limpar(&self) {
        let mut l = self.lista.lock().unwrap();
        l.retain(|d| d.estado == "baixando");
        self.salvar(&l);
    }
}

/// Pasta Downloads do Windows (com fallback para a pasta do usuário)
pub fn pasta_downloads(app: &AppHandle) -> PathBuf {
    app.path()
        .download_dir()
        .or_else(|_| app.path().home_dir().map(|h| h.join("Downloads")))
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// Nome livre na pasta: "foto.jpg", "foto (1).jpg", "foto (2).jpg"...
fn nome_livre(pasta: &Path, nome: &str) -> PathBuf {
    let caminho = pasta.join(nome);
    if !caminho.exists() {
        return caminho;
    }
    let p = Path::new(nome);
    let base = p.file_stem().and_then(|s| s.to_str()).unwrap_or("arquivo");
    let ext = p.extension().and_then(|s| s.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    (1..10_000)
        .map(|i| pasta.join(format!("{base} ({i}){ext}")))
        .find(|c| !c.exists())
        .unwrap_or(caminho)
}

fn limpar_nome(nome: &str) -> String {
    let n: String = nome
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    let n = n.trim().trim_end_matches('.').to_string();
    if n.is_empty() { "arquivo".into() } else { n }
}

fn avisar_lista(app: &AppHandle) {
    let lista = app.state::<Downloads>().copia();
    let _ = app.emit_to("barra", "downloads", lista);
}

/// Começo de um download: decide onde salvar e já mostra na lista como "baixando"
pub fn pedido(app: &AppHandle, servico: &str, url: &Url, destino: &mut PathBuf) {
    let pasta = destino
        .parent()
        .filter(|p| !p.as_os_str().is_empty() && p.exists())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| pasta_downloads(app));
    let nome_sugerido = destino
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .or_else(|| url.path_segments().and_then(|mut s| s.next_back().map(str::to_string)))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "arquivo".into());
    *destino = nome_livre(&pasta, &limpar_nome(&nome_sugerido));

    let estado = app.state::<Downloads>();
    {
        let mut l = estado.lista.lock().unwrap();
        let id = l.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        l.insert(
            0,
            Download {
                id,
                nome: destino.file_name().and_then(|n| n.to_str()).unwrap_or("arquivo").to_string(),
                caminho: destino.to_string_lossy().into_owned(),
                url: if url.scheme() == "data" || url.scheme() == "blob" { String::new() } else { url.to_string() },
                estado: "baixando".into(),
                tamanho: 0,
                quando: chrono::Local::now().timestamp_millis(),
                servico: servico.to_string(),
            },
        );
        l.truncate(MAXIMO);
        estado.salvar(&l);
    }
    avisar_lista(app);
}

/// Fim de um download (deu certo ou não)
pub fn terminou(app: &AppHandle, url: &Url, caminho: Option<PathBuf>, sucesso: bool) -> Option<Download> {
    let estado = app.state::<Downloads>();
    let mut concluido: Option<Download> = None;
    {
        let mut l = estado.lista.lock().unwrap();
        let url_txt = url.to_string();
        let caminho_txt = caminho.as_ref().map(|c| c.to_string_lossy().into_owned());
        let alvo = l.iter().position(|d| {
            d.estado == "baixando" && (Some(&d.caminho) == caminho_txt.as_ref() || d.url == url_txt)
        });
        if let Some(i) = alvo {
            let d = &mut l[i];
            if let Some(c) = &caminho_txt {
                d.caminho = c.clone();
                d.nome = Path::new(c).file_name().and_then(|n| n.to_str()).unwrap_or(&d.nome).to_string();
            }
            d.tamanho = std::fs::metadata(&d.caminho).map(|m| m.len()).unwrap_or(0);
            d.estado = if sucesso { "concluido".into() } else { "falhou".into() };
            if sucesso {
                concluido = Some(d.clone());
            }
        }
        estado.salvar(&l);
    }
    match &concluido {
        Some(d) => {
            anotar(format!("download concluído: {}", d.nome));
            let _ = app.emit_to("barra", "download-concluido", d.clone());
        }
        None if !sucesso => anotar("download falhou ou foi cancelado"),
        None => {}
    }
    avisar_lista(app);
    concluido
}

/// Arrasta um arquivo baixado para fora do app (Área de Trabalho, Explorer, e-mail...)
pub fn arrastar(app: &AppHandle, caminho: PathBuf) -> Result<(), String> {
    if !caminho.exists() {
        return Err("O arquivo não está mais nessa pasta.".into());
    }
    let janela = app.get_window("principal").ok_or("janela principal não encontrada")?;
    let icone = include_bytes!("../icons/64x64.png").to_vec();
    let (tx, rx) = std::sync::mpsc::channel();
    let j = janela.clone();
    janela
        .run_on_main_thread(move || {
            let r = drag::start_drag(
                &j,
                drag::DragItem::Files(vec![caminho]),
                drag::Image::Raw(icone),
                |_, _| {},
                drag::Options::default(),
            );
            let _ = tx.send(r.map_err(|e| e.to_string()));
        })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}
