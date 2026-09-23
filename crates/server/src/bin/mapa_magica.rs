//! Mapa de altura da Ilha Magica, em PNG (na verdade PPM) — so' pra olhar.
//!
//! Existe porque a forma da ilha e' uma funcao pura, e funcao pura nao se
//! julga lendo: o dono reclamou que "as ilhas estao so' redondas bem padrao"
//! e depois que a praia era um contorno, e as duas coisas so' ficaram obvias
//! olhando a silhueta de cima. Rodar isto custa segundos e evita subir uma
//! ilha feia pro TestFlight pra descobrir la'.
//!
//! ```sh
//! cargo run --release --bin mapa_magica
//! magick /tmp/.../magica.ppm magica.png
//! ```
fn main() {
    let lado = 620usize;
    let mut px = vec![0u8; lado * lado * 3];
    for z in 0..lado {
        for x in 0..lado {
            let bx = (x as i32 - lado as i32 / 2) * 2;
            let bz = (z as i32 - lado as i32 / 2) * 2;
            let h = shared::magica::bloco_da_coluna(bx, bz);
            let i = (z * lado + x) * 3;
            let (r, g, b) = if h <= shared::magica::NIVEL_FUNDO {
                (20u8, 40, 80)
            } else if h <= shared::magica::NIVEL_CHAO {
                (222, 206, 152)
            } else {
                let t = ((h - shared::magica::NIVEL_CHAO) as f32 / 26.0).clamp(0.0, 1.0);
                ((60.0 + 110.0 * t) as u8, (140.0 + 80.0 * t) as u8, 62)
            };
            px[i] = r; px[i + 1] = g; px[i + 2] = b;
        }
    }
    let mut out = format!("P6\n{lado} {lado}\n255\n").into_bytes();
    out.extend_from_slice(&px);
    std::fs::write("/tmp/claude-1000/-home-brunji/6887f422-9520-48d5-9fe3-e311b5831797/scratchpad/magica.ppm", out).unwrap();
    println!("ok");
}
