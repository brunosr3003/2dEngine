//! Elementos compartilhados da interface de combate, desenhados em 2D.
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};

// A fonte vive pelo processo: nao deve destruir texturas num destructor
// thread-local depois que a janela/contexto OpenGL ja foi encerrado.
static FONTE: std::sync::OnceLock<Font> = std::sync::OnceLock::new();
fn fonte<T>(f: impl FnOnce(&Font) -> T) -> T {
    f(FONTE.get_or_init(|| {
        let mut fonte=load_ttf_font_from_bytes(include_bytes!("../../../assets/fonts/LiberationSans-Regular.ttf")).expect("fonte da HUD");
        fonte.set_filter(FilterMode::Linear);
        fonte
    }))
}
pub fn medir(s: &str, tamanho: u16) -> f32 { fonte(|f|measure_text(s,Some(f),tamanho,1.0).width) }
pub fn texto(x: f32, y: f32, s: &str, tamanho: u16, cor: Color) {
    fonte(|f| { draw_text_ex(s,x,y,TextParams{font:Some(f),font_size:tamanho,color:cor,..Default::default()}); });
}
pub fn texto_centro(x: f32, y: f32, s: &str, tamanho: u16, cor: Color) { texto(x-medir(s,tamanho)*0.5,y,s,tamanho,cor); }

pub const FUNDO: Color = Color::new(0.025, 0.038, 0.058, 0.90);
pub const BORDA: Color = Color::new(0.43, 0.39, 0.30, 0.75);
pub const OURO: Color = Color::new(0.84, 0.74, 0.51, 1.0);
pub const TEXTO: Color = Color::new(0.93, 0.94, 0.94, 1.0);
pub const SUAVE: Color = Color::new(0.57, 0.64, 0.70, 1.0);
pub const AUTO: Color = Color::new(0.37, 0.94, 0.76, 1.0);

pub fn painel(r: Rect) {
    draw_rectangle(r.x + 2.0, r.y + 4.0, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.22));
    draw_rectangle(r.x, r.y, r.w, r.h, FUNDO);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.26, 0.31, 0.36, 0.7));
    draw_line(r.x + 8.0, r.y, r.x + r.w - 8.0, r.y, 1.0, BORDA);
    for (x, d) in [(r.x, 1.0), (r.x + r.w, -1.0)] {
        draw_line(x, r.y, x + 8.0 * d, r.y, 1.5, OURO);
        draw_line(x, r.y, x, r.y + 8.0, 1.5, OURO);
    }
}

pub fn arco(c: Vec2, r: f32, inicio: f32, fracao: f32, esp: f32, cor: Color) {
    let n = (fracao.abs() * 64.0).ceil().max(1.0) as usize;
    for i in 0..n {
        let a = inicio + TAU * fracao * i as f32 / n as f32;
        let b = inicio + TAU * fracao * (i + 1) as f32 / n as f32;
        let p = c + vec2(a.cos(), a.sin()) * r;
        let q = c + vec2(b.cos(), b.sin()) * r;
        draw_line(p.x, p.y, q.x, q.y, esp, cor);
    }
}

pub fn setor(c: Vec2, r: f32, fracao: f32, cor: Color) {
    let n = (fracao.clamp(0.0, 1.0) * 64.0).ceil() as usize;
    for i in 0..n {
        let a = -PI * 0.5 + TAU * fracao * i as f32 / n as f32;
        let b = -PI * 0.5 + TAU * fracao * (i + 1) as f32 / n as f32;
        draw_triangle(c, c + vec2(a.cos(), a.sin()) * r, c + vec2(b.cos(), b.sin()) * r, cor);
    }
}

pub fn cor_skill(id: u32) -> Color {
    match id { 1..=3 => Color::new(1.0, 0.74, 0.33, 1.0), 4..=6 => Color::new(0.38, 0.86, 1.0, 1.0),
        7..=9 => Color::new(1.0, 0.52, 0.27, 1.0), 10 | 11 => AUTO, _ => Color::new(0.79, 0.58, 1.0, 1.0) }
}

