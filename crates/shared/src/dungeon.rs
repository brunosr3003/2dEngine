//! Dungeons COM FIM: o Porao (solo) e a Gruta (grupo de ate' 5), com
//! estagios, entradas por dia, ressurreicao com espera e o bau de conclusao
//! (docs/DUNGEONS_E_RAIDS.md, "Decisoes").
//!
//! Tudo aqui e' regra pura: o catalogo, o que trava cada estagio, quanto se
//! espera pra reviver, quantas entradas o dia da', o que o bau rola e as
//! mensagens de rede. Quem roda a instancia e' o servidor
//! (`server::world::dungeon`); quem desenha e' o cliente (`dungeon_ui`).

use serde::{Deserialize, Serialize};

use crate::chaves;
use crate::components::PlayerStats;
use crate::constants::item_id;
use crate::forja::Grau;
use crate::items::ItemInstance;

// ─────────────────────────────── catalogo ───────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tipo {
    /// Solo, sem estagio. Entrada ilimitada, recompensa 3/dia.
    Porao,
    /// Grupo por andares, estagios 1–5. 2 entradas/dia (acumula 4).
    Gruta,
    /// Raid: ainda nao existe (cadeado "em breve").
    Cacada,
}

impl Tipo {
    pub fn nome(self) -> &'static str {
        match self {
            Tipo::Porao => "Porão",
            Tipo::Gruta => "Cavern",
            Tipo::Cacada => "Hunt",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conteudo {
    pub id: u16,
    pub nome: &'static str,
    pub tipo: Tipo,
    /// Ilha (`DefIlha::zona`) onde a entrada fica.
    pub zona: &'static str,
    /// Nivel minimo do estagio 1 (e dos inimigos dele).
    pub nivel_min: u32,
    pub grupo_max: u8,
    /// Limite da instancia, em segundos.
    pub limite_s: u32,
    /// Andares de inimigos ANTES do andar do chefe.
    pub andares: u8,
    /// Chefe final: o kind do catalogo de chefes (corpo e golpes
    /// telegrafados), no nivel do estagio.
    pub chefe: u16,
    /// `false` = aparece com cadeado (ilha que nao existe, raid).
    pub disponivel: bool,
}

pub const CONTEUDOS: &[Conteudo] = &[
    Conteudo {
        id: 1,
        nome: "Shipwreck Cellar",
        tipo: Tipo::Porao,
        zona: "ilha_inicial",
        nivel_min: 6,
        grupo_max: 1,
        limite_s: 600,
        andares: 2,
        chefe: 11,
        disponivel: true,
    },
    Conteudo {
        id: 2,
        nome: "Smuggler's Cellar",
        tipo: Tipo::Porao,
        zona: "ilha_inicial",
        nivel_min: 14,
        grupo_max: 1,
        limite_s: 600,
        andares: 2,
        chefe: 12,
        disponivel: true,
    },
    Conteudo {
        id: 3,
        nome: "Frozen Hull",
        tipo: Tipo::Porao,
        zona: "ilha_gelo",
        nivel_min: 22,
        grupo_max: 1,
        limite_s: 600,
        andares: 2,
        chefe: 13,
        disponivel: true,
    },
    Conteudo {
        id: 10,
        nome: "Sea Wolves' Den",
        tipo: Tipo::Gruta,
        zona: "ilha_inicial",
        nivel_min: 10,
        grupo_max: 5,
        limite_s: 1500,
        andares: 3,
        chefe: 10,
        disponivel: true,
    },
    Conteudo {
        id: 11,
        nome: "Deep Ice Caverns",
        tipo: Tipo::Gruta,
        zona: "ilha_gelo",
        nivel_min: 20,
        grupo_max: 5,
        limite_s: 1500,
        andares: 3,
        chefe: 14,
        disponivel: true,
    },
    Conteudo {
        id: 12,
        nome: "Tomb of the Salt Sands",
        tipo: Tipo::Gruta,
        zona: "ilha_deserto",
        nivel_min: 30,
        grupo_max: 5,
        limite_s: 1500,
        andares: 3,
        chefe: 15,
        disponivel: true,
    },
    Conteudo {
        id: 13,
        nome: "Monastery of the Winds",
        tipo: Tipo::Gruta,
        zona: "ilha_planalto",
        nivel_min: 40,
        grupo_max: 5,
        limite_s: 1500,
        andares: 3,
        chefe: 17,
        disponivel: true,
    },
    Conteudo {
        id: 14,
        nome: "Forge of the Titan",
        tipo: Tipo::Gruta,
        zona: "ilha_planalto",
        nivel_min: 50,
        grupo_max: 5,
        limite_s: 1500,
        andares: 3,
        chefe: 18,
        disponivel: true,
    },
    Conteudo {
        id: 15,
        nome: "Ship Graveyard",
        tipo: Tipo::Gruta,
        zona: "recife_tempestade",
        nivel_min: 60,
        grupo_max: 5,
        limite_s: 1500,
        andares: 3,
        chefe: 11,
        disponivel: false,
    },
    Conteudo {
        id: 20,
        nome: "Mother of Blizzards",
        tipo: Tipo::Cacada,
        zona: "ilha_gelo",
        nivel_min: 25,
        grupo_max: 10,
        limite_s: 1800,
        andares: 0,
        chefe: 13,
        disponivel: false,
    },
];

pub fn conteudo(id: u16) -> Option<&'static Conteudo> {
    CONTEUDOS.iter().find(|c| c.id == id)
}

/// Estagios que o conteudo tem. Porao nao tem estagio (1 so').
pub fn estagios(c: &Conteudo) -> u8 {
    if c.tipo == Tipo::Porao {
        1
    } else {
        5
    }
}

/// Nivel dos inimigos e minimo do jogador no estagio `e` (1-based).
pub fn nivel_do_estagio(c: &Conteudo, e: u8) -> u32 {
    c.nivel_min + 2 * e.saturating_sub(1) as u32
}

/// Poder minimo, em % do poder de referencia, por estagio.
///
/// A referencia e' o personagem ESPERADO do nivel (ladder), equipado na
/// faixa a +0 — nao mais o pelado com a arma inicial. Por isso as fracoes
/// cairam: o estagio 1 deixa entrar quem esta' meia faixa atras, e so' o 5
/// pede a ladder inteira.
pub const PODER_PCT: [u32; 5] = [55, 65, 80, 90, 100];

/// O mesmo numero do "Power" da ficha do cliente.
pub fn poder_de_stats(s: &PlayerStats) -> i32 {
    s.attack_damage * 10
        + s.defense * 8
        + s.hp_max
        + s.mp_max / 2
        + (s.dex + s.wis) * 5
        + (s.crit_chance * 1000.0) as i32
}

/// Poder de referencia de um nivel: o do personagem ESPERADO na ladder
/// (docs/ESCADA.md) — ataque, defesa e vida da faixa a +0, na mesma conta
/// da ficha. E' por isso que ele se compara com o poder do jogador: "poder
/// 2.900" nao diz nada, "2.900 de 2.900 esperados" diz tudo.
///
/// Era `base x 1,25 + 10 por nivel`, calibrado pro pelado passar no
/// estagio 1 — e no nivel 34 dava 4.300 enquanto qualquer jogador real
/// tinha o dobro. Porta que ninguem encosta nao e' porta.
pub fn poder_referencia(nivel: u32) -> i32 {
    crate::ladder::power(nivel)
}

pub fn poder_minimo(c: &Conteudo, e: u8) -> i32 {
    let pct = PODER_PCT[(e.clamp(1, 5) - 1) as usize] as i64;
    (poder_referencia(nivel_do_estagio(c, e)) as i64 * pct / 100) as i32
}

/// O estagio 5 de conteudo 60+ so' entra com Selo da Tempestade.
pub fn exige_selo(c: &Conteudo, e: u8) -> bool {
    e >= 5 && c.nivel_min >= 60
}

/// Papel do bau de conclusao no `EntityMeta.kind` (`shared::npc_kind`): o
/// cliente desenha um bau e o toque manda `Interact`. O papel mora nos 7 bits
/// de cima de um u16 (`npc_kind` desloca 9): tem que ser <= 127, senao estoura
/// e o cliente le' outro papel.
pub const PAPEL_BAU: u8 = 120;
/// Depois da vitoria, a instancia fica aberta este tanto (pra juntar o saque
/// do chao; o que sobrar vai pra quem estiver la').
pub const FECHA_DEPOIS_DE_VENCER_S: f32 = 120.0;

/// Selos craftados por semana, POR CONTA.
pub const SELOS_POR_SEMANA: u8 = 2;

/// Por que um estagio esta' fechado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Cadeado {
    EmBreve,
    Nivel(u32),
    Poder {
        tem: i32,
        precisa: i32,
    },
    /// Vencer o estagio anterior primeiro.
    Estagio(u8),
    Selo,
}

