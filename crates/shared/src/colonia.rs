//! A COLONIA: a ilha do jogador (docs/COLONIA.md).
//!
//! Recebida numa quest antes de sair da primeira ilha, alcançada por
//! teleporte no porto. Ela **rende sozinha enquanto voce nao esta' la'** — e
//! e' por isso que existe: um lugar que trabalha por voce e cresce com o que
//! voce investe nele.
//!
//! # Por que ela e' barata
//!
//! O terreno ja' faz o trabalho pesado: `Ilha::gerar` e' funcao pura da
//! semente. A colonia e' `hash(nome do personagem)` num raio pequeno —
//! deterministica, zero de armazenamento, materializada so' quando alguem
//! visita. O que o banco guarda sao QUATRO numeros: nivel de cada eixo e
//! quando foi a ultima colheita.
//!
//! E cada um esta' sozinho na sua: a colonia vive em INSTANCIA, como a
//! dungeon. Sem espaco de coordenadas compartilhado, sem silhueta de vizinho,
//! sem AOI nova. Foi exatamente o que encareceu o Mar Aberto.

use crate::constants::item_id;

/// O nome da zona da colonia.
pub const ZONA: &str = "colonia";

/// Esta zona e' a colonia?
pub fn e_colonia(z: &str) -> bool {
    z == ZONA
}

/// Onde o jogador aparece ao chegar na colonia.
///
/// O centro da propria ilha: ela e' pequena, entao nao ha' "longe" pra
/// nascer, e nascer no meio e' o que faz a primeira visita mostrar tudo de
/// uma vez.
pub const CHEGADA: glam::Vec2 = glam::Vec2::ZERO;

/// Quantos eixos a colonia tem.
pub const EIXOS: usize = 3;

pub mod eixo {
    /// O TAMANHO da ilha: mais chao, mais lugar pra nó.
    pub const TAMANHO: usize = 0;
    /// A DENSIDADE de recurso: quanto ela rende por hora.
    pub const RECURSOS: usize = 1;
    /// O BANCO LOCAL: espacos de deposito na propria ilha.
    pub const BANCO: usize = 2;
    pub const NOMES: [&str; super::EIXOS] = ["Tamanho", "Recursos", "Banco"];
}

/// Niveis por eixo, 0..=MAX.
pub const NIVEL_MAX: u8 = 5;

/// Raio da ilha em blocos, pelo nivel de TAMANHO.
///
/// Pequena de proposito: uma colonia e' um quintal, nao um continente. O raio
/// tambem decide o custo de memoria — a 120 blocos o campo de altura sao
/// 57.600 colunas, uns 115 KB. Sessenta colonias vivas cabem em 7 MB, e e'
/// por isso que dar uma ilha PROPRIA a cada jogador e' possivel.
pub fn raio_blocos(nivel: u8) -> i32 {
    120 + 40 * nivel.min(NIVEL_MAX) as i32
}

/// A semente da colonia de `nome`.
///
/// Do NOME e nao do id: o nome e' o que o jogador reconhece, e duas contas
/// com o mesmo personagem recriado merecem a mesma ilha.
pub fn semente(nome: &str) -> i32 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in nome.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    ((h >> 24) as i32).abs().max(1)
}

// ─────────────────────────── a producao ───────────────────────────

/// Horas ate' a colheita "cheia".
pub const HORAS_CHEIAS: f32 = 12.0;
/// Horas ate' o deposito parar de acumular.
pub const HORAS_TETO: f32 = 24.0;
/// Que fracao do rendimento sobra entre `HORAS_CHEIAS` e `HORAS_TETO`.
///
/// Nao zero e nao um: quem volta em 12 h leva tudo, quem volta em 24 leva
/// 75% do que teria levado em duas visitas. O atraso custa, mas nao apaga —
/// castigo binario faria o jogador sentir que perdeu o dia inteiro por meia
/// hora de sono.
pub const FRACAO_ATRASADA: f32 = 0.5;

/// Quanto de cada recurso a colonia rende por HORA, pelo nivel de RECURSOS.
///
/// Baixo de proposito: a colonia e' renda passiva, e renda passiva que
/// compete com jogar mata o jogo. O teto de 24 h poe um limite duro no que
/// ela pode despejar por dia.
pub fn por_hora(nivel: u8) -> [(u16, u32); 3] {
    let n = nivel.min(NIVEL_MAX) as u32 + 1;
    [
        (item_id::COPPER, 40 * n),
        (item_id::WOOD_T1, 6 * n),
        (item_id::na_cor(item_id::STEEL, 1), 3 * n),
    ]
}

