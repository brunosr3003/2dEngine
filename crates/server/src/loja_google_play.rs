//! Verificacao de compra real da Google Play (Android Publisher API v3),
//! usada por `loja::Provedor::GooglePlay` (docs/LOJA.md).
//!
//! O cliente manda so' o `purchase_token` que o Google Play Billing
//! devolveu; o `product_id` e' calculado AQUI a partir do `pacote.id`
//! (`id_do_produto`), nunca confiado do cliente — senao' um cliente
//! adulterado podia mandar o token de um pacote barato tentando creditar um
//! caro, e o token sozinho nao prova qual produto foi comprado.
//!
//! Autenticacao: conta de servico (service account) do Google Cloud. O JSON
//! dela vive no disco do servidor (`GOOGLE_PLAY_SERVICE_ACCOUNT_JSON`),
//! nunca no repo. Monta um JWT RS256 com a chave privada e troca por um
//! token de acesso OAuth2 em `https://oauth2.googleapis.com/token`,
//! cacheado em memoria ate' ~1 min antes de expirar.
//!
//! `Provedor::do_ambiente` só escolhe este provedor quando
//! `GOOGLE_PLAY_PACKAGE_NAME` e `GOOGLE_PLAY_SERVICE_ACCOUNT_JSON` existem
//! (`configurado()`); sem eles o servidor continua simulado/desligado como
//! hoje — nao muda nada em dev/teste.

use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use shared::loja::PacoteTp;

const ESCOPO: &str = "https://www.googleapis.com/auth/androidpublisher";
const TOKEN_URL_PADRAO: &str = "https://oauth2.googleapis.com/token";

#[derive(Debug)]
pub enum Erro {
    /// Faltam os env vars; o chamador trata como recusa.
    NaoConfigurado,
    Rede(String),
    RespostaInvalida(String),
    Recusado(String),
}

impl std::fmt::Display for Erro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Erro::NaoConfigurado => write!(f, "Google Play billing is not configured."),
            Erro::Rede(e) => write!(f, "Google Play request failed: {e}"),
            Erro::RespostaInvalida(e) => {
                write!(f, "Google Play returned an unexpected response: {e}")
            }
            Erro::Recusado(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Erro {}

/// O id do produto "gerenciado" cadastrado no Play Console pro pacote de TP.
/// Convencao fixa (docs/LOJA.md): quem cadastra os produtos la' usa
/// `tp_pacote_<id>`, o mesmo `id` de `shared::loja::PacoteTp`.
pub fn id_do_produto(pacote_id: u16) -> String {
    format!("tp_pacote_{pacote_id}")
}

fn nome_do_pacote_env() -> Option<String> {
    std::env::var("GOOGLE_PLAY_PACKAGE_NAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn caminho_da_chave_env() -> Option<String> {
    std::env::var("GOOGLE_PLAY_SERVICE_ACCOUNT_JSON")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Os dois env vars presentes? `Provedor::do_ambiente` so' escolhe
/// `GooglePlay` quando isto e' verdade.
pub fn configurado() -> bool {
    nome_do_pacote_env().is_some() && caminho_da_chave_env().is_some()
}

#[derive(Deserialize)]
struct ChaveDeServico {
    client_email: String,
    private_key: String,
    #[serde(default = "token_url_padrao")]
    token_uri: String,
}

fn token_url_padrao() -> String {
    TOKEN_URL_PADRAO.to_string()
}

fn ler_chave() -> Result<ChaveDeServico, Erro> {
    let caminho = caminho_da_chave_env().ok_or(Erro::NaoConfigurado)?;
    let bruto = std::fs::read_to_string(&caminho)
        .map_err(|e| Erro::RespostaInvalida(format!("service account file: {e}")))?;
    serde_json::from_str(&bruto)
        .map_err(|e| Erro::RespostaInvalida(format!("service account json: {e}")))
}

#[derive(Serialize)]
struct Claims {
    iss: String,
    scope: String,
    aud: String,
    iat: usize,
    exp: usize,
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
    expires_in: u64,
}

/// `(token, quando_expira)`. Um so' vale pra qualquer compra em verificacao
/// — o escopo e' o mesmo pra todas.
static TOKEN_CACHE: Mutex<Option<(String, Instant)>> = Mutex::new(None);

/// Assina o JWT da service account e troca por um token de acesso OAuth2,
/// reusando o cache enquanto nao estiver perto de expirar.
async fn obter_token_acesso() -> Result<String, Erro> {
    if let Some((tok, expira)) = TOKEN_CACHE.lock().clone() {
        if expira > Instant::now() {
            return Ok(tok);
        }
    }
    let chave = ler_chave()?;
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Erro::RespostaInvalida(e.to_string()))?
        .as_secs() as usize;
    let claims = Claims {
        iss: chave.client_email,
        scope: ESCOPO.to_string(),
        aud: chave.token_uri.clone(),
        iat: agora,
        exp: agora + 3600,
    };
    let chave_de_assinatura =
        jsonwebtoken::EncodingKey::from_rsa_pem(chave.private_key.as_bytes())
            .map_err(|e| Erro::RespostaInvalida(format!("private key: {e}")))?;
    let jwt = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
        &claims,
        &chave_de_assinatura,
    )
    .map_err(|e| Erro::RespostaInvalida(format!("jwt: {e}")))?;

    let resp = reqwest::Client::new()
        .post(&chave.token_uri)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", jwt.as_str()),
        ])
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| Erro::Rede(e.to_string()))?;
    if !resp.status().is_success() {
        let corpo = resp.text().await.unwrap_or_default();
        return Err(Erro::Rede(format!("token exchange failed: {corpo}")));
    }
    let tok: TokenResp = resp
        .json()
        .await
        .map_err(|e| Erro::RespostaInvalida(format!("token response: {e}")))?;
    // Uma folga de 60s pra nao usar um token que expira no meio da proxima
    // chamada.
    let expira_em = Instant::now() + Duration::from_secs(tok.expires_in.saturating_sub(60));
    *TOKEN_CACHE.lock() = Some((tok.access_token.clone(), expira_em));
    Ok(tok.access_token)
}

