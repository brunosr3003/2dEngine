//! O que o golpe deixa na tela: o numero, a faisca e a dor.
//!
//! Tudo aqui sai de `World::acertos`, que vem do servidor com o dano REAL —
//! e nao da queda de vida, que no modo imortal nao existe e num golpe
//! absorvido tambem nao. Desenhado em 2D por cima do mundo, na posicao
//! projetada de quem apanhou: numero de dano e' leitura, e leitura nao pode
//! sumir atras de uma arvore.

use macroquad::prelude::*;

use crate::render3d::{world_to_screen, Vista};
use crate::ui;
use crate::world::World;

/// Quanto tempo o numero fica na tela.
const VIDA_DO_NUMERO: f32 = 0.95;
/// Quanto a faisca dura.
const VIDA_DA_FAISCA: f32 = 0.2;

fn altura_de(e: &crate::world::Ent) -> f32 {
    let boss = e.state.flags & shared::ent_flags::BOSS != 0;
    if boss && e.meta.tag == shared::EntityTag::Enemy {
        // O chefe e' desenhado em escala: a placa e o numero vao pra cabeca dele.
        return crate::render3d::altura_de_chefe(e);
    }
    crate::bicho::do_mob(e.meta.tag, e.meta.kind, boss).map_or(1.95, |(_, a)| a)
}

/// Texto com contorno: por cima de grama, areia e ceu, so' contorno garante
/// leitura.
fn texto_contornado(s: &str, x: f32, y: f32, tam: f32, cor: Color) {
    let sombra = Color::new(0.0, 0.0, 0.0, cor.a * 0.85);
    let t = tam.round().clamp(8.0, 64.0) as u16;
    for (dx, dy) in [(-1.5, 0.0), (1.5, 0.0), (0.0, -1.5), (0.0, 1.5), (1.5, 2.0)] {
        crate::hud_estilo::texto_forte(x + dx, y + dy, s, t, sombra);
    }
    crate::hud_estilo::texto_forte(x, y, s, t, cor);
}

/// A placa do inimigo, em cima da cabeca: nivel e nome, e a vida embaixo.
/// So' pra quem esta' perto — a tela nao pode virar um mural —, e o alvo
/// ganha a borda dourada e cresce um pouco.
fn placas(world: &World, vista: &Vista) {
    let eu = world
        .self_id
        .and_then(|i| world.ents.get(&i))
        .map(|e| e.render_pos);
    for (id, e) in &world.ents {
        if e.meta.tag != shared::EntityTag::Enemy || e.state.hp == 0 {
            continue;
        }
        let boss = e.state.flags & shared::ent_flags::BOSS != 0;
        if eu.map_or(false, |p| {
            p.distance(e.render_pos) > if boss { 40.0 } else { 26.0 }
        }) {
            continue;
        }
        let alvo = world.alvo == Some(*id);
        let topo = vista.pos_de(e) + vec3(0.0, altura_de(e) + 0.35, 0.0);
        let Some(c) = world_to_screen(&vista.cam, topo) else {
            continue;
        };
        // Chefe: placa maior, moldura dourada e coroa ao lado do nome.
        let (w, h) = if boss {
            (128.0, 9.0)
        } else if alvo {
            (86.0, 7.0)
        } else {
            (64.0, 5.0)
        };
        let f = (e.state.hp as f32 / e.meta.hp_max.max(1) as f32).clamp(0.0, 1.0);
        let (x, y) = (c.x - w * 0.5, c.y);
        draw_rectangle(
            x - 1.0,
            y - 1.0,
            w + 2.0,
            h + 2.0,
            Color::new(0.0, 0.0, 0.0, 0.75),
        );
        draw_rectangle(x, y, w, h, Color::new(0.25, 0.06, 0.05, 0.9));
        draw_rectangle(x, y, w * f, h, Color::new(0.86, 0.22, 0.18, 1.0));
        draw_rectangle(x, y, w * f, h * 0.35, Color::new(1.0, 0.45, 0.38, 0.8));
        if alvo || boss {
            draw_rectangle_lines(x - 2.5, y - 2.5, w + 5.0, h + 5.0, 1.5, ui::OURO);
        }
        let nome = e.meta.name.as_deref().unwrap_or("?");
        let txt = if e.meta.nivel > 0 {
            format!("Lv {} {nome}", e.meta.nivel)
        } else {
            nome.to_string()
        };
        let tam = if boss {
            18.0
        } else if alvo {
            17.0
        } else {
            14.0
        };
        let d = TextDimensions {
            width: crate::hud_estilo::medir_forte(&txt, tam as u16),
            height: tam,
            offset_y: tam,
        };
        if boss {
            crate::telegrafico::desenha_coroa(vec2(c.x - d.width * 0.5 - 14.0, y - 10.0), 6.0);
        }
        let cor = if boss {
            Color::new(1.0, 0.55, 0.25, 1.0)
        } else if alvo {
            Color::new(1.0, 0.93, 0.7, 1.0)
        } else {
            Color::new(0.92, 0.92, 0.92, 0.95)
        };
        texto_contornado(&txt, c.x - d.width * 0.5, y - 5.0, tam, cor);
    }
}