impl Cadeado {
    pub fn texto(&self) -> String {
        match self {
            Cadeado::EmBreve => "Coming soon".into(),
            Cadeado::Nivel(n) => format!("Requires level {n}"),
            Cadeado::Poder { tem, precisa } => format!("Power {tem}/{precisa}"),
            Cadeado::Estagio(e) => format!("Beat stage {e}"),
            Cadeado::Selo => "Requires a Storm Seal".into(),
        }
    }
}

/// O que trava o estagio `e` pra este personagem. `liberado` = maior estagio
/// ja' vencido (0 = nenhum). Entrada zerada NAO trava: entra como Ajudante.
pub fn cadeado(
    c: &Conteudo,
    e: u8,
    nivel: u32,
    poder: i32,
    liberado: u8,
    tem_selo: bool,
) -> Option<Cadeado> {
    if !c.disponivel || e == 0 || e > estagios(c) {
        return Some(Cadeado::EmBreve);
    }
    let precisa = nivel_do_estagio(c, e);
    if nivel < precisa {
        return Some(Cadeado::Nivel(precisa));
    }
    if e > liberado.saturating_add(1) {
        return Some(Cadeado::Estagio(e - 1));
    }
    let minimo = poder_minimo(c, e);
    if poder < minimo {
        return Some(Cadeado::Poder {
            tem: poder,
            precisa: minimo,
        });
    }
    if exige_selo(c, e) && !tem_selo {
        return Some(Cadeado::Selo);
    }
    None
}

// ─────────────────────────────── na instancia ───────────────────────────────

/// Espera pra reviver depois da `mortes`-esima morte na instancia: 10 s, 20 s…
pub fn espera_reviver_s(mortes: u32) -> u32 {
    10 * mortes.max(1)
}

/// Vida dos inimigos pelo tamanho do grupo ⚠️ (grupo cheio = 100%).
pub fn vida_por_grupo(c: &Conteudo, membros: usize) -> f32 {
    if c.grupo_max <= 1 {
        return 1.0;
    }
    match membros {
        0 | 1 => 0.45,
        2 => 0.55,
        3 => 0.70,
        4 => 0.85,
        _ => 1.0,
    }
}

/// (vida, dano) do chefe final sobre os numeros do catalogo ⚠️. O catalogo e'
/// o chefe de campo, feito pra grupo: o Porao (solo) leva uma fracao.
pub fn escala_do_chefe(c: &Conteudo, membros: usize) -> (f32, f32) {
    match c.tipo {
        Tipo::Porao => (0.18, 0.6),
        _ => (vida_por_grupo(c, membros), 1.0),
    }
}

/// Quantos inimigos o andar `andar` (0-based) tem.
pub fn inimigos_do_andar(c: &Conteudo, andar: u8) -> u8 {
    match c.tipo {
        Tipo::Porao => 5 + andar * 2,
        _ => 7 + andar * 2,
    }
}

/// O andar do meio da Gruta tem semi-chefe.
pub fn tem_semi_chefe(c: &Conteudo, andar: u8) -> bool {
    c.tipo == Tipo::Gruta && andar == 1
}

/// Matou o chefe antes de 60% do limite: +50% de Marcas e 1 rolagem extra de
/// material. Nunca peca nem chave.
pub fn bonus_tempo(tempo_s: u32, limite_s: u32) -> bool {
    tempo_s as u64 * 10 <= limite_s as u64 * 6
}

/// Minimo pra fila automatica fechar grupo depois de esperar (decisao 3).
pub fn minimo_da_fila(c: &Conteudo, e: u8) -> u8 {
    if c.grupo_max <= 1 {
        1
    } else if e <= 2 {
        3
    } else {
        4
    }
}

// ─────────────────────────────── tempo: dia e semana ───────────────────────────────

/// Reset diario as 04:00 de Brasilia (07:00 UTC).
pub const RESET_UTC_S: i64 = 7 * 3600;

pub fn dia(unix: i64) -> i64 {
    (unix - RESET_UTC_S).div_euclid(86_400)
}

/// Semana vira na quarta as 04:00 de Brasilia. 1970-01-07 foi quarta.
pub fn semana(unix: i64) -> i64 {
    (unix - 6 * 86_400 - RESET_UTC_S).div_euclid(7 * 86_400)
}

// ─────────────────────────────── entradas ───────────────────────────────

pub const GRUTA_POR_DIA: u8 = 2;
pub const GRUTA_ACUMULA: u8 = 4;
/// Quantas entradas se compra por dia ANTES de o preco comecar a triplicar.
///
/// Nao e' mais um TETO: ate' 21/09/2026 a terceira compra do dia era
/// recusada com "sem mais entradas a' venda hoje", e o dono pediu que
/// houvesse como comprar mais. O limite agora e' economico — o preco triplica
/// a cada compra, e o ouro e' raro (docs/ECONOMIA.md: so' chefe, bau, mercado,
/// venda ao NPC e calendario). A quinta compra do dia num personagem de nivel
/// 20 custa 200 mil.
///
/// Um teto ECONOMICO e' melhor que um de contagem porque ele nao mente: o
/// jogador que quiser muito pode, e paga por isso.
pub const GRUTA_COMPRAS_POR_DIA: u8 = 2;

