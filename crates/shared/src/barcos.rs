//! Os CASCOS (docs/MAR_ABERTO.md): conta pura sobre o barco.
//!
//! Irmao de `pets.rs` e `montarias.rs`, e pelo mesmo motivo: sao funcoes
//! puras, entao cliente e servidor concordam sem uma mensagem de protocolo.
//! O painel do porto desenha exatamente o que o servidor vai cobrar.
//!
//! # Tres eixos, e nao quatro
//!
//! **Casco**, **vela** e **porao** sao os tres verbos que o mar tem:
//! sobreviver, atravessar, carregar. Um quarto eixo hoje seria um numero
//! subindo com nada do outro lado. O canhao existe como `melhorias[3]` e fica
//! zerado ate' o PvP naval.
//!
//! # Nada falha
//!
//! Melhoria e reparo SEMPRE dao certo. Nao ha' dado, nao ha' destruicao, nao
//! ha' decaimento — consequencia direta da regra que rege o mar inteiro:
//!
//! > Nenhum estado alcancavel pode deixar o jogador sem poder navegar.
//!
//! A `forja` ja' e' dona da fantasia de aposta (refino acima de +5 destroi a
//! peca). Por um dado destrutivo na coisa que da' acesso a VIAJAR, o jogador
//! perde o barco pra RNG e, sem dinheiro, nao sai mais da ilha. A valvula de
//! arrependimento e' o mercado: melhoria e' por instancia, entao Chalupa
//! turbinada vale mais que Chalupa nua, e se vende.

use crate::constants::item_id;

/// Quantos niveis cada eixo tem.
pub const MELHORIA_MAX: u8 = 5;

/// Os eixos, por indice em `BarcoData::melhorias`.
pub mod eixo {
    pub const CASCO: usize = 0;
    pub const VELA: usize = 1;
    pub const PORAO: usize = 2;
    /// Fase 2 (PvP naval). Fica em 0 ate' la'.
    pub const CANHAO: usize = 3;
}

/// (nome, casco base, +casco por nivel, vela base, +vela, porao base, +porao,
/// nivel de craft).
/// Os cascos existem numa escala de NAVIO, nao de gente.
///
/// A primeira versao deu 400 de casco pra Chalupa — tres vezes a vida de um
/// jogador. `metas_do_mar` mostrou o que isso significava: dois peixes
/// afundavam a travessia em TREZE segundos. Um veiculo que existe pra
/// aguentar o mar precisa de outra ordem de grandeza, e a que fecha a conta e'
/// esta.
const CASCOS: [(&str, u16, u16, f32, f32, u8, u8, u32); 3] = [
    ("Chalupa", 2400, 720, 9.0, 0.5, 4, 1, 5),
    ("Escuna", 6600, 1560, 10.0, 0.6, 8, 2, 25),
    ("Nau", 15600, 3360, 11.0, 0.7, 14, 3, 45),
];

fn linha(item_id: u16) -> Option<&'static (&'static str, u16, u16, f32, f32, u8, u8, u32)> {
    item_id::casco_de_id(item_id).and_then(|c| CASCOS.get(c as usize - 1))
}

pub fn nome_do_item(item_id: u16) -> Option<String> {
    linha(item_id).map(|l| l.0.to_string())
}

/// Pontos de casco no maximo, com a melhoria dada.
pub fn casco_max(item_id: u16, melhoria: u8) -> u16 {
    linha(item_id).map_or(0, |l| l.1 + l.2 * melhoria.min(MELHORIA_MAX) as u16)
}

/// Velocidade em unidades por segundo.
///
/// O teto duro e' o FIO: `EntityState::vel` e' `i8` a `POS_SCALE`, o que da'
/// 15,9 u/s. A Nau no maximo chega em 14,5 e para ali de proposito — acima de
/// ~16 a AOI do mar daria menos de 2,5 s de aviso, que nao da' num celular.
pub fn velocidade(item_id: u16, melhoria: u8) -> f32 {
    linha(item_id).map_or(9.0, |l| l.3 + l.4 * melhoria.min(MELHORIA_MAX) as f32)
}

/// Slots do porao.
pub fn capacidade_do_porao(item_id: u16, melhoria: u8) -> u8 {
    linha(item_id).map_or(0, |l| l.5 + l.6 * melhoria.min(MELHORIA_MAX))
}

/// Nivel de personagem pra construir este casco.
pub fn nivel_de_craft(item_id: u16) -> u32 {
    linha(item_id).map_or(1, |l| l.7)
}

/// Um casco novinho.
pub fn novo(item_id: u16) -> crate::items::BarcoData {
    crate::items::BarcoData {
        casco: casco_max(item_id, 0),
        ..Default::default()
    }
}

/// O que a instancia diz do barco. `None` = casco de antes do campo, lido
/// como cheio e sem melhoria.
pub fn dados(inst: Option<&crate::items::ItemInstance>, item_id: u16) -> crate::items::BarcoData {
    inst.and_then(|i| i.barco).unwrap_or_else(|| novo(item_id))
}

