//! A ESCADA: o que se espera de um personagem em cada nível, e tudo o que
//! deriva disso (docs/ESCADA.md).
//!
//! Pedido do dono (27/09/2026): *"nível não tem que dizer nada, e sim os
//! atributos; o grande defeito está em como a defesa é calculada hoje, em %
//! e não def − ataque; re-pense o melhor plano possível para ficar sempre
//! balanceado e escalável conforme o jogo flui — nível 35 precisar de tantos
//! itens com tanta defesa e ataque, nível 40 tanto a mais, e por aí vai."*
//!
//! ## O defeito que isto corrige
//!
//! Até aqui a defesa era `1,5% por ponto, teto 75%`, somada a percentuais
//! fixos (degraus de VIT e RES, peso da armadura, escudo) até 90%. Uma
//! porcentagem fixa não olha quem bate: 50 de defesa cortava 75% de um lobo
//! nível 1 e 75% de um Colosso nível 60. O teto se alcançava com 50 pontos —
//! um F2P sem refino já estava nele — e daí em diante o ataque do mob subir
//! não mudava nada. Foi medido (27/09): do 20 ao 45, o personagem do dono
//! limpava a zona com 94–98% de vida sem poção, e a "referência" que o jogo
//! usava pra dizer o poder recomendado tinha um terço da defesa de qualquer
//! jogador real.
//!
//! ## A regra
//!
//! ```text
//! dano = max(ataque − defesa, ataque × PISO)
//! ```
//!
//! Nos dois sentidos (mob → jogador, jogador → mob, PvP). Não olha nível de
//! ninguém. A mesma defesa rende resultados diferentes conforme quem bate, e
//! é isso que faz o equipamento ENVELHECER: o que deixa quase imune no 31
//! toma metade do golpe no 51. O piso impede a imunidade — mob fraco sempre
//! belisca, e vinte deles somam.
//!
//! Consequência que vem de graça: golpe grande fura armadura, golpe pequeno
//! não. Dez mobinhos batendo 40 contra defesa 70 dão 10 × 8; um chefe batendo
//! 400 dá 330. O chefe telegráfico volta a doer pra quem refinou tudo, sem
//! regra especial.
//!
//! ## A escada
//!
//! Três retas: ataque, defesa e vida ESPERADOS de um personagem no nível,
//! equipado com a faixa dele a +0 e pontos distribuídos como o
//! `balanceamento::build_do_nivel` já assume (um terço no principal, dois no
//! VIT). Retas, pra a proporção entre dois níveis nunca mudar: o mob do 40
//! está pro jogador do 40 exatamente como o do 20 está pro do 20.
//!
//! Tudo o mais é derivado daqui, e nada tem número próprio:
//!
//! * **item**: o template (no banco) é só a PROPORÇÃO entre as peças; a
//!   escala vem de `escala_de_*` — um conjunto de referência a +0, na cor
//!   natural do nível, soma exatamente a fatia dos itens na escada;
//! * **mob comum**: `mob()` — ataque, defesa e vida saem de metas contra o
//!   jogador esperado, e a espécie entra como PERFIL relativo ao lobo;
//! * **chefe**: `bosses::dano/defesa` lêem daqui;
//! * **poder recomendado**: `dungeon::poder_referencia` é o poder do
//!   personagem esperado — e por isso se compara com o da ficha;
//! * **refino**: só percentual da própria peça (`items::REFINE_BOOST_PER_LEVEL`),
//!   sem parte fixa: +12 de uma faixa ≈ +0 da faixa quinze níveis acima.
//!   Cash compra adiantamento, não imunidade.
//!
//! O que continua em porcentagem: só o que é IDENTIDADE (escudo, armadura
//! pesada), aplicado DEPOIS da subtração e com teto `REDUCAO_MAX`. Os
//! degraus por ponto (VIT/25, RES/30) saem — eram o mesmo defeito.
//!
//! ## O guarda
//!
//! `balanceamento::metas_da_escada` (servidor) roda o simulador a cada cinco
//! níveis do 20 ao 60, em três perfis (faixa certa +0, uma faixa atrás,
//! refinado) e três lugares (zona, forte, ilhota mágica). Mexeu em mob, item,
//! refino ou ponto e a proporção quebrou: `cargo test -p server --bins`
//! reprova. "Sempre balanceado" é um teste, não uma intenção.

