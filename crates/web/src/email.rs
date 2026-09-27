//! Envio de e-mail pelo Resend, e os dois links que o jogo manda:
//! confirmar a conta e redefinir a senha.
//!
//! Por que um servico e nao SMTP: e-mail de servidor novo cai em spam por
//! padrao. Quem entrega e' reputacao de IP e assinatura (SPF/DKIM/DMARC), e
//! isso e' exatamente o que um servico transacional ja' tem pronto. Um
//! `sendmail` na VPS mandaria e-mail que ninguem recebe — o pior dos casos,
//! porque parece funcionar.
//!
//! DESLIGADO sem `RESEND_API_KEY`: `Email::do_ambiente` devolve `None` e quem
//! chama segue sem mandar nada. O cadastro continua criando conta; so' nao
//! manda o link. E' a mesma escolha do login com Google.

use anyhow::{anyhow, Result};
use shared::idioma::{tr_em, Idioma};

const URL_RESEND: &str = "https://api.resend.com/emails";

/// De onde o e-mail sai e por onde os links voltam.
#[derive(Clone)]
pub struct Email {
    chave: String,
    /// `Tempest <nao-responda@brunji.com.br>`.
    remetente: String,
    /// Raiz publica, sem barra no fim: `https://mmo.brunji.com.br`.
    base: String,
    http: reqwest::Client,
}

