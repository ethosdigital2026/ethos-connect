# Ethos Connect

WhatsApp, WhatsApp Business e outros serviços em um só lugar (Tauri 2 + WebView2).

Código reconstruído em 09/11/2026 a partir do executável da versão 0.2.1 (o original se perdeu).

## Rodar e compilar
- Requisitos: Node, Rust (rustup) e Visual Studio Build Tools com C++.
- Testar sem mexer nos logins reais:
  `npx tauri dev --config '{"identifier":"br.com.ethosdigital.connect.teste"}'`
- Instalador: defina `TAURI_SIGNING_PRIVATE_KEY` com o caminho de `%USERPROFILE%\.tauri\ethos-connect.key` e rode `npx tauri build`.

## Importante
- A chave `ethos-connect.key` assina as atualizações. Guarde uma cópia fora deste PC e nunca a suba no Git.
- `src-tauri/patches/tauri-runtime-wry`: modificação que transforma `webview2-perfil=<nome>` em perfil do WebView2 (um login por serviço).