/// Quantas compras o preco aguenta antes de estourar o `u64`. Com base de
/// ~2.500 e fator 3, a 30ª compra ja' passa de 10^15 — o corte existe pra o
/// numero nao virar lixo, nao pra limitar ninguem.
pub const COMPRAS_ATE_O_ABSURDO: u8 = 24;
/// APOSENTADA em 29/09/2026. O Porão não tem mais cota diária: a chave que se
/// fabrica (`crate::porao`) é o freio, e ela é o freio INTEIRO — sem ela não se
/// entra, e com ela a recompensa é cheia todas as vezes.
///
/// Fica registrada porque o número conta uma história: enquanto existiu, o teto
/// do que o Porão despejava na economia era 3 baús por dia por personagem. Quem
/// for mexer no custo da chave está mexendo nesse teto.
pub const PORAO_RECOMPENSAS_POR_DIA_APOSENTADA: u8 = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entradas {
    /// Dia (`dia`) da ultima atualizacao; 0 = nunca.
    pub dia: i64,
    pub saldo: u8,
    pub compradas: u8,
}

impl Entradas {
    /// Vira o dia: Gruta ganha 2 por dia passado (ate' 4); Porao volta a 3.
    pub fn atualizar(&mut self, tipo: Tipo, hoje: i64) {
        if self.dia == hoje {
            return;
        }
        match tipo {
            // APOSENTADO em 29/09/2026: o Porão virou dungeon física e não tem
            // mais cota — entra e ganha quantas vezes tiver chave
            // (`crate::porao`). O braço fica porque `Entradas` é struct salva
            // no banco e o campo `porao` continua lá; o que sumiu foi o uso.
            Tipo::Porao => {}
            _ => {
                let dias = if self.dia == 0 {
                    1
                } else {
                    (hoje - self.dia).clamp(0, 2) as u8
                };
                self.saldo = self
                    .saldo
                    .saturating_add(GRUTA_POR_DIA * dias)
                    .min(GRUTA_ACUMULA);
            }
        }
        self.compradas = 0;
        self.dia = hoje;
    }

    /// Gasta uma. `false` = estava zerada (entra como Ajudante, sem peca nem chave).
    pub fn consumir(&mut self) -> bool {
        if self.saldo == 0 {
            return false;
        }
        self.saldo -= 1;
        true
    }

    /// Preco em gold da proxima entrada comprada hoje.
    ///
    /// `None` so' pro Porao (que nao vende) e pro absurdo aritmetico. O preco
    /// TRIPLICA a cada compra do dia: a primeira e' barata, a quarta ja'
    /// morde, e a decima e' impagavel. Quem manda no limite e' a carteira.
    pub fn preco_da_compra(&self, tipo: Tipo, nivel: u32) -> Option<u64> {
        if tipo != Tipo::Gruta || self.compradas >= COMPRAS_ATE_O_ABSURDO {
            return None;
        }
        let base = 500 + 100 * nivel as u64;
        Some(base.saturating_mul(3u64.saturating_pow(self.compradas as u32)))
    }

    pub fn comprar(&mut self) {
        self.compradas += 1;
        self.saldo = self.saldo.saturating_add(1);
    }
}

// ─────────────────────────────── estado salvo ───────────────────────────────

/// O que o PERSONAGEM guarda (`characters.dungeon_json`). Vai no mesmo save
/// da bolsa: bau aberto e item recebido nunca se separam.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DadosDungeon {
    /// Relogio UTC para sobreviver a reconexao e troca de canal.
    pub montaria_bloqueada_ate_ms: u64,
    pub gruta: Entradas,
    pub porao: Entradas,
    /// (conteudo, maior estagio vencido).
    pub liberado: Vec<(u16, u8)>,
    /// (conteudo, estagio, vitorias).
    pub vitorias: Vec<(u16, u8, u32)>,
    /// Baus ja' abertos (id unico da instancia): abrir de novo nao da' nada.
    pub baus_abertos: Vec<u64>,
    /// Recompensa que nao coube na bolsa e 1ª vitoria.
    pub correio: Vec<CartaDeCorreio>,
    pub proxima_carta: u64,
    /// A zona de onde o jogador foi pra ARENA, pra onde ele volta.
    ///
    /// Mora AQUI, e não na sessão, pelo motivo que a Ilha Mágica ja' aprendeu:
    /// cada zona e' outro processo, entao a memoria de quem embarcou nao
    /// atravessa o handoff junto com ele. E mora neste JSON, e nao numa coluna
    /// nova, porque coluna nova e' quatro edicoes de UPSERT — e uma que se
    /// esquece derruba o save inteiro sem avisar. Vazio = volta pro Bosque.
    pub arena_volta: String,
}

pub const BAUS_LEMBRADOS: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CartaDeCorreio {
    pub id: u64,
    pub item_id: u16,
    pub qtd: u32,
    pub instance: Option<ItemInstance>,
    /// 0 bolsa cheia, 1 primeira vitoria semanal, 2 primeira vitoria de todas.
    pub motivo: u8,
    pub quando: i64,
}

impl DadosDungeon {
    pub fn entradas(&mut self, tipo: Tipo) -> &mut Entradas {
        if tipo == Tipo::Porao {
            &mut self.porao
        } else {
            &mut self.gruta
        }
    }

    pub fn liberado(&self, conteudo: u16) -> u8 {
        self.liberado
            .iter()
            .find(|l| l.0 == conteudo)
            .map_or(0, |l| l.1)
    }

    pub fn vitorias(&self, conteudo: u16, estagio: u8) -> u32 {
        self.vitorias
            .iter()
            .find(|v| v.0 == conteudo && v.1 == estagio)
            .map_or(0, |v| v.2)
    }

    /// Venceu: libera o proximo e conta. `true` = primeira vez nesse estagio.
    pub fn registrar_vitoria(&mut self, conteudo: u16, estagio: u8) -> bool {
        match self.liberado.iter_mut().find(|l| l.0 == conteudo) {
            Some(l) => l.1 = l.1.max(estagio),
            None => self.liberado.push((conteudo, estagio)),
        }
        match self
            .vitorias
            .iter_mut()
            .find(|v| v.0 == conteudo && v.1 == estagio)
        {
            Some(v) => {
                v.2 += 1;
                false
            }
            None => {
                self.vitorias.push((conteudo, estagio, 1));
                true
            }
        }
    }

    /// Marca o bau como aberto. `false` = ja' tinha aberto (idempotente).
    pub fn abrir_bau(&mut self, bau: u64) -> bool {
        if self.baus_abertos.contains(&bau) {
            return false;
        }
        self.baus_abertos.push(bau);
        let sobra = self.baus_abertos.len().saturating_sub(BAUS_LEMBRADOS);
        self.baus_abertos.drain(..sobra);
        true
    }

    pub fn postar(
        &mut self,
        item_id: u16,
        qtd: u32,
        instance: Option<ItemInstance>,
        motivo: u8,
        quando: i64,
    ) {
        self.proxima_carta += 1;
        self.correio.push(CartaDeCorreio {
            id: self.proxima_carta,
            item_id,
            qtd,
            instance,
            motivo,
            quando,
        });
    }
}

/// O que a CONTA guarda (`dungeon_contas`): o que vale em qualquer
/// personagem — primeira vitoria semanal e o teto de Selo.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DadosConta {
    pub semana: i64,
    /// (conteudo, estagio) ja' vencidos nesta semana.
    pub primeiras: Vec<(u16, u8)>,
    pub selos: u8,
}

