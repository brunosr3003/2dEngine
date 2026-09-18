//! Calendario de presenca (docs/CALENDARIO.md): um premio por dia de login,
//! no molde do "28-Day Check-in" do MIR4.
//!
//! - 28 premios por MES. O mes vira no dia 1 as 04:00 de Brasilia (o mesmo
//!   reset das diarias e dungeons, `dungeon::RESET_UTC_S`) e a grade volta ao
//!   dia 1, como no MIR4.
//! - Conta no RESGATE, nao no dia do calendario: dia perdido nao quebra nada,
//!   o proximo resgate pega o proximo premio da lista.
//! - Marcos nos dias 7, 14, 21 e 28.
//! - Por CONTA: o personagem que resgata recebe; os outros da conta veem o
//!   mesmo progresso.
//! - Eventos: calendario proprio com periodo, que complementa ou substitui o
//!   mensal. `EVENTOS` vem vazio (desligado).
//!
//! Premio e' so' o que ja' e' vinculado (Pocao de XP, Fortuna, Sorte, Marcas),
//! pocao que o Alquimista vende e ouro — nada vai pro mercado, nada de TP nem
//! de equipamento.

use serde::{Deserialize, Serialize};

use crate::constants::item_id;
use crate::dungeon::{dia, RESET_UTC_S};

/// Premios por ciclo.
pub const DIAS: usize = 28;
/// Dias com premio de marco.
pub const MARCOS: [u8; 4] = [7, 14, 21, 28];
/// `Premio::item_id` de ouro (vai pro `gold` do personagem, nao pra bolsa).
pub const OURO: u16 = 0;
/// `calendario` do mensal nas mensagens.
pub const MENSAL: u32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Premio {
    /// `OURO` = ouro.
    pub item_id: u16,
    /// 0 = espaco vazio.
    pub qtd: u32,
}

const fn p(item_id: u16, qtd: u32) -> Premio {
    Premio { item_id, qtd }
}
const NADA: Premio = p(OURO, 0);

pub type Dia = [Premio; 2];

/// A grade do mes. Valores ⚠️ iniciais (docs/CALENDARIO.md).
pub const CALENDARIO: [Dia; DIAS] = {
    use item_id::*;
    [
        [p(OURO, 500), NADA],
        [p(HEALTH_POTION, 10), NADA],
        [p(MANA_POTION, 10), NADA],
        [p(STAMINA_POTION, 5), NADA],
        [p(OURO, 800), NADA],
        [p(GREATER_HEAL, 5), NADA],
        [p(XP_POTION, 1), p(MARCAS_TEMPESTADE, 10)], // 7
        [p(OURO, 1_000), NADA],
        [p(HEALTH_POTION, 15), NADA],
        [p(GREATER_MANA, 5), NADA],
        [p(FORTUNA_POTION, 1), NADA],
        [p(OURO, 1_200), NADA],
        [p(GREATER_HEAL, 8), NADA],
        [p(XP_POTION, 2), p(SORTE_POTION, 1)], // 14
        [p(OURO, 1_500), NADA],
        [p(MANA_POTION, 15), NADA],
        [p(STAMINA_POTION, 8), NADA],
        [p(FORTUNA_POTION, 1), NADA],
        [p(OURO, 1_800), NADA],
        [p(GREATER_HEAL, 10), NADA],
        [p(XP_POTION, 2), p(MARCAS_TEMPESTADE, 20)], // 21
        [p(OURO, 2_000), NADA],
        [p(GREATER_MANA, 8), NADA],
        [p(SORTE_POTION, 1), NADA],
        [p(OURO, 2_500), NADA],
        [p(GREATER_HEAL, 12), NADA],
        [p(FORTUNA_POTION, 1), NADA],
        [p(XP_POTION, 3), p(MARCAS_TEMPESTADE, 40)], // 28
    ]
};

/// Itens que podem aparecer num calendario: vinculados ou vendidos pelo
/// Alquimista. O teste da grade (e de todo evento) confere contra esta lista.
pub const PERMITIDOS: [u16; 9] = [
    item_id::HEALTH_POTION,
    item_id::MANA_POTION,
    item_id::STAMINA_POTION,
    item_id::GREATER_HEAL,
    item_id::GREATER_MANA,
    item_id::XP_POTION,
    item_id::FORTUNA_POTION,
    item_id::SORTE_POTION,
    item_id::MARCAS_TEMPESTADE,
];

