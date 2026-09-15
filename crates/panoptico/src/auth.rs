//! Porta do panoptico: senha, sessao em cookie e freio de tentativas.
//!
//! O painel mostra conta, ouro e posicao de todo mundo. Na internet ele fica
//! atras do nginx (HTTPS) e do tunel, e mesmo assim nada passa sem sessao:
//!
//! * **Senha** de `PANOPTICO_SENHA` (16+ chars), comparada em tempo constante.
//! * **Sessao** aleatoria de 256 bits (`/dev/urandom`), em cookie `HttpOnly`,
//!   `SameSite=Strict`, `Secure` (desligavel so' pra teste local), no caminho
//!   do prefixo, valendo 12 h. Nada de token na URL: URL vai pra log, pra
//!   historico e pro `Referer`.
//! * **Freio**: 5 senhas erradas em 15 min travam o IP ate' a janela passar.
//!   Atras de proxy o IP vem do `X-Real-IP`/`X-Forwarded-For` — so' com
//!   `PANOPTICO_CONFIAR_PROXY=1`, porque sem proxy qualquer um forja o cabecalho.

use std::collections::HashMap;
use std::io::Read;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use parking_lot::Mutex;

pub const VALIDADE: Duration = Duration::from_secs(12 * 3600);
pub const JANELA_DE_FALHAS: Duration = Duration::from_secs(15 * 60);
pub const MAX_FALHAS: u32 = 5;
pub const NOME_DO_COOKIE: &str = "panoptico";
/// Teto de sessoes vivas: cada login cria uma, e ninguem precisa de mil.
const MAX_SESSOES: usize = 256;

pub struct Auth {
    senha: String,
    /// Prefixo do caminho ("" ou "/panoptico"), sem barra no fim.
    pub prefixo: String,
    seguro: bool,
    pub confiar_proxy: bool,
    sessoes: Mutex<HashMap<String, Instant>>,
    falhas: Mutex<HashMap<String, (u32, Instant)>>,
}

/// "" ou "/x/y", sem barra no fim.
pub fn normaliza_prefixo(p: &str) -> String {
    let t = p.trim().trim_matches('/');
    if t.is_empty() {
        String::new()
    } else {
        format!("/{t}")
    }
}

impl Auth {
    pub fn novo(senha: String, prefixo: &str, seguro: bool, confiar_proxy: bool) -> anyhow::Result<Auth> {
        if senha.chars().count() < 16 {
            anyhow::bail!(
                "PANOPTICO_SENHA precisa de pelo menos 16 caracteres — este painel mostra conta, ouro e posicao de todo mundo"
            );
        }
        // Falha cedo se nao ha' fonte de aleatoriedade: sessao previsivel e'
        // o mesmo que nenhuma.
        aleatorio_hex(8)?;
        Ok(Auth {
            senha,
            prefixo: normaliza_prefixo(prefixo),
            seguro,
            confiar_proxy,
            sessoes: Mutex::new(HashMap::new()),
            falhas: Mutex::new(HashMap::new()),
        })
    }

    /// Caminho absoluto dentro do prefixo: `caminho("/api/x")`.
    pub fn caminho(&self, c: &str) -> String {
        format!("{}{c}", self.prefixo)
    }

    pub fn confere(&self, tentativa: &str) -> bool {
        iguais_em_tempo_constante(tentativa.as_bytes(), self.senha.as_bytes())
    }

    pub fn bloqueado(&self, ip: &str, agora: Instant) -> bool {
        let mut g = self.falhas.lock();
        match g.get(ip) {
            Some((n, desde)) if agora.duration_since(*desde) < JANELA_DE_FALHAS => *n >= MAX_FALHAS,
            Some(_) => {
                g.remove(ip);
                false
            }
            None => false,
        }
    }

    pub fn falhou(&self, ip: &str, agora: Instant) {
        let mut g = self.falhas.lock();
        // Nao deixa o mapa crescer com IPs velhos.
        g.retain(|_, (_, desde)| agora.duration_since(*desde) < JANELA_DE_FALHAS);
        let e = g.entry(ip.to_string()).or_insert((0, agora));
        e.0 += 1;
    }

    pub fn limpar_falhas(&self, ip: &str) {
        self.falhas.lock().remove(ip);
    }