impl Email {
    pub fn do_ambiente(http: reqwest::Client) -> Option<Self> {
        let chave = std::env::var("RESEND_API_KEY").ok().filter(|v| !v.is_empty())?;
        let remetente = std::env::var("EMAIL_REMETENTE")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "Tempest <nao-responda@brunji.com.br>".into());
        let base = std::env::var("PUBLIC_BASE_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "https://mmo.brunji.com.br".into())
            .trim_end_matches('/')
            .to_string();
        Some(Self {
            chave,
            remetente,
            base,
            http,
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    async fn manda(&self, para: &str, assunto: &str, html: &str) -> Result<()> {
        let corpo = serde_json::json!({
            "from": self.remetente,
            "to": [para],
            "subject": assunto,
            "html": html,
        });
        let r = self
            .http
            .post(URL_RESEND)
            .bearer_auth(&self.chave)
            .json(&corpo)
            .send()
            .await?;
        if r.status().is_success() {
            return Ok(());
        }
        let codigo = r.status();
        let detalhe = r.text().await.unwrap_or_default();
        Err(anyhow!("resend {codigo}: {detalhe}"))
    }

    /// O e-mail de confirmacao, na lingua de QUEM PEDIU.
    ///
    /// `idioma` vem do `Accept-Language` do cadastro, e nao de uma coluna nova
    /// em `accounts`. A razao e' que a coluna seria migracao, INSERT mexido e
    /// um valor a mais pra errar (ver o que "coluna nova quebra o INSERT" ja'
    /// custou) — e o cabecalho ja' diz, de graca, em que lingua a pessoa esta'
    /// lendo o site NESTE momento, que e' a resposta certa pra este e-mail.
    pub async fn confirmacao(
        &self,
        para: &str,
        usuario: &str,
        token: &str,
        idioma: Idioma,
    ) -> Result<()> {
        let link = format!("{}/api/auth/confirmar?token={token}", self.base);
        let t = |s: &str| tr_em(idioma, s).into_owned();
        self.manda(
            para,
            &t("Confirme sua conta no Tempest"),
            &pagina(
                &format!("{} {}!", t("Olá,"), escapa(usuario)),
                &t("Falta confirmar seu e-mail para a conta ficar completa."),
                &t("Confirmar minha conta"),
                &link,
                &t("O link vale por 48 horas. Se não foi você que criou a conta, é só ignorar."),
                &t("Se o botão não abrir:"),
            ),
        )
        .await
    }

    pub async fn reset(
        &self,
        para: &str,
        usuario: &str,
        token: &str,
        idioma: Idioma,
    ) -> Result<()> {
        let link = format!("{}/api/auth/reset?token={token}", self.base);
        let t = |s: &str| tr_em(idioma, s).into_owned();
        self.manda(
            para,
            &t("Redefinir a senha do Tempest"),
            &pagina(
                &format!("{} {}!", t("Olá,"), escapa(usuario)),
                &t("Alguém pediu para redefinir a senha desta conta."),
                &t("Escolher uma senha nova"),
                &link,
                &t("O link vale por 1 hora e só pode ser usado uma vez. Se não foi você, \
                 pode ignorar — sua senha continua a mesma."),
                &t("Se o botão não abrir:"),
            ),
        )
        .await
    }
}

/// O mesmo corpo para os dois e-mails: tabela e estilo em linha, que e' o que
/// cliente de e-mail entende. Nada de CSS externo nem flexbox.
fn pagina(
    saudacao: &str,
    frase: &str,
    botao: &str,
    link: &str,
    rodape: &str,
    plano_b: &str,
) -> String {
    format!(
        r#"<!doctype html><html><body style="margin:0;padding:24px;background:#151d2b;font-family:-apple-system,Segoe UI,Roboto,Arial,sans-serif;color:#e8e2d4">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0"><tr><td align="center">
<table role="presentation" width="480" cellpadding="0" cellspacing="0" style="max-width:480px;background:#1d2738;border-radius:12px;padding:28px">
<tr><td style="font-size:20px;font-weight:700;color:#f0c674;padding-bottom:10px">Tempest</td></tr>
<tr><td style="font-size:16px;padding-bottom:6px">{saudacao}</td></tr>
<tr><td style="font-size:15px;line-height:22px;color:#c9c2b4;padding-bottom:20px">{frase}</td></tr>
<tr><td align="center" style="padding-bottom:20px">
  <a href="{link}" style="display:inline-block;background:#f0c674;color:#1d2738;font-weight:700;font-size:15px;text-decoration:none;padding:13px 26px;border-radius:8px">{botao}</a>
</td></tr>
<tr><td style="font-size:12px;line-height:18px;color:#8b8578">{rodape}</td></tr>
<tr><td style="font-size:12px;line-height:18px;color:#8b8578;padding-top:12px;word-break:break-all">{plano_b} {link}</td></tr>
</table></td></tr></table></body></html>"#
    )
}

/// Escapa o que vai dentro do HTML do e-mail. O nome de usuario vem do
/// jogador, e nome com `<` viraria marcacao.
fn escapa(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nome_com_marcacao_nao_vira_html() {
        assert_eq!(escapa("<b>zé</b>"), "&lt;b&gt;zé&lt;/b&gt;");
    }

    /// O e-mail sai na lingua do pedido, e o corpo inteiro acompanha — nao so'
    /// o assunto. O caso que isto pega: traduzir o titulo e esquecer o botao,
    /// que e' o que a pessoa precisa clicar.
    #[test]
    fn o_corpo_do_email_acompanha_o_idioma() {
        let en = pagina(
            &shared::idioma::tr_em(Idioma::En, "Olá,"),
            &shared::idioma::tr_em(Idioma::En, "Falta confirmar seu e-mail para a conta ficar completa."),
            &shared::idioma::tr_em(Idioma::En, "Confirmar minha conta"),
            "https://x/y",
            &shared::idioma::tr_em(Idioma::En, "O link vale por 48 horas. Se não foi você que criou a conta, é só ignorar."),
            &shared::idioma::tr_em(Idioma::En, "Se o botão não abrir:"),
        );
        assert!(en.contains("Hello,"), "saudação não traduziu: {en}");
        assert!(en.contains("Confirm my account"), "botão não traduziu");
        assert!(en.contains("48 hours"), "rodapé não traduziu");
        assert!(!en.contains("Confirmar"), "sobrou português: {en}");
    }

    /// O link aparece DUAS vezes: no botão e em texto.
    ///
    /// Cliente de e-mail que bloqueia ou estraga o botão deixaria o jogador
    /// sem saída; o link em texto é o plano B, e ele tem que estar lá.
    #[test]
    fn o_link_vai_no_botao_e_em_texto() {
        let h = pagina(
            "Olá",
            "frase",
            "Clique",
            "https://x/y?token=abc",
            "rodapé",
            "Se o botão não abrir:",
        );
        assert_eq!(h.matches("https://x/y?token=abc").count(), 2);
        assert!(h.contains("Se o botão não abrir"));
    }
}