use serde::{Deserialize, Serialize};

// ───────────────────────────── a régua ─────────────────────────────

/// As retas. A base absorve o que todo personagem tem sem nível nenhum:
/// os 20 de ataque e 100 de vida do `base_player_stats` e o LEGADO das
/// peças (`legado()`, abaixo).
pub const ATAQUE_BASE: f32 = 45.0;
pub const ATAQUE_POR_NIVEL: f32 = 5.0;
pub const DEFESA_BASE: f32 = 10.0;
pub const DEFESA_POR_NIVEL: f32 = 2.0;
pub const VIDA_BASE: f32 = 200.0;
pub const VIDA_POR_NIVEL: f32 = 20.0;

/// Ataque esperado no nível (equipado na faixa, +0).
pub fn ataque(nivel: u32) -> i32 {
    (ATAQUE_BASE + ATAQUE_POR_NIVEL * nivel as f32).round() as i32
}

/// Defesa esperada no nível.
pub fn defesa(nivel: u32) -> i32 {
    (DEFESA_BASE + DEFESA_POR_NIVEL * nivel as f32).round() as i32
}

/// Vida esperada no nível.
pub fn vida(nivel: u32) -> i32 {
    (VIDA_BASE + VIDA_POR_NIVEL * nivel as f32).round() as i32
}

// ───────────────────────────── o golpe ─────────────────────────────

/// Fração do golpe que passa por qualquer defesa. É o que impede a
/// imunidade: um mob quinze níveis abaixo ainda belisca, e uma horda deles
/// ainda soma.
///
/// **De 0,10 pra 0,06 em 27/09/2026**, a pedido do dono, jogando na Ilha
/// Mágica. O piso é a ÚNICA coisa que uma horda entrega a quem já tem defesa
/// de sobra, e por isso é ele que decide se a horda é jogável: com 0,10 o mob
/// de nível 20–30 (ataque 58–81) tirava 6 a 8 por golpe de quem estava no
/// piso, e doze deles somavam mais do que qualquer defesa podia responder.
/// Com 0,06 o mesmo golpe tira 3 a 5.
///
/// O piso é FRAÇÃO, não número: contra mob de 60 (ataque 151) ele ainda tira
/// 9. É isso que faz o equipamento envelhecer, e é de propósito.
///
/// Medido antes de mexer: baixar o piso **não move a escada**. A tabela da
/// ilhota saiu idêntica com 0,10 e com 0,04, porque quem está NA FAIXA não
/// encosta no piso — só quem tem defesa demais pro que está enfrentando
/// encosta. O preço está no refino, e está escrito em `metas_da_escada`.
///
/// **Corrigido pra 0,085 no mesmo dia**, ainda jogando: 0,06 tirou demais.
/// O dono pediu "4-7 the minimal damage" contra os mesmos mobs em que 0,10
/// tirava 5-8 — 4/5 a 7/8 é 0,85 do que era, e `0,10 × 0,85 = 0,085`. Contra
/// ataque 50-80 dá 4,3 a 6,8; contra mob de 60 (ataque 151) dá 13.
pub const PISO: f32 = 0.085;

/// Teto das reduções percentuais de IDENTIDADE (escudo, armadura pesada),
/// aplicadas depois da subtração. Escudo (0,40) mais pesada (0,10) batem
/// exatamente aqui: o tanque de escudo e armadura pesada é o teto.
pub const REDUCAO_MAX: f32 = 0.50;

/// O golpe: `max(ataque − defesa, ataque × PISO)`, nunca abaixo de 1.
pub fn dano(ataque: i32, defesa: i32) -> i32 {
    let a = ataque.max(0) as f32;
    let liquido = (a - defesa.max(0) as f32).max(a * PISO);
    (liquido.round() as i32).max(1)
}

/// O golpe com a redução de identidade do alvo por cima (teto `REDUCAO_MAX`).
pub fn dano_com_reducao(ataque: i32, defesa: i32, reducao: f32) -> i32 {
    let base = dano(ataque, defesa) as f32;
    let r = reducao.clamp(0.0, REDUCAO_MAX);
    ((base * (1.0 - r)).round() as i32).max(1)
}

// ─────────────────────── o que os itens têm que dar ───────────────────────