/// Resposta da Android Publisher API pra uma compra de produto gerenciado.
/// So' os campos que importam aqui; a API devolve bem mais.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ProductPurchase {
    /// 0 = comprado, 1 = cancelado, 2 = pendente.
    #[serde(default)]
    purchase_state: Option<i32>,
    #[serde(default)]
    order_id: Option<String>,
    /// 0 = ainda nao reconhecida, 1 = reconhecida.
    #[serde(default)]
    acknowledgement_state: Option<i32>,
    /// Absent = a real purchase; 0 = TEST (license tester: Google's test card,
    /// nobody is charged), 1 = promo code, 2 = rewarded.
    #[serde(default)]
    purchase_type: Option<i32>,
}

/// Prefix put on the Google order id of a TEST purchase: it is still
/// delivered (testers must see the flow working) but never counted as revenue.
pub const PREFIXO_TESTE: &str = "TESTE:";

pub fn e_teste(externo: &str) -> bool {
    externo.starts_with(PREFIXO_TESTE)
}

/// Confere a compra na Android Publisher API. NAO credita nada (isso e'
/// `loja::confirmar_pagamento`, chamado pelo `loja::comprar_tp` de volta) —
/// so' devolve o id externo (`orderId`) pra registrar, ou o motivo da
/// recusa. O reconhecimento (`acknowledge`) fica pra depois do credito
/// (`reconhecer_best_effort`): reconhecer antes e falhar o credito deixaria
/// uma compra "paga" sem TP nenhuma.
pub async fn verificar(pacote: &PacoteTp, purchase_token: &str) -> Result<String, Erro> {
    verificar_produto(&id_do_produto(pacote.id), purchase_token).await
}

/// `verificar` for any Play product id (TP packs and flash offers). The id
/// always comes from the SERVER's own data, never from the client.
pub async fn verificar_produto(product_id: &str, purchase_token: &str) -> Result<String, Erro> {
    if !token_seguro(purchase_token) || !token_seguro(product_id) {
        return Err(Erro::Recusado("Invalid purchase.".into()));
    }
    let nome_do_pacote = nome_do_pacote_env().ok_or(Erro::NaoConfigurado)?;
    let token_acesso = obter_token_acesso().await?;
    let url = format!(
        "https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{nome_do_pacote}/purchases/products/{product_id}/tokens/{purchase_token}"
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&token_acesso)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| Erro::Rede(e.to_string()))?;
    if !resp.status().is_success() {
        let corpo = resp.text().await.unwrap_or_default();
        return Err(Erro::Recusado(format!("purchase lookup failed: {corpo}")));
    }
    let corpo = resp.text().await.map_err(|e| Erro::Rede(e.to_string()))?;
    estado_da_resposta(&corpo, purchase_token)
}

/// A parte sem rede: le' o JSON e decide aprovar/recusar. Separado de
/// `verificar` pra dar pra testar sem bater na Google de verdade.
fn estado_da_resposta(corpo: &str, purchase_token: &str) -> Result<String, Erro> {
    let compra: ProductPurchase = serde_json::from_str(corpo)
        .map_err(|e| Erro::RespostaInvalida(format!("purchase response: {e} ({corpo})")))?;
    match compra.purchase_state {
        Some(0) => {
            let id = compra.order_id.unwrap_or_else(|| format!("gp-{purchase_token}"));
            Ok(if compra.purchase_type == Some(0) { format!("{PREFIXO_TESTE}{id}") } else { id })
        }
        Some(1) => Err(Erro::Recusado("Purchase was canceled.".into())),
        Some(2) => Err(Erro::Recusado("Purchase is still pending.".into())),
        outro => Err(Erro::RespostaInvalida(format!(
            "unexpected purchaseState: {outro:?}"
        ))),
    }
}