pub fn e_marco(dia_da_grade: u8) -> bool {
    MARCOS.contains(&dia_da_grade)
}

/// Um evento de presenca: grade propria valendo de `inicio_unix` ate'
/// `fim_unix` (exclusive). `substitui` = o mensal some enquanto dura.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventoDePresenca {
    /// > 0 (0 e' o mensal).
    pub id: u32,
    pub nome: &'static str,
    pub inicio_unix: i64,
    pub fim_unix: i64,
    pub substitui: bool,
    pub grade: &'static [Dia],
}

/// Eventos cadastrados. Vazio = so' o mensal.
pub const EVENTOS: &[EventoDePresenca] = &[];

pub fn eventos_ativos(
    unix: i64,
    eventos: &'static [EventoDePresenca],
) -> Vec<&'static EventoDePresenca> {
    eventos
        .iter()
        .filter(|e| unix >= e.inicio_unix && unix < e.fim_unix)
        .collect()
}

/// (ano, mes 1..12, dia 1..31) do dia `dias` desde 1970-01-01 (Hinnant).
pub fn civil(dias: i64) -> (i64, u32, u32) {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// O mes de jogo de `hoje` (`dungeon::dia`): ano×12 + mes.
pub fn mes(hoje: i64) -> i64 {
    let (y, m, _) = civil(hoje);
    y * 12 + (m as i64 - 1)
}

/// Unix do proximo reset (04:00 de Brasilia) depois de `unix`.
pub fn proximo_reset(unix: i64) -> i64 {
    (dia(unix) + 1) * 86_400 + RESET_UTC_S
}

/// O progresso num calendario.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Progresso {
    /// Mensal: `mes`. Evento: o id dele.
    pub ciclo: i64,
    /// Premios ja' resgatados neste ciclo.
    pub resgatados: u8,
    /// `dungeon::dia` do ultimo resgate (0 = nunca).
    pub ultimo_dia: i64,
}

impl Progresso {
    pub fn pode(&self, hoje: i64, tamanho: usize) -> bool {
        self.ultimo_dia != hoje && (self.resgatados as usize) < tamanho
    }
}

/// Por que o resgate nao saiu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recusa {
    JaResgatouHoje,
    CicloCompleto,
    SemCalendario,
}

impl Recusa {
    pub fn texto(self) -> &'static str {
        match self {
            Recusa::JaResgatouHoje => "Você já resgatou hoje. Volte depois do reset (04:00).",
            Recusa::CicloCompleto => "Calendário completo! O próximo começa no dia 1.",
            Recusa::SemCalendario => "Esse calendário não está ativo.",
        }
    }
}

/// O que a CONTA guarda (`presenca_contas`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DadosPresenca {
    pub mensal: Progresso,
    pub eventos: Vec<(u32, Progresso)>,
}

impl DadosPresenca {
    /// O mes virou: o mensal volta ao dia 1.
    pub fn virar(&mut self, hoje: i64) {
        let m = mes(hoje);
        if self.mensal.ciclo != m {
            self.mensal = Progresso {
                ciclo: m,
                ..Default::default()
            };
        }
    }

    fn progresso(&self, calendario: u32) -> Progresso {
        if calendario == MENSAL {
            return self.mensal;
        }
        self.eventos.iter().find(|e| e.0 == calendario).map_or(
            Progresso {
                ciclo: calendario as i64,
                ..Default::default()
            },
            |e| e.1,
        )
    }

    fn progresso_mut(&mut self, calendario: u32) -> &mut Progresso {
        if calendario == MENSAL {
            return &mut self.mensal;
        }
        if let Some(i) = self.eventos.iter().position(|e| e.0 == calendario) {
            return &mut self.eventos[i].1;
        }
        self.eventos.push((
            calendario,
            Progresso {
                ciclo: calendario as i64,
                ..Default::default()
            },
        ));
        &mut self.eventos.last_mut().unwrap().1
    }