/// O CONJUNTO DE REFERÊNCIA: katana, bainha, armadura média e os quatro
/// acessórios. A escada é a dele; espada e escudo com pesada fica acima em
/// defesa e vida, pistola e anel com leve abaixo — é o peso da armadura, e
/// é escolha (`metas_da_escada::a_referencia_anda_na_escada` cobra o
/// corredor).
pub const CONJUNTO_DE_REFERENCIA: [u16; 7] = [
    crate::constants::item_id::KATANA,
    crate::constants::item_id::BAINHA,
    crate::constants::item_id::ARMADURA_MEDIA,
    crate::constants::item_id::BRINCO,
    crate::constants::item_id::AMULETO,
    crate::constants::item_id::BRACELETE,
    crate::constants::item_id::CINTO,
];

/// A soma dos MEIOS de template do conjunto de referência
/// (`items::item_template`, que o banco espelha): a proporção que a escala
/// transforma no número da escada. Destreza entra porque vira ataque na
/// katana (`dex / 5`).
pub const TEMPLATE_ATAQUE: f32 = 20.0;
pub const TEMPLATE_DEX: f32 = 18.0;
pub const TEMPLATE_DEFESA: f32 = 10.5;
pub const TEMPLATE_VIDA: f32 = 77.5;
/// Quanto de ataque um ponto de destreza vale no conjunto de referência.
pub const ATAQUE_POR_DEX: f32 = 0.2;

/// O LEGADO: `effective_stats` ainda soma, por peça vestida, o bônus fixo
/// de `constants::item_bonus` — o item de antes das instâncias, que ficou
/// como piso. Vale o mesmo no nível 1 e no 60, então entra na BASE da
/// escada e não na escala dos itens. Lido da tabela, não copiado: se o
/// legado sair do jogo, isto vira zero e a escada continua fechando.
pub fn legado() -> crate::constants::EquipBonus {
    let mut l = crate::constants::EquipBonus::default();
    for id in CONJUNTO_DE_REFERENCIA {
        let b = crate::constants::item_bonus(id);
        l.hp_max += b.hp_max;
        l.mp_max += b.mp_max;
        l.attack_damage += b.attack_damage;
        l.dex += b.dex;
        l.wis += b.wis;
        l.defense += b.defense;
    }
    l
}

/// O que o personagem traz SEM item da faixa no nível, na build típica: os
/// 20 de base, o legado, e por nível um de FOR (um terço dos pontos), a
/// metade de FOR e a proficiência da katana (`weapon_scaling`).
pub fn ataque_do_personagem(nivel: u32) -> f32 {
    let l = legado();
    20.0 + l.attack_damage as f32 + l.dex as f32 * ATAQUE_POR_DEX + 1.8 * nivel as f32
}

/// Nenhum ponto da build típica dá defesa: só o legado.
pub fn defesa_do_personagem(_nivel: u32) -> f32 {
    legado().defense as f32
}

/// Os 100 de base, o legado, dois por FOR e cinco por VIT (dois terços).
pub fn vida_do_personagem(nivel: u32) -> f32 {
    100.0 + legado().hp_max as f32 + 12.0 * nivel as f32
}

/// A fatia da escada que os itens têm que fornecer no nível.
pub fn ataque_dos_itens(nivel: u32) -> f32 {
    (ataque(nivel) as f32 - ataque_do_personagem(nivel)).max(0.0)
}

pub fn defesa_dos_itens(nivel: u32) -> f32 {
    (defesa(nivel) as f32 - defesa_do_personagem(nivel)).max(0.0)
}

pub fn vida_dos_itens(nivel: u32) -> f32 {
    (vida(nivel) as f32 - vida_do_personagem(nivel)).max(0.0)
}

/// Quanto um ponto de template de ATAQUE (ou destreza, ou sabedoria) vale
/// numa peça deste nível de item.
pub fn escala_de_ataque(item_level: u16) -> f32 {
    ataque_dos_itens(item_level as u32) / (TEMPLATE_ATAQUE + TEMPLATE_DEX * ATAQUE_POR_DEX)
}

pub fn escala_de_defesa(item_level: u16) -> f32 {
    defesa_dos_itens(item_level as u32) / TEMPLATE_DEFESA
}

/// Vida e mana.
pub fn escala_de_vida(item_level: u16) -> f32 {
    vida_dos_itens(item_level as u32) / TEMPLATE_VIDA
}

// ───────────────────────────── os mobs ─────────────────────────────

