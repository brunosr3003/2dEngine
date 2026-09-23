//! A TRILHA: tudo que um bot faz vira uma linha, e nada se perde.
//!
//! O pedido era "tudo isso tem que ser trackável". Trackável de verdade quer
//! dizer duas coisas diferentes, e as duas estao aqui:
//!
//! 1. **O que aconteceu**, em ordem, com hora: um JSONL, uma linha por evento.
//!    E' o que responde "por que o bot 3 travou as 14h12" — pergunta que um
//!    contador agregado nunca responde.
//! 2. **Como esta indo**, em numero: o resumo, que responde "quanto tempo leva
//!    pra chegar no nivel 10" e "quantas missoes por hora".
//!
//! JSONL e nao um banco: o arquivo e' anexavel, sobrevive a queda do processo,
//! e da' pra ler com `tail -f` enquanto roda. Banco exigiria schema e migracao
//! pra um dado que e' descartavel por natureza.

use std::io::Write;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Em que ponto da vida o bot esta'. E' o eixo que separa "o bot nao faz
/// missao" de "o bot nunca chegou a entrar no mundo".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fase {
    Cadastrando,
    Logando,
    CriandoPersonagem,
    NoMundo,
    Morto,
    Desconectado,
}

impl Fase {
    pub fn nome(self) -> &'static str {
        match self {
            Fase::Cadastrando => "cadastrando",
            Fase::Logando => "logando",
            Fase::CriandoPersonagem => "criando_personagem",
            Fase::NoMundo => "no_mundo",
            Fase::Morto => "morto",
            Fase::Desconectado => "desconectado",
        }
    }
}

/// Um evento na vida do bot.
pub struct Evento<'a> {
    pub bot: &'a str,
    pub fase: Fase,
    /// Verbo curto e estavel: `quest_aceita`, `mob_morto`, `item_vendido`.
    /// Estavel porque e' por ele que se agrega depois.
    pub acao: &'a str,
    pub ok: bool,
    /// Livre, pra quem for ler com o olho.
    pub detalhe: String,
    /// Numeros do estado no momento — e' o que deixa reconstruir a curva.
    pub nivel: u32,
    pub xp: i64,
    pub ouro: i64,
}

pub struct Trilha {
    arquivo: Mutex<std::fs::File>,
    inicio: Instant,
    resumo: Mutex<Resumo>,
}

#[derive(Default)]
pub struct Resumo {
    pub eventos: u64,
    pub falhas: u64,
    /// Contagem por ação, que é o agregado que responde "quantas por hora".
    pub por_acao: std::collections::BTreeMap<String, u64>,
    /// Quando cada bot chegou a cada nível, em segundos desde o início. É a
    /// curva de progressão — o número que o dono quer ver pra saber se o jogo
    /// está calibrado.
    pub nivel_em_s: std::collections::BTreeMap<u32, Vec<f64>>,
    /// A última falha de cada ação, pra não precisar caçar no arquivo.
    pub ultima_falha: std::collections::BTreeMap<String, String>,
}

impl Trilha {
    pub fn nova(caminho: &str) -> anyhow::Result<Self> {
        let arquivo = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(caminho)?;
        Ok(Self {
            arquivo: Mutex::new(arquivo),
            inicio: Instant::now(),
            resumo: Mutex::new(Resumo::default()),
        })
    }

    pub fn registra(&self, e: Evento<'_>) {
        let agora = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let desde = self.inicio.elapsed().as_secs_f64();
        let linha = format!(
            r#"{{"t":{agora:.3},"s":{desde:.3},"bot":"{}","fase":"{}","acao":"{}","ok":{},"nivel":{},"xp":{},"ouro":{},"detalhe":{}}}"#,
            escapa(e.bot),
            e.fase.nome(),
            escapa(e.acao),
            e.ok,
            e.nivel,
            e.xp,
            e.ouro,
            json_str(&e.detalhe),
        );
        if let Ok(mut f) = self.arquivo.lock() {
            let _ = writeln!(f, "{linha}");
        }
        if let Ok(mut r) = self.resumo.lock() {
            r.eventos += 1;
            *r.por_acao.entry(e.acao.to_string()).or_default() += 1;
            if !e.ok {
                r.falhas += 1;
                r.ultima_falha
                    .insert(e.acao.to_string(), e.detalhe.clone());
            }
            if e.acao == "subiu_de_nivel" {
                r.nivel_em_s.entry(e.nivel).or_default().push(desde);
            }
        }
    }

    pub fn resumo(&self) -> String {
        let Ok(r) = self.resumo.lock() else {
            return "resumo indisponível".into();
        };
        let mut s = String::new();
        s.push_str(&format!(
            "\n── TRILHA ─────────────────────────────────────────\n\
             {} eventos, {} falhas, {:.1} min de corrida\n\n",
            r.eventos,
            r.falhas,
            self.inicio.elapsed().as_secs_f64() / 60.0,
        ));
        s.push_str("por ação:\n");
        for (a, n) in &r.por_acao {
            s.push_str(&format!("  {a:26} {n:6}\n"));
        }
        if !r.nivel_em_s.is_empty() {
            s.push_str("\ncurva de nível (mediana em min desde o início):\n");
            for (nivel, mut v) in r.nivel_em_s.clone() {
                v.sort_by(f64::total_cmp);
                let mediana = v[v.len() / 2] / 60.0;
                s.push_str(&format!(
                    "  nível {nivel:3}  {mediana:6.1} min   ({} bot(s))\n",
                    v.len()
                ));
            }
        }
        if !r.ultima_falha.is_empty() {
            s.push_str("\núltima falha de cada ação:\n");
            for (a, d) in &r.ultima_falha {
                s.push_str(&format!("  {a:26} {d}\n"));
            }
        }
        s
    }
}

fn escapa(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

fn json_str(v: &str) -> String {
    let mut s = String::with_capacity(v.len() + 2);
    s.push('"');
    for c in v.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if (c as u32) < 0x20 => s.push_str(&format!("\\u{:04x}", c as u32)),
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Detalhe com aspas ou quebra de linha não pode estragar o JSONL.
    ///
    /// Uma linha quebrada torna o arquivo inteiro ilegível pra `jq`, e é
    /// justamente numa FALHA que o detalhe costuma trazer texto de erro com
    /// aspas — ou seja, o defeito apareceria exatamente quando o arquivo mais
    /// importa.
    #[test]
    fn detalhe_com_aspas_nao_quebra_a_linha() {
        assert_eq!(json_str(r#"erro: "x" não achado"#), r#""erro: \"x\" não achado""#);
        assert_eq!(json_str("linha\nnova"), r#""linha\nnova""#);
        assert!(!json_str("a\nb").contains('\n'), "quebra literal no JSONL");
    }

    #[test]
    fn a_fase_tem_nome_estavel() {
        // Os nomes são chave de agregação: mudar um quebra relatório antigo.
        assert_eq!(Fase::NoMundo.nome(), "no_mundo");
        assert_eq!(Fase::Cadastrando.nome(), "cadastrando");
    }
}