/// Onde a placa de um jogador fica e que tamanho tem.
///
/// Fora do desenho pra poder ser medida: a barra e o nome moram um em cima do
/// outro, e medida escrita à mão já pôs texto por cima de barra nesta base.
pub(crate) struct PecasDaPlaca {
    pub barra: Rect,
    pub tam: f32,
    /// A LINHA DE BASE do nome. Sai daqui, e não do desenho: com a conta
    /// repetida nos dois lugares, um teste que a refizesse mediria a si mesmo
    /// — foi o que aconteceu na primeira versão deste teste, e ela passou com
    /// a barra subida em cima do texto.
    pub base_do_nome: f32,
}

pub(crate) fn placa_de_jogador(c: Vec2, eu_mesmo: bool) -> PecasDaPlaca {
    let (w, h) = if eu_mesmo { (76.0, 5.0) } else { (70.0, 5.0) };
    let tam = if eu_mesmo { 15.0 } else { 14.0 };
    let barra = Rect::new(c.x - w * 0.5, c.y, w, h);
    PecasDaPlaca {
        barra,
        tam,
        // O nome fica ACIMA da barra, com folga: sem ela as duas coisas
        // disputam a mesma linha, e o nome some justamente quando o jogador
        // está apanhando.
        base_do_nome: barra.y - 5.0,
    }
}