/// Quanto da personalidade de ATAQUE da espécie entra no líquido: o owlbear
/// tem 2,8 vezes o dano do lobo na tabela, mas contra quem está na escada
/// tira só 1 + 1,8 × 0,6 = 2,1 vezes o líquido. Sem o achatamento, os
/// bichos pesados do nível 10 (owlbear, mago) tiravam 30% a mais do que
/// tiravam no jogo antigo, e a meta do nível 10 morria. A VIDA não achata:
/// bicho grande continua demorando.
pub const ACHATAMENTO_DO_ATAQUE: f32 = 0.6;

/// O que um mob comum de perfil 1,0 TIRA de quem está na escada, por golpe.
/// É a parte do ataque dele que passa da defesa esperada: quem está uma
/// faixa atrás toma isto mais a defesa que lhe falta; quem refinou cai no
/// piso.
///
/// Calibrado pela meta do nível 10 (`metas_de_balanceamento`, a zona pelo
/// centro): o lobo tira do jogador esperado o que tirava no jogo medido de
/// 19/09 — 1,5% da vida —, e o owlbear (2,1× depois do achatamento) uns 3%.
/// A ZONA é fácil de propósito, no 10 e no 60: o que pesa é a densidade
/// (forte, ilhota), e a peça que envelhece.
pub fn dano_liquido_do_mob(nivel: u32) -> f32 {
    1.5 + 0.33 * nivel as f32
}

/// Golpes do jogador esperado pra matar um mob de perfil 1,0 — o LOBO, o
/// mais fraco da tabela. O urso leva 2,3 vezes isso, o owlbear 3,8 e a
/// rainha cinco. Calibrado pra reproduzir o tempo por abate que o jogo
/// medido de 19/09 tinha (3 a 4,5 s no nível 10): o lobo em três golpes, o
/// owlbear em onze. Com 3,5 o mago (que recua a cada golpe) precisava de um
/// golpe a mais do que a investida da katana uma faixa atrás dá, e ela
/// ficava correndo atrás dele sem matar.
pub const GOLPES_POR_MOB: f32 = 3.0;

/// Fração do ataque esperado que a defesa de um mob de perfil 1,0 de defesa
/// segura. Quem está na escada entrega o resto.
pub const DEFESA_DO_MOB: f32 = 0.15;

/// A espécie, RELATIVA AO LOBO da tabela (`economy::KINDS_INICIAIS`): urso
/// tem 2,3 vezes a vida e 1,8 vezes o ataque; a defesa vira uma fração do
/// ataque esperado do jogador (lobo 0, urso 0,08, rochoso 0,22).
///
/// O perfil sai dos MESMOS números que sempre estiveram na tabela do banco:
/// a tabela deixa de ser valor absoluto e passa a ser proporção. Ninguém
/// precisa reescrever o bestiário pra a escada valer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Perfil {
    pub vida: f32,
    pub ataque: f32,
    /// Fração de `ataque(nivel)` que a defesa do bicho segura.
    pub defesa: f32,
}

/// (vida, dano, defesa) do Lobo na tabela: a unidade do perfil.
pub const LOBO: (i32, i32, i32) = (120, 10, 0);
/// Quanto de defesa da tabela vale um ponto de `DEFESA_DO_MOB`... em fração:
/// cada ponto de `def` da tabela segura 1% do ataque esperado, até 25%.
/// Era 2% até 45%: com isso a rainha segurava 40% e a espada e escudo,
/// que já paga o tanque em ataque, batia no piso contra ela.
pub const DEFESA_POR_PONTO_DA_TABELA: f32 = 0.01;
pub const DEFESA_MAX_DO_MOB: f32 = 0.25;

impl Perfil {
    pub const fn novo(vida: f32, ataque: f32, defesa: f32) -> Perfil {
        Perfil { vida, ataque, defesa }
    }

    /// O perfil de uma linha da tabela de mobs (vida, dano, defesa base).
    pub fn relativo_ao_lobo(hp: i32, dmg: i32, def: i32) -> Perfil {
        Perfil {
            vida: (hp.max(1) as f32 / LOBO.0 as f32).max(0.05),
            ataque: (dmg.max(1) as f32 / LOBO.1 as f32).max(0.05),
            defesa: (def.max(0) as f32 * DEFESA_POR_PONTO_DA_TABELA).min(DEFESA_MAX_DO_MOB),
        }
    }
}

