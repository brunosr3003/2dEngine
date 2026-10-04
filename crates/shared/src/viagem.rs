//! Ir e voltar entre as ilhas (menu "Travel" do Capitao do Porto) e o
//! Pergaminho de Teleporte (salto pra um destino marcado DENTRO da ilha).
//!
//! Antes daqui a unica viagem era o passo de historia "Rumo a …": quem ia pra
//! Geleira nao voltava mais ao Bosque. Agora o Capitao de TODA ilha leva a
//! qualquer ilha que a historia ja' liberou — ir e voltar, de graca.
//!
//! O pergaminho nao cruza o mar (cada ilha e' outro processo): ele encurta a
//! caminhada ate' o ponto marcado no mapa, o "Ir" de uma missao ou um NPC.

use serde::{Deserialize, Serialize};

use crate::historia;
use crate::quests::objective_kind;
use crate::terreno::ARQUIPELAGO;

/// Preco do pergaminho no Alquimista, em COBRE (como as pocoes dele).
pub const PRECO_PERGAMINHO: u32 = 100;
/// Destino mais perto que isto: o botao de teleporte nem aparece — andar e'
/// de graca e o pergaminho seria desperdicio.
pub const TELEPORTE_MIN: f32 = 40.0;
/// Quanto o servidor procura chao firme em volta do ponto pedido (o NPC fica
/// na porta da casa; o ponto marcado pode cair num tronco).
pub const TELEPORTE_BUSCA: f32 = 16.0;
/// Perto disto do Capitao o "Board" vale (o menu abre no alcance do
/// clique; o pedido confere de novo, com folga pro passo dado no meio).
pub const PERTO_DO_CAPITAO: f32 = 8.0;

/// Como a ilha aparece no menu do Capitao.
pub mod estado {
    /// E' onde o personagem esta'.
    pub const AQUI: u8 = 0;
    /// Da' pra embarcar.
    pub const LIBERADA: u8 = 1;
    /// Liberada, mas o servidor da ilha nao esta' no ar.
    pub const FORA_DO_AR: u8 = 2;
    /// A historia ainda nao chegou la'.
    pub const BLOQUEADA: u8 = 3;
}

/// Uma linha do menu "Travel".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Destino {
    /// Indice no `ARQUIPELAGO`.
    pub ilha: u8,
    pub nome: String,
    /// Faixa de nivel da ilha.
    pub nivel_min: u16,
    pub nivel_max: u16,
    pub estado: u8,
    /// Bloqueada: o passo da historia que libera ("Bound for the Glacier").
    pub requisito: String,
}

/// O passo da historia que leva a `ilha` pela primeira vez: (indice, titulo).
pub fn passo_que_libera(ilha: usize) -> Option<(u32, &'static str)> {
    historia::PASSOS
        .iter()
        .position(|d| d.obj_kind == objective_kind::VIAGEM && d.obj_target as usize == ilha)
        .map(|i| (i as u32, historia::PASSOS[i].title))
}

/// A ilha esta' liberada pra quem esta' no passo `indice` da historia? A
/// inicial sempre; as outras a partir do passo que manda pra elas (quem esta'
/// NESSE passo ja' pode embarcar — e chegar la' conclui o passo).
pub fn liberada(ilha: usize, indice_da_historia: Option<u32>) -> bool {
    if ilha == 0 {
        return true;
    }
    match (passo_que_libera(ilha), indice_da_historia) {
        (Some((passo, _)), Some(i)) => i >= passo,
        _ => false,
    }
}

/// Who takes you to another island.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meio {
    /// The Harbour Captain, on every island with a port.
    Capitao,
    /// The Sky Bus Professor (Skyreach and Kōgen-tō).
    Onibus,
    /// The Submarine Captain (Kōgen-tō and Abyssia).
    Submarino,
}

impl Meio {
    /// The NPC role that travels this way.
    pub fn papel(self) -> crate::construcao::Papel {
        use crate::construcao::Papel;
        match self {
            Meio::Capitao => Papel::Estaleiro,
            Meio::Onibus => Papel::Motorista,
            Meio::Submarino => Papel::Submarino,
        }
    }

