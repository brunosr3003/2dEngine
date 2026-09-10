//! Colisao circulo-contra-grade.
//!
//! Substituiu o `rapier2d`. Um motor de fisica completo — broad phase, narrow
//! phase, solver de impulso, CCD, joints — resolve problemas que este jogo nao
//! tem: nao ha rampa, empilhamento, junta nem corpo articulado. Ha circulo
//! andando em cima de grade de tile.
//!
//! O que sobra sao duas coisas:
//!
//!   1. **Parede.** Ja resolvida por `WorldMap::move_and_slide`, que desliza o
//!      circulo pela borda do tile bloqueado.
//!   2. **Empurrao entre entidades.** Dois circulos sobrepostos se afastam,
//!      cada um na proporcao da sua MOBILIDADE. Sem restituicao e sem
//!      iteracao de solver — MMO de vista de cima nao precisa e o jogador nao
//!      percebe a diferenca.
//!
//!      A mobilidade existe por um motivo so': **mob nao empurra jogador.**
//!      Com os dois cedendo metade, uma horda de vinte lobos carrega o
//!      personagem pelo mapa e quem joga sente que perdeu o controle do
//!      boneco. Jogador entra com mobilidade zero: ele empurra e nao e'
//!      empurrado.
//!
//! O ganho nao e' so' CPU: some o estado paralelo. Com rapier a posicao
//! autoritativa vivia no rigid body, e TODO teleporte (respawn, leap, montar
//! em barco, dungeon) tinha que escrever nos dois lugares — esquecer um fazia
//! a entidade voltar sozinha pro lugar antigo no tick seguinte. Agora
//! `Position` no ECS e' a unica verdade.

use glam::Vec2;

/// Lado da celula do hash espacial, em tiles. Um pouco maior que o diametro
/// de uma entidade: assim um par sobreposto sempre cai em celulas vizinhas.
const CELULA: f32 = 1.0;

