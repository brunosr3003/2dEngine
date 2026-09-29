//! THE TRAIL: everything a bot does becomes a line, and nothing is lost.
//!
//! The request was "all of this has to be trackable". Really trackable means
//! two different things, and both are here:
//!
//! 1. **What happened**, in order, with a time: a JSONL, one line per event.
//!    It is what answers "why did bot 3 jam at 14:12" — a question an
//! aggregated counter never answers.
//! 2. **How it is going**, as a number: the summary, which answers "how long
//!    does it take to reach level 10" and "how many quests per hour".
//!
//! JSONL and not a database: the file is appendable, survives the process
//! crashing, and can be read with `tail -f` while it runs. A database would
//! demand a schema and a migration for data that is disposable by nature.

use std::io::Write;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// At what point in its life the bot is. It is the axis that separates "the
/// bot does not do quests" from "the bot never got into the world at all".
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
    /// A short, stable verb: `quest_aceita`, `mob_morto`, `item_vendido`. Stable
    /// because it is what gets aggregated later.
    pub acao: &'a str,
    pub ok: bool,
    /// Free-form, for whoever reads it with their eyes.
    pub detalhe: String,
    /// Numbers of the state at that moment — it is what lets the curve be rebuilt.
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
    /// Count per action, which is the aggregate that answers "how many per hour".
    pub por_acao: std::collections::BTreeMap<String, u64>,
    /// When each bot reached each level, in seconds since the start. It is the
    /// progression curve — the number the owner wants to see to know whether the
    /// game is calibrated.
    pub nivel_em_s: std::collections::BTreeMap<u32, Vec<f64>>,
    /// The last failure of each action, so there is no need to hunt in the file.
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

    /// A detail with quotes or a line break must not spoil the JSONL.
    ///
    /// One broken line makes the whole file unreadable to `jq`, and it is
    /// precisely on a FAILURE that the detail tends to carry error text with
    /// quotes — that is, the defect would appear exactly when the file matters most.
    #[test]
    fn detalhe_com_aspas_nao_quebra_a_linha() {
        assert_eq!(json_str(r#"erro: "x" não achado"#), r#""erro: \"x\" não achado""#);
        assert_eq!(json_str("linha\nnova"), r#""linha\nnova""#);
        assert!(!json_str("a\nb").contains('\n'), "quebra literal no JSONL");
    }

    #[test]
    fn a_fase_tem_nome_estavel() {
        // The names are aggregation keys: changing one breaks an old report.
        assert_eq!(Fase::NoMundo.nome(), "no_mundo");
        assert_eq!(Fase::Cadastrando.nome(), "cadastrando");
    }
}