    pub fn nova_sessao(&self, agora: Instant) -> anyhow::Result<String> {
        let token = aleatorio_hex(32)?;
        let mut g = self.sessoes.lock();
        g.retain(|_, expira| *expira > agora);
        if g.len() >= MAX_SESSOES {
            // Derruba a que expira primeiro.
            if let Some(velha) = g.iter().min_by_key(|(_, e)| **e).map(|(k, _)| k.clone()) {
                g.remove(&velha);
            }
        }
        g.insert(token.clone(), agora + VALIDADE);
        Ok(token)
    }

    pub fn valida(&self, token: &str, agora: Instant) -> bool {
        let mut g = self.sessoes.lock();
        match g.get(token) {
            Some(expira) if *expira > agora => true,
            Some(_) => {
                g.remove(token);
                false
            }
            None => false,
        }
    }

    pub fn encerrar(&self, token: &str) {
        self.sessoes.lock().remove(token);
    }

    fn atributos(&self) -> String {
        let caminho = if self.prefixo.is_empty() { "/".to_string() } else { format!("{}/", self.prefixo) };
        format!("Path={caminho}; HttpOnly; SameSite=Strict{}", if self.seguro { "; Secure" } else { "" })
    }

    pub fn cookie(&self, token: &str) -> String {
        format!("{NOME_DO_COOKIE}={token}; Max-Age={}; {}", VALIDADE.as_secs(), self.atributos())
    }

    pub fn cookie_apagar(&self) -> String {
        format!("{NOME_DO_COOKIE}=; Max-Age=0; {}", self.atributos())
    }
}

/// Compara sem sair cedo: o tempo nao diz quantos bytes bateram.
pub fn iguais_em_tempo_constante(a: &[u8], b: &[u8]) -> bool {
    let mut dif = (a.len() ^ b.len()) as u8 | ((a.len() ^ b.len()) >> 8) as u8;
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        dif |= x ^ y;
    }
    dif == 0 && a.len() == b.len()
}

