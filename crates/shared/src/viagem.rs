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

/// O menu inteiro: uma linha por ilha do arquipelago. `no_ar(zona)` diz se ha'
/// canal daquela ilha rodando.
pub fn destinos(
    zona_atual: &str,
    indice_da_historia: Option<u32>,
    no_ar: &dyn Fn(&str) -> bool,
) -> Vec<Destino> {
    ARQUIPELAGO
        .iter()
        .enumerate()
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
        let m = destinos("ilha_gelo", Some(p + 3), &so_bosque_e_gelo);
        assert_eq!(m.len(), ARQUIPELAGO.len());
        assert_eq!(m[0].estado, estado::LIBERADA, "da' pra voltar ao Bosque");
        assert_eq!(m[1].estado, estado::AQUI);
        assert_eq!(m[2].estado, estado::BLOQUEADA);
        let sem_bosque = |z: &str| z == "ilha_gelo";
        assert_eq!(destinos("ilha_gelo", Some(p), &sem_bosque)[0].estado, estado::FORA_DO_AR);
    }
}