/// Pictogramas vetoriais próprios: nenhuma textura ou arte de outro jogo.
pub fn icone(id: u32, c: Vec2, r: f32, cor: Color) {
    let p = |x, y| c + vec2(x, y) * r;
    let linha = |a: Vec2, b: Vec2, w: f32| draw_line(a.x, a.y, b.x, b.y, w * r, cor);
    let lamina = |x: f32, y: f32| {
        draw_triangle(p(x-0.55,y+0.5),p(x+0.55,y-0.65),p(x+0.12,y+0.1),cor);
        linha(p(x-0.48,y+0.26),p(x-0.15,y+0.6),0.10);
        linha(p(x-0.4,y+0.48),p(x-0.6,y+0.7),0.12);
    };
    match id {
        1 => { lamina(0.05,-0.02); for y in [-0.3,0.0,0.3] { linha(p(-0.82,y),p(-0.4,y-0.13),0.065); } }
        2 | 4 => {
            arco(c,r*0.85,-PI*0.85,0.48,0.10*r,cor); lamina(0.0,0.0);
            if id == 2 { arco(c,r*0.65,-PI*0.85,0.40,0.04*r,cor); }
        }
        3 => {
            let ps = [p(-0.62,-0.5),p(0.0,-0.78),p(0.62,-0.5),p(0.48,0.3),p(0.0,0.78),p(-0.48,0.3)];
            for i in 0..6 { linha(ps[i],ps[(i+1)%6],0.10); }
            linha(p(0.0,-0.4),p(0.0,0.38),0.08); linha(p(-0.28,-0.12),p(0.28,-0.12),0.08);
        }
        5 => { for i in 0..3 { arco(c,r*(0.48+i as f32*0.17),i as f32*2.1,0.57,0.1*r,cor); } }
        6 => { for x in [-0.28,0.10,0.46] { linha(p(x-0.4,-0.7),p(x+0.12,0.0),0.12); linha(p(x+0.12,0.0),p(x-0.4,0.7),0.09); } }
        7 => {
            arco(c,r*0.50,0.0,1.0,0.065*r,cor);
            for i in 0..4 { let d=vec2((i as f32*PI/2.0).cos(),(i as f32*PI/2.0).sin()); linha(c+d*r*0.35,c+d*r*0.85,0.09); }
            draw_circle(c.x,c.y,r*0.11,cor);
        }
        8 => { for y in [-0.4,0.0,0.4] { linha(p(-0.75,y),p(0.35,y),0.12); draw_triangle(p(0.32,y-0.14),p(0.65,y),p(0.32,y+0.14),cor); } }
        9 => {
            draw_circle_lines(c.x,c.y+r*0.12,r*0.52,r*0.10,cor);
            linha(p(0.0,-0.4),p(0.18,-0.7),0.10);
            for i in 0..5 { let a=i as f32*TAU/5.0; let q=p(0.25,-0.73); linha(q+vec2(a.cos(),a.sin())*r*0.1,q+vec2(a.cos(),a.sin())*r*0.25,0.05); }
        }
        10 | 11 => {
            linha(p(-0.4,0.0),p(0.4,0.0),0.23); linha(p(0.0,-0.4),p(0.0,0.4),0.23);
            arco(c,r*0.72,0.0,1.0,0.065*r,cor);
            if id == 11 { arco(c,r*0.93,0.2,0.38,0.06*r,cor); arco(c,r*0.93,PI+0.2,0.38,0.06*r,cor); }
        }
        _ => {
            draw_triangle(p(0.25,-0.85),p(-0.48,0.12),p(0.14,0.12),cor);
            draw_triangle(p(-0.14,-0.12),p(0.48,-0.12),p(-0.25,0.85),cor);
            arco(c,r*0.83,0.0,0.33,0.045*r,cor); arco(c,r*0.83,PI,0.33,0.045*r,cor);
        }
    }
}

pub fn texto_ajustado(s: &str, x: f32, y: f32, largura: f32, tamanho: u16, cor: Color) {
    let mut t = s.to_string();
    if medir(&t,tamanho) > largura {
        while !t.is_empty() && medir(&format!("{t}…"),tamanho) > largura { t.pop(); }
        t.push('…');
    }
    texto(x,y,&t,tamanho,cor);
}
