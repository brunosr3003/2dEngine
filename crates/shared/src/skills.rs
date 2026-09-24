//! Skills: doze, todas ativas, três por conjunto de arma.
//!
//! As skills acompanham a arma equipada e sao liberadas pelo nivel do
//! personagem. Nao gastam pontos e nao dependem da proficiencia.

use serde::{Deserialize, Serialize};

/// O conjunto de arma. É também a proficiência: uma árvore por conjunto.
///
/// A arma é o PAR — principal mais secundária amarrada a ela. Não existe
/// offhand livre. A katana e' empunhada com as duas maos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Conjunto {
    /// Espada e escudo, com manto do guerreiro. Linha de frente.
    EspadaEscudo = 0,
    /// Katana com bainha. Corte rápido, saque.
    Katana = 1,
    /// Duas pistolas com coldre. À distância.
    Pistolas = 2,
    /// Anel mágico com manto do mago. Cura e magia.
    AnelMagico = 3,
}

impl Conjunto {
    pub const TODOS: [Conjunto; 4] = [
        Conjunto::EspadaEscudo,
        Conjunto::Katana,
        Conjunto::Pistolas,
        Conjunto::AnelMagico,
    ];

    pub fn de_u8(v: u8) -> Option<Conjunto> {
        Self::TODOS.get(v as usize).copied()
    }

    /// Identificador estável no banco e no fio.
    pub fn chave(self) -> &'static str {
        match self {
            Self::EspadaEscudo => "espada_escudo",
            Self::Katana => "katana",
            Self::Pistolas => "pistolas",
            Self::AnelMagico => "anel_magico",
        }
    }

    pub fn de_chave(s: &str) -> Option<Conjunto> {
        Self::TODOS.into_iter().find(|c| c.chave() == s)
    }

    /// O conjunto que esta arma e'. Mao vazia segura uma espada: sem conjunto
    /// o jogador nao teria verbo nenhum.
    pub fn da_arma(item_id: u16) -> Conjunto {
        use crate::constants::item_id as i;
        match item_id {
            i::KATANA => Conjunto::Katana,
            i::PISTOLAS => Conjunto::Pistolas,
            i::ANEL_MAGICO => Conjunto::AnelMagico,
            _ => Conjunto::EspadaEscudo,
        }
    }

    /// De que conjunto e' esta secundaria (`None` se nao for secundaria).
    pub fn da_secundaria(item_id: u16) -> Option<Conjunto> {
        use crate::constants::item_id as i;
        match item_id {
            i::MANTO_DO_GUERREIRO => Some(Conjunto::EspadaEscudo),
            i::BAINHA => Some(Conjunto::Katana),
            i::COLDRE => Some(Conjunto::Pistolas),
            i::MANTO_DO_MAGO => Some(Conjunto::AnelMagico),
            _ => None,
        }
    }

    /// A arma deste conjunto.
    pub fn arma(self) -> u16 {
        use crate::constants::item_id as i;
        match self {
            Conjunto::EspadaEscudo => i::ESPADA_E_ESCUDO,
            Conjunto::Katana => i::KATANA,
            Conjunto::Pistolas => i::PISTOLAS,
            Conjunto::AnelMagico => i::ANEL_MAGICO,
        }
    }

    /// A secundaria amarrada a este conjunto.
    pub fn secundaria(self) -> u16 {
        use crate::constants::item_id as i;
        match self {
            Conjunto::EspadaEscudo => i::MANTO_DO_GUERREIRO,
            Conjunto::Katana => i::BAINHA,
            Conjunto::Pistolas => i::COLDRE,
            Conjunto::AnelMagico => i::MANTO_DO_MAGO,
        }
    }

    /// Este conjunto luta a' distancia?
    pub fn a_distancia(self) -> bool {
        matches!(self, Conjunto::Pistolas | Conjunto::AnelMagico)
    }

    pub fn nome(self) -> &'static str {
        match self {
            Self::EspadaEscudo => "Espada e Escudo",
            Self::Katana => "Katana",
            Self::Pistolas => "Duas Pistolas",
            Self::AnelMagico => "Anel Mágico",
        }
    }
}