    /// The way an NPC of role `papel` (the wire's `u8`) travels, if it does.
    pub fn do_papel(papel: u8) -> Option<Meio> {
        [Meio::Capitao, Meio::Onibus, Meio::Submarino].into_iter().find(|m| m.papel() as u8 == papel)
    }
}

/// Who takes you where. Kōgen-tō is reached ONLY by the flying bus from
/// Skyreach (the owner, 02/10/2026); Skyreach's bus goes nowhere else, and
/// Kōgen-tō's bus flies to any island but the deep. Abyssia is reached ONLY
/// by the submarine from Kōgen-tō, and Abyssia's submarine sails anywhere.
/// Captains sail to neither.
pub fn rota_permitida(zona_atual: &str, destino: usize, meio: Meio) -> bool {
    let Some(d) = ARQUIPELAGO.get(destino) else {
        return false;
    };
    let para_kogen = crate::kogen::e_kogen(d.zona);
    let para_abissal = crate::abissal::e_abissal(d.zona);
    match meio {
        Meio::Capitao => !para_kogen && !para_abissal,
        Meio::Onibus => {
            if crate::celeste::e_celeste(zona_atual) {
                para_kogen
            } else {
                crate::kogen::e_kogen(zona_atual) && !para_abissal
            }
        }
        Meio::Submarino => {
            if crate::kogen::e_kogen(zona_atual) {
                para_abissal
            } else {
                crate::abissal::e_abissal(zona_atual)
            }
        }
    }
}

/// Who to look for on `zona_atual` to get to `destino`: the first way that
/// is allowed (`None` = no direct route from here).
pub fn meio_para(zona_atual: &str, destino: usize) -> Option<Meio> {
    [Meio::Submarino, Meio::Onibus, Meio::Capitao].into_iter().find(|m| rota_permitida(zona_atual, destino, *m))
}