/// (material, quantidade) pra subir `eixo` ate' `nivel_alvo`.
///
/// Ressuscita `WOOD_T1..T4`, que hoje caem da coleta de arvore e quase nada
/// consome. A floresta passa a alimentar o mar, e a ilha inicial vira
/// fornecedora — e e' a unica parte da economia que nao compete com a forja.
pub fn custo_da_melhoria(item_id: u16, nivel_alvo: u8) -> [(u16, u32); 3] {
    let h = item_id::casco_de_id(item_id).unwrap_or(1) as u32;
    let n = nivel_alvo.clamp(1, MELHORIA_MAX) as u32;
    [
        (madeira_da_classe(h as u8), 20 * n * h),
        (item_id::na_cor(item_id::STEEL, h as u8), 10 * n * h),
        (item_id::COPPER, 150 * n * n * h * h),
    ]
}

/// A madeira da classe do casco.
pub fn madeira_da_classe(classe: u8) -> u16 {
    match classe {
        1 => item_id::WOOD_T1,
        2 => item_id::WOOD_T2,
        _ => item_id::WOOD_T3,
    }
}

/// (cobre, madeira) pra reparar o casco ate' cheio.
pub fn custo_do_reparo(item_id: u16, d: &crate::items::BarcoData) -> (u32, u32) {
    let max = casco_max(item_id, d.melhorias[eixo::CASCO]);
    let faltando = max.saturating_sub(d.casco) as f32;
    if faltando <= 0.0 {
        return (0, 0);
    }
    let h = item_id::casco_de_id(item_id).unwrap_or(1) as f32;
    // O cobre acompanha a ambicao, nao o tamanho do numero: com casco em
    // escala de navio, meio cobre por ponto viraria milhares por conserto de
    // barco inicial.
    let cobre = (faltando * (0.1 * h)).ceil() as u32;
    let madeira = ((faltando * 100.0 / max.max(1) as f32) * h / 5.0).ceil() as u32;
    (cobre, madeira)
}

/// Quanto o reparo DE GRACA devolve: um quarto do casco.
///
/// Incondicional, sem cooldown e sem teste de riqueza — e' o que impede a
/// travessia obrigatoria de travar uma conta. Enunciado numa regra: *"a doca
/// nao deixa ninguem sem barco; o resto e' com voce"*. Um quarto atravessa se
/// voce desviar do perigo, e nao sobrevive a atravessar por dentro dele,
/// entao pagar continua estritamente melhor.
pub fn reparo_de_graca(item_id: u16, d: &crate::items::BarcoData) -> u16 {
    casco_max(item_id, d.melhorias[eixo::CASCO]) / 4
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Melhorar e' a PONTE, trocar de casco e' o SALTO.
    ///
    /// A sobreposicao e' de proposito: uma Chalupa no maximo fica logo ABAIXO
    /// de uma Escuna nua. Assim ninguem e' punido por investir no barco que
    /// tem, e ninguem pula uma classe investindo.
    #[test]
    fn melhorar_chega_perto_da_classe_seguinte_e_nao_passa() {
        let chalupa = item_id::BARCO_BASE;
        let escuna = item_id::BARCO_ESCUNA;
        let nau = item_id::BARCO_NAU;
        assert!(casco_max(chalupa, MELHORIA_MAX) < casco_max(escuna, 0));
        assert!(casco_max(escuna, MELHORIA_MAX) < casco_max(nau, 0));
        // E chega PERTO: menos de 15% abaixo.
        let folga = casco_max(escuna, 0) as f32 / casco_max(chalupa, MELHORIA_MAX) as f32;
        assert!(folga < 1.15, "o salto ficou grande demais: {folga:.2}x");
    }

    /// A velocidade nunca passa do que o fio carrega.
    #[test]
    fn nenhum_casco_passa_do_teto_do_fio() {
        let teto = i8::MAX as f32 / crate::components::POS_SCALE;
        for id in item_id::BARCO_BASE..=item_id::BARCO_ULTIMO {
            let v = velocidade(id, MELHORIA_MAX);
            assert!(v < teto, "casco {id} a {v} u/s passa do teto {teto}");
        }
    }

    /// Reparar barco caro custa mais — e barco barato, quase nada.
    ///
    /// E' a forma que importa: o naufragio cobra pela AMBICAO do jogador, nao
    /// pelo bolso dele. O pior dia de um novato e' o reparo de uma Chalupa.
    #[test]
    fn o_reparo_cobra_pela_ambicao() {
        let quebrado = |id| crate::items::BarcoData {
            casco: 0,
            ..novo(id)
        };
        let (chalupa, _) = custo_do_reparo(item_id::BARCO_BASE, &quebrado(item_id::BARCO_BASE));
        let (nau, _) = custo_do_reparo(item_id::BARCO_NAU, &quebrado(item_id::BARCO_NAU));
        assert!(chalupa < 300, "reparo da Chalupa caro demais: {chalupa}");
        assert!(nau > chalupa * 10, "a Nau tinha que doer: {nau}");
        // Barco cheio nao cobra nada.
        assert_eq!(custo_do_reparo(item_id::BARCO_BASE, &novo(item_id::BARCO_BASE)), (0, 0));
    }

    /// O piso de graca tira todo mundo do porto, e nao substitui pagar.
    #[test]
    fn o_piso_de_graca_nao_substitui_pagar() {
        let id = item_id::BARCO_BASE;
        let d = crate::items::BarcoData { casco: 0, ..novo(id) };
        let piso = reparo_de_graca(id, &d);
        assert!(piso > 0, "o piso tem que tirar o jogador do porto");
        assert!(
            (piso as f32) < casco_max(id, 0) as f32 * 0.3,
            "o piso nao pode ser bom o bastante pra ninguem pagar"
        );
    }
}