/// A forma geometrica do efeito. Cada uma das doze skills tem gesto proprio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Forma {
    /// Em si mesmo: cura, escudo, impulso.
    EmSi,
    /// Disparo vinculado ao alvo selecionado.
    Projetil,
    /// Cone à frente.
    Cone,
    /// Círculo centrado na entidade alvo.
    Circulo,
    /// Linha reta do corpo até o alcance.
    Linha,
}

impl Forma {
    pub fn chave(self) -> &'static str {
        match self {
            Self::EmSi => "em_si",
            Self::Projetil => "projetil",
            Self::Cone => "cone",
            Self::Circulo => "circulo",
            Self::Linha => "linha",
        }
    }

    pub fn de_chave(s: &str) -> Option<Forma> {
        [
            Self::EmSi,
            Self::Projetil,
            Self::Cone,
            Self::Circulo,
            Self::Linha,
        ]
        .into_iter()
        .find(|f| f.chave() == s)
    }
}

/// Uma skill. Doze campos, e nenhum deles é rank, afinidade ou payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: u32,
    pub nome: String,
    pub conjunto: Conjunto,
    /// 1 a 3. Também é a ordem em que destrava.
    pub ordem: u8,
    pub forma: Forma,
    pub custo_mp: i32,
    /// Espera entre usos, em segundos.
    pub espera_s: f32,
    /// Tempo parado conjurando. Zero = instantânea.
    pub conjuracao_s: f32,
    pub dano: i32,
    pub cura: i32,
    /// Alcance em unidades de mundo.
    pub alcance: f32,
    /// Raio do efeito. Zero pra `EmSi` e `Projetil`.
    pub raio: f32,
}

/// Nivel do personagem que destrava cada ordem.
///
/// A primeira vem junto com a arma: pegar o conjunto e não ter o que apertar
/// seria uma arma sem verbo.
pub const DESTRAVA_EM: [u32; 3] = [1, 5, 10];
/// Quanto o corpo fica TRAVADO depois do impacto da skill.
///
/// O dono, jogando de pistola contra chefe: "morre muito rápido pros ataques,
/// então tem que desviar, mas com skills ativas é mais difícil de desviar
/// porque as skills são lentas de lançar".
///
/// O que trava não é o cast — quase toda skill é instantânea — é ISTO: o
/// servidor segura o jogador até `impacto_em() + RECUPERACAO_S`
/// (`habilidades.rs`), e nessa janela o chefe telegrafa e não dá pra sair.
/// Era 0,36 s, e com as skills de carga chegava a 0,76.
///
/// Medi antes de mexer, e o número descartou a outra suspeita: mover cancela
/// o cast, mas a janela em que isso é possível vai de 0,3 s até o fim da
/// trava — 60 ms numa skill instantânea. Em todos os chefes simulados, ZERO
/// skills perdidas por esquiva. O problema nunca foi o cancelamento; era o
/// tempo parado.
pub const RECUPERACAO_S: f32 = 0.36;

/// A janela em que o DANO da skill foi calibrado. Ver `basico_deslocado`.
///
/// Separada da trava de propósito, e isto é uma decisão, não um descuido: o
/// dano de skill sai do "quanto de básico a trava joga fora". Se ele seguisse
/// a trava, cortar a trava pela metade cortaria o dano junto — e quem depende
/// de skill (pistola, anel) não ganharia nada com a mudança, que existe
/// justamente pra ajudar esses dois.
///
/// Então a trava encolheu e o dano ficou onde estava. Na prática é um presente
/// de tempo de reação pra quem conjura, e é exatamente o que foi pedido.
pub const JANELA_DE_REFERENCIA_S: f32 = 0.36;

// ───────────────────── evolução das habilidades ────────────────────────

pub const SKILL_COUNT: usize = 12;
pub const TIER_MAX: u8 = 10;

