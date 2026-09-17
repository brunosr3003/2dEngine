//! Pocoes de recurso com cura AO LONGO DO TEMPO, no molde das Large/Great HP
//! Potion da MIR4: uma parte na hora e o resto em ticks de 1 s, sempre em
//! fracao do MAXIMO (acompanha o nivel, ao contrario de valor fixo).
//!
//! A recarga e' por GRUPO — Vida e Vida+ dividem a mesma — e nunca e' menor
//! que a propria cura: nao existe tomar outra no meio. Isso esta' garantido
//! em tempo de compilacao logo abaixo da tabela, e o servidor recusa o
//! `UseItem` do grupo em recarga sem gastar a pocao.
//!
//! O servidor e' a autoridade; o cliente usa estas mesmas contas pro AUTO da
//! barra (escolher o tier) e pro HUD.
use crate::constants::item_id as it;

/// Quantos grupos de recarga existem.
pub const GRUPOS: usize = 3;

/// Intervalo entre ticks da cura, em segundos.
pub const TICK_S: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grupo {
    Vida = 0,
    Mana = 1,
    Vigor = 2,
}

impl Grupo {
    pub fn de_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Grupo::Vida),
            1 => Some(Grupo::Mana),
            2 => Some(Grupo::Vigor),
            _ => None,
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Grupo::Vida => "Vida",
            Grupo::Mana => "Mana",
            Grupo::Vigor => "Vigor",
        }
    }
}

/// Uma pocao: quanto cura na hora e por tick (fracao do maximo do recurso),
/// quantos ticks e a recarga do grupo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cura {
    pub grupo: Grupo,
    pub na_hora: f32,
    pub por_tick: f32,
    pub ticks: u8,
    /// Em segundos inteiros: e' o que deixa checar `recarga >= duracao` em
    /// tempo de compilacao sem aritmetica de ponto flutuante em const.
    pub recarga_s: u8,
}

impl Cura {
    pub fn duracao_s(&self) -> f32 {
        self.ticks as f32 * TICK_S as f32
    }

    /// Fracao total do maximo: a parte na hora mais todos os ticks.
    pub fn total(&self) -> f32 {
        self.na_hora + self.por_tick * self.ticks as f32
    }
}

/// A tabela (proposta da pesquisa sobre a MIR4; ver docs/ITENS.md).
pub const CURAS: [(u16, Cura); 5] = [
    (it::HEALTH_POTION, Cura { grupo: Grupo::Vida, na_hora: 0.04, por_tick: 0.02, ticks: 5, recarga_s: 8 }),
    (it::GREATER_HEAL, Cura { grupo: Grupo::Vida, na_hora: 0.06, por_tick: 0.03, ticks: 5, recarga_s: 8 }),
    (it::MANA_POTION, Cura { grupo: Grupo::Mana, na_hora: 0.05, por_tick: 0.03, ticks: 5, recarga_s: 8 }),
    (it::GREATER_MANA, Cura { grupo: Grupo::Mana, na_hora: 0.08, por_tick: 0.045, ticks: 5, recarga_s: 8 }),
    (it::STAMINA_POTION, Cura { grupo: Grupo::Vigor, na_hora: 0.0, por_tick: 0.05, ticks: 6, recarga_s: 15 }),
];

// Por construcao: nenhuma recarga menor que a propria cura. Se alguem baixar
// uma recarga abaixo da duracao, o crate nao compila.
const _: () = {
    let mut i = 0;
    while i < CURAS.len() {
        let c = CURAS[i].1;
        assert!(c.recarga_s as u32 >= c.ticks as u32 * TICK_S as u32, "recarga de pocao menor que a cura");
        i += 1;
    }
};

/// Pocao de recurso na loja do NPC se paga com COBRE, nao com ouro (pedido do
/// dono, 17/09/2026). O preco e' o mesmo numero de `items.buy_price`.
pub fn compra_com_cobre(item_id: u16) -> bool {
    cura_de(item_id).is_some()
}

pub fn cura_de(item_id: u16) -> Option<Cura> {
    CURAS.iter().find(|(id, _)| *id == item_id).map(|(_, c)| *c)
}