impl DadosConta {
    pub fn virar(&mut self, semana: i64) {
        if self.semana != semana {
            *self = DadosConta {
                semana,
                ..Default::default()
            };
        }
    }

    /// `true` = e' a primeira vitoria da semana nesse estagio (e marca).
    pub fn primeira_da_semana(&mut self, conteudo: u16, estagio: u8) -> bool {
        if self.primeiras.contains(&(conteudo, estagio)) {
            return false;
        }
        self.primeiras.push((conteudo, estagio));
        true
    }

    pub fn pode_craftar_selo(&self) -> bool {
        self.selos < SELOS_POR_SEMANA
    }
}

// ─────────────────────────────── recompensa ───────────────────────────────

/// Um premio do bau.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Premio {
    pub item_id: u16,
    pub qtd: u32,
    /// Peca de equipamento: o grau e o nivel da instancia a rolar (vinculada).
    pub peca: Option<(Grau, u16)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bau {
    pub itens: Vec<Premio>,
    pub marcas: u32,
}

/// Teto duro de grau pelo nivel do estagio: Epico so' 40+, Lendario so' 50+.
///
/// Era 60 e 80. Desceu em 28/09/2026 junto com `shared::chaves` (epica no 40,
/// lendaria no 50), a pedido do dono: "epic is lvl 40 and legendary lvl 50 —
/// the craft and equip". Chave e PECA tinham que contar a mesma historia; com
/// o teto velho a dungeon de 50 dava chave lendaria e peça so' ate' Rara.
pub fn teto_de_grau(nivel: u32) -> Grau {
    if nivel >= 50 {
        Grau::Lendario
    } else if nivel >= 40 {
        Grau::Epico
    } else {
        Grau::Raro
    }
}

/// (chance de peca, distribuicao em ‰ de Comum..Lendario) — docs seção 8.
pub fn tabela_de_peca(tipo: Tipo, nivel: u32, estagio: u8) -> (f32, [u32; 5]) {
    let alto = estagio >= 3;
    let topo = estagio >= 5;
    let (chance, dist) = match nivel {
        0..=9 => (0.20, [1000, 0, 0, 0, 0]),
        10..=19 => {
            if alto {
                (0.30, [500, 480, 20, 0, 0])
            } else {
                (0.25, [750, 250, 0, 0, 0])
            }
        }
        20..=29 => {
            if alto {
                (0.30, [150, 750, 100, 0, 0])
            } else {
                (0.25, [550, 420, 30, 0, 0])
            }
        }
        30..=39 => {
            if alto {
                (0.30, [0, 550, 450, 0, 0])
            } else {
                (0.25, [0, 800, 200, 0, 0])
            }
        }
        // DE 40 PRA CIMA a ladder desceu 20 niveis em 28/09/2026: as FORMAS
        // que estavam em 60-69, 70-79 e 80+ passaram pra 40-49, 50-59 e 60+,
        // sem numero novo inventado. E' o que faz "Epico no 40, Lendario no
        // 50" valer na PECA e nao so' no teto — `teto_de_grau` sozinho nao
        // mudava nada, porque a distribuicao nunca dava peso a Epico antes do
        // 60. O Raro puro (a linha de 1000 que morava no 50-59) sai: e' o
        // degrau que a faixa nova ocupa.
        40..=49 => {
            if topo {
                (0.35, [0, 0, 800, 200, 0])
            } else if alto {
                (0.30, [0, 0, 920, 80, 0])
            } else {
                (0.25, [0, 0, 980, 20, 0])
            }
        }
        50..=59 => {
            if topo {
                (0.35, [0, 0, 400, 595, 5])
            } else if alto {
                (0.30, [0, 0, 650, 350, 0])
            } else {
                (0.25, [0, 0, 850, 150, 0])
            }
        }
        _ => {
            if topo {
                (0.35, [0, 0, 0, 995, 5])
            } else {
                (0.30, [0, 0, 500, 500, 0])
            }
        }
    };
    // O Porao nao tem estagio: a primeira linha da faixa, chance de 20%.
    if tipo == Tipo::Porao {
        (0.20, dist)
    } else {
        (chance, dist)
    }
}

/// Rola um grau na distribuicao e corta no teto.
pub fn rolar_grau(dist: [u32; 5], r: f32, teto: Grau) -> Grau {
    let total: u32 = dist.iter().sum::<u32>().max(1);
    let alvo = (r.clamp(0.0, 0.999_999) * total as f32) as u32;
    let mut acc = 0;
    let mut g = Grau::Comum;
    for (i, p) in dist.iter().enumerate() {
        acc += p;
        if alvo < acc {
            g = Grau::TODOS[i];
            break;
        }
    }
    g.min(teto)
}

/// Nivel da instancia pra a peca sair no grau pedido (`tier_from_ilvl`).
pub fn nivel_da_peca(grau: Grau, nivel: u32) -> u16 {
    // O PISO de Epico (46) e Lendario (71) NAO desceu com a faixa do bau.
    //
    // Tentei descer pra 40/50 e o invariante `o nivel da peca da' o grau`
    // reprovou: `items::tier_from_ilvl(40)` e' Raro, entao uma peca Epica de
    // nivel de item 40 seria uma peca cuja cor nao bate com o proprio nivel, e
    // esse par e' testado. Descer `tier_from_ilvl` junto resolveria — e
    // reescalaria TODA peca do jogo (e' o eixo de `escala_do_roll`,
    // docs/ESCADA.md), o que e' outra mudanca, bem maior.
    //
    // Consequencia de ficar como esta': a dungeon de nivel 40 entrega Epico de
    // nivel de item 46 — seis a frente do conteudo. Cor mais rara vindo um
    // pouco adiantada e' defensavel; o contrario (cor sem nivel que a sustente)
    // nao seria.
    let (lo, hi) = match grau {
        Grau::Comum => (1, 10),
        Grau::Fino => (11, 25),
        Grau::Raro => (26, 45),
        Grau::Epico => (46, 70),
        Grau::Lendario => (71, 100),
    };
    (nivel as u16).clamp(lo, hi)
}

/// Uma peca sorteada. Se a categoria for arma, sorteia de novo entre os
/// quatro tipos de arma; a primeira vitoria nao fica presa a um modelo.
pub fn sortear_peca(grau: Grau, nivel: u32, r: f32, r_arma: f32) -> Premio {
    let n = (item_id::CINTO - item_id::ESPADA_E_ESCUDO + 1) as f32;
    let pos = (r.clamp(0.0, 0.999_999) * n) as u16;
    let id = if pos < 4 {
        const ARMAS: [u16; 4] = [
            item_id::ESPADA_E_ESCUDO,
            item_id::KATANA,
            item_id::PISTOLAS,
            item_id::ANEL_MAGICO,
        ];
        ARMAS[(r_arma.clamp(0.0, 0.999_999) * ARMAS.len() as f32) as usize]
    } else {
        item_id::ESPADA_E_ESCUDO + pos
    };
    Premio {
        item_id: id,
        qtd: 1,
        peca: Some((grau, nivel_da_peca(grau, nivel))),
    }
}