/// Tomos dos três despertares. Podem ser condensados pelo jogador para uma
/// habilidade escolhida ou invocados por pergaminho; nunca falham ao evoluir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum GrauTomo {
    Verde = 0,
    Roxo = 1,
    Lendario = 2,
}

impl GrauTomo {
    pub const TODOS: [Self; 3] = [Self::Verde, Self::Roxo, Self::Lendario];

    pub fn nome(self) -> &'static str {
        match self {
            Self::Verde => "Verde",
            Self::Roxo => "Roxo",
            Self::Lendario => "Lendário",
        }
    }

    pub fn de_u8(v: u8) -> Option<Self> {
        Self::TODOS.get(v as usize).copied()
    }
}

/// Estado persistente de evolução de um personagem. A Energia é saldo, não
/// ocupa bolsa. Cada habilidade começa no Tier I e guarda seus próprios tomos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgressoDeSkills {
    pub energia: u64,
    pub tiers: [u8; SKILL_COUNT],
    pub tomos: [[u16; 3]; SKILL_COUNT],
}

impl Default for ProgressoDeSkills {
    fn default() -> Self {
        Self {
            energia: 0,
            tiers: [1; SKILL_COUNT],
            tomos: [[0; 3]; SKILL_COUNT],
        }
    }
}

impl ProgressoDeSkills {
    pub fn normalizar(&mut self) {
        for tier in &mut self.tiers {
            *tier = (*tier).clamp(1, TIER_MAX);
        }
    }

    pub fn tier(&self, skill_id: u32) -> u8 {
        skill_id
            .checked_sub(1)
            .and_then(|i| self.tiers.get(i as usize))
            .copied()
            .unwrap_or(1)
            .clamp(1, TIER_MAX)
    }