/// Os atributos de um mob no nível: o que a tela mostra e a conta usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mob {
    pub vida: i32,
    pub ataque: i32,
    pub defesa: i32,
}

/// Nos primeiros níveis o personagem recém-criado não tem armadura — a
/// jornada do início (`metas_do_inicio`) é medida só com a arma —, então a
/// parte do ataque do mob que a defesa esperada absorveria sobe em rampa
/// até aqui, em vez de nascer inteira. Do nível 12 em diante vale a escada
/// cheia. (O jogo antigo tinha a mesma coisa em `CURVA_DO_INICIO`.)
pub const RAMPA_DO_INICIO: u32 = 12;

/// Quanto da defesa esperada o mob do nível "fura": tudo, depois da rampa.
pub fn furavel(nivel: u32) -> f32 {
    defesa(nivel) as f32 * (nivel as f32 / RAMPA_DO_INICIO as f32).min(1.0)
}

/// Um mob comum deste perfil nascido no nível.
///
/// * defesa: `DEFESA_DO_MOB`-ésimos do ataque esperado, pela fração do perfil;
/// * vida: `GOLPES_POR_MOB` golpes do jogador esperado contra um bicho de
///   defesa 1,0 — a vida não depende da defesa da espécie, então bicho de
///   casca dura demora pelos dois lados, e é isso que ele é;
/// * ataque: a defesa esperada MAIS o líquido do perfil. A parte que "fura"
///   é igual pra toda espécie do nível; a personalidade escala só o que
///   sobra. Sem isso um owlbear de 2,8× teria 2,8× o ataque inteiro e
///   atravessaria qualquer armadura.
pub fn mob(p: &Perfil, nivel: u32) -> Mob {
    let a = ataque(nivel) as f32;
    let golpe_esperado = a * (1.0 - DEFESA_DO_MOB);
    Mob {
        vida: (GOLPES_POR_MOB * golpe_esperado * p.vida).round().max(1.0) as i32,
        ataque: (furavel(nivel)
            + dano_liquido_do_mob(nivel) * (1.0 + (p.ataque - 1.0) * ACHATAMENTO_DO_ATAQUE))
            .round()
            .max(1.0) as i32,
        defesa: (a * p.defesa).round().max(0.0) as i32,
    }
}

// ───────────────────────────── os chefes ─────────────────────────────

/// O golpe COMUM do chefe tira isto de quem está na escada (o telegrafado é
/// fração da vida, `bosses::dano_telegrafado`). Três lobos: doí, mas não é
/// o que mata — o que mata é ficar parado no telegráfico.
pub const LIQUIDO_DO_CHEFE: f32 = 2.5;
/// A defesa do chefe, em fração do ataque esperado: mais que qualquer mob
/// comum. Quem está uma faixa atrás bate no piso.
pub const DEFESA_DO_CHEFE: f32 = 0.40;

pub fn ataque_do_chefe(nivel: u32) -> i32 {
    (furavel(nivel) + dano_liquido_do_mob(nivel) * LIQUIDO_DO_CHEFE).round() as i32
}

/// Golpes do jogador esperado pra derrubar um chefe: dimensionado pra luta
/// de um a quatro minutos esquivando (`metas_dos_chefes`), que é o que a
/// vida de antes (`8 960 + 269 × nível`, teto 20 720) dava contra o
/// jogador de antes. Sem teto: o ataque esperado é reta, então a luta não
/// encurta com o nível.
pub const GOLPES_DO_CHEFE: f32 = 250.0;

pub fn vida_do_chefe(nivel: u32) -> i32 {
    let golpe = dano(ataque(nivel), defesa_do_chefe(nivel)) as f32;
    // O fio manda a vida do chefe em u16.
    ((GOLPES_DO_CHEFE * golpe).round() as i32).min(u16::MAX as i32)
}

pub fn defesa_do_chefe(nivel: u32) -> i32 {
    (ataque(nivel) as f32 * DEFESA_DO_CHEFE).round() as i32
}

// ───────────────────────────── o poder ─────────────────────────────

/// O poder (a conta da ficha, `dungeon::poder_de_stats`) do personagem
/// esperado no nível — só as três retas, sem mana nem destreza.
pub fn poder(nivel: u32) -> i32 {
    ataque(nivel) * 10 + defesa(nivel) * 8 + vida(nivel)
}

