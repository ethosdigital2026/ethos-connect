//! Avisos do Windows que o app precisa ouvir: Win+L, volta da suspensão e desligamento.
//! Também confere se o WebView2 está instalado e mostra erros fatais numa caixa.

use std::sync::OnceLock;
use tauri::AppHandle;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_YESNO, WM_ENDSESSION, WM_POWERBROADCAST,
    WM_WTSSESSION_CHANGE,
};

use crate::diario::anotar;

static APP: OnceLock<AppHandle> = OnceLock::new();

const WTS_SESSION_LOCK: usize = 0x7;
const PBT_APMRESUMESUSPEND: usize = 0x7;
const PBT_APMRESUMEAUTOMATIC: usize = 0x12;

pub fn ouvir_janela(app: &AppHandle, hwnd: HWND) {
    let _ = APP.set(app.clone());
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(procedimento), 0xE7_05, 0);
        let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
    }
}

unsafe extern "system" fn procedimento(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _dados: usize,
) -> LRESULT {
    if let Some(app) = APP.get() {
        match msg {
            WM_WTSSESSION_CHANGE if wparam.0 == WTS_SESSION_LOCK => {
                crate::bloquear(app, "o Windows foi bloqueado");
            }
            WM_POWERBROADCAST if wparam.0 == PBT_APMRESUMEAUTOMATIC || wparam.0 == PBT_APMRESUMESUSPEND => {
                crate::bloquear(app, "o computador voltou da suspensão");
            }
            WM_ENDSESSION if wparam.0 != 0 => {
                anotar("Windows desligando ou reiniciando: fechando com segurança");
                crate::encerrar_servicos(app);
            }
            _ => {}
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// Versão do WebView2 instalada, ou None se faltar
pub fn versao_webview2() -> Option<String> {
    let mut versao = PWSTR::null();
    unsafe {
        webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString(
            PCWSTR::null(),
            &mut versao,
        )
        .ok()?;
        if versao.is_null() {
            return None;
        }
        let texto = versao.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(versao.0 as _));
        texto
    }
}

fn caixa(titulo: &str, texto: &str, estilo: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE) -> bool {
    unsafe { MessageBoxW(None, &HSTRING::from(texto), &HSTRING::from(titulo), estilo) == IDYES }
}

pub fn avisar_sem_webview2() {
    let abrir = caixa(
        "Ethos Connect",
        "Falta um componente do Windows para o Ethos Connect funcionar: o \"Microsoft Edge WebView2 Runtime\".\n\n\
         Clique em Sim para abrir a página oficial da Microsoft, baixe e instale o componente, e depois abra o Ethos Connect de novo.",
        MB_YESNO | MB_ICONWARNING,
    );
    if abrir {
        let _ = tauri_plugin_opener::open_url("https://go.microsoft.com/fwlink/p/?LinkId=2124703", None::<&str>);
    }
}

pub fn erro_fatal(erro: &str) {
    anotar(format!("ERRO ao executar: {erro}"));
    let mut texto = format!(
        "O Ethos Connect encontrou um erro e precisa fechar.\n\n{erro}\n\nSe repetir, mande uma foto desta mensagem para quem cuida do app."
    );
    if erro.contains("WebView2") || erro.contains("0x80070002") {
        texto.push_str(
            "\n\nSe o problema continuar, instale ou atualize o \"Microsoft Edge WebView2 Runtime\". Clique em Sim para abrir a página oficial da Microsoft.",
        );
        if caixa("Ethos Connect: erro", &texto, MB_YESNO | MB_ICONERROR) {
            let _ = tauri_plugin_opener::open_url("https://go.microsoft.com/fwlink/p/?LinkId=2124703", None::<&str>);
        }
    } else {
        caixa("Ethos Connect: erro", &texto, MB_OK | MB_ICONERROR);
    }
}