/// A pocao que o AUTO deve beber: a MENOR cujo total cobre `deficit` (fracao
/// que falta pro maximo); nenhuma cobre, a maior que houver. `familia` =
/// (item, quantidade na bolsa).
pub fn escolher(familia: &[(u16, u32)], deficit: f32) -> Option<u16> {
    let mut com: Vec<(u16, f32)> = familia
        .iter()
        .filter(|(_, q)| *q > 0)
        .filter_map(|(id, _)| cura_de(*id).map(|c| (*id, c.total())))
        .collect();
    com.sort_by(|a, b| a.1.total_cmp(&b.1));
    com.iter().find(|(_, t)| *t >= deficit).or_else(|| com.last()).map(|(id, _)| *id)
}

/// Cura em andamento de um grupo.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ativa {
    pub por_tick: f32,
    pub restantes: u8,
    /// Instante (tempo de simulacao) do proximo tick.
    pub proximo: f32,
}

/// Por que o servidor recusou.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Recusa {
    /// Grupo em recarga: faltam estes segundos. A pocao nao e' gasta.
    Recarga(f32),
    /// Recurso ja' cheio. A pocao nao e' gasta.
    Cheio,
}

/// Recargas e curas de um personagem, por grupo.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EstadoDePocoes {
    pub recarga_ate: [f32; GRUPOS],
    pub cura: [Option<Ativa>; GRUPOS],
}

impl EstadoDePocoes {
    pub fn recarga_restante(&self, g: Grupo, agora: f32) -> f32 {
        (self.recarga_ate[g as usize] - agora).max(0.0)
    }

    pub fn cura_restante(&self, g: Grupo, agora: f32) -> f32 {
        self.cura[g as usize].map_or(0.0, |a| {
            ((a.proximo - agora) + a.restantes.saturating_sub(1) as f32 * TICK_S as f32).max(0.0)
        })
    }

    /// Beber: recusa em recarga (ou cheio) sem mudar nada; senao liga a
    /// recarga e a cura do grupo e devolve a fracao que cura NA HORA.
    pub fn beber(&mut self, c: &Cura, agora: f32, cheio: bool) -> Result<f32, Recusa> {
        let g = c.grupo as usize;
        if agora < self.recarga_ate[g] {
            return Err(Recusa::Recarga(self.recarga_ate[g] - agora));
        }
        if cheio {
            return Err(Recusa::Cheio);
        }
        self.recarga_ate[g] = agora + c.recarga_s as f32;
        self.cura[g] = (c.ticks > 0).then(|| Ativa { por_tick: c.por_tick, restantes: c.ticks, proximo: agora + TICK_S as f32 });
        Ok(c.na_hora)
    }

    /// Um passo: fracao do maximo a curar AGORA, por grupo. Ticks atrasados
    /// (passo longo) saem todos de uma vez.
    pub fn tick(&mut self, agora: f32) -> [f32; GRUPOS] {
        let mut ganho = [0.0; GRUPOS];
        for g in 0..GRUPOS {
            let Some(a) = self.cura[g].as_mut() else { continue };
            while a.restantes > 0 && agora >= a.proximo {
                ganho[g] += a.por_tick;
                a.restantes -= 1;
                a.proximo += TICK_S as f32;
            }
            if a.restantes == 0 {
                self.cura[g] = None;
            }
        }
        ganho
    }

    /// Morreu ou caiu: a cura acaba (a recarga continua correndo).
    pub fn encerrar_curas(&mut self) {
        self.cura = [None; GRUPOS];
    }
}

#[cfg(test)]
mod testes {

    /// Pocao de vida, mana e vigor da loja do NPC custa cobre; equipamento, ouro.
    #[test]
    fn pocao_se_compra_com_cobre() {
        use crate::constants::item_id;
        for id in [item_id::HEALTH_POTION, item_id::MANA_POTION, item_id::GREATER_HEAL] {
            assert!(compra_com_cobre(id), "{id} deveria custar cobre");
        }
        assert!(!compra_com_cobre(401), "katana continua em ouro");
        assert!(!compra_com_cobre(item_id::COPPER));
    }

