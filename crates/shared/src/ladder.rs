//! A ESCADA: o que se espera de um personagem em cada nível, e tudo o que
//! deriva disso (docs/ESCADA.md).
//!
//! Pedido do dono (27/09/2026): *"nível não tem que dizer nada, e sim os
//! atributos; o grande defeito está em como a defense é calculada hoje, em %
//! e não def − attack; re-pense o melhor plano possível para ficar sempre
//! balanceado e escalável conforme o jogo flui — nível 35 precisar de tantos
//! itens com tanta defense e attack, nível 40 tanto a mais, e por aí vai."*
//!
//! ## O defeito que isto corrige
//!
//! Até aqui a defense era `1,5% por ponto, teto 75%`, somada a percentuais
//! fixos (degraus de VIT e RES, peso da armadura, escudo) até 90%. Uma
//! porcentagem fixa não olha quem bate: 50 de defense cortava 75% de um lobo
//! nível 1 e 75% de um Colosso nível 60. O teto se alcançava com 50 pontos —
//! um F2P sem refino já estava nele — e daí em diante o attack do mob subir
//! não mudava nada. Foi medido (27/09): do 20 ao 45, o personagem do dono
//! limpava a zona com 94–98% de health sem poção, e a "referência" que o jogo
//! usava pra dizer o power recomendado tinha um terço da defense de qualquer
//! jogador real.
//!
//! ## A regra
//!
//! ```text
//! damage = max(attack − defense, attack × FLOOR)
//! ```
//!
//! Nos dois sentidos (mob → jogador, jogador → mob, PvP). Não olha nível de
//! ninguém. A mesma defense rende resultados diferentes conforme quem bate, e
//! é isso que faz o equipamento ENVELHECER: o que deixa quase imune no 31
//! toma metade do golpe no 51. O piso impede a imunidade — mob fraco sempre
//! belisca, e vinte deles somam.
//!
//! Consequência que vem de graça: golpe grande fura armadura, golpe pequeno
//! não. Dez mobinhos batendo 40 contra defense 70 dão 10 × 8; um chefe batendo
//! 400 dá 330. O chefe telegráfico volta a doer pra quem refinou tudo, sem
//! regra especial.
//!
//! ## A ladder
//!
//! Três retas: attack, defense e health ESPERADOS de um personagem no nível,
//! equipado com a faixa dele a +0 e pontos distribuídos como o
//! `balanceamento::build_do_nivel` já assume (um terço no principal, dois no
//! VIT). Retas, pra a proporção entre dois níveis nunca mudar: o mob do 40
//! está pro jogador do 40 exatamente como o do 20 está pro do 20.
//!
//! Tudo o mais é derivado daqui, e nada tem número próprio:
//!
//! * **item**: o template (no banco) é só a PROPORÇÃO entre as peças; a
//!   escala vem de `escala_de_*` — um conjunto de referência a +0, na cor
//!   natural do nível, soma exatamente a fatia dos itens na ladder;
//! * **mob comum**: `mob()` — attack, defense e health saem de metas contra o
//!   jogador esperado, e a espécie entra como PERFIL relativo ao lobo;
//! * **chefe**: `bosses::damage/defense` lêem daqui;
//! * **power recomendado**: `dungeon::poder_referencia` é o power do
//!   personagem esperado — e por isso se compara com o da ficha;
//! * **refino**: só percentual da própria peça (`items::REFINE_BOOST_PER_LEVEL`),
//!   sem parte fixa: +12 de uma faixa ≈ +0 da faixa quinze níveis acima.
//!   Cash compra adiantamento, não imunidade.
//!
//! O que continua em porcentagem: só o que é IDENTIDADE (escudo, armadura
//! pesada), aplicado DEPOIS da subtração e com teto `MAX_REDUCTION`. Os
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
/// os 20 de attack e 100 de health do `base_player_stats` e o LEGADO das
/// peças (`legacy()`, abaixo).
pub const BASE_ATTACK: f32 = 45.0;
pub const ATTACK_PER_LEVEL: f32 = 5.0;
pub const BASE_DEFENSE: f32 = 10.0;
pub const DEFENSE_PER_LEVEL: f32 = 2.0;
pub const BASE_HEALTH: f32 = 200.0;
pub const HEALTH_PER_LEVEL: f32 = 20.0;

