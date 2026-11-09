//! Diário de diagnóstico: logs/diagnostico.log na pasta local do app.
//! Uma linha por acontecimento importante, para entender problemas de quem usa o app.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static ARQUIVO: OnceLock<PathBuf> = OnceLock::new();
static TRAVA: Mutex<()> = Mutex::new(());

/// Tamanho máximo antes de cortar a metade mais antiga
const LIMITE: u64 = 512 * 1024;

pub fn iniciar(pasta_local: PathBuf) {
    let pasta = pasta_local.join("logs");
    let _ = std::fs::create_dir_all(&pasta);
    let _ = ARQUIVO.set(pasta.join("diagnostico.log"));
}

pub fn anotar(texto: impl AsRef<str>) {
    let Some(caminho) = ARQUIVO.get() else { return };
    let _trava = TRAVA.lock();
    if std::fs::metadata(caminho).map(|m| m.len() > LIMITE).unwrap_or(false) {
        if let Ok(todo) = std::fs::read_to_string(caminho) {
            let metade = todo.len() / 2;
            let corte = todo[metade..].find('\n').map(|i| metade + i + 1).unwrap_or(metade);
            let _ = std::fs::write(caminho, &todo[corte..]);
        }
    }
    let linha = format!("{} {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), texto.as_ref());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(caminho) {
        let _ = f.write_all(linha.as_bytes());
    }
    #[cfg(debug_assertions)]
    eprint!("{linha}");
}