    /// Resgata o proximo premio de `calendario`. Idempotente no dia: o
    /// segundo pedido do mesmo dia volta `JaResgatouHoje` sem dar nada.
    /// Devolve (dia da grade, 1..=N) e os premios.
    pub fn resgatar(
        &mut self,
        calendario: u32,
        unix: i64,
        eventos: &'static [EventoDePresenca],
    ) -> Result<(u8, Vec<Premio>), Recusa> {
        let hoje = dia(unix);
        self.virar(hoje);
        let grade = grade_ativa(calendario, unix, eventos).ok_or(Recusa::SemCalendario)?;
        let pr = self.progresso_mut(calendario);
        if pr.ultimo_dia == hoje {
            return Err(Recusa::JaResgatouHoje);
        }
        if pr.resgatados as usize >= grade.len() {
            return Err(Recusa::CicloCompleto);
        }
        let n = pr.resgatados as usize;
        pr.resgatados += 1;
        pr.ultimo_dia = hoje;
        let premios = grade[n].iter().copied().filter(|p| p.qtd > 0).collect();
        Ok((n as u8 + 1, premios))
    }

    /// O que a janela mostra agora.
    pub fn estado(&self, unix: i64, eventos: &'static [EventoDePresenca]) -> EstadoPresenca {
        let hoje = dia(unix);
        let mut d = self.clone();
        d.virar(hoje);
        let ativos = eventos_ativos(unix, eventos);
        let mut calendarios = Vec::new();
        if !ativos.iter().any(|e| e.substitui) {
            calendarios.push(CalendarioNet {
                id: MENSAL,
                nome: "Presença do mês".into(),
                resgatados: d.mensal.resgatados,
                pode_hoje: d.mensal.pode(hoje, DIAS),
                fim_unix: 0,
                grade: CALENDARIO.to_vec(),
            });
        }
        for e in ativos {
            let pr = d.progresso(e.id);
            calendarios.push(CalendarioNet {
                id: e.id,
                nome: e.nome.into(),
                resgatados: pr.resgatados,
                pode_hoje: pr.pode(hoje, e.grade.len()),
                fim_unix: e.fim_unix,
                grade: e.grade.to_vec(),
            });
        }
        EstadoPresenca {
            calendarios,
            proximo_reset_unix: proximo_reset(unix),
        }
    }
}

// ─────────────────────────────── resgates no banco ───────────────────────────────

/// Um resgate feito (uma linha de `presenca_resgates`, no banco do realm).
/// O banco e' a fonte da verdade: o estado da janela sai destas linhas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResgateFeito {
    pub calendario: u32,
    pub ciclo: i64,
    /// 1..=N.
    pub dia_grade: u8,
    /// `dungeon::dia` do resgate.
    pub dia_jogo: i64,
}

/// Mensal: o mes de jogo. Evento: o id dele (um ciclo so').
pub fn ciclo_de(calendario: u32, hoje: i64) -> i64 {
    if calendario == MENSAL {
        mes(hoje)
    } else {
        calendario as i64
    }
}

/// O proximo resgate de `calendario`, a partir do que a conta ja' fez.
#[derive(Debug, Clone, PartialEq)]
pub struct Plano {
    pub calendario: u32,
    pub ciclo: i64,
    pub dia_grade: u8,
    pub dia_jogo: i64,
    pub premios: Vec<Premio>,
}

/// Decide o resgate de agora. Pura: o servidor chama DENTRO da transacao que
/// trava a conta, com as linhas lidas nela.
pub fn planejar(
    calendario: u32,
    unix: i64,
    eventos: &'static [EventoDePresenca],
    feitos: &[ResgateFeito],
) -> Result<Plano, Recusa> {
    let hoje = dia(unix);
    let grade = grade_ativa(calendario, unix, eventos).ok_or(Recusa::SemCalendario)?;
    if feitos
        .iter()
        .any(|f| f.calendario == calendario && f.dia_jogo == hoje)
    {
        return Err(Recusa::JaResgatouHoje);
    }
    let ciclo = ciclo_de(calendario, hoje);
    let n = feitos
        .iter()
        .filter(|f| f.calendario == calendario && f.ciclo == ciclo)
        .count();
    if n >= grade.len() {
        return Err(Recusa::CicloCompleto);
    }
    Ok(Plano {
        calendario,
        ciclo,
        dia_grade: n as u8 + 1,
        dia_jogo: hoje,
        premios: grade[n].iter().copied().filter(|p| p.qtd > 0).collect(),
    })
}