/// Quantas HORAS EFETIVAS valeram, dadas `horas` desde a ultima colheita.
///
/// A curva inteira num lugar so': cheio ate' 12 h, meio de 12 a 24, nada
/// depois. Enunciavel numa frase — "colha a cada 12, ela guarda ate' 24" — e
/// e' isso que faz o jogador conseguir planejar sem abrir planilha.
pub fn horas_efetivas(horas: f32) -> f32 {
    let h = horas.max(0.0);
    if h <= HORAS_CHEIAS {
        h
    } else {
        HORAS_CHEIAS + (h.min(HORAS_TETO) - HORAS_CHEIAS) * FRACAO_ATRASADA
    }
}

/// O que ha' pra colher agora.
pub fn colheita(nivel_recursos: u8, horas: f32) -> Vec<(u16, u32)> {
    let h = horas_efetivas(horas);
    por_hora(nivel_recursos)
        .into_iter()
        .filter_map(|(id, q)| {
            let n = (q as f32 * h) as u32;
            (n > 0).then_some((id, n))
        })
        .collect()
}

/// Espacos do banco local, pelo nivel de BANCO. Zero no nivel 0: o banco e'
/// uma MELHORIA, e nao um brinde.
pub fn espacos_do_banco(nivel: u8) -> u8 {
    10 * nivel.min(NIVEL_MAX)
}

/// (item, quantidade) pra subir `eixo` ate' `nivel_alvo`.
pub fn custo(eixo: usize, nivel_alvo: u8) -> [(u16, u32); 2] {
    let n = nivel_alvo.clamp(1, NIVEL_MAX) as u32;
    // O eixo pesa: banco e' o mais caro porque e' o que mais muda a vida.
    let peso = match eixo {
        self::eixo::TAMANHO => 1,
        self::eixo::RECURSOS => 2,
        _ => 3,
    };
    [
        (item_id::COPPER, 2_000 * n * n * peso),
        (item_id::WOOD_T1, 40 * n * peso),
    ]
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A curva de acumulo, enunciada: cheio ate' 12, meio ate' 24, nada
    /// depois.
    #[test]
    fn colher_em_doze_leva_tudo_e_atrasar_custa_metade() {
        assert_eq!(horas_efetivas(0.0), 0.0);
        assert_eq!(horas_efetivas(6.0), 6.0);
        assert_eq!(horas_efetivas(HORAS_CHEIAS), HORAS_CHEIAS);
        // De 12 a 24 rende metade.
        assert_eq!(horas_efetivas(18.0), 12.0 + 6.0 * FRACAO_ATRASADA);
        assert_eq!(horas_efetivas(HORAS_TETO), 12.0 + 12.0 * FRACAO_ATRASADA);
        // Depois de 24 nao acumula mais nada: dormir dois dias nao dobra.
        assert_eq!(horas_efetivas(48.0), horas_efetivas(HORAS_TETO));
        assert_eq!(horas_efetivas(1_000.0), horas_efetivas(HORAS_TETO));
    }

    /// Duas colheitas de 12 h rendem MAIS que uma de 24.
    ///
    /// E' o que faz a regra ser um convite a voltar em vez de um castigo por
    /// nao voltar: quem joga todo dia ganha mais, e quem sumiu um dia ainda
    /// leva a maior parte.
    #[test]
    fn voltar_cedo_rende_mais_e_sumir_nao_zera() {
        let duas = horas_efetivas(12.0) * 2.0;
        let uma = horas_efetivas(24.0);
        assert!(duas > uma, "voltar cedo tinha que render mais");
        assert!(
            uma > duas * 0.7,
            "sumir um dia nao pode apagar o dia: {uma} contra {duas}"
        );
    }

    /// A ilha cresce e rende mais a cada nivel, e o banco so' existe depois
    /// de investido.
    #[test]
    fn todo_eixo_sobe_e_o_banco_comeca_zerado() {
        for n in 0..NIVEL_MAX {
            assert!(raio_blocos(n + 1) > raio_blocos(n));
            assert!(por_hora(n + 1)[0].1 > por_hora(n)[0].1);
            assert!(custo(eixo::TAMANHO, n + 1)[0].1 > 0);
        }
        assert_eq!(espacos_do_banco(0), 0);
        assert!(espacos_do_banco(1) > 0);
        // O banco e' o eixo mais caro: e' o que mais muda a vida.
        assert!(custo(eixo::BANCO, 1)[0].1 > custo(eixo::TAMANHO, 1)[0].1);
    }

    /// Cada personagem tem a SUA ilha, e sempre a mesma.
    #[test]
    fn a_semente_e_do_nome_e_nao_muda() {
        assert_eq!(semente("curandinho"), semente("curandinho"));
        assert_ne!(semente("curandinho"), semente("onurb"));
        assert!(semente("a") > 0 && semente("").max(1) > 0);
    }
}

// ─────────────────────────── protocolo ───────────────────────────