/// A linha da escada num nível, pra ficha e pra doc: (ataque, defesa, vida).
pub fn linha(nivel: u32) -> (i32, i32, i32) {
    (ataque(nivel), defesa(nivel), vida(nivel))
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A tabela da doc sai daqui (`cargo test -p shared escada -- --nocapture`).
    #[test]
    fn a_escada_e_reta_e_cresce() {
        let mut antes = linha(0);
        println!("| Nível | Ataque | Defesa | Vida | Mob comum (vida/ataque) | Poder |");
        for n in [1u32, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60] {
            let l = linha(n);
            let m = mob(&Perfil::novo(1.0, 1.0, 0.0), n);
            println!("| {n} | {} | {} | {} | {}/{} | {} |", l.0, l.1, l.2, m.vida, m.ataque, poder(n));
            assert!(l.0 > antes.0 && l.1 > antes.1 && l.2 > antes.2, "nível {n} não cresceu");
            antes = l;
        }
        // Retas: a diferença entre dois níveis não depende de onde se está.
        assert_eq!(ataque(40) - ataque(30), ataque(60) - ataque(50));
        assert_eq!(defesa(40) - defesa(30), defesa(60) - defesa(50));
    }

    /// O piso sai de `PISO`, e não de um 10 escrito à mão: quando ele desceu
    /// pra 0,06 este teste falhava sem que nada estivesse errado.
    #[test]
    fn o_golpe_subtrai_e_tem_piso() {
        let piso_de_100 = (100.0 * PISO).round() as i32;
        assert_eq!(dano(100, 30), 70);
        assert_eq!(dano(100, 100), piso_de_100, "defesa igual ao ataque: o piso");
        assert_eq!(dano(100, 1000), piso_de_100, "defesa absurda: o piso, nunca imune");
        assert_eq!(dano(3, 1000), 1, "nunca abaixo de 1");
        assert_eq!(dano(100, 0), 100);
        assert_eq!(dano(100, -5), 100, "defesa negativa não amplifica");
        // O que o piso É: nunca zero, e nunca a defesa inteira.
        assert!(piso_de_100 > 0 && piso_de_100 < 100);
    }

    #[test]
    fn a_reducao_de_identidade_vem_depois_e_tem_teto() {
        assert_eq!(dano_com_reducao(100, 30, 0.40), 42);
        assert_eq!(dano_com_reducao(100, 30, 0.90), 35, "teto");
        // Duas etapas, como `dano_com_reducao`: o golpe arredonda PRIMEIRO, e a
        // reducao vem por cima do inteiro. Calcular `100 x PISO x 0,5` de uma
        // vez erra por um quando o piso cai em meio ponto.
        let com_teto = ((dano(100, 100) as f32) * (1.0 - REDUCAO_MAX)).round().max(1.0) as i32;
        assert_eq!(dano_com_reducao(100, 100, 0.50), com_teto, "piso e teto juntos: ainda nao e' zero");
        assert!(com_teto >= 1, "piso mais teto nunca chega a zero");
        assert_eq!(dano_com_reducao(100, 30, 0.0), 70);
    }

    /// O que o dono sentiu, em número: a mesma defesa toma cada vez mais
    /// conforme o mob sobe — o equipamento envelhece.
    #[test]
    fn a_mesma_defesa_envelhece() {
        let p = Perfil::novo(1.0, 1.0, 0.0);
        let def_do_31 = defesa(31);
        let toma = |n: u32| dano(mob(&p, n).ataque, def_do_31);
        assert!(toma(31) < toma(41) && toma(41) < toma(51), "{} {} {}", toma(31), toma(41), toma(51));
        // Refinado (+48%, o teto do refino) cai no piso: no máximo ~4× menos
        // que o esperado, nunca zero.
        let refinado = (def_do_31 as f32 * 1.48) as i32;
        let m = mob(&p, 31);
        assert_eq!(dano(m.ataque, refinado), (m.ataque as f32 * PISO).round() as i32);
    }

    /// As metas dos mobs, em cima do jogador esperado.
    #[test]
    fn o_mob_comum_sai_das_metas() {
        for n in [12u32, 20, 30, 40, 50, 60] {
            let m = mob(&Perfil::novo(1.0, 1.0, 0.0), n);
            // O esperado toma exatamente o líquido (depois da rampa).
            assert_eq!(dano(m.ataque, defesa(n)), dano_liquido_do_mob(n).round() as i32, "nível {n}");
            // E mata em GOLPES_POR_MOB golpes contra defesa 1,0.
            let def_um = (ataque(n) as f32 * DEFESA_DO_MOB).round() as i32;
            let golpes = (m.vida as f32 / dano(ataque(n), def_um) as f32).ceil();
            assert!((golpes - GOLPES_POR_MOB).abs() <= 1.0, "nível {n}: {golpes} golpes");
        }
        // O perfil sai da tabela como sempre esteve.
        let urso = Perfil::relativo_ao_lobo(280, 18, 8);
        assert!((urso.vida - 2.333).abs() < 0.01 && (urso.ataque - 1.8).abs() < 0.01);
        assert!((urso.defesa - 0.08).abs() < 0.001);
        let rochoso = Perfil::relativo_ao_lobo(560, 30, 22);
        assert!((rochoso.defesa - 0.22).abs() < 0.001);
        assert_eq!(Perfil::relativo_ao_lobo(9999, 99, 999).defesa, DEFESA_MAX_DO_MOB);
    }

    /// A personalidade escala só o líquido: o owlbear (2,8×) não fura
    /// armadura 2,8 vezes mais.
    #[test]
    fn a_especie_escala_o_liquido_e_nao_o_ataque_inteiro() {
        let n = 30;
        let lobo = mob(&Perfil::relativo_ao_lobo(120, 10, 0), n);
        let owl = mob(&Perfil::relativo_ao_lobo(460, 28, 4), n);
        let d = defesa(n);
        let (tl, to) = (dano(lobo.ataque, d), dano(owl.ataque, d));
        let esperado = 1.0 + 1.8 * ACHATAMENTO_DO_ATAQUE;
        assert!((to as f32 / tl as f32 - esperado).abs() < 0.2, "{tl} {to}");
        assert!((owl.ataque as f32) < lobo.ataque as f32 * 2.0, "o ataque inteiro não dobra");
    }

    #[test]
    fn a_escala_dos_itens_fecha_no_conjunto_de_referencia() {
        for ilvl in [5u16, 18, 35, 60] {
            let n = ilvl as u32;
            let atk = (TEMPLATE_ATAQUE + TEMPLATE_DEX * ATAQUE_POR_DEX) * escala_de_ataque(ilvl)
                + ataque_do_personagem(n);
            let def = TEMPLATE_DEFESA * escala_de_defesa(ilvl) + defesa_do_personagem(n);
            let hp = TEMPLATE_VIDA * escala_de_vida(ilvl) + vida_do_personagem(n);
            assert!((atk - ataque(n) as f32).abs() < 1.0, "ilvl {ilvl}: {atk} vs {}", ataque(n));
            assert!((def - defesa(n) as f32).abs() < 1.0);
            assert!((hp - vida(n) as f32).abs() < 1.0);
        }
    }

    #[test]
    fn o_chefe_bate_mais_e_segura_mais_que_o_comum() {
        for n in [12u32, 30, 60] {
            let c = ataque_do_chefe(n);
            let comum = mob(&Perfil::novo(1.0, 1.0, 0.0), n);
            assert!(c > comum.ataque);
            assert_eq!(dano(c, defesa(n)), (dano_liquido_do_mob(n) * LIQUIDO_DO_CHEFE).round() as i32);
            assert!(defesa_do_chefe(n) > comum.defesa);
            assert!(vida_do_chefe(n) > 20 * comum.vida);
        }
        assert!(vida_do_chefe(60) < u16::MAX as i32, "cabe no u16 do fio");
        assert_eq!(vida_do_chefe(200), u16::MAX as i32, "e nunca estoura");
    }

    /// Nos primeiros níveis o mob fura menos: o recém-criado não tem armadura.
    #[test]
    fn a_rampa_do_inicio() {
        let p = Perfil::novo(1.0, 1.0, 0.0);
        let sem_armadura = 0;
        let toma = |n: u32| dano(mob(&p, n).ataque, sem_armadura);
        assert!(toma(1) < 10, "lobo do nível 1 contra quem só tem a arma: {}", toma(1));
        assert!(toma(3) < toma(6) && toma(6) < toma(12));
        assert_eq!(furavel(RAMPA_DO_INICIO), defesa(RAMPA_DO_INICIO) as f32);
        assert_eq!(furavel(60), defesa(60) as f32);
    }
}