impl DadosPresenca {
    /// O estado da conta a partir das linhas do banco.
    pub fn de_resgates(feitos: &[ResgateFeito], unix: i64) -> Self {
        let hoje = dia(unix);
        let progresso = |calendario: u32| {
            let ciclo = ciclo_de(calendario, hoje);
            let doc = feitos.iter().filter(|f| f.calendario == calendario);
            Progresso {
                ciclo,
                resgatados: doc
                    .clone()
                    .filter(|f| f.ciclo == ciclo)
                    .count()
                    .min(u8::MAX as usize) as u8,
                ultimo_dia: doc.map(|f| f.dia_jogo).max().unwrap_or(0),
            }
        };
        let mut ids: Vec<u32> = feitos
            .iter()
            .map(|f| f.calendario)
            .filter(|c| *c != MENSAL)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        DadosPresenca {
            mensal: progresso(MENSAL),
            eventos: ids.into_iter().map(|id| (id, progresso(id))).collect(),
        }
    }
}

/// A grade de `calendario` se ele vale agora.
pub fn grade_ativa(
    calendario: u32,
    unix: i64,
    eventos: &'static [EventoDePresenca],
) -> Option<&'static [Dia]> {
    let ativos = eventos_ativos(unix, eventos);
    if calendario == MENSAL {
        return (!ativos.iter().any(|e| e.substitui)).then_some(&CALENDARIO[..]);
    }
    ativos
        .into_iter()
        .find(|e| e.id == calendario)
        .map(|e| e.grade)
}

/// Todo item que algum calendario da (mensal e eventos), com os dias.
pub fn itens_com_dias(eventos: &'static [EventoDePresenca]) -> Vec<(u16, Vec<u8>)> {
    let mut v: Vec<(u16, Vec<u8>)> = Vec::new();
    let grades = std::iter::once(&CALENDARIO[..]).chain(eventos.iter().map(|e| e.grade));
    for grade in grades {
        for (i, d) in grade.iter().enumerate() {
            for pr in d.iter().filter(|p| p.qtd > 0 && p.item_id != OURO) {
                match v.iter_mut().find(|x| x.0 == pr.item_id) {
                    Some(x) => {
                        if !x.1.contains(&(i as u8 + 1)) {
                            x.1.push(i as u8 + 1);
                        }
                    }
                    None => v.push((pr.item_id, vec![i as u8 + 1])),
                }
            }
        }
    }
    v.sort_by_key(|x| x.0);
    v
}

// ─────────────────────────────── rede ───────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarioNet {
    /// `MENSAL` ou id do evento.
    pub id: u32,
    pub nome: String,
    pub resgatados: u8,
    pub pode_hoje: bool,
    /// Evento: quando acaba. Mensal: 0.
    pub fim_unix: i64,
    pub grade: Vec<Dia>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstadoPresenca {
    pub calendarios: Vec<CalendarioNet>,
    pub proximo_reset_unix: i64,
}

