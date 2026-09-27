//! Lembrancas pequenas entre uma sessao e a proxima.
//!
//! O dono: "tem que ter um lembrar do ultimo login na hora de logar". Ate'
//! hoje a tela de login nascia VAZIA — o usuario so' vinha de `MMO_USER`, que
//! e' variavel de ambiente e no celular nao existe. Quem jogava pelo
//! TestFlight redigitava usuario e senha a cada abertura.
//!
//! O que fica guardado, e o que NAO fica:
//!
//! - o **nome de usuario**, sempre, pra o campo ja' vir preenchido;
//! - a **sessao** (o mesmo token de `login_tokens` que o login com Google
//!   usa), so' se o jogador marcar "lembrar de mim";
//! - a **senha**, NUNCA. Guardar senha em claro no aparelho e' trocar a
//!   comodidade de um por um risco que o jogador nao escolheu. O token faz o
//!   mesmo servico, vence sozinho e da' pra revogar no banco sem trocar a
//!   senha de ninguem.
//!
//! O arquivo e' texto `chave=valor`, uma por linha, com permissao 0600 no
//! unix. Tudo aqui e' melhor-esforco: sem lugar pra escrever (o caso do
//! Android hoje), o jogo funciona igual, so' nao lembra.

use std::path::PathBuf;

#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct Prefs {
    /// O ultimo usuario que entrou. Vazio = nunca entrou aqui.
    pub usuario: String,
    /// A sessao guardada, se o jogador pediu pra ser lembrado.
    pub sessao: Option<String>,
    /// O estado da caixinha "lembrar de mim".
    pub lembrar: bool,
    /// O idioma da interface.
    ///
    /// Fica no APARELHO, e nao no personagem, por dois motivos. O primeiro e'
    /// que a tela de login tem que estar traduzida ANTES de haver personagem
    /// — se o idioma viesse do servidor, quem joga em ingles veria a primeira
    /// tela em portugues, toda vez. O segundo e' que a escolha e' de quem
    /// segura o aparelho: a mesma conta num celular emprestado nao deve levar
    /// o idioma de outra pessoa.
    pub idioma: shared::idioma::Idioma,
}

/// Onde o arquivo mora, por plataforma.
///
/// No iOS o `HOME` aponta pra caixa de areia do app e `Documents` sobrevive a
/// fechar e reabrir — e' o lugar certo. No desktop vale o `XDG_DATA_HOME` e
/// depois `~/.local/share`. No Android nao ha' caminho garantido sem JNI, e o
/// `None` daqui e' o que faz o resto degradar em silencio.
pub(crate) fn caminho() -> Option<PathBuf> {
    #[cfg(target_os = "ios")]
    {
        let casa = std::env::var_os("HOME")?;
        let mut p = PathBuf::from(casa);
        p.push("Documents");
        std::fs::create_dir_all(&p).ok()?;
        p.push("tempest.prefs");
        return Some(p);
    }
    #[cfg(target_os = "android")]
    {
        return None;
    }
    #[cfg(target_os = "windows")]
    {
        let mut p = PathBuf::from(std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))?);
        p.push("Tempest");
        std::fs::create_dir_all(&p).ok()?;
        p.push("prefs");
        return Some(p);
    }
    #[cfg(not(any(target_os = "ios", target_os = "android", target_os = "windows")))]
    {
        let mut p = match std::env::var_os("XDG_DATA_HOME") {
            Some(d) if !d.is_empty() => PathBuf::from(d),
            _ => {
                let mut c = PathBuf::from(std::env::var_os("HOME")?);
                c.push(".local");
                c.push("share");
                c
            }
        };
        p.push("tempest");
        std::fs::create_dir_all(&p).ok()?;
        p.push("prefs");
        Some(p)
    }
}

/// Le' o que estiver guardado. Sem arquivo, ou com arquivo estragado, devolve
/// o vazio — nunca falha.
pub fn carrega() -> Prefs {
    let Some(p) = caminho() else {
        return Prefs::default();
    };
    match std::fs::read_to_string(&p) {
        Ok(texto) => decodifica(&texto),
        Err(_) => Prefs::default(),
    }
}

/// Grava. Melhor-esforco: se nao der, o jogo segue sem lembrar.
pub fn salva(v: &Prefs) {
    let Some(p) = caminho() else {
        return;
    };
    if std::fs::write(&p, codifica(v)).is_err() {
        return;
    }
    // 0600: o arquivo carrega uma credencial de sessao. No iOS a caixa de
    // areia ja' isola, mas no desktop `~/.local/share` e' legivel por padrao
    // pra quem estiver na mesma maquina.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
}

/// Esquece a sessao (e so' ela): o nome do usuario continua, porque e' ele
/// que faz o campo vir preenchido na proxima vez.
pub fn esquece_a_sessao() {
    let mut v = carrega();
    v.sessao = None;
    v.lembrar = false;
    salva(&v);
}