    pub fn tomos(&self, skill_id: u32, grau: GrauTomo) -> u16 {
        skill_id
            .checked_sub(1)
            .and_then(|i| self.tomos.get(i as usize))
            .map_or(0, |v| v[grau as usize])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustoDeEvolucao {
    pub destino: u8,
    pub nivel: u32,
    pub energia: u64,
    pub cobre: u32,
    pub tomo: Option<GrauTomo>,
}

/// Custo para sair de `tier`. Os despertares V, VIII e X consomem um tomo
/// específico da habilidade; os demais são treino direto com Energia.
pub fn custo_de_evolucao(tier: u8) -> Option<CustoDeEvolucao> {
    Some(match tier {
        1 => CustoDeEvolucao {
            destino: 2,
            nivel: 1,
            energia: 100,
            cobre: 100,
            tomo: None,
        },
        2 => CustoDeEvolucao {
            destino: 3,
            nivel: 5,
            energia: 400,
            cobre: 300,
            tomo: None,
        },
        3 => CustoDeEvolucao {
            destino: 4,
            nivel: 10,
            energia: 1_200,
            cobre: 1_000,
            tomo: None,
        },
        4 => CustoDeEvolucao {
            destino: 5,
            nivel: 15,
            energia: 1_500,
            cobre: 2_000,
            tomo: Some(GrauTomo::Verde),
        },
        5 => CustoDeEvolucao {
            destino: 6,
            nivel: 20,
            energia: 5_000,
            cobre: 4_000,
            tomo: None,
        },
        6 => CustoDeEvolucao {
            destino: 7,
            nivel: 30,
            energia: 12_000,
            cobre: 8_000,
            tomo: None,
        },
        7 => CustoDeEvolucao {
            destino: 8,
            nivel: 40,
            energia: 10_000,
            cobre: 10_000,
            tomo: Some(GrauTomo::Roxo),
        },
        8 => CustoDeEvolucao {
            destino: 9,
            nivel: 55,
            energia: 60_000,
            cobre: 40_000,
            tomo: None,
        },
        9 => CustoDeEvolucao {
            destino: 10,
            nivel: 70,
            energia: 50_000,
            cobre: 50_000,
            tomo: Some(GrauTomo::Lendario),
        },
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustoDeTomo {
    pub energia: u64,
    pub cobre: u32,
}

pub fn custo_de_tomo(grau: GrauTomo) -> CustoDeTomo {
    match grau {
        GrauTomo::Verde => CustoDeTomo {
            energia: 3_000,
            cobre: 2_000,
        },
        GrauTomo::Roxo => CustoDeTomo {
            energia: 25_000,
            cobre: 20_000,
        },
        GrauTomo::Lendario => CustoDeTomo {
            energia: 150_000,
            cobre: 100_000,
        },
    }
}

/// Multiplicador numérico fixo. Os saltos maiores coincidem com os três
/// despertares; Tier X fica 21% acima do Tier I antes do efeito especial.
pub fn multiplicador_do_tier(tier: u8) -> f32 {
    const M: [f32; 10] = [1.00, 1.02, 1.04, 1.06, 1.09, 1.11, 1.13, 1.16, 1.18, 1.21];
    M[(tier.clamp(1, TIER_MAX) - 1) as usize]
}

pub fn tier_romano(tier: u8) -> &'static str {
    const R: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    R[(tier.clamp(1, TIER_MAX) - 1) as usize]
}

/// Texto curto dos três despertares. A regra correspondente mora no servidor,
/// mas o texto é compartilhado para a tela nunca prometer outra coisa.
pub fn despertar(skill_id: u32, tier: u8) -> &'static str {
    match (skill_id, tier) {
        (1, 5) => "Impacto mais forte",
        (1, 8) => "Escudo por 2 s após a investida",
        (1, 10) => "Golpe final ampliado",
        (2, 5) => "Cone mais longo",
        (2, 8) => "Corte reforçado",
        (2, 10) => "Corte final ampliado",
        (3, 5) => "Muralha por 6 s",
        (3, 8) => "Reduz 55% do dano",
        (3, 10) => "Reduz 60% por 8 s",
        (4, 5) => "Linha mais longa",
        (4, 8) => "Corte atravessa mais longe",
        (4, 10) => "Saque final ampliado",
        (5, 5) => "Área maior",
        (5, 8) => "Área e dano ampliados",
        (5, 10) => "Golpe final",
        (6, 5) => "Onda mais forte e longa",
        (6, 8) => "Onda atravessa mais longe",
        (6, 10) => "Onda final ampliada",
        (7, 5) => "Tiro mais forte",
        (7, 8) => "Empurra o alvo",
        (7, 10) => "Crítico garantido",
        (8, 5) => "Leque maior",
        (8, 8) => "Empurra inimigos",
        (8, 10) => "Rajada final",
        (9, 5) => "Explosão maior",
        (9, 8) => "Explosão mais forte",
        (9, 10) => "Incêndio devastador",
        (10, 5) => "Cura reforçada",
        (10, 8) => "Recuperação adicional",
        (10, 10) => "Cura máxima",
        (11, 5) => "Aura maior",
        (11, 8) => "Área e cura reforçadas",
        (11, 10) => "Pulso de cura máximo",
        (12, 5) => "Impacto maior",
        (12, 8) => "Atinge alvos mais distantes",
        (12, 10) => "Julgamento final",
        _ => "",
    }
}

/// Energia por ciclo de um cristal, conforme a ilha. O índice é a ilha do
/// arquipélago; canais sem ilha usam o primeiro valor.
pub fn energia_por_coleta(indice_da_ilha: usize) -> u64 {
    [12, 28, 60, 120].get(indice_da_ilha).copied().unwrap_or(12)
}

/// O `dano` de cada skill do catalogo e' peso relativo, e este e' o meio da
/// escala: uma skill de `dano` 30 rende o ganho cheio da forma dela.
pub const DANO_DE_REFERENCIA: f32 = 30.0;
/// Quanto a skill rende contra o ataque basico que ela desliga, pro `dano`
/// de referencia. Alvo unico rende mais, porque acerta um so'; area rende
/// menos POR ALVO, porque o lucro dela e' acertar varios.
pub const GANHO_ALVO_UNICO: f32 = 1.60;
pub const GANHO_EM_AREA: f32 = 1.30;
/// PISO: contra UM alvo, toda skill ofensiva rende pelo menos isto do basico
/// que desliga — de area inclusive.
///
/// Ate' 19/09/2026 a area rendia 0,47x a 0,82x por alvo e o alvo unico batia
/// num teto de 1,05x: contra um bicho ou um chefe, apertar quase qualquer
/// skill era PERDER dano pro basico (o jogador sentiu: "o tempinho de
/// carregamento deixa a skill pior que o ataque basico"). A conta so' fechava
/// contando tres alvos, e o basico corpo a corpo tambem acerta todo mundo no
/// cone.
pub const PISO_ALVO_UNICO: f32 = 1.60;
pub const PISO_EM_AREA: f32 = 1.35;
/// Teto do quanto uma skill pode render sobre o basico que desliga: nenhuma
/// decide a luta sozinha. A duracao das lutas de chefe (60 a 240 s, no
/// simulador) e' segurada pela vida do chefe (`bosses::vida`), nao por skill
/// fraca.
pub const TETO_DO_GANHO: f32 = 1.80;

impl Skill {
    /// Skills ofensivas exigem uma entidade selecionada; suporte usa o conjurador.
    pub fn alcance_alvo(&self) -> f32 {
        if self.alcance > 0.0 {
            self.alcance
        } else {
            self.raio
        }
    }

    pub fn nivel_necessario(&self) -> u32 {
        DESTRAVA_EM
            .get(self.ordem.wrapping_sub(1) as usize)
            .copied()
            .unwrap_or(u32::MAX)
    }

    /// Inicio do efeito depois da antecipacao e da conjuracao, em segundos.
    pub fn impacto_em(&self) -> f32 {
        self.conjuracao_s.max(0.0)
            + match self.id {
                1 => 0.52,
                2 => 0.48,
                3 => 0.42,
                4 => 0.42,
                5 => 0.64,
                6 => 0.38,
                7 => 0.34,
                8 => 0.46,
                9 => 0.48,
                10 => 0.55,
                11 => 0.35,
                12 => 0.38,
                _ => crate::PLAYER_ATTACK_IMPACT_S,
            }
    }

    pub fn duracao_efeito(&self) -> f32 {
        if self.id == 3 {
            5.0
        } else {
            0.65
        }
    }

    /// Dano final da skill, em cima do ataque de quem conjura.
    ///
    /// Conjurar DESLIGA o ataque basico (o servidor zera `frame.buttons`
    /// enquanto dura o `casting_until`), entao o que a skill precisa vencer nao
    /// e' um numero fixo: e' o dano que o basico daria na janela travada. A
    /// janela vale `impacto_em() + RECUPERACAO_S` e o basico rende `atk / cd`,
    /// dai a conta ser "basico deslocado × ganho".
    ///
    /// Antes o dano era o `dano` do catalogo, cru. No nivel 80 o Tiro Certeiro
    /// entregava 28 onde o basico entregaria 400 na MESMA janela: apertar a
    /// skill era perder dano. Um coeficiente fixo tambem nao resolveria, porque
    /// a cadencia do basico melhora com o nivel e a divida cresce junto — por
    /// isso a cadencia entra na conta, e nao so' o ataque.
    ///
    /// O `dano` do catalogo passa a ser PESO RELATIVO entre as doze, que e'
    /// como ele ja' estava ajustado a mao.
    pub fn dano_efetivo(&self, atk: i32, cd_basico: f32) -> i32 {
        if self.dano <= 0 {
            return 0;
        }
        (self.basico_deslocado(atk, cd_basico) * self.ganho())
            .round()
            .max(1.0) as i32
    }

    /// Quantas vezes o basico deslocado a skill rende contra UM alvo: o peso
    /// do catalogo na escala da forma, entre o piso e o teto.
    pub fn ganho(&self) -> f32 {
        let (ganho, piso) = if self.forma == Forma::Projetil {
            (GANHO_ALVO_UNICO, PISO_ALVO_UNICO)
        } else {
            (GANHO_EM_AREA, PISO_EM_AREA)
        };
        (ganho * (self.dano as f32 / DANO_DE_REFERENCIA)).clamp(piso, TETO_DO_GANHO)
    }

    /// O dano de basico que o cast joga fora (a janela travada × a cadencia).
    pub fn basico_deslocado(&self, atk: i32, cd_basico: f32) -> f32 {
        (self.impacto_em() + RECUPERACAO_S) * atk.max(1) as f32 / cd_basico.max(0.05)
    }

    pub fn descricao(&self) -> &'static str {
        match self.id {
            1 => "Avança até o alvo e atinge inimigos no caminho.",
            2 => "Corte amplo voltado para o alvo selecionado.",
            3 => "Reduz o dano recebido em 50% por 5 segundos.",
            4 => "Saca a katana e corta em linha até o alvo.",
            5 => "Atinge o alvo próximo e os inimigos ao redor dele.",
            6 => "Uma onda cortante atinge o alvo selecionado.",
            7 => "Dispara um tiro poderoso no alvo selecionado.",
            8 => "Rajada em leque voltada para o alvo selecionado.",
            9 => "Arremessa um barril que explode no alvo.",
            10 => "Restaura a própria vida.",
            11 => "Cura você e aliados ao seu redor.",
            12 => "Impacto mágico no alvo e nos inimigos próximos.",
            _ => "",
        }
    }

    /// Esta skill está disponível com este nivel de personagem?
    pub fn destravada(&self, nivel_do_personagem: u32) -> bool {
        (1..=3).contains(&self.ordem) && nivel_do_personagem >= self.nivel_necessario()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn evolucao_e_deterministica_e_tem_tres_despertares() {
        let mut anterior = 0.0;
        for tier in 1..=TIER_MAX {
            let m = multiplicador_do_tier(tier);
            assert!(m >= anterior);
            anterior = m;
        }
        assert_eq!(multiplicador_do_tier(1), 1.0);
        assert_eq!(multiplicador_do_tier(10), 1.21);
        for id in 1..=SKILL_COUNT as u32 {
            for tier in [5, 8, 10] {
                assert!(!despertar(id, tier).is_empty(), "skill {id} T{tier}");
            }
        }
    }

    #[test]
    fn custos_cobrem_i_ao_x_e_tomos_so_nos_despertares() {
        let destinos: Vec<u8> = (1..TIER_MAX)
            .map(|tier| custo_de_evolucao(tier).unwrap().destino)
            .collect();
        assert_eq!(destinos, (2..=TIER_MAX).collect::<Vec<_>>());
        for tier in 1..TIER_MAX {
            let c = custo_de_evolucao(tier).unwrap();
            assert_eq!(c.tomo.is_some(), matches!(c.destino, 5 | 8 | 10));
        }
        assert!(custo_de_evolucao(TIER_MAX).is_none());
    }

    #[test]
    fn progresso_antigo_vira_tier_um() {
        let mut p = ProgressoDeSkills::default();
        p.tiers[0] = 0;
        p.tiers[1] = 99;
        p.normalizar();
        assert_eq!(p.tier(1), 1);
        assert_eq!(p.tier(2), TIER_MAX);
        assert_eq!(p.tier(999), 1);
    }

    /// A primeira skill vem com a arma. Pegar um conjunto novo e não ter o que
    /// apertar seria uma arma sem verbo.
    #[test]
    fn a_primeira_vem_junto_com_a_arma() {
        let s = Skill {
            id: 1,
            nome: "x".into(),
            conjunto: Conjunto::Katana,
            ordem: 1,
            forma: Forma::Cone,
            custo_mp: 0,
            espera_s: 1.0,
            conjuracao_s: 0.0,
            dano: 10,
            cura: 0,
            alcance: 3.0,
            raio: 0.0,
        };
        assert!(s.destravada(1), "ordem 1 tem que vir com a arma");
        let mut terceira = s.clone();
        terceira.ordem = 3;
        assert!(!terceira.destravada(1));
        assert!(terceira.destravada(DESTRAVA_EM[2]));
    }

    /// Chave de banco e volta: se uma delas mudar sozinha, as skills todas
    /// caem no conjunto errado no proximo carregamento.
    #[test]
    fn as_chaves_dao_a_volta() {
        for c in Conjunto::TODOS {
            assert_eq!(Conjunto::de_chave(c.chave()), Some(c));
        }
        for f in [
            Forma::EmSi,
            Forma::Projetil,
            Forma::Cone,
            Forma::Circulo,
            Forma::Linha,
        ] {
            assert_eq!(Forma::de_chave(f.chave()), Some(f));
        }
    }

    #[test]
    fn catalogo_tem_tres_skills_por_arma_e_desbloqueios_validos() {
        let todas = playtest();
        assert_eq!(todas.len(), 12);
        for conjunto in Conjunto::TODOS {
            let skills: Vec<_> = todas.iter().filter(|s| s.conjunto == conjunto).collect();
            assert_eq!(skills.len(), 3);
            for (i, skill) in skills.iter().enumerate() {
                assert_eq!(skill.ordem, i as u8 + 1);
                let nivel = DESTRAVA_EM[i];
                assert!(!skill.destravada(nivel - 1));
                assert!(skill.destravada(nivel));
                assert!(skill.impacto_em() > skill.conjuracao_s);
            }
        }
        let mut invalida = todas[0].clone();
        for ordem in [0, 4, 255] {
            invalida.ordem = ordem;
            assert!(!invalida.destravada(100));
        }
    }

    /// A regra que faltava no jogo: apertar uma skill tem que render MAIS que o
    /// ataque basico que ela desliga enquanto conjura. Sem isto o dano de skill
    /// era numero fixo do catalogo e, no nivel alto, toda skill do pistoleiro
    /// virava prejuizo — o jogador era punido por usar a propria habilidade.
    ///
    /// A faixa de ataque/cadencia e' larga de proposito: a conta nao pode valer
    /// so' num nivel. Como `dano_efetivo` e `basico_deslocado` sao os dois
    /// lineares em `atk` e em `1/cd`, a razao entre eles nao depende de nenhum
    /// dos dois — e' isso que faz a garantia valer do nivel 1 ao fim do jogo.
    #[test]
    fn toda_skill_ofensiva_rende_mais_que_o_basico_que_desliga() {
        for atk in [20, 75, 136, 200, 400] {
            for cd in [0.65f32, 0.53, 0.43, 0.35, 0.25] {
                for s in playtest().iter().filter(|s| s.dano > 0) {
                    let deslocado = s.basico_deslocado(atk, cd);
                    let rende = s.dano_efetivo(atk, cd) as f32;
                    // Contra UM alvo so' — chefe, bicho sozinho. Era contando
                    // tres que a area passava, e perdia pro basico no resto.
                    let piso = if s.forma == Forma::Projetil {
                        PISO_ALVO_UNICO
                    } else {
                        PISO_EM_AREA
                    };
                    assert!(
                        rende >= deslocado * piso - 1.0,
                        "{} com atk {atk} e cd {cd}: rende {rende:.0} num alvo e joga fora {deslocado:.0} de basico",
                        s.nome
                    );
                }
            }
        }
        // E o peso do catalogo continua valendo: a mais pesada de cada forma
        // rende mais que a mais leve.
        let t = playtest();
        let g = |id: u32| t.iter().find(|s| s.id == id).unwrap().ganho();
        assert!(g(12) > g(8), "Julgamento (55) acima da Rajada (20)");
        assert!(
            g(6) >= g(7),
            "Vento Cortante (45) nao abaixo do Tiro Certeiro (28)"
        );
    }

    /// O dano de skill tem que ANDAR com o ataque de quem conjura. Se algum dia
    /// voltar a ser o numero cru do catalogo, este teste cai primeiro.
    #[test]
    fn dano_de_skill_escala_com_o_ataque_de_quem_conjura() {
        for s in playtest().iter().filter(|s| s.dano > 0) {
            let (fraco, forte) = (s.dano_efetivo(50, 0.5), s.dano_efetivo(200, 0.5));
            assert!(
                forte >= fraco * 3,
                "{}: de {fraco} pra {forte} nao acompanhou o ataque",
                s.nome
            );
        }
    }

    /// Cura e Muralha nao tem dano: a conta nova nao pode inventar um.
    #[test]
    fn skill_sem_dano_continua_sem_dano() {
        for s in playtest().iter().filter(|s| s.dano == 0) {
            assert_eq!(
                s.dano_efetivo(200, 0.35),
                0,
                "{} ganhou dano do nada",
                s.nome
            );
        }
    }
}

/// Catalogo inicial unico, tambem usado para semear o banco.
pub fn playtest() -> Vec<Skill> {
    let linhas: [(u32, &str, &str, u8, &str, i32, f32, f32, i32, i32, f32, f32); 12] = [
        // ── espada e escudo: segurar a linha ──
        (
            1,
            "Investida",
            "espada_escudo",
            1,
            "linha",
            10,
            8.0,
            0.0,
            25,
            0,
            6.0,
            1.0,
        ),
        (
            2,
            "Golpe Largo",
            "espada_escudo",
            2,
            "cone",
            15,
            6.0,
            0.0,
            35,
            0,
            3.5,
            0.0,
        ),
        (
            3,
            "Muralha",
            "espada_escudo",
            3,
            "em_si",
            25,
            20.0,
            0.0,
            0,
            0,
            0.0,
            0.0,
        ),
        // ── katana: corte rapido ──
        (
            4, "Saque", "katana", 1, "linha", 8, 10.0, 0.0, 30, 0, 4.0, 0.8,
        ),
        (
            5, "Dança", "katana", 2, "circulo", 18, 15.0, 0.0, 28, 0, 0.0, 2.5,
        ),
        (
            6,
            "Vento Cortante",
            "katana",
            3,
            "projetil",
            22,
            20.0,
            0.3,
            45,
            0,
            9.0,
            0.0,
        ),
        // ── duas pistolas: distancia ──
        (
            7,
            "Tiro Certeiro",
            "pistolas",
            1,
            "projetil",
            8,
            4.0,
            0.0,
            28,
            0,
            11.0,
            0.0,
        ),
        (
            8, "Rajada", "pistolas", 2, "cone", 16, 9.0, 0.0, 20, 0, 6.0, 0.0,
        ),
        (
            9, "Barril", "pistolas", 3, "circulo", 24, 16.0, 0.4, 50, 0, 8.0, 3.0,
        ),
        // ── anel magico: cura e magia ──
        (
            10,
            "Bênção",
            "anel_magico",
            1,
            "em_si",
            14,
            10.0,
            0.0,
            0,
            40,
            0.0,
            0.0,
        ),
        (
            11,
            "Aura",
            "anel_magico",
            2,
            "circulo",
            26,
            18.0,
            0.5,
            0,
            30,
            7.0,
            4.0,
        ),
        (
            12,
            "Julgamento",
            "anel_magico",
            3,
            "circulo",
            30,
            14.0,
            0.6,
            55,
            0,
            9.0,
            3.0,
        ),
    ];
    linhas
        .into_iter()
        .map(|l| Skill {
            id: l.0,
            nome: l.1.into(),
            conjunto: Conjunto::de_chave(l.2).unwrap(),
            ordem: l.3,
            forma: Forma::de_chave(l.4).unwrap(),
            custo_mp: l.5,
            espera_s: l.6,
            conjuracao_s: l.7,
            dano: l.8,
            cura: l.9,
            alcance: l.10,
            raio: l.11,
        })
        .collect()
}
