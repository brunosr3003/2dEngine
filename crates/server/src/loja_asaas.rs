//! ASAAS (docs/LOJA.md): the TP shop's payment on the PC builds (Windows,
//! Mac, Linux), where there is no store billing. The server opens an Asaas
//! hosted CHECKOUT for the order (the buyer types name/CPF and pays there,
//! card or Pix), the game opens its link in the browser, and the server
//! watches for the payment (`pagamento_do_checkout`) and credits the TP
//! (`loja::confirmar_pagamento`, idempotent).
//!
//! Configuration (env): `ASAAS_API_KEY_FILE` (a file holding the key, mode
//! 600) or `ASAAS_API_KEY`; `ASAAS_URL` (default production
//! `https://api.asaas.com/v3`); `ASAAS_TIPOS` (default `CREDIT_CARD`; add
//! `PIX` once the account has a Pix key). The key is never logged.

use std::time::Duration;

use serde::Deserialize;
use shared::loja::PacoteTp;

/// Asaas refuses a charge under R$ 5,00: the cheapest pack costs that much
/// on PC (on Google Play it keeps its own price).
pub const MINIMO_CENTAVOS: u32 = 500;
/// How long the checkout link stays open, minutes (Asaas: 10–1440).
pub const MINUTOS_DO_CHECKOUT: u32 = 60;

#[derive(Debug, thiserror::Error)]
pub enum Erro {
    #[error("Asaas is not configured")]
    NaoConfigurado,
    #[error("network: {0}")]
    Rede(String),
    #[error("Asaas refused: {0}")]
    Recusado(String),
}

fn chave() -> Option<String> {
    if let Ok(caminho) = std::env::var("ASAAS_API_KEY_FILE") {
        if let Ok(s) = std::fs::read_to_string(caminho.trim()) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    std::env::var("ASAAS_API_KEY").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn base() -> String {
    std::env::var("ASAAS_URL").unwrap_or_else(|_| "https://api.asaas.com/v3".into())
}

fn tipos() -> Vec<String> {
    std::env::var("ASAAS_TIPOS")
        .unwrap_or_else(|_| "CREDIT_CARD".into())
        .split(',')
        .map(|t| t.trim().to_uppercase())
        .filter(|t| matches!(t.as_str(), "CREDIT_CARD" | "PIX"))
        .collect()
}

/// Is the Asaas provider set up on this server?
pub fn configurado() -> bool {
    chave().is_some()
}

/// The price charged on PC for `pacote`, centavos.
pub fn centavos_no_pc(pacote: &PacoteTp) -> u32 {
    pacote.centavos.max(MINIMO_CENTAVOS)
}

fn cliente() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("tempest-server")
        .build()
        .unwrap_or_default()
}

#[derive(Deserialize)]
struct Criado {
    id: String,
    link: String,
}

/// Opens the hosted checkout for order `pedido` buying `pacote`: (checkout
/// id, link to open in the browser).
pub async fn criar_checkout(pedido: &str, pacote: &PacoteTp) -> Result<(String, String), Erro> {
    let chave = chave().ok_or(Erro::NaoConfigurado)?;
    let site = "https://mmo.brunji.com.br/";
    let corpo = serde_json::json!({
        "billingTypes": tipos(),
        "chargeTypes": ["DETACHED"],
        "minutesToExpire": MINUTOS_DO_CHECKOUT,
        "externalReference": pedido,
        "callback": { "successUrl": site, "cancelUrl": site, "expiredUrl": site },
        "items": [{
            "name": pacote.nome.chars().take(30).collect::<String>(),
            "description": format!("Tempest: {} TP", pacote.total()),
            "quantity": 1,
            "value": centavos_no_pc(pacote) as f64 / 100.0,
        }],
    });
    let resp = cliente()
        .post(format!("{}/checkouts", base()))
        .header("access_token", chave)
        .json(&corpo)
        .send()
        .await
        .map_err(|e| Erro::Rede(e.to_string()))?;
    let status = resp.status();
    let texto = resp.text().await.map_err(|e| Erro::Rede(e.to_string()))?;
    if !status.is_success() {
        return Err(Erro::Recusado(texto));
    }
    let c: Criado = serde_json::from_str(&texto).map_err(|e| Erro::Recusado(format!("{e}: {texto}")))?;
    Ok((c.id, c.link))
}

#[derive(Deserialize)]
struct Pagamento {
    id: String,
    status: String,
}

#[derive(Deserialize)]
struct Lista {
    #[serde(default)]
    data: Vec<Pagamento>,
}

/// Money in: a card payment made (CONFIRMED) or a Pix received (RECEIVED).
pub fn pago(status: &str) -> bool {
    matches!(status, "CONFIRMED" | "RECEIVED" | "RECEIVED_IN_CASH")
}

/// The PAID payment of checkout `id`, if any: its Asaas payment id.
pub async fn pagamento_do_checkout(id: &str) -> Result<Option<String>, Erro> {
    let chave = chave().ok_or(Erro::NaoConfigurado)?;
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(Erro::Recusado("bad checkout id".into()));
    }
    let resp = cliente()
        .get(format!("{}/payments?checkoutSession={id}", base()))
        .header("access_token", chave)
        .send()
        .await
        .map_err(|e| Erro::Rede(e.to_string()))?;
    let status = resp.status();
    let texto = resp.text().await.map_err(|e| Erro::Rede(e.to_string()))?;
    if !status.is_success() {
        return Err(Erro::Recusado(texto));
    }
    let l: Lista = serde_json::from_str(&texto).map_err(|e| Erro::Recusado(format!("{e}: {texto}")))?;
    Ok(l.data.into_iter().find(|p| pago(&p.status)).map(|p| p.id))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_pacote_mais_barato_respeita_o_minimo_do_asaas() {
        for p in shared::loja::PACOTES.iter() {
            assert!(centavos_no_pc(p) >= MINIMO_CENTAVOS);
            assert!(centavos_no_pc(p) >= p.centavos);
        }
        assert!(pago("CONFIRMED") && pago("RECEIVED") && !pago("PENDING") && !pago("REFUNDED"));
    }

    #[test]
    fn a_lista_de_pagamentos_acha_o_pago() {
        let l: Lista = serde_json::from_str(r#"{"data":[{"id":"pay_1","status":"PENDING"},{"id":"pay_2","status":"CONFIRMED"}],"totalCount":2}"#).unwrap();
        assert_eq!(l.data.into_iter().find(|p| pago(&p.status)).map(|p| p.id).as_deref(), Some("pay_2"));
    }
}