/// Ataque esperado no nível (equipado na faixa, +0).
pub fn attack(level: u32) -> i32 {
    (BASE_ATTACK + ATTACK_PER_LEVEL * level as f32).round() as i32
}

/// Defesa esperada no nível.
pub fn defense(level: u32) -> i32 {
    (BASE_DEFENSE + DEFENSE_PER_LEVEL * level as f32).round() as i32
}

/// Vida esperada no nível.
pub fn health(level: u32) -> i32 {
    (BASE_HEALTH + HEALTH_PER_LEVEL * level as f32).round() as i32
}

// ───────────────────────────── o golpe ─────────────────────────────

/// Fração do golpe que passa por qualquer defense. É o que impede a
/// imunidade: um mob quinze níveis abaixo ainda belisca, e uma horda deles
/// ainda soma.
///
/// **De 0,10 pra 0,06 em 27/09/2026**, a pedido do dono, jogando na Ilha
/// Mágica. O piso é a ÚNICA coisa que uma horda entrega a quem já tem defense
/// de sobra, e por isso é ele que decide se a horda é jogável: com 0,10 o mob
/// de nível 20–30 (attack 58–81) tirava 6 a 8 por golpe de quem estava no
/// piso, e doze deles somavam mais do que qualquer defense podia responder.
/// Com 0,06 o mesmo golpe tira 3 a 5.
///
/// O piso é FRAÇÃO, não número: contra mob de 60 (attack 151) ele ainda tira
/// 9. É isso que faz o equipamento envelhecer, e é de propósito.
///
/// Medido antes de mexer: baixar o piso **não move a ladder**. A tabela da
/// ilhota saiu idêntica com 0,10 e com 0,04, porque quem está NA FAIXA não
/// encosta no piso — só quem tem defense demais pro que está enfrentando
/// encosta. O preço está no refino, e está escrito em `metas_da_escada`.
///
/// **Corrigido pra 0,085 no mesmo dia**, ainda jogando: 0,06 tirou demais.
/// O dono pediu "4-7 the minimal damage" contra os mesmos mobs em que 0,10
/// tirava 5-8 — 4/5 a 7/8 é 0,85 do que era, e `0,10 × 0,85 = 0,085`. Contra
/// attack 50-80 dá 4,3 a 6,8; contra mob de 60 (attack 151) dá 13.
pub const FLOOR: f32 = 0.085;

/// Teto das reduções percentuais de IDENTIDADE (escudo, armadura pesada),
/// aplicadas depois da subtração. Escudo (0,40) mais pesada (0,10) batem
/// exatamente aqui: o tanque de escudo e armadura pesada é o teto.
pub const MAX_REDUCTION: f32 = 0.50;

/// O golpe: `max(attack − defense, attack × FLOOR)`, nunca abaixo de 1.
pub fn damage(attack: i32, defense: i32) -> i32 {
    let a = attack.max(0) as f32;
    let liquido = (a - defense.max(0) as f32).max(a * FLOOR);
    (liquido.round() as i32).max(1)
}

/// O golpe com a redução de identidade do alvo por cima (teto `MAX_REDUCTION`).
pub fn damage_with_reduction(attack: i32, defense: i32, reduction: f32) -> i32 {
    let base = damage(attack, defense) as f32;
    let r = reduction.clamp(0.0, MAX_REDUCTION);
    ((base * (1.0 - r)).round() as i32).max(1)
}

// ─────────────────────── o que os itens têm que dar ───────────────────────