/// O que o jogador pede a colonia.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PedidoColonia {
    /// So' o estado, pro painel. Nao viaja.
    Painel,
    /// No porto: teleporta pra ilha.
    Visitar,
    /// Na ilha: volta pro porto de onde veio.
    Voltar,
    /// Colhe o que ela rendeu.
    Colher,
    /// Sobe um eixo (`colonia::eixo`).
    Melhorar { eixo: u8 },
}

/// O estado da colonia, pro painel.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AvisoColonia {
    Estado {
        niveis: [u8; EIXOS],
        /// Horas desde a ultima colheita (ja' cruas, nao efetivas: o painel
        /// mostra as duas, e ver a diferenca e' o que ensina a regra).
        horas: f32,
        /// O que ha' pra colher agora.
        colheita: Vec<(u16, u32)>,
        /// Por eixo: o custo do proximo nivel. Vazio = no maximo.
        custos: Vec<Vec<(u16, u32)>>,
        /// Espacos do banco local.
        banco: u8,
    },
    /// O relevo mudou (subiu o TAMANHO): redesenhe com esta semente e raio.
    Terreno { semente: i32, raio: i32 },
    Recusa(String),
}

// ─────────────────────────── persistencia ───────────────────────────

/// O que a colonia guarda entre sessoes. Uma coluna JSON so' — a colonia vai
/// ganhar campos (nome dado pelo jogador, decoracao), e cada um deles nao pode
/// custar uma coluna nova e as quatro edicoes de UPSERT que ela exige.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DadosColonia {
    /// Nivel de cada eixo (`eixo`).
    pub niveis: [u8; EIXOS],
    /// Unix seconds da ultima colheita. 0 = nunca colheu (e a conta comeca
    /// na primeira visita, nao na criacao do personagem).
    pub colhida_em: i64,
    /// Se a quest ja' entregou a ilha.
    pub tem: bool,
    /// A zona de onde o jogador saiu pra visitar. E' o caminho de VOLTA, e
    /// ele tem que ser persistido: a colonia e' outro processo, e a sessao
    /// que sabia de onde ele veio morre na troca de zona. Vazio = volta pra
    /// ilha inicial.
    pub volta: String,
}

impl Default for DadosColonia {
    fn default() -> Self {
        Self { niveis: [1; EIXOS], colhida_em: 0, tem: false, volta: String::new() }
    }
}

#[cfg(test)]
mod testes_persistencia {
    use super::*;

    #[test]
    fn coluna_vazia_vira_colonia_nivel_um_sem_quest() {
        // O default e' o que TODO personagem velho le' no primeiro login: a
        // ilha existe no papel, mas `tem` falso mantem o teleporte escondido
        // ate' a quest. Nivel 0 nao pode ser o default — `raio_blocos(0)` da'
        // uma ilha, e um eixo em 0 custaria um caso especial em cada conta.
        let d: DadosColonia = serde_json::from_str("{}").unwrap();
        assert_eq!(d.niveis, [1; EIXOS]);
        assert!(!d.tem);
        assert_eq!(d.colhida_em, 0);
    }
}

#[cfg(test)]
mod testes_do_chao {
    use super::*;

    /// "lugar onde anda". A colonia nasce de um hash do NOME, e nada garante
    /// que um hash qualquer de' uma ilha com terra no meio — um pico submerso
    /// entregaria o jogador boiando, sem chao e sem saida a nao ser o painel.
    ///
    /// Entao isto e' testado pra NOMES DE VERDADE, em todos os niveis de
    /// Tamanho: a chegada tem que cair em terra, e perto.
    #[test]
    fn toda_colonia_tem_chao_onde_se_chega() {
        for nome in [
            "brunji", "A", "Zyx", "maria", "ÁÊÎÕÜ", "jogador_muito_longo_mesmo_123",
        ] {
            for nivel in 1..=NIVEL_MAX {
                let ilha = crate::terreno::Ilha::gerar(
                    semente(nome),
                    raio_blocos(nivel),
                    crate::terreno::Bioma::Floresta,
                    crate::terreno::ESCALA_ALTURA,
                );
                let p = ilha.terra_mais_proxima(CHEGADA.x, CHEGADA.y, 400.0);
                assert!(
                    !ilha.agua(p.x, p.y),
                    "'{nome}' nivel {nivel}: chegada na agua em {p:?}"
                );
                assert!(
                    p.distance(CHEGADA) < 300.0,
                    "'{nome}' nivel {nivel}: chao a {:.0}u da chegada",
                    p.distance(CHEGADA)
                );
            }
        }
    }

    /// Subir o Tamanho so' pode CRESCER a ilha. Se um nivel encolhesse, quem
    /// estivesse na beirada acordaria na agua depois da melhoria — e pagando
    /// material por isso.
    #[test]
    fn melhorar_o_tamanho_nunca_encolhe() {
        for n in 1..NIVEL_MAX {
            assert!(
                raio_blocos(n + 1) > raio_blocos(n),
                "nivel {n} -> {}: nao cresceu",
                n + 1
            );
        }
    }
}