/// A PLACA DO JOGADOR: nome em cima da cabeça e a vida embaixo.
///
/// O dono: "falta o nome e vida em cima do player, padrão de MMO". Os
/// inimigos já tinham (`placas`); jogador não tinha nada, então numa praça com
/// doze bots não dava pra saber quem é quem, nem quem está apanhando.
///
/// Inclui o PRÓPRIO jogador, como em MIR4: é por ela que se vê o próprio nome
/// e a própria vida sem tirar o olho do boneco.
///
/// A cor separa aliado de quem pode bater: verde para a mesma facção, vermelho
/// para outra. Numa ilha de PvP aberto essa é a informação que decide se você
/// corre ou não — e ela precisa ser lida de relance, não conferida.
fn placas_de_jogador(world: &World, vista: &Vista) {
    let eu = world.self_id.and_then(|i| world.ents.get(&i));
    let minha_pos = eu.map(|e| e.render_pos);
    let minha_faccao = eu.and_then(|e| e.meta.faction);
    for (id, e) in &world.ents {
        if e.meta.tag != shared::EntityTag::Player {
            continue;
        }
        let eu_mesmo = world.self_id == Some(*id);
        // Longe demais não recebe placa: a tela não pode virar um mural. O
        // próprio jogador é sempre visível (distância zero).
        if !eu_mesmo && minha_pos.is_some_and(|p| p.distance(e.render_pos) > 34.0) {
            continue;
        }
        // Morto não tem placa: o corpo caído já diz o que precisa.
        if e.state.hp == 0 {
            continue;
        }
        let topo = vista.pos_de(e) + vec3(0.0, altura_de(e) + 0.35, 0.0);
        let Some(c) = world_to_screen(&vista.cam, topo) else {
            continue;
        };
        let pl = placa_de_jogador(c, eu_mesmo);
        let (r, tam) = (pl.barra, pl.tam);
        let f = (e.state.hp as f32 / e.meta.hp_max.max(1) as f32).clamp(0.0, 1.0);
        let inimigo = !eu_mesmo
            && minha_faccao.is_some()
            && e.meta.faction.is_some()
            && e.meta.faction != minha_faccao;
        let (cheia, brilho) = if inimigo {
            (
                Color::new(0.86, 0.22, 0.18, 1.0),
                Color::new(1.0, 0.45, 0.38, 0.8),
            )
        } else {
            (
                Color::new(0.30, 0.78, 0.35, 1.0),
                Color::new(0.55, 0.95, 0.60, 0.8),
            )
        };
        draw_rectangle(
            r.x - 1.0,
            r.y - 1.0,
            r.w + 2.0,
            r.h + 2.0,
            Color::new(0.0, 0.0, 0.0, 0.75),
        );
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.10, 0.12, 0.10, 0.9));
        draw_rectangle(r.x, r.y, r.w * f, r.h, cheia);
        draw_rectangle(r.x, r.y, r.w * f, r.h * 0.35, brilho);
        if eu_mesmo {
            draw_rectangle_lines(r.x - 2.5, r.y - 2.5, r.w + 5.0, r.h + 5.0, 1.5, ui::OURO);
        }
        let nome = e.meta.name.as_deref().unwrap_or("?");
        let txt = if e.meta.nivel > 0 {
            format!("Lv {} {nome}", e.meta.nivel)
        } else {
            nome.to_string()
        };
        let largura = crate::hud_estilo::medir_forte(&txt, tam as u16);
        let cor = if eu_mesmo {
            ui::OURO
        } else if inimigo {
            Color::new(1.0, 0.72, 0.68, 0.98)
        } else {
            Color::new(0.86, 0.94, 1.0, 0.95)
        };
        texto_contornado(&txt, c.x - largura * 0.5, pl.base_do_nome, tam, cor);
    }
}

pub fn desenha(world: &World, vista: &Vista) {
    placas(world, vista);
    // Depois das dos inimigos: numa mistura, o nome do jogador é o que se
    // procura primeiro, então ele fica por cima.
    placas_de_jogador(world, vista);
    for ef in &world.efeitos {
        let Some(e) = world.ents.get(&ef.alvo) else {
            continue;
        };
        let pe = vista.pos_de(e);
        let alt = altura_de(e);

        // ── a faisca: raios saindo do peito de quem apanhou ──
        if ef.t < VIDA_DA_FAISCA {
            if let Some(c) = world_to_screen(&vista.cam, pe + vec3(0.0, alt * 0.55, 0.0)) {
                let u = ef.t / VIDA_DA_FAISCA;
                let cor = if ef.eu {
                    Color::new(1.0, 0.35, 0.28, 1.0 - u)
                } else {
                    Color::new(1.0, 0.95, 0.72, 1.0 - u)
                };
                let escala = if ef.critico { 1.5 } else { 1.0 };
                draw_circle(
                    c.x,
                    c.y,
                    (18.0 * (1.0 - u) + 4.0) * escala,
                    Color::new(1.0, 1.0, 1.0, 0.55 * (1.0 - u)),
                );
                for k in 0..8 {
                    let ang = ef.semente * 6.283 + k as f32 * 0.785;
                    let (s, co) = ang.sin_cos();
                    let r0 = 8.0 + 26.0 * u * escala;
                    let r1 = r0 + (16.0 - 10.0 * u) * escala;
                    draw_line(
                        c.x + co * r0,
                        c.y + s * r0,
                        c.x + co * r1,
                        c.y + s * r1,
                        3.0 * (1.0 - u) + 1.0,
                        cor,
                    );
                }
            }
        }

        // ── o numero: pula grande, sobe e some ──
        if ef.t < VIDA_DO_NUMERO {
            let Some(c) = world_to_screen(&vista.cam, pe + vec3(0.0, alt + 0.25, 0.0)) else {
                continue;
            };
            let u = ef.t / VIDA_DO_NUMERO;
            let sobe = 46.0 * (1.0 - (1.0 - u).powi(3));
            let pula = 1.0 + 0.7 * (-ef.t * 14.0).exp();
            let base = if ef.critico { 30.0 } else { 23.0 };
            let tam = base * pula;
            let alfa = if u < 0.6 { 1.0 } else { 1.0 - (u - 0.6) / 0.4 };
            let cor = if ef.eu {
                Color::new(1.0, 0.32, 0.28, alfa)
            } else if ef.critico {
                Color::new(1.0, 0.62, 0.18, alfa)
            } else {
                Color::new(1.0, 0.95, 0.78, alfa)
            };
            let txt = if ef.critico {
                format!("{}!", ef.dano)
            } else {
                ef.dano.to_string()
            };
            let d = TextDimensions {
                width: crate::hud_estilo::medir_forte(&txt, tam as u16),
                height: tam,
                offset_y: tam,
            };
            let x = c.x - d.width * 0.5 + (ef.semente - 0.5) * 36.0;
            texto_contornado(&txt, x, c.y - sobe, tam, cor);
        }
    }

    // ── a dor: a borda da tela fica vermelha quando e' VOCE que apanha ──
    if world.dor > 0.01 {
        let (w, h) = (screen_width(), screen_height());
        for i in 0..14 {
            let f = i as f32 / 14.0;
            let a = world.dor * 0.30 * (1.0 - f).powi(2);
            let m = i as f32 * 7.0;
            draw_rectangle_lines(
                m,
                m,
                w - 2.0 * m,
                h - 2.0 * m,
                7.0,
                Color::new(0.85, 0.08, 0.06, a),
            );
        }
    }
}