fn codifica(v: &Prefs) -> String {
    let mut s = format!(
        "usuario={}\nlembrar={}\nidioma={}\n",
        v.usuario,
        v.lembrar as u8,
        v.idioma.codigo()
    );
    if let Some(t) = v.sessao.as_ref().filter(|_| v.lembrar) {
        s.push_str(&format!("sessao={t}\n"));
    }
    s
}

/// Parte na PRIMEIRA igualdade, e nao em todas: o token e' base64 e pode ter
/// `=` de enchimento. Partir em todas devolveria um token cortado, que e'
/// pior que nenhum — o jogador veria "sessao invalida" sem entender.
fn decodifica(texto: &str) -> Prefs {
    let mut v = Prefs::default();
    for linha in texto.lines() {
        let Some((chave, valor)) = linha.split_once('=') else {
            continue;
        };
        match chave.trim() {
            "usuario" => v.usuario = valor.trim().to_string(),
            "lembrar" => v.lembrar = valor.trim() == "1",
            "sessao" if !valor.trim().is_empty() => v.sessao = Some(valor.trim().to_string()),
            // Codigo desconhecido (arquivo de uma versao futura, ou editado na
            // mao) cai no padrao em vez de falhar: preferencia estragada nao
            // e' motivo pra nao abrir o jogo.
            "idioma" => v.idioma = shared::idioma::Idioma::do_codigo(valor).unwrap_or_default(),
            _ => {}
        }
    }
    // Sessao sem "lembrar" e' contradicao: o unico jeito de ela existir e' o
    // jogador ter pedido. Na duvida, nao usa.
    if !v.lembrar {
        v.sessao = None;
    }
    v
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_que_vai_volta() {
        let v = Prefs {
            usuario: "brunji".into(),
            sessao: Some("abc123".into()),
            lembrar: true,
            idioma: shared::idioma::Idioma::Pt,
        };
        assert_eq!(decodifica(&codifica(&v)), v);
    }

    /// A SENHA NUNCA ENTRA. O teste existe porque "guardar o login" é o tipo
    /// de pedido que convida a gravar a senha em claro, e o convite tem que
    /// ser recusado aqui, onde dá pra ver.
    #[test]
    fn a_senha_nunca_e_gravada() {
        let v = Prefs {
            usuario: "brunji".into(),
            sessao: Some("token".into()),
            lembrar: true,
            idioma: shared::idioma::Idioma::Pt,
        };
        let texto = codifica(&v);
        assert!(
            !texto.contains("senha"),
            "campo de senha no arquivo: {texto}"
        );
        // E o struct não tem onde pôr uma: se alguém acrescentar o campo,
        // este teste continua passando, mas `o_que_vai_volta` quebra na
        // comparação — que é o aviso.
    }

    /// O idioma vai e volta, e o desconhecido não derruba o resto.
    ///
    /// O caso que motiva: arquivo escrito por uma versão que fala uma língua
    /// que esta não fala. Ler `idioma=fr` tem que dar português e MANTER o
    /// usuário — perder o nome de quem entra por causa de um idioma é trocar
    /// um defeito pequeno por um grande.
    #[test]
    fn o_idioma_vai_e_volta_e_o_desconhecido_cai_no_padrao() {
        let v = Prefs {
            usuario: "brunji".into(),
            sessao: None,
            lembrar: false,
            idioma: shared::idioma::Idioma::En,
        };
        assert_eq!(decodifica(&codifica(&v)).idioma, shared::idioma::Idioma::En);

        let estranho = decodifica("usuario=brunji\nidioma=fr\n");
        assert_eq!(estranho.idioma, shared::idioma::Idioma::Pt);
        assert_eq!(estranho.usuario, "brunji");
    }

    /// Sem "lembrar", a sessão não é gravada nem lida.
    #[test]
    fn sem_lembrar_a_sessao_nao_fica() {
        let v = Prefs {
            usuario: "brunji".into(),
            sessao: Some("abc123".into()),
            lembrar: false,
            idioma: shared::idioma::Idioma::Pt,
        };
        let texto = codifica(&v);
        assert!(!texto.contains("abc123"), "sessão gravada sem permissão");
        assert_eq!(decodifica(&texto).sessao, None);
    }

    /// Token com `=` de enchimento sobrevive à ida e volta.
    #[test]
    fn token_com_igual_nao_e_cortado() {
        let v = Prefs {
            usuario: "a".into(),
            sessao: Some("YWJjZGVmZw==".into()),
            lembrar: true,
            idioma: shared::idioma::Idioma::Pt,
        };
        assert_eq!(
            decodifica(&codifica(&v)).sessao.as_deref(),
            Some("YWJjZGVmZw==")
        );
    }

    /// Arquivo estragado não derruba o jogo.
    #[test]
    fn lixo_no_arquivo_vira_vazio() {
        assert_eq!(
            decodifica("\u{0}\nsem igualdade\n=sem chave\n"),
            Prefs::default()
        );
    }
}