/// Cor do material pela faixa (1 cinza, 2 verde, 3 azul) e o multiplicador.
fn cor_do_material(nivel: u32, r: f32) -> (u8, u32) {
    let (verde, azul, mult) = match nivel {
        0..=9 => (0.0, 0.0, 1),
        10..=19 => (0.10, 0.0, 1),
        20..=29 => (0.40, 0.0, 1),
        30..=39 => (0.80, 0.20, 1),
        40..=49 => (0.55, 0.45, 1),
        50..=59 => (0.20, 0.80, 1),
        60..=69 => (0.0, 1.0, 1),
        70..=79 => (0.0, 1.0, 2),
        _ => (0.0, 1.0, 2),
    };
    let cor = if r < azul {
        3
    } else if r < azul + verde {
        2
    } else {
        1
    };
    (cor, mult)
}

fn darksteel_e_po(nivel: u32) -> ((u32, u32), (f32, u32, u32)) {
    match nivel {
        0..=9 => ((50, 100), (0.0, 0, 0)),
        10..=19 => ((150, 300), (0.10, 1, 1)),
        20..=29 => ((400, 800), (0.25, 1, 1)),
        30..=39 => ((1_000, 2_000), (0.40, 1, 2)),
        40..=49 => ((2_500, 5_000), (0.60, 2, 2)),
        50..=59 => ((6_000, 12_000), (1.0, 2, 3)),
        60..=69 => ((15_000, 25_000), (1.0, 3, 5)),
        70..=79 => ((30_000, 50_000), (1.0, 5, 8)),
        _ => ((60_000, 100_000), (1.0, 8, 12)),
    }
}

fn entre(r: f32, lo: u32, hi: u32) -> u32 {
    lo + ((r.clamp(0.0, 0.999_999) * (hi - lo + 1) as f32) as u32).min(hi - lo)
}

const MATERIAIS_DO_BAU: [u16; 8] = [
    item_id::STEEL,
    item_id::PLATINUM,
    item_id::DARK_HEART_STONE,
    item_id::MOON_SHADOW_STONE,
    item_id::QUINTESSENCE,
    item_id::EXORCISM_BAUBLE,
    item_id::ILLUMINATING_FRAGMENT,
    item_id::ANIMA_STONE,
];

fn rolar_material(nivel: u32, rng: &mut dyn FnMut() -> f32, metade: bool) -> Premio {
    let (cor, mult) = cor_do_material(nivel, rng());
    let base =
        MATERIAIS_DO_BAU[((rng().clamp(0.0, 0.999_999)) * MATERIAIS_DO_BAU.len() as f32) as usize];
    let mut qtd = entre(rng(), 20, 40) * mult;
    if metade {
        qtd = (qtd / 2).max(1);
    }
    Premio {
        item_id: item_id::na_cor(base, cor),
        qtd,
        peca: None,
    }
}

/// Marcas da conclusao ⚠️: Gruta 10, Porao 4; Ajudante 60%; bonus de tempo +50%.
pub fn marcas(tipo: Tipo, ajudante: bool, bonus: bool) -> u32 {
    let base = if tipo == Tipo::Porao { 4.0 } else { 10.0 };
    let a = if ajudante { 0.6 } else { 1.0 };
    let b = if bonus { 1.5 } else { 1.0 };
    (base * a * b as f32).round() as u32
}

/// O bau de conclusao de um membro.
///
/// Ajudante (entrou com a entrada zerada): cobre, material e Marcas, sem peca
/// nem chave. Estagio 5 com Selo dobra darksteel e po. O teto de grau vale
/// sempre.
pub fn rolar_bau(
    c: &Conteudo,
    estagio: u8,
    ajudante: bool,
    bonus: bool,
    rng: &mut dyn FnMut() -> f32,
) -> Bau {
    let nivel = nivel_do_estagio(c, estagio);
    let porao = c.tipo == Tipo::Porao;
    let mut itens = Vec::new();
    let cobre = (200 + 30 * nivel) / if porao { 2 } else { 1 };
    itens.push(Premio {
        item_id: item_id::COPPER,
        qtd: cobre,
        peca: None,
    });
    let ((ds_lo, ds_hi), (po_chance, po_lo, po_hi)) = darksteel_e_po(nivel);
    let selo = exige_selo(c, estagio);
    let mut ds = entre(rng(), ds_lo, ds_hi) / if porao { 2 } else { 1 };
    if selo {
        ds *= 2;
    }
    itens.push(Premio {
        item_id: item_id::DARKSTEEL,
        qtd: ds.max(1),
        peca: None,
    });
    if po_chance > 0.0 && rng() < po_chance {
        let mut po = entre(rng(), po_lo, po_hi);
        if selo {
            po *= 2;
        }
        itens.push(Premio {
            item_id: item_id::GLITTERING_POWDER,
            qtd: po,
            peca: None,
        });
    }
    // OURO do bau: com as marcas, e' o que faz a dungeon valer ouro (o resto
    // da economia corre em cobre — docs/ECONOMIA.md).
    itens.push(Premio {
        item_id: crate::item_id::GOLD,
        qtd: ouro_do_bau(c.tipo, nivel, ajudante),
        peca: None,
    });
    itens.push(rolar_material(nivel, rng, porao));
    if bonus {
        itens.push(rolar_material(nivel, rng, porao));
    }
    if !ajudante {
        let (chance, dist) = tabela_de_peca(c.tipo, nivel, estagio);
        if rng() < chance {
            let g = rolar_grau(dist, rng(), teto_de_grau(nivel));
            itens.push(sortear_peca(g, nivel, rng(), rng()));
        }
        if let Some(chave) = chaves::rolar(nivel, chaves::Fonte::Dungeon, 1.0, rng(), rng()) {
            itens.push(Premio {
                item_id: chave,
                qtd: 1,
                peca: None,
            });
        }
    }
    Bau {
        itens,
        marcas: marcas(c.tipo, ajudante, bonus),
    }
}

/// Ouro do bau de conclusao: pela faixa do estagio e pelo tipo. Ajudante (quem
/// ja' venceu esta semana) leva menos, como nas marcas.
pub fn ouro_do_bau(tipo: Tipo, nivel: u32, ajudante: bool) -> u32 {
    let base = match tipo {
        Tipo::Porao => 150,
        Tipo::Gruta => 400,
        _ => 600,
    };
    let bruto = base + nivel * 25;
    if ajudante {
        bruto * 6 / 10
    } else {
        bruto
    }
}

/// Peca garantida da 1ª vitoria (semanal ou de todas): rolada uma linha
/// ACIMA na coluna de grau, mas o teto da faixa vale sempre.
pub fn peca_garantida(c: &Conteudo, estagio: u8, rng: &mut dyn FnMut() -> f32) -> Premio {
    let nivel = nivel_do_estagio(c, estagio);
    let (_, dist) = tabela_de_peca(c.tipo, nivel, estagio);
    let teto = teto_de_grau(nivel);
    let g = rolar_grau(dist, rng(), teto);
    let g = g.acima().unwrap_or(g).min(teto);
    sortear_peca(g, nivel, rng(), rng())
}

