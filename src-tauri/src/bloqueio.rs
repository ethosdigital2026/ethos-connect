//! Bloqueio com PIN: guarda o PIN (só o hash), conta tentativas erradas e decide
//! quando bloquear (ao abrir, por inatividade, Win+L e volta da suspensão).

use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::diario::anotar;

/// Tentativas erradas liberadas antes de começar a fazer esperar
const TENTATIVAS_LIVRES: u32 = 5;
const ESPERA_INICIAL: u64 = 30;
const ESPERA_MAXIMA: u64 = 15 * 60;
const ITERACOES: u32 = 200_000;

#[derive(Serialize, Deserialize, Clone)]
pub struct ArquivoPin {
    pub versao: u32,
    pub sal: String,
    pub iteracoes: u32,
    pub hash: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Resposta {
    pub ok: bool,
    pub espera_segundos: u64,
    pub restantes: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Estado {
    pub pin_definido: bool,
    pub bloqueado: bool,
}

pub struct Bloqueio {
    caminho: PathBuf,
    pub pin: Option<ArquivoPin>,
    pub bloqueado: bool,
    pub minutos: u32,
    pub ultima_atividade: Instant,
    falhas: u32,
    espera_ate: Option<Instant>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn de_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn hmac_sha256(chave: &[u8], dados: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if chave.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(chave));
    } else {
        k[..chave.len()].copy_from_slice(chave);
    }
    let mut interno = Sha256::new();
    interno.update(k.map(|b| b ^ 0x36));
    interno.update(dados);
    let mut externo = Sha256::new();
    externo.update(k.map(|b| b ^ 0x5c));
    externo.update(interno.finalize());
    externo.finalize().into()
}

/// PBKDF2-HMAC-SHA256 com 32 bytes de saída (formato versão 2)
fn pbkdf2(pin: &[u8], sal: &[u8], iteracoes: u32) -> [u8; 32] {
    let mut bloco = sal.to_vec();
    bloco.extend_from_slice(&1u32.to_be_bytes());
    let mut u = hmac_sha256(pin, &bloco);
    let mut t = u;
    for _ in 1..iteracoes {
        u = hmac_sha256(pin, &u);
        for (a, b) in t.iter_mut().zip(u.iter()) {
            *a ^= b;
        }
    }
    t
}

fn sha_iterado(inicio: &[u8], iteracoes: u32, mistura: &dyn Fn(&[u8; 32]) -> Vec<u8>) -> [u8; 32] {
    let mut h: [u8; 32] = Sha256::digest(inicio).into();
    for _ in 1..iteracoes {
        h = Sha256::digest(mistura(&h)).into();
    }
    h
}

/// O formato da versão 1 (antes da reconstrução do código) não está documentado.
/// Testamos as combinações mais prováveis; a que bater converte o arquivo para a versão 2.
fn candidatos_v1(pin: &str, arq: &ArquivoPin) -> Vec<[u8; 32]> {
    let sal = de_hex(&arq.sal).unwrap_or_default();
    let sal_txt = arq.sal.as_bytes();
    let p = pin.as_bytes();
    let n = arq.iteracoes.max(1);
    let junta = |a: &[u8], b: &[u8]| [a, b].concat();
    let mut v = vec![
        pbkdf2(p, &sal, n),
        pbkdf2(p, sal_txt, n),
        sha_iterado(&junta(&sal, p), n, &|h| h.to_vec()),
        sha_iterado(&junta(p, &sal), n, &|h| h.to_vec()),
        sha_iterado(&junta(sal_txt, p), n, &|h| h.to_vec()),
        sha_iterado(&junta(p, sal_txt), n, &|h| h.to_vec()),
        sha_iterado(&junta(&sal, p), n, &|h| junta(h, &sal)),
        sha_iterado(&junta(&sal, p), n, &|h| junta(&sal, h)),
        sha_iterado(&junta(&sal, p), n, &|h| [h.as_slice(), &sal, p].concat()),
        sha_iterado(&junta(&sal, p), n, &|h| [&sal, h.as_slice(), p].concat()),
        sha_iterado(&junta(&sal, p), n, &|h| junta(h, p)),
    ];
    // Variação com o hash em texto (hex) a cada rodada
    let mut h = hex(&Sha256::digest(junta(&sal, p)));
    for _ in 1..n {
        h = hex(&Sha256::digest(h.as_bytes()));
    }
    v.push(de_hex(&h).unwrap_or_default().try_into().unwrap_or([0; 32]));
    v
}

fn iguais(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Bloqueio {
    pub fn carregar(pasta_config: &Path) -> Self {
        let caminho = pasta_config.join("pin.json");
        let pin = match std::fs::read_to_string(&caminho) {
            Ok(t) => match serde_json::from_str::<ArquivoPin>(&t) {
                Ok(p) => Some(p),
                Err(_) => {
                    anotar("arquivo do PIN inválido");
                    None
                }
            },
            Err(_) => None,
        };
        let bloqueado = pin.is_some();
        Bloqueio {
            caminho,
            pin,
            bloqueado,
            minutos: 0,
            ultima_atividade: Instant::now(),
            falhas: 0,
            espera_ate: None,
        }
    }

    pub fn estado(&self) -> Estado {
        Estado { pin_definido: self.pin.is_some(), bloqueado: self.bloqueado }
    }

    fn espera_restante(&self) -> u64 {
        self.espera_ate
            .map(|t| t.saturating_duration_since(Instant::now()).as_secs())
            .filter(|s| *s > 0)
            .map(|s| s + 1)
            .unwrap_or(0)
    }

    fn gravar(&mut self, pin: &str) -> Result<(), String> {
        let mut sal = [0u8; 16];
        rand::rng().fill_bytes(&mut sal);
        let arq = ArquivoPin {
            versao: 2,
            sal: hex(&sal),
            iteracoes: ITERACOES,
            hash: hex(&pbkdf2(pin.as_bytes(), &sal, ITERACOES)),
        };
        let texto = serde_json::to_string(&arq).map_err(|e| e.to_string())?;
        std::fs::write(&self.caminho, texto).map_err(|e| format!("não foi possível salvar o PIN: {e}"))?;
        self.pin = Some(arq);
        Ok(())
    }

    fn confere(&mut self, pin: &str) -> bool {
        let Some(arq) = self.pin.clone() else { return false };
        let Some(esperado) = de_hex(&arq.hash) else { return false };
        if arq.versao >= 2 {
            let sal = de_hex(&arq.sal).unwrap_or_default();
            return iguais(&pbkdf2(pin.as_bytes(), &sal, arq.iteracoes), &esperado);
        }
        let bateu = candidatos_v1(pin, &arq).iter().any(|c| iguais(c, &esperado));
        if bateu {
            anotar("PIN antigo conferido: convertendo para o formato novo");
            let _ = self.gravar(pin);
        }
        bateu
    }

    /// Confere o PIN contando tentativas. Sem PIN definido, qualquer coisa passa.
    pub fn tentar(&mut self, pin: &str) -> Resposta {
        if self.pin.is_none() {
            return Resposta { ok: true, ..Default::default() };
        }
        let espera = self.espera_restante();
        if espera > 0 {
            return Resposta { ok: false, espera_segundos: espera, restantes: 0 };
        }
        if self.confere(pin) {
            self.falhas = 0;
            self.espera_ate = None;
            return Resposta { ok: true, ..Default::default() };
        }
        self.falhas += 1;
        anotar(format!("PIN errado (erro nº {})", self.falhas));
        if self.falhas >= TENTATIVAS_LIVRES {
            let rodada = (self.falhas - TENTATIVAS_LIVRES).min(10);
            let segundos = (ESPERA_INICIAL << rodada).min(ESPERA_MAXIMA);
            self.espera_ate = Some(Instant::now() + Duration::from_secs(segundos));
            Resposta { ok: false, espera_segundos: segundos, restantes: 0 }
        } else {
            Resposta { ok: false, espera_segundos: 0, restantes: TENTATIVAS_LIVRES - self.falhas }
        }
    }

    pub fn definir(&mut self, atual: Option<&str>, novo: &str) -> Result<Resposta, String> {
        if novo.len() < 4 || novo.len() > 8 || !novo.chars().all(|c| c.is_ascii_digit()) {
            return Err("O PIN precisa ter de 4 a 8 números.".into());
        }
        if self.pin.is_some() {
            let r = self.tentar(atual.unwrap_or(""));
            if !r.ok {
                return Ok(r);
            }
        }
        self.gravar(novo)?;
        anotar("PIN definido");
        Ok(Resposta { ok: true, ..Default::default() })
    }

    pub fn remover(&mut self, atual: &str) -> Result<Resposta, String> {
        let r = self.tentar(atual);
        if !r.ok {
            return Ok(r);
        }
        let _ = std::fs::remove_file(&self.caminho);
        self.pin = None;
        self.bloqueado = false;
        anotar("PIN removido");
        Ok(r)
    }

    /// Hora de bloquear por falta de uso?
    pub fn venceu_inatividade(&self) -> bool {
        self.pin.is_some()
            && !self.bloqueado
            && self.minutos > 0
            && self.ultima_atividade.elapsed() >= Duration::from_secs(self.minutos as u64 * 60)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn pbkdf2_vetor_conhecido() {
        // RFC 7914 / vetores públicos de PBKDF2-HMAC-SHA256
        let r = pbkdf2(b"password", b"salt", 1);
        assert_eq!(hex(&r), "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b");
        let r = pbkdf2(b"password", b"salt", 2);
        assert_eq!(hex(&r), "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43");
    }

    #[test]
    fn pin_novo_confere_e_conta_erros() {
        let pasta = std::env::temp_dir().join(format!("ethos-teste-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        let mut b = Bloqueio::carregar(&pasta);
        assert!(b.definir(None, "12a4").is_err());
        assert!(b.definir(None, "1234").unwrap().ok);
        let mut b = Bloqueio::carregar(&pasta);
        assert!(b.bloqueado);
        assert_eq!(b.tentar("0000").restantes, 4);
        assert!(b.tentar("1234").ok);
        for _ in 0..4 {
            b.tentar("9999");
        }
        let r = b.tentar("9999");
        assert!(!r.ok && r.espera_segundos == 30);
        assert!(!b.tentar("1234").ok, "durante a espera nem o PIN certo passa");
        std::fs::remove_dir_all(&pasta).ok();
    }
}