    use super::*;

    #[test]
    fn a_tabela_bate_com_o_planejado_e_recarga_cobre_a_cura() {
        let total = |id| cura_de(id).unwrap().total();
        assert!((total(it::HEALTH_POTION) - 0.14).abs() < 1e-4);
        assert!((total(it::GREATER_HEAL) - 0.21).abs() < 1e-4);
        assert!((total(it::MANA_POTION) - 0.20).abs() < 1e-4);
        assert!((total(it::GREATER_MANA) - 0.305).abs() < 1e-4);
        assert!((total(it::STAMINA_POTION) - 0.30).abs() < 1e-4);
        for (_, c) in CURAS {
            assert!(c.recarga_s as f32 >= c.duracao_s(), "{c:?}");
        }
        assert_eq!(cura_de(it::XP_POTION), None, "buff nao e' cura");
    }

    #[test]
    fn na_hora_mais_ticks_somam_o_total() {
        let c = cura_de(it::HEALTH_POTION).unwrap();
        let mut e = EstadoDePocoes::default();
        let mut soma = e.beber(&c, 10.0, false).unwrap();
        assert!((soma - 0.04).abs() < 1e-6);
        assert!((e.cura_restante(Grupo::Vida, 10.0) - 5.0).abs() < 1e-4);
        let mut t = 10.0;
        while t < 20.0 {
            t += 1.0 / 30.0;
            soma += e.tick(t)[0];
        }
        assert!((soma - c.total()).abs() < 1e-4, "{soma}");
        assert!(e.cura[0].is_none(), "acabou");
    }

    #[test]
    fn recarga_do_grupo_bloqueia_e_nao_substitui() {
        let vida = cura_de(it::HEALTH_POTION).unwrap();
        let forte = cura_de(it::GREATER_HEAL).unwrap();
        let mana = cura_de(it::MANA_POTION).unwrap();
        let mut e = EstadoDePocoes::default();
        e.beber(&vida, 0.0, false).unwrap();
        e.tick(2.0);
        let antes = e.clone();
        match e.beber(&forte, 2.0, false) {
            Err(Recusa::Recarga(s)) => assert!((s - 6.0).abs() < 1e-4),
            r => panic!("Vida+ na recarga da Vida: {r:?}"),
        }
        assert_eq!(e, antes, "recusa nao mexe em nada — sem substituicao");
        assert!(e.beber(&mana, 2.0, false).is_ok(), "outro grupo e' livre");
        assert!(e.beber(&forte, 8.0, false).is_ok(), "fim da recarga libera");
        assert_eq!(e.beber(&vida, 30.0, true), Err(Recusa::Cheio));
    }

    #[test]
    fn morrer_encerra_e_passo_longo_nao_perde_tick() {
        let c = cura_de(it::STAMINA_POTION).unwrap();
        let mut e = EstadoDePocoes::default();
        assert_eq!(e.beber(&c, 0.0, false), Ok(0.0));
        let g = e.tick(3.5);
        assert!((g[2] - 0.15).abs() < 1e-5, "3 ticks atrasados de uma vez: {g:?}");
        e.encerrar_curas();
        assert_eq!(e.tick(10.0), [0.0; GRUPOS]);
        assert!(e.recarga_restante(Grupo::Vigor, 10.0) > 0.0, "a recarga continua");
    }

    #[test]
    fn escolher_pega_a_menor_que_cobre() {
        let f = [(it::HEALTH_POTION, 3), (it::GREATER_HEAL, 2)];
        assert_eq!(escolher(&f, 0.10), Some(it::HEALTH_POTION));
        assert_eq!(escolher(&f, 0.18), Some(it::GREATER_HEAL));
        assert_eq!(escolher(&f, 0.60), Some(it::GREATER_HEAL), "nenhuma cobre: a maior");
        assert_eq!(escolher(&[(it::HEALTH_POTION, 3), (it::GREATER_HEAL, 0)], 0.60), Some(it::HEALTH_POTION));
        assert_eq!(escolher(&[], 0.5), None);
    }
}