/// O CONJUNTO DE REFERÊNCIA: katana, bainha, armadura média e os quatro
/// acessórios. A ladder é a dele; espada e escudo com pesada fica acima em
/// defense e health, pistola e anel com leve abaixo — é o peso da armadura, e
/// é escolha (`metas_da_escada::a_referencia_anda_na_escada` cobra o
/// corredor).
pub const REFERENCE_SET: [u16; 7] = [
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
/// transforma no número da ladder. Destreza entra porque vira attack na
/// katana (`dex / 5`).
pub const TEMPLATE_ATTACK: f32 = 20.0;
pub const TEMPLATE_DEX: f32 = 18.0;
pub const TEMPLATE_DEFENSE: f32 = 10.5;
pub const TEMPLATE_HEALTH: f32 = 77.5;
/// Quanto de attack um ponto de destreza vale no conjunto de referência.
pub const ATTACK_PER_DEX: f32 = 0.2;

/// O LEGADO: `effective_stats` ainda soma, por peça vestida, o bônus fixo
/// de `constants::item_bonus` — o item de antes das instâncias, que ficou
/// como piso. Vale o mesmo no nível 1 e no 60, então entra na BASE da
/// ladder e não na escala dos itens. Lido da tabela, não copiado: se o
/// legacy sair do jogo, isto vira zero e a ladder continua fechando.
pub fn legacy() -> crate::constants::EquipBonus {
    let mut l = crate::constants::EquipBonus::default();
    for id in REFERENCE_SET {
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
/// 20 de base, o legacy, e por nível um de FOR (um terço dos pontos), a
/// metade de FOR e a proficiência da katana (`weapon_scaling`).
pub fn character_attack(level: u32) -> f32 {
    let l = legacy();
    20.0 + l.attack_damage as f32 + l.dex as f32 * ATTACK_PER_DEX + 1.8 * level as f32
}

/// Nenhum ponto da build típica dá defense: só o legacy.
pub fn character_defense(_nivel: u32) -> f32 {
    legacy().defense as f32
}

/// Os 100 de base, o legacy, dois por FOR e cinco por VIT (dois terços).
pub fn character_health(level: u32) -> f32 {
    100.0 + legacy().hp_max as f32 + 12.0 * level as f32
}

/// A fatia da ladder que os itens têm que fornecer no nível.
pub fn item_attack(level: u32) -> f32 {
    (attack(level) as f32 - character_attack(level)).max(0.0)
}

pub fn item_defense(level: u32) -> f32 {
    (defense(level) as f32 - character_defense(level)).max(0.0)
}

pub fn item_health(level: u32) -> f32 {
    (health(level) as f32 - character_health(level)).max(0.0)
}

/// Quanto um ponto de template de ATAQUE (ou destreza, ou sabedoria) vale
/// numa peça deste nível de item.
pub fn attack_scale(item_level: u16) -> f32 {
    item_attack(item_level as u32) / (TEMPLATE_ATTACK + TEMPLATE_DEX * ATTACK_PER_DEX)
}

pub fn defense_scale(item_level: u16) -> f32 {
    item_defense(item_level as u32) / TEMPLATE_DEFENSE
}

/// Vida e mana.
pub fn health_scale(item_level: u16) -> f32 {
    item_health(item_level as u32) / TEMPLATE_HEALTH
}

// ───────────────────────────── os mobs ─────────────────────────────

/// Quanto da personalidade de ATAQUE da espécie entra no líquido: o owlbear
/// tem 2,8 vezes o damage do lobo na tabela, mas contra quem está na ladder
/// tira só 1 + 1,8 × 0,6 = 2,1 vezes o líquido. Sem o achatamento, os
/// bichos pesados do nível 10 (owlbear, mago) tiravam 30% a mais do que
/// tiravam no jogo antigo, e a meta do nível 10 morria. A VIDA não achata:
/// bicho grande continua demorando.
pub const ATTACK_FLATTENING: f32 = 0.6;

/// O que um mob comum de perfil 1,0 TIRA de quem está na ladder, por golpe.
/// É a parte do attack dele que passa da defense esperada: quem está uma
/// faixa atrás toma isto mais a defense que lhe falta; quem refinou cai no
/// piso.
///
/// Calibrado pela meta do nível 10 (`metas_de_balanceamento`, a zona pelo
/// centro): o lobo tira do jogador esperado o que tirava no jogo medido de
/// 19/09 — 1,5% da health —, e o owlbear (2,1× depois do achatamento) uns 3%.
/// A ZONA é fácil de propósito, no 10 e no 60: o que pesa é a densidade
/// (forte, ilhota), e a peça que envelhece.
pub fn mob_net_damage(level: u32) -> f32 {
    1.5 + 0.33 * level as f32
}

/// Golpes do jogador esperado pra matar um mob de perfil 1,0 — o WOLF, o
/// mais fraco da tabela. O urso leva 2,3 vezes isso, o owlbear 3,8 e a
/// rainha cinco. Calibrado pra reproduzir o tempo por abate que o jogo
/// medido de 19/09 tinha (3 a 4,5 s no nível 10): o lobo em três golpes, o
/// owlbear em onze. Com 3,5 o mago (que recua a cada golpe) precisava de um
/// golpe a mais do que a investida da katana uma faixa atrás dá, e ela
/// ficava correndo atrás dele sem matar.
pub const STRIKES_PER_MOB: f32 = 3.0;

/// Fração do attack esperado que a defense de um mob de perfil 1,0 de defense
/// segura. Quem está na ladder entrega o resto.
pub const MOB_DEFENSE: f32 = 0.15;

/// A espécie, RELATIVA AO WOLF da tabela (`economy::KINDS_INICIAIS`): urso
/// tem 2,3 vezes a health e 1,8 vezes o attack; a defense vira uma fração do
/// attack esperado do jogador (lobo 0, urso 0,08, rochoso 0,22).
///
/// O perfil sai dos MESMOS números que sempre estiveram na tabela do banco:
/// a tabela deixa de ser valor absoluto e passa a ser proporção. Ninguém
/// precisa reescrever o bestiário pra a ladder valer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub health: f32,
    pub attack: f32,
    /// Fração de `attack(level)` que a defense do bicho segura.
    pub defense: f32,
}

/// (health, damage, defense) do Lobo na tabela: a unidade do perfil.
pub const WOLF: (i32, i32, i32) = (120, 10, 0);
/// Quanto de defense da tabela vale um ponto de `MOB_DEFENSE`... em fração:
/// cada ponto de `def` da tabela segura 1% do attack esperado, até 25%.
/// Era 2% até 45%: com isso a rainha segurava 40% e a espada e escudo,
/// que já paga o tanque em attack, batia no piso contra ela.
pub const DEFENSE_PER_TABLE_POINT: f32 = 0.01;
pub const MOB_MAX_DEFENSE: f32 = 0.25;

impl Profile {
    pub const fn novo(health: f32, attack: f32, defense: f32) -> Profile {
        Profile { health, attack, defense }
    }

    /// O perfil de uma row da tabela de mobs (health, damage, defense base).
    pub fn relativo_ao_lobo(hp: i32, dmg: i32, def: i32) -> Profile {
        Profile {
            health: (hp.max(1) as f32 / WOLF.0 as f32).max(0.05),
            attack: (dmg.max(1) as f32 / WOLF.1 as f32).max(0.05),
            defense: (def.max(0) as f32 * DEFENSE_PER_TABLE_POINT).min(MOB_MAX_DEFENSE),
        }
    }
}

/// Os atributos de um mob no nível: o que a tela mostra e a conta usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mob {
    pub health: i32,
    pub attack: i32,
    pub defense: i32,
}

/// Nos primeiros níveis o personagem recém-criado não tem armadura — a
/// jornada do início (`metas_do_inicio`) é medida só com a arma —, então a
/// parte do attack do mob que a defense esperada absorveria sobe em rampa
/// até aqui, em vez de nascer inteira. Do nível 12 em diante vale a ladder
/// cheia. (O jogo antigo tinha a mesma coisa em `CURVA_DO_INICIO`.)
pub const EARLY_RAMP: u32 = 12;

/// Quanto da defense esperada o mob do nível "fura": tudo, depois da rampa.
pub fn pierceable(level: u32) -> f32 {
    defense(level) as f32 * (level as f32 / EARLY_RAMP as f32).min(1.0)
}

/// Um mob comum deste perfil nascido no nível.
///
/// * defense: `MOB_DEFENSE`-ésimos do attack esperado, pela fração do perfil;
/// * health: `STRIKES_PER_MOB` golpes do jogador esperado contra um bicho de
///   defense 1,0 — a health não depende da defense da espécie, então bicho de
///   casca dura demora pelos dois lados, e é isso que ele é;
/// * attack: a defense esperada MAIS o líquido do perfil. A parte que "fura"
///   é igual pra toda espécie do nível; a personalidade escala só o que
///   sobra. Sem isso um owlbear de 2,8× teria 2,8× o attack inteiro e
///   atravessaria qualquer armadura.
pub fn mob(p: &Profile, level: u32) -> Mob {
    let a = attack(level) as f32;
    let golpe_esperado = a * (1.0 - MOB_DEFENSE);
    Mob {
        health: (STRIKES_PER_MOB * golpe_esperado * p.health).round().max(1.0) as i32,
        attack: (pierceable(level)
            + mob_net_damage(level) * (1.0 + (p.attack - 1.0) * ATTACK_FLATTENING))
            .round()
            .max(1.0) as i32,
        defense: (a * p.defense).round().max(0.0) as i32,
    }
}

// ───────────────────────────── os chefes ─────────────────────────────

/// O golpe COMUM do chefe tira isto de quem está na ladder (o telegrafado é
/// fração da health, `bosses::dano_telegrafado`). Três lobos: doí, mas não é
/// o que mata — o que mata é ficar parado no telegráfico.
pub const BOSS_NET: f32 = 2.5;
/// A defense do chefe, em fração do attack esperado: mais que qualquer mob
/// comum. Quem está uma faixa atrás bate no piso.
pub const BOSS_DEFENSE: f32 = 0.40;

pub fn boss_attack(level: u32) -> i32 {
    (pierceable(level) + mob_net_damage(level) * BOSS_NET).round() as i32
}

/// Golpes do jogador esperado pra derrubar um chefe: dimensionado pra luta
/// de um a quatro minutos esquivando (`metas_dos_chefes`), que é o que a
/// health de antes (`8 960 + 269 × nível`, teto 20 720) dava contra o
/// jogador de antes. Sem teto: o attack esperado é reta, então a luta não
/// encurta com o nível.
pub const BOSS_STRIKES: f32 = 250.0;

pub fn boss_health(level: u32) -> i32 {
    let golpe = damage(attack(level), boss_defense(level)) as f32;
    // O fio manda a health do chefe em u16.
    ((BOSS_STRIKES * golpe).round() as i32).min(u16::MAX as i32)
}

pub fn boss_defense(level: u32) -> i32 {
    (attack(level) as f32 * BOSS_DEFENSE).round() as i32
}

// ───────────────────────────── o power ─────────────────────────────

/// O power (a conta da ficha, `dungeon::poder_de_stats`) do personagem
/// esperado no nível — só as três retas, sem mana nem destreza.
pub fn power(level: u32) -> i32 {
    attack(level) * 10 + defense(level) * 8 + health(level)
}

/// A row da ladder num nível, pra ficha e pra doc: (attack, defense, health).
pub fn row(level: u32) -> (i32, i32, i32) {
    (attack(level), defense(level), health(level))
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A tabela da doc sai daqui (`cargo test -p shared ladder -- --nocapture`).
    #[test]
    fn a_escada_e_reta_e_cresce() {
        let mut antes = row(0);
        println!("| Nível | Ataque | Defesa | Vida | Mob comum (health/attack) | Poder |");
        for n in [1u32, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60] {
            let l = row(n);
            let m = mob(&Profile::novo(1.0, 1.0, 0.0), n);
            println!("| {n} | {} | {} | {} | {}/{} | {} |", l.0, l.1, l.2, m.health, m.attack, power(n));
            assert!(l.0 > antes.0 && l.1 > antes.1 && l.2 > antes.2, "nível {n} não cresceu");
            antes = l;
        }
        // Retas: a diferença entre dois níveis não depende de onde se está.
        assert_eq!(attack(40) - attack(30), attack(60) - attack(50));
        assert_eq!(defense(40) - defense(30), defense(60) - defense(50));
    }

    /// O piso sai de `FLOOR`, e não de um 10 escrito à mão: quando ele desceu
    /// pra 0,06 este teste falhava sem que nada estivesse errado.
    #[test]
    fn o_golpe_subtrai_e_tem_piso() {
        let piso_de_100 = (100.0 * FLOOR).round() as i32;
        assert_eq!(damage(100, 30), 70);
        assert_eq!(damage(100, 100), piso_de_100, "defense igual ao attack: o piso");
        assert_eq!(damage(100, 1000), piso_de_100, "defense absurda: o piso, nunca imune");
        assert_eq!(damage(3, 1000), 1, "nunca abaixo de 1");
        assert_eq!(damage(100, 0), 100);
        assert_eq!(damage(100, -5), 100, "defense negativa não amplifica");
        // O que o piso É: nunca zero, e nunca a defense inteira.
        assert!(piso_de_100 > 0 && piso_de_100 < 100);
    }

    #[test]
    fn a_reducao_de_identidade_vem_depois_e_tem_teto() {
        assert_eq!(damage_with_reduction(100, 30, 0.40), 42);
        assert_eq!(damage_with_reduction(100, 30, 0.90), 35, "teto");
        // Duas etapas, como `damage_with_reduction`: o golpe arredonda PRIMEIRO, e a
        // reduction vem por cima do inteiro. Calcular `100 x FLOOR x 0,5` de uma
        // vez erra por um quando o piso cai em meio ponto.
        let com_teto = ((damage(100, 100) as f32) * (1.0 - MAX_REDUCTION)).round().max(1.0) as i32;
        assert_eq!(damage_with_reduction(100, 100, 0.50), com_teto, "piso e teto juntos: ainda nao e' zero");
        assert!(com_teto >= 1, "piso mais teto nunca chega a zero");
        assert_eq!(damage_with_reduction(100, 30, 0.0), 70);
    }

    /// O que o dono sentiu, em número: a mesma defense toma cada vez mais
    /// conforme o mob sobe — o equipamento envelhece.
    #[test]
    fn a_mesma_defesa_envelhece() {
        let p = Profile::novo(1.0, 1.0, 0.0);
        let def_do_31 = defense(31);
        let toma = |n: u32| damage(mob(&p, n).attack, def_do_31);
        assert!(toma(31) < toma(41) && toma(41) < toma(51), "{} {} {}", toma(31), toma(41), toma(51));
        // Refinado (+48%, o teto do refino) cai no piso: no máximo ~4× menos
        // que o esperado, nunca zero.
        let refinado = (def_do_31 as f32 * 1.48) as i32;
        let m = mob(&p, 31);
        assert_eq!(damage(m.attack, refinado), (m.attack as f32 * FLOOR).round() as i32);
    }

    /// As metas dos mobs, em cima do jogador esperado.
    #[test]
    fn o_mob_comum_sai_das_metas() {
        for n in [12u32, 20, 30, 40, 50, 60] {
            let m = mob(&Profile::novo(1.0, 1.0, 0.0), n);
            // O esperado toma exatamente o líquido (depois da rampa).
            assert_eq!(damage(m.attack, defense(n)), mob_net_damage(n).round() as i32, "nível {n}");
            // E mata em STRIKES_PER_MOB golpes contra defense 1,0.
            let def_um = (attack(n) as f32 * MOB_DEFENSE).round() as i32;
            let golpes = (m.health as f32 / damage(attack(n), def_um) as f32).ceil();
            assert!((golpes - STRIKES_PER_MOB).abs() <= 1.0, "nível {n}: {golpes} golpes");
        }
        // O perfil sai da tabela como sempre esteve.
        let urso = Profile::relativo_ao_lobo(280, 18, 8);
        assert!((urso.health - 2.333).abs() < 0.01 && (urso.attack - 1.8).abs() < 0.01);
        assert!((urso.defense - 0.08).abs() < 0.001);
        let rochoso = Profile::relativo_ao_lobo(560, 30, 22);
        assert!((rochoso.defense - 0.22).abs() < 0.001);
        assert_eq!(Profile::relativo_ao_lobo(9999, 99, 999).defense, MOB_MAX_DEFENSE);
    }

    /// A personalidade escala só o líquido: o owlbear (2,8×) não fura
    /// armadura 2,8 vezes mais.
    #[test]
    fn a_especie_escala_o_liquido_e_nao_o_ataque_inteiro() {
        let n = 30;
        let lobo = mob(&Profile::relativo_ao_lobo(120, 10, 0), n);
        let owl = mob(&Profile::relativo_ao_lobo(460, 28, 4), n);
        let d = defense(n);
        let (tl, to) = (damage(lobo.attack, d), damage(owl.attack, d));
        let esperado = 1.0 + 1.8 * ATTACK_FLATTENING;
        assert!((to as f32 / tl as f32 - esperado).abs() < 0.2, "{tl} {to}");
        assert!((owl.attack as f32) < lobo.attack as f32 * 2.0, "o attack inteiro não dobra");
    }

    #[test]
    fn a_escala_dos_itens_fecha_no_conjunto_de_referencia() {
        for ilvl in [5u16, 18, 35, 60] {
            let n = ilvl as u32;
            let atk = (TEMPLATE_ATTACK + TEMPLATE_DEX * ATTACK_PER_DEX) * attack_scale(ilvl)
                + character_attack(n);
            let def = TEMPLATE_DEFENSE * defense_scale(ilvl) + character_defense(n);
            let hp = TEMPLATE_HEALTH * health_scale(ilvl) + character_health(n);
            assert!((atk - attack(n) as f32).abs() < 1.0, "ilvl {ilvl}: {atk} vs {}", attack(n));
            assert!((def - defense(n) as f32).abs() < 1.0);
            assert!((hp - health(n) as f32).abs() < 1.0);
        }
    }

    #[test]
    fn o_chefe_bate_mais_e_segura_mais_que_o_comum() {
        for n in [12u32, 30, 60] {
            let c = boss_attack(n);
            let comum = mob(&Profile::novo(1.0, 1.0, 0.0), n);
            assert!(c > comum.attack);
            assert_eq!(damage(c, defense(n)), (mob_net_damage(n) * BOSS_NET).round() as i32);
            assert!(boss_defense(n) > comum.defense);
            assert!(boss_health(n) > 20 * comum.health);
        }
        assert!(boss_health(60) < u16::MAX as i32, "cabe no u16 do fio");
        assert_eq!(boss_health(200), u16::MAX as i32, "e nunca estoura");
    }

    /// Nos primeiros níveis o mob fura menos: o recém-criado não tem armadura.
    #[test]
    fn a_rampa_do_inicio() {
        let p = Profile::novo(1.0, 1.0, 0.0);
        let sem_armadura = 0;
        let toma = |n: u32| damage(mob(&p, n).attack, sem_armadura);
        assert!(toma(1) < 10, "lobo do nível 1 contra quem só tem a arma: {}", toma(1));
        assert!(toma(3) < toma(6) && toma(6) < toma(12));
        assert_eq!(pierceable(EARLY_RAMP), defense(EARLY_RAMP) as f32);
        assert_eq!(pierceable(60), defense(60) as f32);
    }
}