#[cfg(test)]
mod testes_da_placa {
    use super::*;

    /// A BARRA NÃO ENCOSTA NO NOME.
    ///
    /// Os dois moram um em cima do outro e são desenhados por medidas
    /// escritas à mão. Se a barra subir o bastante, o nome fica ilegível
    /// exatamente quando mais importa — com o jogador apanhando.
    ///
    /// O teste mede a folga em todas as escalas de fonte que a placa usa.
    #[test]
    fn o_nome_nao_cai_em_cima_da_barra_de_vida() {
        for eu_mesmo in [false, true] {
            let c = Vec2::new(400.0, 300.0);
            let pl = placa_de_jogador(c, eu_mesmo);
            let (r, tam) = (pl.barra, pl.tam);
            // O nome é desenhado com a base em `r.y - 5.0`, então ele ocupa
            // de `r.y - 5 - tam` até `r.y - 5`.
            // A BASE VEM DA FUNÇÃO. Refazer a conta aqui faria o teste medir
            // a si mesmo: mover a barra moveria o nome junto e nada
            // reprovaria — foi assim que a primeira versão passou com a barra
            // por cima do texto.
            let base_do_nome = pl.base_do_nome;
            assert!(
                base_do_nome <= r.y,
                "eu_mesmo={eu_mesmo}: o nome desce dentro da barra"
            );
            assert!(
                r.y - base_do_nome >= 4.0,
                "eu_mesmo={eu_mesmo}: folga de {:.1}px entre nome e barra é pouca",
                r.y - base_do_nome
            );
            assert!(tam >= 13.0, "fonte {tam} pequena demais pra ler de longe");
            assert!(r.w >= 60.0 && r.h >= 4.0, "barra {r:?} pequena demais");
            // A barra tem que ficar CENTRADA no ponto da cabeça: fora do
            // centro, ela aponta pro boneco errado numa aglomeração.
            assert!(
                ((r.x + r.w * 0.5) - c.x).abs() < 0.01,
                "a barra não está centrada na cabeça"
            );
        }
    }

    /// A PLACA DO PRÓPRIO JOGADOR É MAIOR, e isso é de propósito: numa praça
    /// com doze bots, achar a si mesmo é a primeira coisa que se faz.
    #[test]
    fn a_propria_placa_se_destaca() {
        let c = Vec2::new(400.0, 300.0);
        let m = placa_de_jogador(c, true);
        let o = placa_de_jogador(c, false);
        let (meu, meu_tam) = (m.barra, m.tam);
        let (outro, outro_tam) = (o.barra, o.tam);
        assert!(meu.w > outro.w, "a própria placa não é mais larga");
        assert!(meu_tam >= outro_tam, "o próprio nome não é maior");
    }
}