/// The raw `orders.get` answer for one Google order (what the buyer paid,
/// tax, the developer's revenue) — read by `financeiro`.
pub async fn pedido_bruto(order_id: &str) -> Result<String, Erro> {
    if !token_seguro(order_id) {
        return Err(Erro::Recusado("Invalid order id.".into()));
    }
    let nome_do_pacote = nome_do_pacote_env().ok_or(Erro::NaoConfigurado)?;
    let token_acesso = obter_token_acesso().await?;
    let url = format!(
        "https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{nome_do_pacote}/orders/{order_id}"
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&token_acesso)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| Erro::Rede(e.to_string()))?;
    let ok = resp.status().is_success();
    let corpo = resp.text().await.map_err(|e| Erro::Rede(e.to_string()))?;
    if ok { Ok(corpo) } else { Err(Erro::Recusado(format!("order lookup failed: {corpo}"))) }
}

/// Purchase tokens and product ids go into a URL path: only the characters
/// Google actually uses (letters, digits, `.`, `_`, `-`).
fn token_seguro(t: &str) -> bool {
    !t.is_empty() && t.len() <= 4096 && t.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// CONSUMES the purchase on Google, AFTER it was credited. Consuming also
/// acknowledges it (no automatic refund in 3 days) and — what the old
/// acknowledge-only code missed — frees the product to be bought AGAIN: a TP
/// pack bought once stayed "owned" forever and Google refused the second
/// purchase of the same pack. Best-effort: the player already has what they
/// paid for; a failure only logs.
pub async fn consumir_best_effort(product_id: &str, purchase_token: &str) -> Result<(), Erro> {
    if !token_seguro(purchase_token) || !token_seguro(product_id) {
        return Err(Erro::Recusado("Invalid purchase.".into()));
    }
    let nome_do_pacote = nome_do_pacote_env().ok_or(Erro::NaoConfigurado)?;
    let token_acesso = obter_token_acesso().await?;
    let url = format!(
        "https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{nome_do_pacote}/purchases/products/{product_id}/tokens/{purchase_token}:consume"
    );
    let resp = reqwest::Client::new()
        .post(&url)
        .bearer_auth(&token_acesso)
        .header("content-type", "application/json")
        .body("{}")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| Erro::Rede(e.to_string()))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        let corpo = resp.text().await.unwrap_or_default();
        Err(Erro::Rede(format!("consume failed: {corpo}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purchase_state_0_aprova_e_devolve_order_id() {
        let json = r#"{"purchaseState":0,"orderId":"GPA.1234","acknowledgementState":0}"#;
        assert_eq!(
            estado_da_resposta(json, "tok123").unwrap(),
            "GPA.1234".to_string()
        );
    }

    #[test]
    fn compra_de_teste_vem_marcada() {
        let json = r#"{"purchaseState":0,"orderId":"GPA.9","purchaseType":0}"#;
        let ext = estado_da_resposta(json, "t").unwrap();
        assert_eq!(ext, "TESTE:GPA.9");
        assert!(e_teste(&ext));
        assert!(!e_teste("GPA.9"));
    }

    #[test]
    fn purchase_state_0_sem_order_id_usa_o_token() {
        let json = r#"{"purchaseState":0}"#;
        assert_eq!(
            estado_da_resposta(json, "tok123").unwrap(),
            "gp-tok123".to_string()
        );
    }

    #[test]
    fn purchase_state_1_e_recusado() {
        let json = r#"{"purchaseState":1}"#;
        assert!(matches!(
            estado_da_resposta(json, "tok123"),
            Err(Erro::Recusado(_))
        ));
    }

    #[test]
    fn purchase_state_2_pendente_e_recusado() {
        let json = r#"{"purchaseState":2}"#;
        assert!(matches!(
            estado_da_resposta(json, "tok123"),
            Err(Erro::Recusado(_))
        ));
    }

    #[test]
    fn id_do_produto_segue_a_convencao() {
        assert_eq!(id_do_produto(1), "tp_pacote_1");
        assert_eq!(id_do_produto(4), "tp_pacote_4");
    }

    #[test]
    fn nao_configurado_sem_env_vars() {
        // Nao mexe nos env vars globais (testes rodam em paralelo); so'
        // confere que as funcoes nao dao panic sem eles.
        let _ = configurado();
    }
}
