//! Skills: doze, todas ativas, três por conjunto de arma.
//!
//! Substitui um sistema de 64 skills com oito árvores, ranks, afinidades e
//! passivas. O que sobrou é o que se vê acontecer na tela.
//!
//! **Passiva não existe.** Vinte das sessenta e quatro eram — número que sobe
//! sem nada acontecer. Num jogo de vista alta, o que o outro jogador vê você
//! fazer é metade do combate, e uma passiva não é vista por ninguém.
//!
//! **Não se APRENDE skill.** Ela vem com o conjunto de arma que está na mão, e
//! a ordem dela destrava com a proficiência daquele conjunto. Trocar de arma é
//! trocar de classe — então trocar de arma é trocar de skills, e não haveria
//! sentido em decorar as de uma arma que não se usa.

use serde::{Deserialize, Serialize};

/// O conjunto de arma. É também a proficiência: uma árvore por conjunto.
///
/// A arma é o PAR — principal mais secundária amarrada a ela. Não existe
/// offhand livre nem "duas mãos".
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

/// A forma do efeito — e, por consequência, o gesto que o corpo faz.
///
/// É por AQUI que a animação é dimensionada: o gesto sai da forma, não da
/// skill. Doze skills em cinco formas são cinco gestos, e a diferença entre
/// duas skills da mesma forma é velocidade e efeito visual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Forma {
    /// Em si mesmo: cura, escudo, impulso.
    EmSi,
    /// Projétil na direção da mira.
    Projetil,
    /// Cone à frente.
    Cone,
    /// Círculo num ponto do chão.
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
        [Self::EmSi, Self::Projetil, Self::Cone, Self::Circulo, Self::Linha]
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

/// Nível de proficiência que destrava cada ordem.
///
/// A primeira vem junto com a arma: pegar o conjunto e não ter o que apertar
/// seria uma arma sem verbo.
pub const DESTRAVA_EM: [u32; 3] = [1, 10, 25];

impl Skill {
    /// Esta skill está disponível com este nível de proficiência?
    pub fn destravada(&self, nivel_da_proficiencia: u32) -> bool {
        let i = (self.ordem.max(1) - 1).min(2) as usize;
        nivel_da_proficiencia >= DESTRAVA_EM[i]
    }
}

#[cfg(test)]
mod testes {
    use super::*;

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
        for f in [Forma::EmSi, Forma::Projetil, Forma::Cone, Forma::Circulo, Forma::Linha] {
            assert_eq!(Forma::de_chave(f.chave()), Some(f));
        }
    }
}