/// `n` bytes aleatorios do sistema, em hexadecimal.
pub fn aleatorio_hex(n: usize) -> anyhow::Result<String> {
    let mut buf = vec![0u8; n];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// O valor do cookie de sessao, se veio.
pub fn token_do_cookie(headers: &HeaderMap) -> Option<String> {
    for v in headers.get_all(axum::http::header::COOKIE) {
        let Ok(s) = v.to_str() else { continue };
        for par in s.split(';') {
            if let Some((k, val)) = par.trim().split_once('=') {
                if k == NOME_DO_COOKIE && !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// De onde veio o pedido. Cabecalho de proxy so' vale com `confiar_proxy`.
pub fn ip_do_pedido(headers: &HeaderMap, par: std::net::SocketAddr, confiar_proxy: bool) -> String {
    if confiar_proxy {
        let cab = |n: &str| headers.get(n).and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty());
        if let Some(ip) = cab("x-real-ip") {
            return ip.to_string();
        }
        if let Some(lista) = cab("x-forwarded-for") {
            if let Some(primeiro) = lista.split(',').next() {
                return primeiro.trim().to_string();
            }
        }
    }
    par.ip().to_string()
}

/// A pagina de entrada. `erro` e' texto fixo nosso, nunca eco do pedido.
pub fn pagina_de_login(acao: &str, erro: Option<&str>) -> String {
    let aviso = erro.map_or(String::new(), |e| format!("<p class=\"erro\">{e}</p>"));
    format!(
        r#"<!doctype html>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Panóptico — entrar</title>
<style>
  body {{ margin:0; min-height:100vh; display:grid; place-items:center; background:#0d1014; color:#d8e0ea;
         font:14px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace; }}
  form {{ background:#151a21; border:1px solid #232b35; border-radius:6px; padding:28px; width:min(340px, 90vw); }}
  h1 {{ font-size:13px; text-transform:uppercase; letter-spacing:.14em; color:#7c8ba0; margin:0 0 18px; }}
  input {{ width:100%; box-sizing:border-box; background:#0d1014; border:1px solid #232b35; color:#d8e0ea;
          padding:9px 10px; font:inherit; border-radius:4px; }}
  button {{ margin-top:12px; width:100%; background:#1e2a38; color:#d8e0ea; border:1px solid #3d4a5a;
           padding:9px; font:inherit; border-radius:4px; cursor:pointer; }}
  button:hover {{ background:#26364a; }}
  .erro {{ color:#e06a5a; margin:12px 0 0; }}
</style>
<form method="post" action="{acao}">
  <h1>Panóptico · Tempest</h1>
  <input type="password" name="senha" placeholder="senha" autocomplete="current-password" autofocus required>
  <button type="submit">entrar</button>
  {aviso}
</form>"#
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    fn auth() -> Auth {
        Auth::novo("uma-senha-bem-comprida".into(), "/panoptico/", true, false).unwrap()
    }

    #[test]
    fn senha_curta_nao_sobe() {
        assert!(Auth::novo("curta".into(), "", true, false).is_err());
    }

    #[test]
    fn compara_certo() {
        assert!(iguais_em_tempo_constante(b"abc", b"abc"));
        assert!(!iguais_em_tempo_constante(b"abc", b"abd"));
        assert!(!iguais_em_tempo_constante(b"abc", b"abcd"));
        assert!(!iguais_em_tempo_constante(b"", b"a"));
        assert!(iguais_em_tempo_constante(b"", b""));
        let a = auth();
        assert!(a.confere("uma-senha-bem-comprida"));
        assert!(!a.confere("uma-senha-bem-comprid"));
    }

    #[test]
    fn prefixo_normalizado() {
        assert_eq!(normaliza_prefixo(""), "");
        assert_eq!(normaliza_prefixo("/"), "");
        assert_eq!(normaliza_prefixo("panoptico"), "/panoptico");
        assert_eq!(normaliza_prefixo("/panoptico/"), "/panoptico");
        assert_eq!(auth().caminho("/api/mundo"), "/panoptico/api/mundo");
    }

    #[test]
    fn sessao_vale_ate_expirar_e_sai_ao_encerrar() {
        let a = auth();
        let t0 = Instant::now();
        let tok = a.nova_sessao(t0).unwrap();
        assert_eq!(tok.len(), 64);
        assert!(a.valida(&tok, t0));
        assert!(!a.valida("outro", t0));
        assert!(!a.valida(&tok, t0 + VALIDADE + Duration::from_secs(1)), "expirada");
        let tok2 = a.nova_sessao(t0).unwrap();
        assert_ne!(tok, tok2);
        a.encerrar(&tok2);
        assert!(!a.valida(&tok2, t0));
    }

    #[test]
    fn cinco_falhas_travam_o_ip_ate_a_janela_passar() {
        let a = auth();
        let t0 = Instant::now();
        for _ in 0..MAX_FALHAS - 1 {
            a.falhou("1.2.3.4", t0);
        }
        assert!(!a.bloqueado("1.2.3.4", t0));
        a.falhou("1.2.3.4", t0);
        assert!(a.bloqueado("1.2.3.4", t0));
        assert!(!a.bloqueado("5.6.7.8", t0), "outro IP segue livre");
        assert!(!a.bloqueado("1.2.3.4", t0 + JANELA_DE_FALHAS + Duration::from_secs(1)));
        a.falhou("9.9.9.9", t0);
        a.limpar_falhas("9.9.9.9");
        assert!(!a.bloqueado("9.9.9.9", t0));
    }

    #[test]
    fn cookie_seguro_no_caminho_do_prefixo() {
        let a = auth();
        let c = a.cookie("abc");
        assert!(c.starts_with("panoptico=abc;"));
        for parte in ["HttpOnly", "SameSite=Strict", "Secure", "Path=/panoptico/", "Max-Age=43200"] {
            assert!(c.contains(parte), "{c} sem {parte}");
        }
        assert!(a.cookie_apagar().contains("Max-Age=0"));
        let sem = Auth::novo("uma-senha-bem-comprida".into(), "", false, false).unwrap();
        assert!(!sem.cookie("x").contains("Secure"));
        assert!(sem.cookie("x").contains("Path=/;"));
    }

    #[test]
    fn le_o_cookie_e_o_ip() {
        let mut h = HeaderMap::new();
        h.insert("cookie", "a=1; panoptico=tok123; b=2".parse().unwrap());
        assert_eq!(token_do_cookie(&h).as_deref(), Some("tok123"));
        assert_eq!(token_do_cookie(&HeaderMap::new()), None);
        let par: std::net::SocketAddr = "127.0.0.1:5000".parse().unwrap();
        h.insert("x-forwarded-for", "8.8.8.8, 10.0.0.1".parse().unwrap());
        assert_eq!(ip_do_pedido(&h, par, false), "127.0.0.1", "sem confiar, cabecalho nao vale");
        assert_eq!(ip_do_pedido(&h, par, true), "8.8.8.8");
        h.insert("x-real-ip", "1.1.1.1".parse().unwrap());
        assert_eq!(ip_do_pedido(&h, par, true), "1.1.1.1");
    }
}