/// Graus possíveis da peça garantida, exatamente como `peca_garantida`.
pub fn graus_da_primeira(c: &Conteudo, estagio: u8) -> Vec<Grau> {
    let nivel = nivel_do_estagio(c, estagio);
    let (_, dist) = tabela_de_peca(c.tipo, nivel, estagio);
    let teto = teto_de_grau(nivel);
    let mut graus = Vec::new();
    for (i, peso) in dist.into_iter().enumerate() {
        if peso == 0 { continue; }
        let g = Grau::TODOS[i].min(teto);
        let g = g.acima().unwrap_or(g).min(teto);
        if !graus.contains(&g) { graus.push(g); }
    }
    graus
}

// ─────────────────────────────── rede ───────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pedido {
    /// Estado da janela (conteudos, cadeados, entradas, fila, sala).
    Estado,
    /// Porao pelo PAINEL. Aposentado em 29/09/2026, quando o Porao virou
    /// dungeon fisica: quem entra e' `AbrirPorao`, na porta.
    ///
    /// A variante fica pra nao renumerar o enum e derrubar cliente antigo por
    /// nada — o servidor responde a ela com o convite de ir ate' a porta.
    EntrarSolo {
        conteudo: u16,
    },
    /// Porao pela PORTA, no cenario: a entrada fisica.
    ///
    /// Vem sem posicao de proposito. Quem diz onde o jogador esta' e' o
    /// servidor, que ja' tem a entidade dele; posicao mandada pelo cliente e'
    /// posicao que se mente, e aqui ela decide se uma porta abre.
    AbrirPorao {
        conteudo: u16,
    },
    FilaEntrar {
        conteudo: u16,
        estagio: u8,
    },
    FilaSair,
    SalaCriar {
        conteudo: u16,
        estagio: u8,
        completar_pela_fila: bool,
    },
    SalasBuscar {
        conteudo: u16,
        estagio: u8,
    },
    SalaEntrar {
        sala: u32,
    },
    SalaSair,
    /// So' o lider: abre o pronto-check com quem esta' na sala.
    SalaIniciar,
    Pronto {
        partida: u32,
        aceito: bool,
    },
    ComprarEntrada,
    /// A porta embaixo do relogio: sai e a entrada fica gasta.
    Sair,
    Reviver,
    AbrirBau {
        eid: u64,
    },
    Correio,
    CorreioRetirar {
        id: u64,
    },
    /// Gruta sem grupo: entra direto, sozinho, no estagio. A vida dos
    /// inimigos cai com o grupo (`vida_por_grupo`). Anexado no fim.
    GrutaSolo {
        conteudo: u16,
        estagio: u8,
    },
    /// Leva o jogador pra ARENA, a zona onde as dungeons acontecem.
    ///
    /// A fila e as salas vivem em UM processo só (`arena::ZONA`) — é o que
    /// faz elas serem as mesmas pra todo mundo, sem sincronizar nada. Quem
    /// está noutra zona não as enxerga, e por isso vai até lá, como quem
    /// entra num saguão.
    ///
    /// ANEXADO NO FIM: postcard é posicional, e variante no meio desloca o
    /// discriminante de todas as seguintes.
    IrParaArena,
    /// Volta da Arena pra zona de onde veio.
    SairDaArena,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConteudoEstado {
    pub id: u16,
    pub liberado: u8,
    /// Um por estagio: `None` = aberto.
    pub cadeados: Vec<Option<Cadeado>>,
    pub vitorias: u32,
    /// Uma entrada por estágio; a estreia é controlada por personagem.
    pub primeiras_concluidas: Vec<bool>,
    /// A primeira vitória semanal é controlada por conta.
    pub semanais_recebidas: Vec<bool>,
    /// Catálogo efetivo do servidor: tabela ativa + saque extra do chefe.
    pub drops_chefe: Vec<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntradasNet {
    pub gruta: u8,
    pub gruta_preco: Option<u64>,
    pub porao: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MembroNet {
    /// `Nome@REALM`.
    pub nome: String,
    pub lider: bool,
    pub aceitou: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SalaNet {
    pub id: u32,
    pub conteudo: u16,
    pub estagio: u8,
    pub membros: Vec<MembroNet>,
    pub vagas: u8,
    pub completar_pela_fila: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilaNet {
    pub conteudo: u16,
    pub estagio: u8,
    pub esperando_s: u32,
    pub na_fila: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MembroDaInstancia {
    pub nome: String,
    pub vivo: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartaNet {
    pub id: u64,
    pub item_id: u16,
    pub qtd: u32,
    pub motivo: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Aviso {
    Estado {
        conteudos: Vec<ConteudoEstado>,
        entradas: EntradasNet,
        fila: Option<FilaNet>,
        sala: Option<SalaNet>,
    },
    Salas {
        lista: Vec<SalaNet>,
    },
    Pronto {
        partida: u32,
        conteudo: u16,
        estagio: u8,
        membros: Vec<MembroNet>,
        expira_s: u8,
    },
    ProntoFechou {
        partida: u32,
        texto: String,
    },
    /// A cada segundo dentro da instancia.
    Instancia {
        conteudo: u16,
        estagio: u8,
        /// 0-based; `andar == andares` e' o andar do chefe.
        andar: u8,
        andares: u8,
        restante_s: u32,
        inimigos: u16,
        reviver_em_s: Option<u16>,
        membros: Vec<MembroDaInstancia>,
        concluida: bool,
    },
    Resultado {
        conteudo: u16,
        estagio: u8,
        vitoria: bool,
        tempo_s: u32,
        bonus_tempo: bool,
        primeira_vitoria: bool,
        #[serde(default)]
        recompensas_primeira: Vec<(u16, u32)>,
    },
    Bau {
        itens: Vec<(u16, u32)>,
        marcas: u32,
        no_correio: u8,
    },
    Saiu,
    Correio {
        cartas: Vec<CartaNet>,
    },
    Texto {
        ok: bool,
        texto: String,
    },
    /// O pedido exige estar na ARENA, e o jogador não está.
    ///
    /// A tela usa isto pra oferecer a ida em vez de falhar em silêncio: um
    /// botão que não faz nada é pior que um botão que explica.
    PrecisaDaArena,
    /// Está na Arena? A tela muda: aqui a fila e as salas funcionam.
    NaArena {
        dentro: bool,
    },
}

#[cfg(test)]
mod testes {
    use super::*;

    fn gruta() -> &'static Conteudo {
        conteudo(10).unwrap()
    }

    fn rng(seed: u64) -> impl FnMut() -> f32 {
        let mut s = seed;
        move || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 11) as f32 / (1u64 << 53) as f32
        }
    }

    #[test]
    fn previa_da_primeira_cobre_o_sorteio_real_em_todos_os_estagios() {
        for c in CONTEUDOS {
            for e in 1..=estagios(c) {
                let previstos = graus_da_primeira(c,e);
                assert!(!previstos.is_empty());
                for seed in 0..200 {
                    let premio = peca_garantida(c,e,&mut rng(seed));
                    assert!(previstos.contains(&premio.peca.unwrap().0));
                    assert!((item_id::ESPADA_E_ESCUDO..=item_id::CINTO).contains(&premio.item_id));
                }
            }
        }
    }

    #[test]
    fn catalogo_com_ids_unicos_e_ilhas_que_existem() {
        let mut ids: Vec<u16> = CONTEUDOS.iter().map(|c| c.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CONTEUDOS.len());
        for c in CONTEUDOS.iter().filter(|c| c.disponivel) {
            assert!(
                crate::terreno::def_da_zona(c.zona).is_some(),
                "{} numa ilha que nao existe",
                c.nome
            );
            assert!(
                crate::bosses::chefe(c.chefe).is_some(),
                "{} sem chefe",
                c.nome
            );
        }
        assert!(
            conteudo(20).is_some_and(|c| c.tipo == Tipo::Cacada && !c.disponivel),
            "raid com cadeado"
        );
    }

    #[test]
    fn estagios_sobem_dois_niveis_e_liberam_em_ordem() {
        let c = gruta();
        assert_eq!(estagios(c), 5);
        assert_eq!((nivel_do_estagio(c, 1), nivel_do_estagio(c, 5)), (10, 18));
        let forte = 1_000_000;
        assert_eq!(cadeado(c, 1, 9, forte, 0, false), Some(Cadeado::Nivel(10)));
        assert_eq!(cadeado(c, 1, 10, forte, 0, false), None);
        assert_eq!(
            cadeado(c, 2, 30, forte, 0, false),
            Some(Cadeado::Estagio(1)),
            "tem que vencer o 1"
        );
        assert_eq!(cadeado(c, 2, 30, forte, 1, false), None);
        assert_eq!(
            cadeado(c, 3, 30, forte, 5, false),
            None,
            "liberou o 5, nao precisa repetir o 3"
        );
        assert!(matches!(
            cadeado(c, 1, 10, 10, 0, false),
            Some(Cadeado::Poder { .. })
        ));
        assert_eq!(estagios(conteudo(1).unwrap()), 1, "Porao sem estagio");
        assert_eq!(
            cadeado(conteudo(20).unwrap(), 1, 99, forte, 0, false),
            Some(Cadeado::EmBreve)
        );
    }

    #[test]
    fn poder_minimo_sobe_por_estagio_e_um_personagem_do_nivel_passa() {
        let c = gruta();
        let p: Vec<i32> = (1..=5).map(|e| poder_minimo(c, e)).collect();
        assert!(p.windows(2).all(|w| w[1] > w[0]), "{p:?}");
        // Quem esta' meia faixa atras (a ladder de cinco niveis abaixo)
        // passa no estagio 1; quem esta' na ladder passa no 5.
        let n = c.nivel_min;
        assert!(
            crate::ladder::power(n.saturating_sub(5)) >= poder_minimo(c, 1),
            "a porta tranca quem esta' quase no nivel: {} < {}",
            crate::ladder::power(n.saturating_sub(5)),
            poder_minimo(c, 1)
        );
        assert!(crate::ladder::power(nivel_do_estagio(c, 5)) >= poder_minimo(c, 5));
        // O pelado com a arma inicial NAO passa: a porta voltou a ser porta.
        let pelado = poder_de_stats(&crate::components::base_player_stats());
        assert!(pelado + 80 < poder_minimo(c, 1));
    }

    #[test]
    fn selo_so_no_topo_de_conteudo_60() {
        let sessenta = Conteudo {
            nivel_min: 60,
            disponivel: true,
            ..*gruta()
        };
        assert!(exige_selo(&sessenta, 5));
        assert!(!exige_selo(&sessenta, 4));
        assert!(!exige_selo(gruta(), 5), "Gruta do Bosque nao pede Selo");
        assert_eq!(
            cadeado(&sessenta, 5, 99, 1_000_000, 4, false),
            Some(Cadeado::Selo)
        );
        assert_eq!(cadeado(&sessenta, 5, 99, 1_000_000, 4, true), None);
        let mut conta = DadosConta::default();
        conta.virar(10);
        assert!(conta.pode_craftar_selo());
        conta.selos = SELOS_POR_SEMANA;
        assert!(!conta.pode_craftar_selo());
        conta.virar(11);
        assert!(conta.pode_craftar_selo(), "a semana vira e o teto volta");
    }

    #[test]
    fn papel_do_bau_volta_do_kind() {
        assert_eq!(
            crate::npc_papel_de_kind(crate::npc_kind(None, PAPEL_BAU)),
            PAPEL_BAU
        );
    }

    /// O chefe do Porao (solo) sai com uma fracao da vida e do dano do
    /// catalogo; a Gruta cheia, com tudo.
    #[test]
    fn chefe_do_porao_e_mais_fraco() {
        let (v, d) = escala_do_chefe(conteudo(1).unwrap(), 1);
        assert!(v < 0.5 && d < 1.0);
        assert_eq!(escala_do_chefe(conteudo(10).unwrap(), 5), (1.0, 1.0));
    }

    #[test]
    fn espera_de_reviver_cresce_dez_por_morte() {
        assert_eq!([1, 2, 3, 7].map(espera_reviver_s), [10, 20, 30, 70]);
    }

    #[test]
    fn bonus_de_tempo_ate_sessenta_por_cento() {
        assert!(bonus_tempo(900, 1500));
        assert!(!bonus_tempo(901, 1500));
    }

    #[test]
    fn entradas_do_dia_acumulam_ate_quatro_e_a_compra_triplica() {
        let mut e = Entradas::default();
        e.atualizar(Tipo::Gruta, 100);
        assert_eq!(e.saldo, 2);
        assert!(e.consumir() && e.consumir());
        assert!(!e.consumir(), "zerou: entra como ajudante");
        e.atualizar(Tipo::Gruta, 103);
        assert_eq!(e.saldo, 4, "tres dias fora acumulam so' ate' 4");
        e.atualizar(Tipo::Gruta, 103);
        assert_eq!(e.saldo, 4, "mesmo dia nao da' de novo");

        // O PRECO e' o limite, e nao a contagem. Ate' 21/09/2026 a terceira
        // compra do dia era recusada; o dono pediu que houvesse como comprar
        // mais, entao o teto virou economico.
        let base = e.preco_da_compra(Tipo::Gruta, 10).unwrap();
        let mut anterior = base;
        for n in 1..6 {
            e.comprar();
            let p = e
                .preco_da_compra(Tipo::Gruta, 10)
                .unwrap_or_else(|| panic!("a compra {n} foi recusada"));
            assert_eq!(p, anterior * 3, "a compra {n} tinha que triplicar");
            anterior = p;
        }
        // Cinco compras depois o preco ja' e' 243x o da primeira: quem
        // quiser muito pode, e paga por isso.
        assert_eq!(anterior, base * 243);
        // E o saldo cresceu com as compras.
        assert!(e.saldo >= 4);

        // O PORÃO NÃO TEM MAIS COTA NENHUMA pra virar o dia (29/09/2026):
        // ele virou dungeon física e o freio passou a ser a chave que se
        // fabrica (`crate::porao`). `atualizar` não mexe mais no saldo dele, e
        // continuar sem venda de entrada é consequência disso, não regra à
        // parte — não há entrada pra vender.
        let mut p = Entradas::default();
        p.atualizar(Tipo::Porao, 5);
        assert_eq!(p.saldo, 0, "o Porão voltou a ganhar cota diária");
        assert_eq!(p.preco_da_compra(Tipo::Porao, 10), None);
    }

    /// O preco nunca vira lixo por estouro de inteiro.
    ///
    /// Triplicar sem parar estoura o `u64` por volta da 40ª compra, e um
    /// preco que dá a volta no zero seria uma entrada de graça — o oposto do
    /// que o preco crescente existe pra fazer.
    #[test]
    fn o_preco_cresce_sempre_e_nunca_da_a_volta() {
        let mut e = Entradas::default();
        e.atualizar(Tipo::Gruta, 1);
        let mut anterior = 0u64;
        for _ in 0..COMPRAS_ATE_O_ABSURDO {
            let p = e.preco_da_compra(Tipo::Gruta, 60).expect("dentro do corte");
            assert!(p > anterior, "o preco caiu: {anterior} -> {p}");
            anterior = p;
            e.comprar();
        }
        assert_eq!(
            e.preco_da_compra(Tipo::Gruta, 60),
            None,
            "depois do corte a venda para, em vez de dar um numero sem sentido"
        );
    }

    #[test]
    fn dia_e_semana_viram_as_quatro_da_manha_de_brasilia() {
        // 2026-09-16 (quarta) 06:59 UTC = 03:59 em Brasilia: ainda terca.
        let quarta_0659 = 1_789_541_940;
        assert_eq!(dia(quarta_0659) + 1, dia(quarta_0659 + 60));
        assert_eq!(semana(quarta_0659) + 1, semana(quarta_0659 + 60));
        assert_eq!(
            semana(quarta_0659 + 60),
            semana(quarta_0659 + 60 + 6 * 86_400)
        );
    }

    #[test]
    fn vitoria_libera_o_proximo_e_bau_abre_uma_vez() {
        let mut d = DadosDungeon::default();
        assert!(d.registrar_vitoria(10, 1));
        assert!(!d.registrar_vitoria(10, 1));
        assert_eq!((d.liberado(10), d.vitorias(10, 1)), (1, 2));
        assert!(d.abrir_bau(77));
        assert!(!d.abrir_bau(77), "abrir de novo nao da' nada");
        let json = serde_json::to_string(&d).unwrap();
        let mut volta: DadosDungeon = serde_json::from_str(&json).unwrap();
        assert!(
            !volta.abrir_bau(77),
            "reiniciar o servidor nao reabre o bau"
        );
        let vazio: DadosDungeon = serde_json::from_str("{}").unwrap();
        assert!(
            vazio.correio.is_empty()
                && vazio.baus_abertos.is_empty()
                && vazio.gruta == Entradas::default()
        );
    }

    #[test]
    fn primeira_vitoria_semanal_e_por_conta() {
        let mut c = DadosConta::default();
        c.virar(3);
        assert!(c.primeira_da_semana(10, 2));
        assert!(
            !c.primeira_da_semana(10, 2),
            "outro personagem da mesma conta: nao repete"
        );
        assert!(c.primeira_da_semana(10, 3));
        c.virar(4);
        assert!(c.primeira_da_semana(10, 2));
    }

    /// 80 mil baus por faixa: o grau do bau nunca passa de `teto_de_grau`.
    ///
    /// O nome dizia "antes do 60 / antes do 80" e o corpo repetia os numeros;
    /// quando a faixa desceu pra 40/50 em 28/09/2026 o teste reprovou por estar
    /// desatualizado, e nao por o bau estar errado. Agora sai do proprio teto.
    #[test]
    fn bau_nunca_passa_do_teto_de_grau() {
        let mut r = rng(0xBA_0001);
        for &(nivel_min, tipo) in &[
            (6u32, Tipo::Porao),
            (10, Tipo::Gruta),
            (30, Tipo::Gruta),
            (50, Tipo::Gruta),
            (60, Tipo::Gruta),
            (70, Tipo::Gruta),
            (80, Tipo::Gruta),
        ] {
            let c = Conteudo {
                nivel_min,
                tipo,
                disponivel: true,
                ..*gruta()
            };
            for estagio in 1..=estagios(&c) {
                let nivel = nivel_do_estagio(&c, estagio);
                let mut maior = Grau::Comum;
                for _ in 0..(80_000 / estagios(&c) as u32) {
                    for p in rolar_bau(&c, estagio, false, true, &mut r).itens {
                        if let Some((g, ilvl)) = p.peca {
                            maior = maior.max(g);
                            assert_eq!(
                                crate::items::tier_from_ilvl(ilvl),
                                g as u8,
                                "o nivel da peca da' o grau"
                            );
                        }
                    }
                    let g = peca_garantida(&c, estagio, &mut r).peca.unwrap().0;
                    maior = maior.max(g);
                }
                assert!(
                    maior <= teto_de_grau(nivel),
                    "nv {nivel}: {maior:?} passa do teto {:?}",
                    teto_de_grau(nivel)
                );
            }
        }
    }

    #[test]
    fn chave_do_bau_usa_a_tabela_de_dungeon_e_ajudante_nao_ganha() {
        let c = gruta();
        let chaves_da_cor: Vec<u16> = item_id::CHAVES
            .iter()
            .map(|&b| item_id::chave_na_cor(b, 1))
            .collect();
        let mut r = rng(7);
        const N: u32 = 60_000;
        let mut caiu = 0;
        for _ in 0..N {
            let bau = rolar_bau(c, 1, false, false, &mut r);
            caiu += bau
                .itens
                .iter()
                .filter(|p| chaves_da_cor.contains(&p.item_id))
                .count() as u32;
            let ajud = rolar_bau(c, 1, true, false, &mut r);
            assert!(ajud
                .itens
                .iter()
                .all(|p| p.peca.is_none() && !item_id::todas_as_chaves().contains(&p.item_id)));
        }
        let taxa = caiu as f32 / N as f32;
        let esperado = chaves::faixa(10).chance;
        assert!((taxa - esperado).abs() < 0.01, "{taxa} vs {esperado}");
    }

    #[test]
    fn marcas_do_ajudante_e_do_bonus() {
        assert_eq!(marcas(Tipo::Gruta, false, false), 10);
        assert_eq!(marcas(Tipo::Gruta, true, false), 6);
        assert_eq!(marcas(Tipo::Gruta, false, true), 15);
    }

    #[test]
    fn mensagens_vao_e_voltam_no_postcard() {
        let a = Aviso::Instancia {
            conteudo: 10,
            estagio: 2,
            andar: 1,
            andares: 3,
            restante_s: 900,
            inimigos: 7,
            reviver_em_s: Some(10),
            membros: vec![MembroDaInstancia {
                nome: "a@SA01".into(),
                vivo: false,
            }],
            concluida: false,
        };
        let b = postcard::to_allocvec(&a).unwrap();
        assert_eq!(postcard::from_bytes::<Aviso>(&b).unwrap(), a);
        let p = Pedido::FilaEntrar {
            conteudo: 10,
            estagio: 3,
        };
        assert_eq!(
            postcard::from_bytes::<Pedido>(&postcard::to_allocvec(&p).unwrap()).unwrap(),
            p
        );
    }
}