/// Afasta entidades sobrepostas, no lugar.
///
/// `corpos` e' (posicao, raio, **mobilidade**). Mobilidade 1 cede normalmente,
/// 0 nao sai do lugar; a penetracao e' repartida na proporcao entre os dois.
/// Dois corpos imoveis nao se separam — e' o unico caso em que a sobreposicao
/// fica, e e' preferivel a um deles teleportar.
///
/// O hash espacial evita o O(n²): com 1000 mobs a comparacao de todos contra
/// todos seriam 500 mil pares por tick.
pub fn separar(corpos: &mut [(Vec2, f32, f32)]) {
    use std::collections::HashMap;

    let mut grade: HashMap<(i32, i32), Vec<usize>> = HashMap::with_capacity(corpos.len());
    for (i, (p, _, _)) in corpos.iter().enumerate() {
        let c = ((p.x / CELULA).floor() as i32, (p.y / CELULA).floor() as i32);
        grade.entry(c).or_default().push(i);
    }

    // Correcoes acumuladas: aplicar durante a varredura tornaria o resultado
    // dependente da ordem de iteracao do hash.
    let mut ajuste = vec![Vec2::ZERO; corpos.len()];

    for (&(cx, cy), indices) in &grade {
        for dy in 0..=1 {
            for dx in -1..=1 {
                // Meia vizinhanca: (0,0), (1,0), (-1,1), (0,1), (1,1). A outra
                // metade e' o mesmo par visto do outro lado.
                if dy == 0 && dx < 0 {
                    continue;
                }
                let Some(vizinhos) = grade.get(&(cx + dx, cy + dy)) else { continue };
                for &i in indices {
                    for &j in vizinhos {
                        if j <= i && dx == 0 && dy == 0 {
                            continue;
                        }
                        if i == j {
                            continue;
                        }
                        let (pi, ri, mi) = corpos[i];
                        let (pj, rj, mj) = corpos[j];
                        let mob = mi + mj;
                        if mob <= 0.0 {
                            continue;
                        }
                        let d = pj - pi;
                        let soma = ri + rj;
                        let d2 = d.length_squared();
                        if d2 >= soma * soma || d2 <= f32::EPSILON {
                            continue;
                        }
                        let dist = d2.sqrt();
                        let penetracao = soma - dist;
                        let n = d / dist;
                        ajuste[i] -= n * penetracao * (mi / mob);
                        ajuste[j] += n * penetracao * (mj / mob);
                    }
                }
            }
        }
    }

    for (i, (p, _, _)) in corpos.iter_mut().enumerate() {
        *p += ajuste[i];
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Mob nao empurra jogador. Este e' o teste que existe pra o
    /// comportamento nao voltar sozinho numa refatoracao: com os dois cedendo
    /// metade, uma horda carrega o personagem pelo mapa e quem joga sente que
    /// perdeu o controle do boneco.
    #[test]
    fn horda_nao_carrega_o_jogador() {
        // jogador no centro (mobilidade 0) cercado de doze mobs sobrepostos
        let mut corpos = vec![(Vec2::ZERO, 0.35, 0.0)];
        for i in 0..12 {
            let a = i as f32 / 12.0 * std::f32::consts::TAU;
            corpos.push((Vec2::new(a.cos(), a.sin()) * 0.2, 0.35, 1.0));
        }
        separar(&mut corpos);
        assert_eq!(corpos[0].0, Vec2::ZERO, "o jogador saiu do lugar");
        assert!(
            corpos[1..].iter().all(|(p, _, _)| p.length() > 0.2),
            "os mobs deviam ter sido afastados"
        );
    }

    /// Dois moveis continuam dividindo a penetracao meio a meio — a
    /// mobilidade generaliza o comportamento antigo, nao o troca.
    #[test]
    fn dois_moveis_dividem_igual() {
        let mut corpos = vec![
            (Vec2::new(-0.2, 0.0), 0.35, 1.0),
            (Vec2::new(0.2, 0.0), 0.35, 1.0),
        ];
        separar(&mut corpos);
        assert!((corpos[0].0.x + corpos[1].0.x).abs() < 1e-5, "assimetrico");
        assert!(corpos[0].0.x < -0.2 && corpos[1].0.x > 0.2);
    }

    /// Dois imoveis nao se separam. Sobreposicao e' feia, mas melhor que um
    /// deles teleportar.
    /// Dois corpos IMOVEIS ficam sobrepostos, e isso e' de proposito: nao ha'
    /// pra onde empurrar quando ninguem cede.
    ///
    /// A consequencia e' que dois JOGADORES atravessam um ao outro, porque
    /// jogador entra com mobilidade zero contra a horda de mob. Quem resolve
    /// e' o servidor, numa segunda passada so' entre jogadores onde todos
    /// cedem igual — ver `world.rs`, "SEGUNDA PASSADA".
    ///
    /// O teste esta' aqui pra ninguem "consertar" isto na `separar` e trazer
    /// de volta a horda carregando o personagem pelo mapa.
    #[test]
    fn dois_imoveis_ficam_onde_estao() {
        let mut corpos = vec![
            (Vec2::new(-0.1, 0.0), 0.35, 0.0),
            (Vec2::new(0.1, 0.0), 0.35, 0.0),
        ];
        separar(&mut corpos);
        assert_eq!(corpos[0].0.x, -0.1);
        assert_eq!(corpos[1].0.x, 0.1);
    }
}

#[cfg(test)]
mod testes_entre_iguais {
    use super::*;

    /// Corpos que cedem IGUAL se separam ate' encostar, e nao mais.
    ///
    /// E' a passada que o servidor roda so' entre jogadores. Medido com 12
    /// bots no desembarque antes dela existir: dois pares a 0,368 de
    /// distancia, com os corpos ocupando 0,70.
    #[test]
    fn iguais_se_separam_ate_encostar() {
        let r = 0.35f32;
        let mut corpos = vec![
            (Vec2::new(0.0, 0.0), r, 1.0),
            (Vec2::new(0.368, 0.0), r, 1.0),
            (Vec2::new(9.0, 9.0), r, 1.0), // longe, nao pode se mexer
        ];
        let longe_antes = corpos[2].0;
        for _ in 0..40 {
            separar(&mut corpos);
        }
        let d = corpos[0].0.distance(corpos[1].0);
        assert!(
            d >= 2.0 * r - 1e-3,
            "sobraram {d:.3} entre corpos que ocupam {:.2}", 2.0 * r
        );
        assert!(d < 2.0 * r + 0.05, "afastaram demais: {d:.3}");
        assert_eq!(corpos[2].0, longe_antes, "corpo distante se mexeu");
        // O par se abre pros DOIS lados: ninguem e' privilegiado.
        assert!(corpos[0].0.x < 0.0 && corpos[1].0.x > 0.368);
    }
}