impl EstadoPresenca {
    pub fn tem_resgate(&self) -> bool {
        self.calendarios.iter().any(|c| c.pode_hoje)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PedidoPresenca {
    Estado,
    Resgatar { calendario: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AvisoPresenca {
    Estado(EstadoPresenca),
    Resgatou {
        calendario: u32,
        dia: u8,
        premios: Vec<Premio>,
        no_correio: u8,
    },
    Recusado {
        texto: String,
    },
}

/// `CartaDeCorreio::motivo` do premio que nao coube na bolsa.
pub const MOTIVO_CORREIO: u8 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    /// Unix de (ano, mes, dia, hora UTC).
    fn unix(y: i64, m: u32, d: u32, h: i64) -> i64 {
        // dias desde 1970 pela inversa de `civil`: busca simples.
        let mut dias = (y - 1970) * 365 + (y - 1969) / 4 - 40;
        while civil(dias) < (y, m, d) {
            dias += 1;
        }
        while civil(dias) > (y, m, d) {
            dias -= 1;
        }
        dias * 86_400 + h * 3600
    }

    #[test]
    fn civil_bate_com_datas_conhecidas() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(19_723), (2024, 1, 1));
        assert_eq!(civil(20_711), (2026, 9, 15));
    }

    #[test]
    fn um_por_dia_e_idempotente() {
        let mut d = DadosPresenca::default();
        let t = unix(2026, 9, 15, 12);
        let (n, premios) = d.resgatar(MENSAL, t, EVENTOS).unwrap();
        assert_eq!((n, premios), (1, vec![CALENDARIO[0][0]]));
        assert_eq!(
            d.resgatar(MENSAL, t + 60, EVENTOS),
            Err(Recusa::JaResgatouHoje)
        );
        assert_eq!(d.mensal.resgatados, 1);
    }

    #[test]
    fn dia_perdido_nao_quebra_a_sequencia() {
        let mut d = DadosPresenca::default();
        d.resgatar(MENSAL, unix(2026, 9, 2, 12), EVENTOS).unwrap();
        // Pula tres dias: o proximo resgate e' o dia 2 da grade.
        let (n, _) = d.resgatar(MENSAL, unix(2026, 9, 6, 12), EVENTOS).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn reset_as_quatro_de_brasilia() {
        let mut d = DadosPresenca::default();
        // 06:59 UTC = 03:59 em Brasilia: ainda e' o "dia" anterior.
        d.resgatar(MENSAL, unix(2026, 9, 16, 6) + 59 * 60, EVENTOS)
            .unwrap();
        assert_eq!(
            d.resgatar(MENSAL, unix(2026, 9, 16, 6) + 3599, EVENTOS),
            Err(Recusa::JaResgatouHoje)
        );
        // 07:00 UTC = 04:00: dia novo.
        assert!(d.resgatar(MENSAL, unix(2026, 9, 16, 7), EVENTOS).is_ok());
        assert_eq!(proximo_reset(unix(2026, 9, 16, 7)), unix(2026, 9, 17, 7));
    }

    #[test]
    fn mes_novo_volta_ao_dia_um() {
        let mut d = DadosPresenca::default();
        for dd in 1..=20 {
            d.resgatar(MENSAL, unix(2026, 9, dd, 12), EVENTOS).unwrap();
        }
        assert_eq!(d.mensal.resgatados, 20);
        // 1º de outubro antes das 04:00 ainda e' setembro.
        d.resgatar(MENSAL, unix(2026, 10, 1, 5), EVENTOS).unwrap();
        assert_eq!(d.mensal.resgatados, 21);
        let (n, _) = d.resgatar(MENSAL, unix(2026, 10, 1, 8), EVENTOS).unwrap();
        assert_eq!(n, 1, "virou o mes as 04:00 do dia 1");
    }

    #[test]
    fn ciclo_completo_para_no_28() {
        let mut d = DadosPresenca::default();
        for dd in 1..=28 {
            d.resgatar(MENSAL, unix(2026, 7, dd, 12), EVENTOS).unwrap();
        }
        assert_eq!(
            d.resgatar(MENSAL, unix(2026, 7, 29, 12), EVENTOS),
            Err(Recusa::CicloCompleto)
        );
        assert!(!d.estado(unix(2026, 7, 30, 12), EVENTOS).tem_resgate());
    }

    #[test]
    fn marcos_sao_os_melhores_e_a_grade_so_tem_permitido() {
        let valor = |d: &Dia| d.iter().filter(|p| p.qtd > 0 && p.item_id != OURO).count();
        for (i, d) in CALENDARIO.iter().enumerate() {
            let n = i as u8 + 1;
            if e_marco(n) {
                assert_eq!(valor(d), 2, "dia {n}: marco com dois premios");
                assert!(d.iter().any(|p| p.item_id == item_id::XP_POTION));
            } else {
                assert!(d[1].qtd == 0, "dia {n}: dia comum tem um premio so'");
            }
            for pr in d.iter().filter(|p| p.qtd > 0 && p.item_id != OURO) {
                assert!(
                    PERMITIDOS.contains(&pr.item_id),
                    "dia {n}: item {} fora da lista",
                    pr.item_id
                );
                assert!(crate::equip_slot_of(pr.item_id).is_none());
            }
        }
    }

    static GRADE_TESTE: [Dia; 3] = [
        [p(OURO, 1), NADA],
        [p(item_id::XP_POTION, 1), NADA],
        [p(item_id::SORTE_POTION, 1), NADA],
    ];
    static EV_COMPLEMENTA: [EventoDePresenca; 1] = [EventoDePresenca {
        id: 7,
        nome: "Festival",
        inicio_unix: 1_000_000_000,
        fim_unix: 3_000_000_000,
        substitui: false,
        grade: &GRADE_TESTE,
    }];
    static EV_SUBSTITUI: [EventoDePresenca; 1] = [EventoDePresenca {
        id: 9,
        nome: "Aniversário",
        inicio_unix: 1_000_000_000,
        fim_unix: 3_000_000_000,
        substitui: true,
        grade: &GRADE_TESTE,
    }];

    #[test]
    fn evento_complementa_ou_substitui() {
        let t = unix(2026, 9, 15, 12);
        let e = DadosPresenca::default().estado(t, &EV_COMPLEMENTA);
        assert_eq!(
            e.calendarios.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![MENSAL, 7]
        );
        let mut d = DadosPresenca::default();
        d.resgatar(MENSAL, t, &EV_COMPLEMENTA).unwrap();
        assert!(
            d.resgatar(7, t, &EV_COMPLEMENTA).is_ok(),
            "evento tem progresso proprio"
        );

        let s = DadosPresenca::default().estado(t, &EV_SUBSTITUI);
        assert_eq!(
            s.calendarios.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![9]
        );
        assert_eq!(
            DadosPresenca::default().resgatar(MENSAL, t, &EV_SUBSTITUI),
            Err(Recusa::SemCalendario)
        );
        // Fora do periodo o evento some.
        assert_eq!(
            DadosPresenca::default().resgatar(7, 10, &EV_COMPLEMENTA),
            Err(Recusa::SemCalendario)
        );
    }

    #[test]
    fn planejar_pelas_linhas_do_banco() {
        let t = unix(2026, 9, 15, 12);
        let hoje = dia(t);
        let p = planejar(MENSAL, t, EVENTOS, &[]).unwrap();
        assert_eq!((p.dia_grade, p.dia_jogo, p.ciclo), (1, hoje, mes(hoje)));
        let feito = ResgateFeito {
            calendario: MENSAL,
            ciclo: p.ciclo,
            dia_grade: 1,
            dia_jogo: hoje,
        };
        assert_eq!(
            planejar(MENSAL, t, EVENTOS, &[feito]),
            Err(Recusa::JaResgatouHoje)
        );
        let amanha = planejar(MENSAL, t + 86_400, EVENTOS, &[feito]).unwrap();
        assert_eq!(amanha.dia_grade, 2);
        // Linha de mes passado nao conta no ciclo novo.
        let velho = ResgateFeito {
            calendario: MENSAL,
            ciclo: mes(hoje) - 1,
            dia_grade: 9,
            dia_jogo: hoje - 40,
        };
        assert_eq!(planejar(MENSAL, t, EVENTOS, &[velho]).unwrap().dia_grade, 1);
        let d = DadosPresenca::de_resgates(&[feito, velho], t);
        assert_eq!((d.mensal.resgatados, d.mensal.ultimo_dia), (1, hoje));
        assert!(!d.estado(t, EVENTOS).tem_resgate());
        assert!(d.estado(t + 86_400, EVENTOS).tem_resgate());
    }

    #[test]
    fn itens_do_calendario_com_os_dias() {
        let v = itens_com_dias(EVENTOS);
        let xp = v.iter().find(|x| x.0 == item_id::XP_POTION).unwrap();
        assert_eq!(xp.1, vec![7, 14, 21, 28]);
        assert!(v.iter().all(|x| x.0 != OURO));
    }
}