/// O menu inteiro: uma linha por ilha do arquipelago. `no_ar(zona)` diz se ha'
/// canal daquela ilha rodando. `meio`: who opened it (`rota_permitida`
/// decides which lines show).
pub fn destinos(
    zona_atual: &str,
    indice_da_historia: Option<u32>,
    no_ar: &dyn Fn(&str) -> bool,
    meio: Meio,
) -> Vec<Destino> {
    ARQUIPELAGO
        .iter()
        .enumerate()
        .filter(|(i, d)| d.zona == zona_atual || rota_permitida(zona_atual, *i, meio))
        .map(|(i, d)| {
            let estado = if d.zona == zona_atual {
                estado::AQUI
            } else if !liberada(i, indice_da_historia) {
                estado::BLOQUEADA
            } else if !no_ar(d.zona) {
                estado::FORA_DO_AR
            } else {
                estado::LIBERADA
            };
            Destino {
                ilha: i as u8,
                nome: d.nome.to_string(),
                nivel_min: d.nivel.0 as u16,
                nivel_max: d.nivel.1 as u16,
                estado,
                requisito: passo_que_libera(i)
                    .map(|(_, t)| t.to_string())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn toda_ilha_alem_da_inicial_tem_um_passo_que_libera() {
        assert!(passo_que_libera(0).is_none());
        for i in 1..ARQUIPELAGO.len() {
            assert!(passo_que_libera(i).is_some(), "ilha {i} sem passo de viagem");
        }
        assert_eq!(passo_que_libera(1).unwrap().1, "Bound for the Glacier");
    }

    #[test]
    fn a_geleira_abre_no_passo_da_viagem_e_fica_aberta() {
        let (p, _) = passo_que_libera(1).unwrap();
        assert!(liberada(0, None), "o Bosque sempre");
        assert!(!liberada(1, None));
        assert!(!liberada(1, Some(p - 1)));
        assert!(liberada(1, Some(p)), "no passo da viagem ja' embarca");
        assert!(liberada(1, Some(p + 20)), "e depois continua indo e voltando");
        assert!(!liberada(2, Some(p + 1)), "o Ermo ainda nao");
    }

    #[test]
    fn menu_marca_onde_estou_e_o_que_esta_fora_do_ar() {
        let (p, _) = passo_que_libera(1).unwrap();
        let so_bosque_e_gelo = |z: &str| z == "ilha_inicial" || z == "ilha_gelo";
        let m = destinos("ilha_gelo", Some(p + 3), &so_bosque_e_gelo, Meio::Capitao);
        assert_eq!(m.len(), ARQUIPELAGO.len() - 2, "a captain never lists Kōgen-tō or Abyssia");
        assert_eq!(m[0].estado, estado::LIBERADA, "da' pra voltar ao Bosque");
        assert_eq!(m[1].estado, estado::AQUI);
        assert_eq!(m[2].estado, estado::BLOQUEADA);
        let sem_bosque = |z: &str| z == "ilha_gelo";
        assert_eq!(destinos("ilha_gelo", Some(p), &sem_bosque, Meio::Capitao)[0].estado, estado::FORA_DO_AR);
    }

    /// Kōgen-tō only by the bus from Skyreach; captains never list it.
    #[test]
    fn kogen_so_pelo_onibus_de_skyreach() {
        let kogen = ARQUIPELAGO.iter().position(|d| crate::kogen::e_kogen(d.zona)).unwrap();
        let celeste = ARQUIPELAGO.iter().position(|d| crate::celeste::e_celeste(d.zona)).unwrap();
        let no_ar = |_: &str| true;
        for d in ARQUIPELAGO.iter() {
            assert!(!destinos(d.zona, Some(10_000), &no_ar, Meio::Capitao).iter().any(|x| x.ilha as usize == kogen && d.zona != crate::kogen::ZONA),
                "a captain in {} lists Kōgen-tō", d.zona);
        }
        let do_onibus = destinos(crate::celeste::ZONA, Some(10_000), &no_ar, Meio::Onibus);
        assert!(do_onibus.iter().any(|x| x.ilha as usize == kogen));
        assert!(do_onibus.iter().all(|x| x.ilha as usize == kogen || x.ilha as usize == celeste));
        assert!(rota_permitida(crate::kogen::ZONA, 0, Meio::Onibus), "Kōgen-tō's bus flies anywhere");
        assert!(!rota_permitida("ilha_inicial", kogen, Meio::Onibus), "no bus from the Bosque");
    }

    /// Abyssia only by the submarine from Kōgen-tō; its submarine sails
    /// anywhere; no captain, no bus goes down.
    #[test]
    fn abissal_so_pelo_submarino_de_kogen() {
        let abissal = ARQUIPELAGO.iter().position(|d| crate::abissal::e_abissal(d.zona)).unwrap();
        let kogen = ARQUIPELAGO.iter().position(|d| crate::kogen::e_kogen(d.zona)).unwrap();
        assert!(rota_permitida(crate::kogen::ZONA, abissal, Meio::Submarino));
        assert!(!rota_permitida(crate::kogen::ZONA, abissal, Meio::Onibus), "the bus does not dive");
        assert!(!rota_permitida(crate::kogen::ZONA, 0, Meio::Submarino), "Kōgen-tō's submarine only dives");
        for (i, d) in ARQUIPELAGO.iter().enumerate() {
            if i != abissal {
                assert!(!rota_permitida(d.zona, abissal, Meio::Capitao));
                assert!(rota_permitida(crate::abissal::ZONA, i, Meio::Submarino), "Abyssia's submarine to {}", d.zona);
            }
        }
        assert_eq!(meio_para(crate::kogen::ZONA, abissal), Some(Meio::Submarino));
        assert_eq!(meio_para(crate::abissal::ZONA, kogen), Some(Meio::Submarino));
        assert_eq!(meio_para("ilha_inicial", abissal), None);
    }
}
