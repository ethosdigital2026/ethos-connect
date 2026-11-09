const COMANDOS: &[&str] = &[
    "abrir_externo", "abrir_servico", "bloquear_agora", "carregar_config", "definir_bloqueio",
    "definir_contador", "definir_notificacoes", "definir_notificacoes_servico", "definir_pin",
    "definir_zoom", "desbloquear", "esqueci_pin", "estado_bloqueio", "hibernar_servico",
    "menu_servico", "mostrar_servico", "ocultar_barra", "recarregar_servico", "registrar_atividade",
    "remover_pin", "remover_servico", "salvar_config", "servico_evento", "ui_expandir",
    // Área de downloads
    "listar_downloads", "abrir_download", "mostrar_download", "remover_download",
    "limpar_downloads", "arrastar_download",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMANDOS)),
    )
    .expect("falha no build do Tauri");
}
