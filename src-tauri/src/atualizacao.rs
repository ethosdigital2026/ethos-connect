//! Atualização automática pelos releases do GitHub (plugin updater).
//! Confere ao abrir e a cada 6 horas; se houver versão nova, baixa, instala e reabre.

use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

use crate::diario::anotar;

pub fn iniciar(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio_sleep(Duration::from_secs(45)).await;
        loop {
            verificar(&app).await;
            tokio_sleep(Duration::from_secs(6 * 60 * 60)).await;
        }
    });
}

async fn tokio_sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

async fn verificar(app: &AppHandle) {
    let atualizador = match app.updater() {
        Ok(a) => a,
        Err(e) => {
            anotar(format!("atualização: não foi possível verificar ({e})"));
            return;
        }
    };
    let atualizacao = match atualizador.check().await {
        Ok(Some(a)) => a,
        Ok(None) => return,
        Err(e) => {
            anotar(format!("atualização: não foi possível verificar ({e})"));
            return;
        }
    };
    let versao = atualizacao.version.clone();
    anotar(format!("atualização: baixando a versão {versao}"));
    let bytes = match atualizacao.download(|_, _| {}, || {}).await {
        Ok(b) => b,
        Err(e) => {
            anotar(format!("atualização: falhou ao baixar ({e})"));
            return;
        }
    };
    let _ = app
        .notification()
        .builder()
        .title("Ethos Connect está atualizando")
        .body(format!("Instalando a versão {versao}. O app reabre sozinho em alguns segundos."))
        .show();
    anotar(format!("atualização: instalando a versão {versao}"));
    crate::encerrar_servicos(app);
    if let Err(e) = atualizacao.install(bytes) {
        anotar(format!("atualização: falhou ao instalar ({e})"));
        // Os serviços já tinham sido fechados: recarrega a barra, que abre tudo de novo
        crate::reabrir_servicos(app);
        return;
    }
    app.restart();
}
