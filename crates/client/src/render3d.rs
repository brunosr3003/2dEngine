//! Desenho 3D com camera de cima. Nenhuma decisao de jogo mora aqui.
//!
//! A vista e' a de MMO top-down (MIR4): camera alta, inclinada, seguindo o
//! player. Como o mundo nao precisa ser complexo, o chao e' reconstruido por
//! quadro so' com os tiles visiveis — algumas centenas de quads, mais barato
//! que manter chunk em cache e invalidar.

use macroquad::material::{load_material, Material, MaterialParams};
use macroquad::miniquad::graphics::{PipelineParams, UniformDesc, UniformType};
use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use shared::constants::tile_id;

use crate::map::Map;
use crate::vox::VoxCache;
use crate::world::World;

/// Um tile do servidor = uma unidade de mundo 3D.
pub const TILE: f32 = 1.0;
/// Altura da camera sobre o alvo, em tiles, no zoom padrao.
const CAM_HEIGHT: f32 = 14.0;
/// Recuo da camera atras do alvo. Junto com a altura da a inclinacao.
const CAM_BACK: f32 = 10.0;
/// Extremos da banda de inclinacao, em radianos acima do horizonte.
///
/// A banda nao e' gosto, e' o alcance do mundo carregado. Medido: o topo da
/// tela encosta no chao a 22 unidades a 54°, a 45 a 35° — e a 167 a 25°. O
/// mundo so' existe ate' 88 (raio de 5 pedacos), entao abaixo de ~30° o
/// jogador ve' a BORDA: dali pra baixo o preco deixa de ser desenho e vira
/// memoria, porque cobrir 167 unidades pede raio 11 — de 131 MB de malha pra
/// 573 MB.
///
/// Em cima o limite e' outro: passando de 70° a vista fica cenital, o relevo
/// perde relevo e o mundo achata.
///
/// E o piso NAO e' fixo: ele sobe conforme a camera se afasta. Ver
/// `pitch_min_para`.
pub const PITCH_MIN: f32 = 0.471_239; // 27°
pub const PITCH_MAX: f32 = 1.308_997; // 75°

/// Piso da inclinacao no zoom mais AFASTADO.
///
/// Afastado tambem da' pra deitar, so' que menos: os dois pisos foram
/// escolhidos medindo, e o que os limita e' o mesmo em ambos — o alcance do
/// topo da tela contra as 88 unidades de mundo carregado.
///
/// ```text
///  zoom  pitch   altura do olho   topo da tela alcanca
///  0,55    26°            4,1 u                  68 u
///  0,55    27°            4,3 u                  55 u   <- piso de perto
///  1,50    32°           13,7 u                  82 u   (no fio)
///  1,50    34°           14,4 u                  71 u   <- piso de longe
///  1,50    42°           17,3 u                  49 u   (sobrava banda)
/// ```
pub const PITCH_MIN_LONGE: f32 = 0.593_412; // 34°

/// A inclinacao mais baixa permitida NESTE zoom.
///
/// Colada, a camera deita ate' 27°; afastada, ate' 42°. Nao e' gosto — e' a
/// mesma conta da banda com o zoom dentro dela: o alcance do topo da tela
/// cresce com a ALTURA do olho, e a altura e' `cam_dist() * zoom *
/// sin(pitch)`. Deitar e afastar sao a mesma vontade de ver mais longe, e as
/// duas no maximo mostram a borda do mundo carregado.
pub fn pitch_min_para(zoom: f32) -> f32 {
    pitch_min_com_raio(zoom, crate::config_graficos::raio_terreno())
}

/// The floor as measured, for the default 5-chunk world.
fn piso_medido(zoom: f32) -> f32 {
    let z = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let t = (z - ZOOM_MIN) / (ZOOM_MAX - ZOOM_MIN);
    PITCH_MIN + (PITCH_MIN_LONGE - PITCH_MIN) * t
}

/// Vertical field of view of the game camera (`Camera3D::default`).
const FOVY_PADRAO: f32 = 0.785_398; // 45°

/// How far from the eye the TOP edge of the screen meets flat ground — the
/// "topo da tela alcanca" column of the table above, as a formula.
fn alcance_do_topo(zoom: f32, pitch: f32) -> f32 {
    let altura = cam_dist() * zoom.clamp(ZOOM_MIN, ZOOM_MAX) * pitch.sin();
    let abaixo = pitch - FOVY_PADRAO * 0.5;
    if abaixo <= 1e-3 {
        f32::INFINITY
    } else {
        altura / abaixo.tan()
    }
}

/// The floor for a world loaded `raio` chunks out (Graphics → View distance).
///
/// At the default radius it is the measured floor, untouched. Elsewhere the
/// top of the screen may reach as far, in proportion, as it does at the
/// default: a bigger world lets the camera lie lower toward the horizon, a
/// smaller one stands it up so the edge never shows.
pub fn pitch_min_com_raio(zoom: f32, raio: i32) -> f32 {
    let base = piso_medido(zoom);
    let padrao = crate::config_graficos::RAIO_PADRAO;
    if raio == padrao {
        return base;
    }
    let alvo = alcance_do_topo(zoom, base) * (raio as f32 + 0.5) / (padrao as f32 + 0.5);
    // The reach falls as the camera stands up: bisect for where it meets the
    // target. The bottom stays above half the field of view, where the top
    // of the screen would be looking at the horizon itself.
    let (mut baixo, mut alto) = (FOVY_PADRAO * 0.5 + 0.03, PITCH_MAX - 0.3);
    for _ in 0..40 {
        let meio = (baixo + alto) * 0.5;
        if alcance_do_topo(zoom, meio) > alvo {
            baixo = meio;
        } else {
            alto = meio;
        }
    }
    alto
}

/// A inclinacao que o zoom PEDE, antes do ajuste manual.
///
/// Aproximar deita a camera, afastar levanta — sozinho, sem o jogador pedir.
/// A vista de perto quer horizonte (o boneco, a arvore ao lado, o barranco a'
/// frente); a de longe quer planta baixa (onde estao os mobs, por onde da'
/// pra passar). Fazer as duas coisas com um gesto so' e' o que "semi
/// automatico" quer dizer: a roda escolhe o enquadramento inteiro, e a mao
/// so' corrige se discordar.
pub fn pitch_do_zoom(zoom: f32) -> f32 {
    let z = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let t = (z - ZOOM_MIN) / (ZOOM_MAX - ZOOM_MIN);
    // Um pouco acima do piso: o automatico nao entrega a camera no limite,
    // senao nao sobra pra onde deitar na mao.
    let piso = pitch_min_para(z);
    piso + (PITCH_MAX - piso) * (0.18 + 0.34 * t)
}

/// Distancia da camera ao alvo no zoom 1. E' ela que fica FIXA quando a
/// inclinacao muda — girar a camera pra baixo nao pode aproximar o boneco.
fn cam_dist() -> f32 {
    (CAM_HEIGHT * CAM_HEIGHT + CAM_BACK * CAM_BACK).sqrt()
}

/// A inclinacao com que o jogo comeca.
///
/// Sai da altura e do recuo originais em vez de ser um numero solto: assim a
/// vista inicial continua sendo exatamente a de antes de a banda existir.
///
/// E' so' o ponto de partida: depois disso a camera fica onde o jogador
/// deixou. Ja' houve uma mola puxando de volta pra ca' e ela foi tirada —
/// corrigir sozinha a vista que o jogador acabou de escolher e' a camera
/// discordando dele.
pub fn pitch_padrao() -> f32 {
    CAM_HEIGHT.atan2(CAM_BACK)
}

/// Limites do zoom, como fator sobre a distancia ate' o alvo (`cam_dist()`).
///
/// O maximo baixou de 2,2 pra 1,5: la' em cima a camera ficava a 38 unidades
/// do boneco e o mundo lia como maquete — bonito de printar, ruim de jogar.
pub const ZOOM_MIN: f32 = 0.55;
pub const ZOOM_MAX: f32 = 1.5;
/// Metade da largura da area de chao desenhada, em tiles.
const GROUND_RADIUS: i32 = 26;
/// Tamanho de um voxel em unidades de mundo. Um bicho de ~40 voxels de altura
/// fica com ~1,6 tile — a escala que a vista de cima pede.
pub const VOXEL: f32 = 0.04;

/// A camera do jogo: gira em torno do alvo (yaw), aproxima e afasta (zoom) e
/// inclina dentro de uma BANDA.
///
/// Inclinacao livre de verdade nao da': apontada pro horizonte ela mostra a
/// borda do mundo carregado e o orcamento de pedaco estoura. Travada, o
/// mundo achata. A banda resolve os dois, e dentro dela a camera fica onde o
/// jogador deixou.
///
/// `chao` sobe a camera junto com o relevo; sem isso o jogador some dentro do
/// morro assim que o terreno passou a ter 34 unidades.
pub fn camera(target: Vec2, chao: f32, yaw: f32, zoom: f32, pitch: f32) -> Camera3D {
    let z = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let (recuo, altura) = recuo_e_altura(z, pitch);
    let olho = vec3(
        target.x - yaw.sin() * recuo,
        chao + altura,
        target.y + yaw.cos() * recuo,
    );
    Camera3D {
        position: olho,
        target: vec3(target.x, chao, target.y),
        up: vec3(0.0, 1.0, 0.0),
        ..Default::default()
    }
}

/// Quantos pixels o botao direito precisa andar pra deixar de ser defesa e
/// virar camera.
///
/// O botao tem dois sentidos, e e' o MOVIMENTO que separa: parado ele
/// bloqueia, arrastado ele gira. Zero nao serve — a mao treme, e o jogador
/// perderia o bloqueio sem ter pedido. Seis pixels sao mais que o tremor e
/// menos que qualquer arrasto de proposito.
pub const ARRASTO_MINIMO: f32 = 6.0;

/// Persegue a altura do alvo com MOLA, e devolve altura e velocidade novas.
///
/// A camera olhava direto pro APOIO do jogador — o topo do bloco embaixo dele.
/// Apoio e' funcao degrau: muda meia unidade de uma vez toda vez que o corpo
/// cruza a divisa de um bloco, e andando a 5 unidades por segundo isso e' um
/// tranco vertical a cada dois passos.
///
/// O atraso e' so' na VERTICAL de proposito. Horizontal com atraso da' a
/// sensacao de arrastar o boneco por um elastico — quem joga quer que o
/// personagem responda no eixo em que ele manda. Altura ninguem "manda": ela
/// e' consequencia do chao, e suavizar consequencia nao tira controle de
/// ninguem.
///
/// ── Por que MOLA e nao interpolacao ──────────────────────────────────────
///
/// A primeira versao era exponencial, e exponencial anda PROPORCIONAL A
/// DISTANCIA: erro grande, arranque grande. O sintoma foi exatamente esse —
/// cair de um barranco ficava perfeito e alguns degraus saiam rapido demais.
/// Nao era coincidencia: caindo, o corpo acelera do zero e o alvo muda aos
/// poucos, entao a camera nunca acumula erro; num degrau o alvo pula meia
/// unidade de uma vez, e degraus encadeados acumulam mais ainda.
///
/// A mola tem INERCIA: parte do repouso e freia no fim, entao a mesma
/// perseguicao serve pro degrau, pra ladder e pra queda. Criticamente
/// amortecida — chega e para, sem passar do ponto e voltar, que numa camera
/// leria como enjoo.
///
/// A integracao e' a do `SmoothDamp` da Unity: aproximacao racional da
/// exponencial, estavel em qualquer `dt`. A integracao ingenua explode quando
/// um quadro demora.
pub fn altura_da_camera(atual: f32, vel: f32, alvo: f32, dt: f32) -> (f32, f32) {
    /// Tempo aproximado pra fechar a distancia.
    ///
    /// Escolhido contra o RELOGIO DO CORPO, e nao por gosto: o boneco sobe
    /// meio bloco em 125 ms, e a camera precisa estar andando DURANTE isso.
    /// Com 0,34 o pico da velocidade dela caia no quadro 10 (167 ms) — ela so'
    /// comecava a se mexer quando o degrau ja' tinha acabado, e o efeito era
    /// a camera parecer presa no piso ate' o jogador sair do bloco.
    ///
    /// ```text
    ///     T      1o quadro     pico    quadro do pico    50%      90%
    ///   0,15       10,5 mm   40,7 mm         4          117 ms   283 ms
    ///   0,34        2,2 mm   18,0 mm        10          283 ms   650 ms
    /// ```
    ///
    /// O arranque continua macio — 10,5 mm contra os 21,2 mm da interpolacao
    /// exponencial que estava aqui antes. E' a mola que tira o tranco; o
    /// tempo so' decide se ela acompanha o corpo ou chega atrasada.
    const TEMPO: f32 = 0.15;
    /// Acima disto nao e' relevo, e' teleporte (respawn, viagem, entrar no
    /// AOI). Deslizar por vinte unidades de mundo seria pior que o corte.
    const SALTO: f32 = 6.0;

    if atual == f32::MIN || (alvo - atual).abs() > SALTO {
        return (alvo, 0.0);
    }
    let w = 2.0 / TEMPO;
    let x = w * dt;
    let decai = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let erro = atual - alvo;
    let temp = (vel + w * erro) * dt;
    (alvo + (erro + temp) * decai, (vel - w * temp) * decai)
}

/// Recuo e altura da camera pra um zoom e uma inclinacao.
///
/// Uma conta so', usada pela camera E pelo desvio de morro: elas ja'
/// divergiram uma vez, e o resultado foi a camera subindo por um morro que
/// nao estava mais no caminho.
fn recuo_e_altura(zoom: f32, pitch: f32) -> (f32, f32) {
    let p = pitch.clamp(pitch_min_para(zoom), PITCH_MAX);
    let d = cam_dist() * zoom;
    (d * p.cos(), d * p.sin())
}

/// Gira um vetor de input pra que "pra frente" seja **longe da camera**.
///
/// E' o que faz a camera girar sem virar quebra-cabeca: o jogador aperta pra
/// cima e o boneco anda pra cima da TELA, nao pro norte do mundo. A conta
/// mora no cliente e o servidor continua recebendo direcao em espaco de
/// mundo — girar a camera nao concede confianca nenhuma nova.
pub fn input_para_mundo(dir: Vec2, yaw: f32) -> Vec2 {
    // A camera fica em `(-sin·recuo, +cos·recuo)` relativa ao alvo, entao o
    // vetor camera→alvo — que e' o "pra frente" do jogador — e' `(sin, -cos)`.
    // Com W valendo `(0,-1)`, e' exatamente o que esta rotacao devolve.
    //
    // A primeira versao tinha o SINAL TROCADO e passou por um teste que so'
    // olhava se a direcao MUDAVA com o yaw. Mudava — pro lado errado. Daí os
    // asserts abaixo compararem contra a direcao esperada, e nao contra a
    // anterior.
    let (s, c) = (yaw.sin(), yaw.cos());
    vec2(dir.x * c - dir.y * s, dir.x * s + dir.y * c)
}

#[cfg(test)]
mod testes_camera {
    use super::*;

    fn perto(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-5
    }

    /// A banda existe pra o jogador nao chegar na borda do mundo carregado
    /// nem achatar o relevo. Se ela deixar de valer, a camera passa a mostrar
    /// o vazio — e o sintoma nao parece camera, parece buraco na malha.
    #[test]
    fn a_inclinacao_fica_dentro_da_banda() {
        for pedido in [-3.0f32, 0.0, 0.3, PITCH_MIN, 1.0, PITCH_MAX, 2.0, 9.0] {
            for zoom in [ZOOM_MIN, 1.0, ZOOM_MAX] {
                let cam = camera(Vec2::ZERO, 0.0, 0.0, zoom, pedido);
                let d = cam.position - cam.target;
                let p = d.y.atan2(vec2(d.x, d.z).length());
                let piso = pitch_min_para(zoom);
                assert!(
                    p >= piso - 1e-4 && p <= PITCH_MAX + 1e-4,
                    "pitch {pedido} no zoom {zoom} virou {p} — fora de [{piso}, {PITCH_MAX}]"
                );
            }
        }
    }

    /// Inclinar NAO pode aproximar: se a distancia mudasse com o angulo, girar
    /// a camera pra baixo daria zoom de graca e o alcance de visao viraria
    /// funcao de para onde o jogador aponta.
    #[test]
    fn inclinar_nao_aproxima() {
        let base = (camera(Vec2::ZERO, 0.0, 0.0, 1.0, pitch_padrao()).position).length();
        for p in [PITCH_MIN, 0.8, pitch_padrao(), 1.1, PITCH_MAX] {
            let d = camera(Vec2::ZERO, 0.0, 0.0, 1.0, p).position.length();
            assert!(
                (d - base).abs() < 1e-3,
                "pitch {p} mudou a distancia: {d} vs {base}"
            );
        }
    }

    /// Afastar tem que FECHAR a inclinacao: deitar a camera e afastar sao a
    /// mesma vontade de ver mais longe, e as duas juntas passam da borda do
    /// mundo carregado.
    #[test]
    fn afastar_fecha_a_inclinacao() {
        let perto = pitch_min_para(ZOOM_MIN);
        let longe = pitch_min_para(ZOOM_MAX);
        assert!(
            (perto - PITCH_MIN).abs() < 1e-5,
            "colado tem que deitar ate' o limite"
        );
        assert!(
            (longe - PITCH_MIN_LONGE).abs() < 1e-5,
            "afastado o piso tem que ser PITCH_MIN_LONGE, foi {longe}"
        );
        assert!(perto < longe, "colado tem que deitar MAIS que afastado");
        // Monotonica: qualquer degrau intermediario tem que subir.
        let mut anterior = perto;
        for k in 1..=10 {
            let z = ZOOM_MIN + (ZOOM_MAX - ZOOM_MIN) * k as f32 / 10.0;
            let p = pitch_min_para(z);
            assert!(
                p >= anterior - 1e-6,
                "piso caiu de {anterior} pra {p} no zoom {z}"
            );
            anterior = p;
        }
    }

    /// The formula has to reproduce the measured table, or scaling it for
    /// the other view distances would scale the wrong thing.
    #[test]
    fn o_alcance_bate_com_a_tabela_medida() {
        for (zoom, graus, medido) in [
            (0.55f32, 26.0f32, 68.0f32),
            (0.55, 27.0, 55.0),
            (1.5, 34.0, 71.0),
            (1.5, 42.0, 49.0),
        ] {
            let a = alcance_do_topo(zoom, graus.to_radians());
            assert!((a - medido).abs() < 2.0, "zoom {zoom} a {graus}°: {a} vs {medido}");
        }
    }

    /// More world, lower camera — and never past what that world covers.
    #[test]
    fn mais_distancia_deita_mais() {
        for k in 0..=10 {
            let z = ZOOM_MIN + (ZOOM_MAX - ZOOM_MIN) * k as f32 / 10.0;
            let pisos: Vec<f32> = [3, 5, 7, 9, 12].iter().map(|&r| pitch_min_com_raio(z, r)).collect();
            assert!(pisos.windows(2).all(|w| w[0] > w[1]), "zoom {z}: {pisos:?}");
            assert!((pisos[1] - piso_medido(z)).abs() < 1e-6, "o padrao mudou");
            for (r, p) in [3, 5, 7, 9, 12].iter().zip(&pisos) {
                let mundo = (*r as f32 + 0.5) * 16.0;
                assert!(alcance_do_topo(z, *p) < mundo, "raio {r}, zoom {z}: ve' a borda");
                assert!(*p < PITCH_MAX - 0.25, "raio {r}, zoom {z}: sem banda");
            }
        }
    }

    /// A roda sozinha tem que dar um enquadramento util em todo o percurso:
    /// deitada perto, de cima longe, e sempre dentro da banda daquele zoom.
    #[test]
    fn a_roda_escolhe_o_enquadramento() {
        let mut anterior = 0.0f32;
        for k in 0..=10 {
            let z = ZOOM_MIN + (ZOOM_MAX - ZOOM_MIN) * k as f32 / 10.0;
            let p = pitch_do_zoom(z);
            assert!(
                p >= pitch_min_para(z) - 1e-5 && p <= PITCH_MAX + 1e-5,
                "zoom {z}: automatico {p} fora da banda"
            );
            assert!(
                p > anterior,
                "afastar tem que LEVANTAR: {anterior} -> {p} no zoom {z}"
            );
            anterior = p;
        }
        // E tem que sobrar espaco pra mao nos dois sentidos.
        for z in [ZOOM_MIN, 1.0, ZOOM_MAX] {
            let p = pitch_do_zoom(z);
            assert!(
                p - pitch_min_para(z) > 0.05,
                "zoom {z}: sem espaco pra deitar na mao"
            );
            assert!(
                PITCH_MAX - p > 0.05,
                "zoom {z}: sem espaco pra levantar na mao"
            );
        }
    }

    /// ANDANDO PRA FRENTE, A CAMERA TEM QUE VER AS COSTAS.
    ///
    /// A corrente inteira num teste so': tecla -> direcao de mundo -> angulo
    /// da entidade -> rotacao da malha -> comparacao com a camera. Cada elo
    /// ja' foi conferido isolado e todos passaram; o que ninguem tinha
    /// medido era a corrente fechada.
    #[test]
    fn andando_pra_frente_a_camera_ve_as_costas() {
        for cam_yaw in [0.0f32, 0.9, 2.4, -1.7] {
            // W, em espaco de mundo.
            let dir = input_para_mundo(vec2(0.0, -1.0), cam_yaw);
            // O que `World::tick` faz com a velocidade.
            let yaw = dir.x.atan2(dir.y);
            // Pra onde a FRENTE do modelo aponta depois da rotacao de
            // `draw_mesh_at`. O modelo nasce olhando pro +Z.
            let (sin, cos) = yaw.sin_cos();
            let frente_modelo = vec2(sin, cos);
            // Da camera pro alvo.
            let cam = camera(Vec2::ZERO, 0.0, cam_yaw, 1.0, pitch_padrao());
            let d = cam.target - cam.position;
            let camera_pra_alvo = vec2(d.x, d.z).normalize();
            // Andando pra frente, o boneco vai NA MESMA direcao em que a
            // camera olha — logo ela ve' as costas dele.
            let alinhamento = frente_modelo.dot(camera_pra_alvo);
            assert!(
                alinhamento > 0.9,
                "cam_yaw {cam_yaw}: o modelo aponta {frente_modelo:?} e a camera olha \
                 pra {camera_pra_alvo:?} (alinhamento {alinhamento:.2}) — \
                 negativo quer dizer rosto virado pra camera"
            );
        }
    }

    /// O furo tem que ficar EM CIMA do jogador e comecar antes dele.
    ///
    /// Sao as duas coisas que decidem se o recorte serve: centrado fora do
    /// boneco ele mostra o lugar errado, e comecando na profundidade do
    /// proprio jogador ele abre um buraco no chao em volta dos pes — porque
    /// visto de cima, o terreno logo a' frente do corpo esta' mais perto da
    /// camera que o corpo.
    #[test]
    fn o_furo_fica_em_cima_do_jogador_e_comeca_antes_dele() {
        for (yaw, zoom) in [(0.0f32, 1.0f32), (1.3, 0.6), (-2.2, 1.4)] {
            let cam = camera(Vec2::ZERO, 10.0, yaw, zoom, pitch_do_zoom(zoom));
            let jogador = vec3(0.0, 10.0, 0.0);
            let tela = vec2(1920.0, 1080.0);
            let (recorte, corte_z) = recorte_do_jogador(&cam, jogador, 0.85, tela);

            // O centro do furo cai onde o corpo e' desenhado.
            let m = matriz_da_camera(&cam, tela.x / tela.y);
            let meio = jogador + vec3(0.0, 0.6, 0.0);
            let alvo = world_to_screen_com(&m, meio, tela).expect("jogador na tela");
            assert!(
                (recorte.x - alvo.x).abs() < 1.0 && (recorte.y - (tela.y - alvo.y)).abs() < 1.0,
                "yaw {yaw}: furo em ({:.0}, {:.0}), jogador em ({:.0}, {:.0})",
                recorte.x,
                recorte.y,
                alvo.x,
                tela.y - alvo.y
            );

            // E o corte comeca ANTES do jogador: a profundidade de referencia
            // e' menor (mais perto da camera) que a dele.
            assert!(
                corte_z < alvo.z,
                "yaw {yaw}: corte em {corte_z:.5} nao esta' na frente do \
                 jogador em {:.5} — o furo vai comer o chao",
                alvo.z
            );
            // E nao TAO antes a ponto de nunca pegar nada.
            assert!(
                alvo.z - corte_z < 0.02,
                "yaw {yaw}: corte {:.5} longe demais do jogador",
                alvo.z - corte_z
            );
        }
    }

    /// O furo tem que crescer com o BONECO, e nao com a tela.
    ///
    /// Em fracao da tela ele erra nas duas pontas: aproximando, o boneco
    /// cresce na perspectiva e o furo fica pequeno demais pra ele; afastando,
    /// sobra furo em volta de um boneco minusculo.
    #[test]
    fn o_furo_acompanha_o_tamanho_do_boneco() {
        let tela = vec2(1920.0, 1080.0);
        let jogador = vec3(0.0, 10.0, 0.0);
        let medida = |zoom: f32| {
            let cam = camera(Vec2::ZERO, 10.0, 0.0, zoom, pitch_do_zoom(zoom));
            let m = matriz_da_camera(&cam, tela.x / tela.y);
            let pes = world_to_screen_com(&m, jogador, tela).unwrap();
            let cabeca = world_to_screen_com(
                &m,
                jogador + vec3(0.0, crate::vegetacao::ALTURA_QUE_ESCONDE, 0.0),
                tela,
            )
            .unwrap();
            let corpo = (cabeca.y - pes.y).abs();
            let (r, _) = recorte_do_jogador(&cam, jogador, 0.85, tela);
            (corpo, r.z)
        };
        let (corpo_perto, raio_perto) = medida(ZOOM_MIN);
        let (corpo_longe, raio_longe) = medida(ZOOM_MAX);

        assert!(
            corpo_perto > corpo_longe * 1.5,
            "o teste nao esta' medindo nada: o boneco tem {corpo_perto:.0} px \
             colado e {corpo_longe:.0} px afastado"
        );
        // A razao entre furo e corpo tem que ser a MESMA nas duas pontas.
        let razao_perto = raio_perto / corpo_perto;
        let razao_longe = raio_longe / corpo_longe;
        assert!(
            (razao_perto - razao_longe).abs() < 0.02,
            "furo/corpo mudou com o zoom: {razao_perto:.3} colado contra \
             {razao_longe:.3} afastado"
        );
        assert!(raio_perto > raio_longe, "o furo nao cresceu ao aproximar");
    }

    /// A camera nunca pode ficar DENTRO do chao.
    ///
    /// Com o descarte de face de costas, de dentro da terra nao se ve' terra:
    /// as faces do relevo olham pra fora, e o que aparece e' o vazio atras
    /// delas. O sintoma nao parece camera — parece o mundo ter acabado.
    #[test]
    fn a_camera_nao_entra_no_chao() {
        // Um morro de dez unidades onde a camera iria parar, e chao baixo
        // onde o jogador esta'.
        let alvo = Vec2::ZERO;
        let apoio = 1.0;
        for (yaw, zoom) in [(0.0f32, 1.0f32), (2.0, ZOOM_MIN), (-1.0, ZOOM_MAX)] {
            let ideal = camera(alvo, apoio, yaw, zoom, pitch_do_zoom(zoom));
            // Relevo alto EXATAMENTE onde o olho iria: e' o caso do morro
            // atras do jogador, que e' o que acontece em ilha.
            let morro = |x: f32, z: f32| {
                if vec2(x, z).distance(vec2(ideal.position.x, ideal.position.z)) < 6.0 {
                    ideal.position.y + 4.0
                } else {
                    0.0
                }
            };
            let cam = camera_com_chao(alvo, apoio, yaw, zoom, pitch_do_zoom(zoom), &morro);
            let solo = morro(cam.position.x, cam.position.z);
            assert!(
                cam.position.y >= solo + FOLGA_DA_CAMERA - 1e-4,
                "yaw {yaw}: olho em {:.2} com chao em {solo:.2}",
                cam.position.y
            );
            // E o alvo nao se mexe: quem sobe e' o olho, o enquadramento
            // continua no boneco.
            assert!((cam.target - ideal.target).length() < 1e-5);
        }
    }

    /// Em terreno plano ela NAO se mexe. O desvio de morro foi tirado daqui
    /// de proposito, e o conserto de nao-entrar-no-chao nao pode traze-lo de
    /// volta pela porta dos fundos.
    #[test]
    fn em_terreno_plano_a_camera_nao_sobe() {
        let plano = |_: f32, _: f32| 0.0f32;
        for zoom in [ZOOM_MIN, 1.0, ZOOM_MAX] {
            let p = pitch_do_zoom(zoom);
            let ideal = camera(Vec2::ZERO, 0.0, 0.7, zoom, p);
            let cam = camera_com_chao(Vec2::ZERO, 0.0, 0.7, zoom, p, &plano);
            assert!(
                (cam.position - ideal.position).length() < 1e-5,
                "zoom {zoom}: a camera se mexeu {:.3} em terreno plano",
                (cam.position - ideal.position).length()
            );
        }
    }

    /// A camera nao pode ARRANCAR num degrau — e' o defeito que a mola veio
    /// consertar. A interpolacao exponencial anda proporcional a distancia,
    /// entao um degrau isolado saia macio e uma ladder saia chicoteando.
    #[test]
    fn a_camera_engole_o_degrau() {
        let dt = 1.0 / 60.0;
        // Primeiro quadro assenta: entrar no mundo nao e' deslizar do zero.
        assert_eq!(altura_da_camera(f32::MIN, 0.0, 12.0, dt), (12.0, 0.0));
        // Teleporte corta em vez de deslizar o mundo inteiro.
        assert_eq!(altura_da_camera(12.0, 0.0, 90.0, dt), (90.0, 0.0));

        // Meio bloco partindo do repouso: o primeiro quadro quase nao anda,
        // porque a mola precisa acelerar.
        let (h1, v1) = altura_da_camera(12.0, 0.0, 12.5, dt);
        // Metade do que a interpolacao exponencial que estava aqui fazia
        // (21,2 mm). O numero exato depende de `TEMPO`; o que nao pode e' o
        // primeiro quadro ser o maior de todos, e disso cuida o teste do pico.
        assert!(
            h1 - 12.0 < 0.5 * 0.05,
            "arrancou {:.4} de 0,5 no primeiro quadro",
            h1 - 12.0
        );
        assert!(v1 > 0.0, "a mola nem comecou a andar");

        // Chega, e SEM PASSAR DO PONTO: camera que passa e volta le' como
        // enjoo.
        let (mut h, mut v) = (12.0f32, 0.0f32);
        let mut maior = f32::MIN;
        for _ in 0..120 {
            (h, v) = altura_da_camera(h, v, 12.5, dt);
            maior = maior.max(h);
        }
        assert!(
            (h - 12.5).abs() < 0.01,
            "dois segundos depois estava em {h}"
        );
        assert!(maior <= 12.5 + 1e-3, "passou do ponto ate' {maior}");

        // O PICO da velocidade fica no MEIO do caminho, e nao no primeiro
        // quadro. E' nisso que a mola difere da interpolacao, e e' o que o
        // olho le' como "a camera acompanhou" em vez de "a camera arrancou":
        // a exponencial e' mais rapida justamente no instante em que o degrau
        // acontece, e vai freando dali em diante.
        let (mut h, mut v) = (0.0f32, 0.0f32);
        let (mut pico, mut quadro_do_pico) = (0.0f32, 0usize);
        for k in 0..120 {
            let antes = h;
            (h, v) = altura_da_camera(h, v, 0.5, dt);
            if h - antes > pico {
                pico = h - antes;
                quadro_do_pico = k;
            }
        }
        // Nem no primeiro quadro (seria arranque), nem tarde demais: o corpo
        // sobe o degrau em 7,5 quadros, e a camera tem que estar no meio do
        // movimento dentro dessa janela — senao ela parece presa no piso.
        assert!(
            (2..=7).contains(&quadro_do_pico),
            "pico da camera no quadro {quadro_do_pico}: fora da janela da subida do corpo (2..7)"
        );
    }

    /// A vista de repouso tem que ser EXATAMENTE a de antes de a banda
    /// existir: a banda foi pra dar liberdade, nao pra mudar o padrao.
    #[test]
    fn a_vista_de_repouso_e_a_de_sempre() {
        let cam = camera(Vec2::ZERO, 0.0, 0.0, 1.0, pitch_padrao());
        assert!(
            (cam.position.y - CAM_HEIGHT).abs() < 1e-3,
            "altura {}",
            cam.position.y
        );
        assert!(
            (cam.position.z - CAM_BACK).abs() < 1e-3,
            "recuo {}",
            cam.position.z
        );
    }

    /// W tem que apontar pra LONGE da camera em qualquer angulo, e D pra
    /// direita da tela. E' a unica coisa que "camera relativa" quer dizer.
    #[test]
    fn w_aponta_pra_longe_da_camera() {
        let w = vec2(0.0, -1.0);
        let d = vec2(1.0, 0.0);
        for yaw in [0.0f32, 0.7, 1.5707964, 3.14159, -2.1] {
            // Direcao camera→alvo, tirada da MESMA conta que posiciona a
            // camera em `camera()`: se uma mudar sem a outra, isto quebra.
            let cam = camera(Vec2::ZERO, 0.0, yaw, 1.0, pitch_padrao());
            let frente = (cam.target - cam.position).normalize();
            let frente = vec2(frente.x, frente.z).normalize();
            let obtido = input_para_mundo(w, yaw);
            assert!(
                perto(obtido, frente),
                "yaw {yaw}: W deu {obtido:?}, esperado {frente:?}"
            );
            // D e' a direita DA TELA. Com o eixo Z crescendo pra baixo na
            // tela, a direita de `(fx, fz)` e' `(-fz, fx)` — a mao troca em
            // relacao a' convencao 3D, e foi ai' que eu errei o primeiro
            // assert (o codigo estava certo, o teste e' que nao).
            let dir_d = input_para_mundo(d, yaw);
            let direita = vec2(-frente.y, frente.x);
            assert!(
                perto(dir_d, direita),
                "yaw {yaw}: D deu {dir_d:?}, esperado {direita:?}"
            );
        }
    }
}

/// Tudo que traduz entre MUNDO e TELA num quadro.
///
/// Existe porque a mesma pergunta estava sendo respondida em tres lugares com
/// contas diferentes, e as tres divergiram em sequencia: a camera do desenho
/// usava um chao, a do clique usava outro; depois a mira projetava a entidade
/// no nivel do mar enquanto o desenho a punha no topo do morro. Cada um desses
/// foi um bug separado com o mesmo formato.
///
/// Agora desenho e mira **chamam a mesma funcao**. Nao e' disciplina: nao ha'
/// como divergirem porque nao existe a segunda conta.
pub struct Vista<'a> {
    pub cam: Camera3D,
    chao: &'a dyn Fn(f32, f32) -> f32,
}

/// Folga minima entre o olho e o chao, em unidades.
///
/// Meio metro e pouco: mais que o plano de corte perto (0,01) com sobra, e o
/// bastante pra o chao nao encher a base da tela quando a camera raspa um
/// barranco.
const FOLGA_DA_CAMERA: f32 = 0.6;

/// A camera do quadro, garantidamente FORA do chao.
///
/// A camera nao desvia de morro — isso existiu aqui e foi tirado, porque o
/// preco de nunca perder o boneco era ela se levantando sozinha perto de
/// qualquer elevacao, e camera que se mexe sem o jogador pedir incomoda mais
/// que o instante em que o relevo tapa a vista.
///
/// Mas ter o morro NA FRENTE e estar DENTRO dele sao coisas diferentes. Com o
/// descarte de face de costas ligado, de dentro da terra nao se ve' terra: as
/// faces do relevo olham todas pra fora, e o que aparece e' o vazio atras
/// delas — meia tela de ceu, como se o mundo tivesse acabado ali.
///
/// Entao a unica correcao e' esta: subir o olho o MINIMO pra ele nao ficar
/// enterrado. O relevo e' campo de altura e nao tem saliencia, entao estar
/// acima da altura naquele ponto ja' garante estar do lado de fora.
pub fn camera_com_chao(
    alvo: Vec2,
    apoio: f32,
    yaw: f32,
    zoom: f32,
    pitch: f32,
    chao: &dyn Fn(f32, f32) -> f32,
) -> Camera3D {
    let mut cam = camera(alvo, apoio, yaw, zoom, pitch);
    let minimo = chao(cam.position.x, cam.position.z) + FOLGA_DA_CAMERA;
    if cam.position.y < minimo {
        cam.position.y = minimo;
    }
    cam
}

impl<'a> Vista<'a> {
    /// Monta a vista do quadro: camera girada e a funcao de chao que todo o
    /// resto vai consultar.
    ///
    /// A camera nao desvia do relevo, mas nunca fica DENTRO dele. Ver
    /// `camera_com_chao`.
    pub fn nova(
        alvo: Vec2,
        yaw: f32,
        zoom: f32,
        pitch: f32,
        apoio: f32,
        chao: &'a dyn Fn(f32, f32) -> f32,
    ) -> Self {
        Self {
            cam: camera_com_chao(alvo, apoio, yaw, zoom, pitch, chao),
            chao,
        }
    }

    pub fn chao_em(&self, x: f32, z: f32) -> f32 {
        (self.chao)(x, z)
    }

    /// Onde a entidade esta', em mundo. **Unico lugar que responde isso.**
    pub fn pos_de(&self, e: &crate::world::Ent) -> Vec3 {
        // `render_y` ja' e' a altura do corpo, no chao ou no ar. Mira, anel
        // de alvo e modelo sobem juntos porque todos passam por aqui.
        vec3(e.render_pos.x, e.render_y, e.render_pos.y)
    }

    /// Onde se MIRA na entidade: meio corpo acima dos pes. Clicar nos pes
    /// obriga o jogador a acertar a sombra, nao o bicho.
    pub fn mira_de(&self, e: &crate::world::Ent) -> Vec3 {
        self.pos_de(e) + vec3(0.0, 0.5, 0.0)
    }

    pub fn na_tela(&self, p: Vec3) -> Option<Vec2> {
        world_to_screen(&self.cam, p)
    }

    pub fn raio(&self, tela: Vec2) -> (Vec3, Vec3) {
        raio_da_tela(&self.cam, tela)
    }
}

/// Raio que sai da camera pelo pixel apontado.
///
/// A macroquad nao expoe unproject, entao a conta e' na mao: monta a base da
/// camera e desloca pelo tamanho do plano de projecao no alvo.
pub fn raio_da_tela(cam: &Camera3D, tela: Vec2) -> (Vec3, Vec3) {
    let frente = (cam.target - cam.position).normalize();
    let direita = frente.cross(cam.up).normalize();
    let cima = direita.cross(frente);
    let (lw, lh) = (screen_width(), screen_height());
    // NDC com Y pra cima.
    let nx = tela.x / lw * 2.0 - 1.0;
    let ny = 1.0 - tela.y / lh * 2.0;
    let fov = cam.fovy;
    let alt = (fov * 0.5).tan();
    let larg = alt * (lw / lh);
    let dir = (frente + direita * (nx * larg) + cima * (ny * alt)).normalize();
    (cam.position, dir)
}

/// Onde e ate' onde furar o mundo pra o jogador aparecer.
///
/// Devolve `(centro_x, centro_y, raio)` em pixels de `gl_FragCoord` — origem
/// embaixo — e a profundidade de janela a partir da qual um fragmento conta
/// como estando NA FRENTE do jogador.
///
/// ── A margem e' o detalhe que faz funcionar ──────────────────────────────
///
/// Cortar tudo que estiver mais perto que o jogador parece certo e abre um
/// buraco no chao em volta dos pes dele: a camera olha de cima, entao o
/// terreno logo a' frente do corpo esta' a poucos centimetros de distancia da
/// camera — mais perto, portanto cortado.
///
/// Entao a referencia nao e' o jogador: e' um ponto `MARGEM` unidades NA
/// DIRECAO DA CAMERA a partir dele. Só o que estiver antes disso vira furo, e
/// isso e' obstaculo de verdade — parede, tronco, casa — e nao o chao em que
/// ele pisa.
pub fn recorte_do_jogador(cam: &Camera3D, jogador: Vec3, fator: f32, tela: Vec2) -> (Vec3, f32) {
    /// Quanto um obstaculo precisa estar a' frente do jogador pra virar furo.
    ///
    /// Mais que a metade da profundidade do corpo e menos que a distancia
    /// tipica de uma arvore vizinha.
    const MARGEM: f32 = 1.1;

    let m = matriz_da_camera(cam, tela.x / tela.y);
    // Mira no MEIO do corpo e nao nos pes: o furo existe pra mostrar o
    // boneco, e centrar nos pes joga metade dele pra fora do circulo.
    let meio = jogador + vec3(0.0, 0.6, 0.0);
    let Some(no_alvo) = world_to_screen_com(&m, meio, tela) else {
        return (Vec3::ZERO, 0.0);
    };
    let pra_camera = (cam.position - meio).normalize_or_zero();
    let corte = meio + pra_camera * MARGEM;
    let Some(atras) = world_to_screen_com(&m, corte, tela) else {
        return (Vec3::ZERO, 0.0);
    };
    // `gl_FragCoord` conta o Y de baixo pra cima; a tela, de cima pra baixo.
    // ── O RAIO SAI DO TAMANHO APARENTE DO CORPO ──
    //
    // Em fracao da tela ele erra nas duas pontas: aproximando, o boneco cresce
    // na perspectiva e o furo fica pequeno demais pra ele; afastando, sobra
    // furo em volta de um boneco minusculo. O tamanho na tela e' a unica
    // medida que ja' leva junto o zoom, a inclinacao, a abertura da lente e a
    // resolucao.
    // Mede o corpo INTEIRO, dos pes a' cabeca. Medir do meio pra cima e
    // dobrar parece equivalente e nao e': em perspectiva, metades iguais no
    // mundo nao dao metades iguais na tela, e a razao entre furo e boneco
    // passava a depender do zoom — que e' justamente o que este raio existe
    // pra tirar.
    let cabeca = jogador + vec3(0.0, crate::vegetacao::ALTURA_QUE_ESCONDE, 0.0);
    let alto_px = match (
        world_to_screen_com(&m, jogador, tela),
        world_to_screen_com(&m, cabeca, tela),
    ) {
        (Some(pes), Some(topo)) => (topo.y - pes.y).abs().max(1.0),
        _ => tela.y * 0.1,
    };
    (
        vec3(no_alvo.x, tela.y - no_alvo.y, alto_px * fator),
        atras.z,
    )
}

/// A matriz projecao×vista da camera, com a proporcao vinda de fora.
///
/// A `Camera3D::matrix()` da macroquad consulta o tamanho da JANELA mesmo
/// quando a proporcao esta' preenchida — e' a primeira coisa que ela faz.
/// Isso amarra a conta ao contexto grafico e deixa a projecao sem teste, que
/// e' justamente onde os erros de "clique mira num lugar, o olho ve' outro"
/// moram. Aqui a proporcao e' argumento.
fn matriz_da_camera(cam: &Camera3D, proporcao: f32) -> Mat4 {
    Mat4::perspective_rh_gl(cam.fovy, proporcao, cam.z_near, cam.z_far)
        * Mat4::look_at_rh(cam.position, cam.target, cam.up)
}

/// Projeta pra pixel de tela mais PROFUNDIDADE DE JANELA (0..1), que e' a
/// escala em que o `gl_FragCoord.z` do shader vive.
///
/// A `world_to_screen` da macroquad joga o z fora, e e' justamente ele que
/// decide o que esta' na frente de quem.
fn world_to_screen_com(m: &Mat4, p: Vec3, tela: Vec2) -> Option<Vec3> {
    let c = *m * p.extend(1.0);
    if c.w.abs() < 1e-6 {
        return None;
    }
    let ndc = c.truncate() / c.w;
    Some(vec3(
        (ndc.x * 0.5 + 0.5) * tela.x,
        (0.5 - ndc.y * 0.5) * tela.y,
        ndc.z * 0.5 + 0.5,
    ))
}

/// Material que DESCARTA a face de costas.
///
/// A macroquad desenha com `CullFace::Nothing` — todo triangulo e' rasterizado
/// dos dois lados. Num mundo de blocos isso e' trabalho jogado fora: metade
/// das faces de qualquer superficie fechada olha pra longe da camera, e todas
/// elas passam por transformacao, rasterizacao e teste de profundidade antes
/// de perder pro que esta' na frente.
///
/// So' da' pra ligar porque o enrolamento e' garantido por construcao nos tres
/// geradores de malha (`terreno::ordem`, `vegetacao::emite`, `vox`): quad
/// enrolado ao contrario SOME com o descarte ligado, e some de um lado so' —
/// o buraco espera o jogador virar a camera pra aparecer.
///
/// O shader e' o mesmo da macroquad. Ele esta' copiado aqui porque o modulo
/// dela e' privado; se um dia ela expuser, isto vira um `use`.
pub(crate) const SOLIDO_VERTICE: &str = r#"#version 100
    attribute vec3 position;
    attribute vec2 texcoord;
    attribute vec4 color0;
    attribute vec4 normal;

    varying lowp vec2 uv;
    varying lowp vec4 color;
    // 1 = esta' geometria pode virar furo; 0 = passa batido. Vem no
    // `normal.x`, que este shader nao usa pra mais nada.
    varying lowp float recortavel;
    // World position, for the distance fog.
    varying highp vec3 mundo;
    // The voxel's own colour before night darkens it: what the city's
    // lights (`LuzPos`) light up.
    varying lowp vec3 base;

    uniform mat4 Model;
    uniform mat4 Projection;
    // Clarao do golpe / corpo escurecido: puxa o rgb pra `Tinta.rgb` na
    // fracao `Tinta.a` (zero = cor do voxel). Antes era recopia na CPU.
    uniform vec4 Tinta;
    uniform float LuzDia;

    // AS FAIXAS DE PALETA (docs/ARTE_DO_PERSONAGEM.md).
    //
    // A arte escreve uma cor de rascunho nos indices 241-252 e o vertice traz
    // em `normal.yzw` qual faixa (1 tier, 2 cabelo, 3 pele), o degrau na rampa
    // e a sombra da face. A cor final e' interpolada aqui, entre o claro e o
    // escuro que o draw mandou.
    //
    // E' isto que faz tom de pele e cor de cabelo custarem ZERO malha: sem
    // ele, cada tom seria uma copia da geometria, e a peca `cabeca` — que tem
    // as duas faixas — daria 24 copias de cada rosto.
    //
    // `.a == 0` numa faixa = nao tinge (o caso de bicho, terreno e NPC), e ai'
    // o desenho e' byte-a-byte o de antes.
    uniform vec4 TierClaro;   uniform vec4 TierEsc;
    uniform vec4 CabeloClaro; uniform vec4 CabeloEsc;
    uniform vec4 PeleClaro;   uniform vec4 PeleEsc;

    void main() {
        mundo = (Model * vec4(position, 1)).xyz;
        gl_Position = Projection * Model * vec4(position, 1);
        color = color0 / 255.0;

        // Sem array indexado dinamicamente: GLSL ES 1.00 nao garante isso.
        lowp float slot = normal.y;
        lowp float t = normal.z;
        lowp float sombra = normal.w;
        lowp vec4 claro = vec4(0.0);
        lowp vec4 escuro = vec4(0.0);
        if (slot > 2.5)      { claro = PeleClaro;   escuro = PeleEsc; }
        else if (slot > 1.5) { claro = CabeloClaro; escuro = CabeloEsc; }
        else if (slot > 0.5) { claro = TierClaro;   escuro = TierEsc; }
        if (claro.a > 0.0) {
            color.rgb = mix(escuro.rgb, claro.rgb, t) * sombra;
        }

        color.rgb = mix(color.rgb, Tinta.rgb, Tinta.a);
        base = color.rgb;
        if (LuzDia < -1.5) {
            // THE DEEP (Abyssia): everything sinks into a blue-green murk —
            // except what glows (coral, jellyfish, lanterns), which stays.
            lowp float hi = max(color.r, max(color.g, color.b));
            lowp float lo = min(color.r, min(color.g, color.b));
            lowp float luz = smoothstep(0.33, 0.58, hi - lo) * smoothstep(0.55, 0.85, hi);
            color.rgb = mix(color.rgb * vec3(0.28, 0.52, 0.60) + vec3(0.0, 0.03, 0.05), color.rgb * 1.2, luz);
        } else if (LuzDia < -0.5) {
            // NIGHT (Kōgen-tō, always): everything sinks into a cold dark
            // blue — except what is bright AND saturated (neon, lit windows,
            // screens), which glows. No emissive channel: the colour itself
            // says what is a light.
            lowp float hi = max(color.r, max(color.g, color.b));
            lowp float lo = min(color.r, min(color.g, color.b));
            lowp float luz = smoothstep(0.33, 0.58, hi - lo) * smoothstep(0.55, 0.85, hi);
            color.rgb = mix(color.rgb * vec3(0.30, 0.34, 0.52), color.rgb * 1.2, luz);
        } else {
            // Sol baixo de fim de tarde: cor quente sobre a luz de face ja' assada.
            color.rgb *= mix(vec3(1.0), vec3(1.13, 1.04, 0.82), LuzDia);
        }
        uv = texcoord;
        recortavel = normal.x;
    }"#;
// O recorte que deixa o jogador aparecer atraves do que estiver na
// frente dele. Ver `recorte_do_jogador`.
//
//   Recorte.xy  centro do furo, em pixels (origem embaixo, como o
//               `gl_FragCoord`)
//   Recorte.z   raio do furo, em pixels. Zero desliga.
//   RecorteZ    profundidade de janela a partir da qual um fragmento
//               conta como "na frente do jogador"
pub(crate) const SOLIDO_FRAGMENTO: &str = r#"#version 100
    varying lowp vec4 color;
    varying lowp vec2 uv;
    varying lowp float recortavel;
    varying highp vec3 mundo;
    varying lowp vec3 base;

    uniform sampler2D Texture;
    // THE CITY'S LIGHTS (`luzes`): up to 16 point lights near the camera.
    // LuzPos.xyz = where (world), .w = reach (0 ends the list); LuzCor.rgb
    // = colour, .a = strength. Night only: by day the list is empty.
    uniform highp vec4 LuzPos[16];
    uniform lowp vec4 LuzCor[16];
    // -2 = the deep (its fog is sea, not night sky); see the vertex shader.
    uniform highp float LuzDia;
    uniform highp vec3 Recorte;
    uniform highp float RecorteZ;
    // Distance fog: xy = centre (world x, z), z = where it starts, w = where
    // it is solid sky. w <= 0 turns it off (panels, previews).
    uniform highp vec4 Neblina;

    void main() {
        if (recortavel > 0.5 && Recorte.z > 0.0 && gl_FragCoord.z < RecorteZ) {
            highp float d = distance(gl_FragCoord.xy, Recorte.xy) / Recorte.z;
            if (d < 1.0) {
                // A borda some com PADRAO DE TELA e nao com transparencia:
                // recorte transparente exigiria ordenar o mundo de tras pra
                // frente, e o mundo aqui e' um monte de pedaco em cache. O
                // pontilhado da a mesma leitura de "esta' sumindo" custando
                // um `discard`.
                highp float ruido = fract(sin(dot(
                    floor(gl_FragCoord.xy * 0.5),
                    vec2(12.9898, 78.233))) * 43758.5453);
                // Nao apaga TUDO: deixa uma fracao dos pixels de pe' no meio
                // do furo. Sao eles que dizem "tem coisa aqui" — buraco limpo
                // faz a arvore sumir e o jogador parecer estar num descampado
                // que nao existe.
                //
                // Sobrar 10% e' pouco pra atrapalhar a leitura do boneco e o
                // bastante pra o olho ver o veu.
                highp float restante = 0.10;
                highp float fica = mix(restante, 1.0, smoothstep(0.62, 1.0, d));
                if (ruido >= fica) discard;
            }
        }
        gl_FragColor = color * texture2D(Texture, uv);
        if (LuzPos[0].w > 0.0) {
            lowp vec3 soma = vec3(0.0);
            for (int i = 0; i < 16; i++) {
                highp float r = LuzPos[i].w;
                if (r <= 0.0) break;
                highp vec3 d = mundo - LuzPos[i].xyz;
                highp float t = clamp(1.0 - dot(d, d) / (r * r), 0.0, 1.0);
                soma += LuzCor[i].rgb * (t * t * LuzCor[i].a);
            }
            gl_FragColor.rgb += base * soma * texture2D(Texture, uv).rgb;
        }
        if (Neblina.w > 0.0) {
            // A NEGATIVE start means night: the fog is the night sky.
            highp float d = distance(mundo.xz, Neblina.xy);
            lowp float f = smoothstep(abs(Neblina.z), Neblina.w, d);
            lowp vec3 ceu = LuzDia < -1.5 ? vec3(0.016, 0.10, 0.13)
                : (Neblina.z < 0.0 ? vec3(0.031, 0.039, 0.094) : vec3(0.588, 0.729, 0.839));
            gl_FragColor.rgb = mix(gl_FragColor.rgb, ceu, f);
        }
    }"#;

/// Estado de pipeline do mundo: descarte de face de costas, profundidade e
/// alfa. Um lugar so': o material da macroquad e o desenho direto na GPU
/// (`gpu_estatica`) montam o pipeline daqui.
pub(crate) fn params_solido() -> PipelineParams {
    PipelineParams {
        cull_face: miniquad::graphics::CullFace::Back,
        depth_test: miniquad::graphics::Comparison::LessOrEqual,
        depth_write: true,
        color_blend: Some(miniquad::graphics::BlendState::new(
            miniquad::graphics::Equation::Add,
            miniquad::graphics::BlendFactor::Value(miniquad::graphics::BlendValue::SourceAlpha),
            miniquad::graphics::BlendFactor::OneMinusValue(
                miniquad::graphics::BlendValue::SourceAlpha,
            ),
        )),
        ..Default::default()
    }
}

/// Transparencia das sombras sem gravar profundidade: copas sobrepostas
/// continuam mesclando, e a borda transparente nao tapa o mundo atras.
pub(crate) fn params_sombra() -> PipelineParams {
    PipelineParams {
        depth_write: false,
        ..params_solido()
    }
}

pub fn material_solido() -> Material {
    load_material(
        ShaderSource::Glsl {
            vertex: SOLIDO_VERTICE,
            fragment: SOLIDO_FRAGMENTO,
        },
        MaterialParams {
            uniforms: vec![
                UniformDesc::new("Crop", UniformType::Float3),
                UniformDesc::new("RecorteZ", UniformType::Float1),
                UniformDesc::new("Tinta", UniformType::Float4),
                UniformDesc::new("LuzDia", UniformType::Float1),
                // As faixas de paleta. Este caminho (o `Material` do
                // macroquad) e' o das PREVIAS 2D; o desenho do mundo vai pelo
                // `gpu_estatica`. Os dois declaram a mesma lista, senao o
                // shader nao compila num deles.
                UniformDesc::new("TierClaro", UniformType::Float4),
                UniformDesc::new("TierEsc", UniformType::Float4),
                UniformDesc::new("CabeloClaro", UniformType::Float4),
                UniformDesc::new("CabeloEsc", UniformType::Float4),
                UniformDesc::new("PeleClaro", UniformType::Float4),
                UniformDesc::new("PeleEsc", UniformType::Float4),
                UniformDesc::new("Neblina", UniformType::Float4),
                UniformDesc::new("LuzPos", UniformType::Float4).array(crate::gpu_estatica::MAX_LUZES),
                UniformDesc::new("LuzCor", UniformType::Float4).array(crate::gpu_estatica::MAX_LUZES),
            ],
            pipeline_params: params_solido(),
            ..Default::default()
        },
    )
    .expect("world shader")
}

thread_local! {
    /// Kōgen-tō is always night (`define_noite`).
    static NOITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Abyssia is always the deep (`define_abismo`).
    static ABISMO: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the world is drawn under the sea (Abyssia, always). Implies night
/// (`noite`) for everything that only asks whether lights are on.
pub fn define_abismo(abismo: bool) {
    ABISMO.with(|n| n.set(abismo));
}

pub fn abismo() -> bool {
    ABISMO.with(|n| n.get())
}

/// The world shader's `LuzDia` for the current mood: -2 the deep, -1 night,
/// else `dia` (0 or 1, the late sun).
pub fn luz_dia(dia: f32) -> f32 {
    if abismo() {
        -2.0
    } else if noite() {
        -1.0
    } else {
        dia
    }
}

/// Whether the world is drawn at night (Kōgen-tō, always).
pub fn define_noite(noite: bool) {
    NOITE.with(|n| n.set(noite));
}

pub fn noite() -> bool {
    NOITE.with(|n| n.get())
}

pub fn clear() {
    // Ceu, e nao quase-preto. O fundo aparece em todo horizonte e em todo vao
    // do relevo; escuro ele le' como buraco na malha — foi exatamente o que me
    // fez cacar bug de geometria por um bom tempo.
    // At night the sky is the fog's night blue, never black: the same reason.
    if abismo() {
        clear_background(Color::from_rgba(4, 25, 33, 255));
        return;
    }
    if noite() {
        clear_background(Color::from_rgba(8, 10, 24, 255));
        return;
    }
    clear_background(Color::from_rgba(150, 186, 214, 255));
}

fn tile_color(id: u16) -> Color {
    match id {
        tile_id::FLOOR => Color::from_rgba(58, 82, 48, 255),
        tile_id::WALL => Color::from_rgba(44, 40, 44, 255),
        tile_id::DIRT => Color::from_rgba(104, 88, 62, 255),
        tile_id::WATER => Color::from_rgba(28, 52, 88, 255),
        tile_id::DUNGEON_FLOOR => Color::from_rgba(52, 46, 58, 255),
        _ => Color::from_rgba(20, 20, 24, 255),
    }
}

/// Chao dos tiles em volta do alvo. Parede sobe um bloco pra ler como parede.
pub fn draw_ground(map: &Map, center: Vec2) {
    let cx = center.x.floor() as i32;
    let cy = center.y.floor() as i32;
    let mut verts: Vec<Vertex> = Vec::new();
    let mut idx: Vec<u16> = Vec::new();

    // A macroquad corta o desenho em 10.000 vertices por chamada (o aviso
    // "geometry() exceeded max drawcall size" some o resto do chao). Entao a
    // malha e' despejada em pedacos — e GPU de celular prefere lote menor.
    const FLUSH: usize = 2000;
    let mut quad = |verts: &mut Vec<Vertex>, idx: &mut Vec<u16>, p: [Vec3; 4], c: Color| {
        if verts.len() + 4 > FLUSH {
            draw_mesh(&Mesh {
                vertices: std::mem::take(verts),
                indices: std::mem::take(idx),
                texture: None,
            });
        }
        let b = verts.len() as u16;
        let color = [
            (c.r * 255.0) as u8,
            (c.g * 255.0) as u8,
            (c.b * 255.0) as u8,
            255,
        ];
        for v in p {
            verts.push(Vertex {
                position: v,
                uv: vec2(0.0, 0.0),
                color,
                normal: Vec4::ZERO,
            });
        }
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    };

    for ty in (cy - GROUND_RADIUS)..(cy + GROUND_RADIUS) {
        for tx in (cx - GROUND_RADIUS)..(cx + GROUND_RADIUS) {
            let id = map.tile(tx, ty);
            let c = tile_color(id);
            let (x0, z0) = (tx as f32 * TILE, ty as f32 * TILE);
            let (x1, z1) = (x0 + TILE, z0 + TILE);
            let h = if id == tile_id::WALL { 1.0 } else { 0.0 };
            quad(
                &mut verts,
                &mut idx,
                [
                    vec3(x0, h, z0),
                    vec3(x1, h, z0),
                    vec3(x1, h, z1),
                    vec3(x0, h, z1),
                ],
                c,
            );
            // Lateral norte da parede, so' pra ela nao parecer um decalque.
            if h > 0.0 {
                let side = Color::new(c.r * 0.7, c.g * 0.7, c.b * 0.7, 1.0);
                quad(
                    &mut verts,
                    &mut idx,
                    [
                        vec3(x0, 0.0, z1),
                        vec3(x1, 0.0, z1),
                        vec3(x1, h, z1),
                        vec3(x0, h, z1),
                    ],
                    side,
                );
            }
        }
    }
    if !verts.is_empty() {
        draw_mesh(&Mesh {
            vertices: verts,
            indices: idx,
            texture: None,
        });
    }
}

/// Modelo `.vox` de cada entidade.
///
/// Orcamento de arte: **mob comum e' barato, boss e' caro**. O `lobo.vox`
/// original da 14.112 triangulos — isso e' modelo de boss, do qual ha um na
/// tela. Mob comum usa a versao reduzida (`tools/voxrender/voxsimplify.py`),
/// menor e com uma fracao dos triangulos, porque ha dezenas deles.
fn model_for(tag: shared::EntityTag, boss: bool, kind: u16) -> Option<&'static str> {
    use shared::EntityTag as T;
    match tag {
        T::Player | T::Npc => Some("player"),
        T::Enemy if boss => Some("lobo"),
        T::Enemy => Some(modelo_do_mob(kind)),
        // O pet: o bicho da CRIATURA daquele grau, sem o prefixo de pasta —
        // e' o nome do modelo inteiro, usado quando o arquivo em pecas falta.
        T::Pet => shared::pets::de_item(kind)
            .map(|(e, _)| e.bicho.rsplit('/').next().unwrap_or("lobo_pequeno")),
        // O saque vem com o tier do item no `kind` (0 = ouro/pocao).
        T::Loot => Some(match kind {
            1 => "saque_1",
            2 => "saque_2",
            3 => "saque_3",
            4 => "saque_4",
            _ => "saque_0",
        }),
        _ => None,
    }
}

/// O modelo de cada tipo de mob (`enemy_kinds.kind`). Quem MORDE e' bicho,
/// quem ATIRA e' gente — ver docs/PERSONAGEM.md.
pub fn modelo_do_mob(kind: u16) -> &'static str {
    // Human island variants have their own whole model; a beast variant falls
    // back to its species' (its own mesh is in `bicho::modelo_de_kind`).
    if let Some(nome) = rig_de_gente(kind).filter(|_| shared::bestiary::variant(kind).is_some()) {
        return nome.trim_start_matches("humanoides/");
    }
    match shared::bestiary::species_of(kind) {
        1 => "urso",
        2 => "pistoleiro",
        3 => "tigre",
        4 => "mago",
        5 => "owlbear",
        6 => "arqueiro",
        _ => "lobo_pequeno",
    }
}

/// Altura de cada BICHO na tela, em unidades de mundo.
///
/// O arquivo decide a resolucao (quantas faces cabem no orcamento, ver
/// `tools/voxrender/mobs.py`); isto decide o tamanho. Separar os dois e' o que
/// deixa um urso de 16 voxels de altura aparecer maior que o lobo de 19 — e o
/// que impede os dois de desencontrarem: a escala e' calculada no carregamento
/// pela altura do proprio modelo.
///
/// Gente (jogador, pistoleiro, mago, arqueiro) nao entra aqui: ela usa o
/// mesmo `VOXEL` do corpo, porque o rig inteiro depende de 1 voxel = 4 cm.
pub const ALTURA_DO_BICHO: [(&str, f32); 5] = [
    ("lobo_pequeno", 0.9),
    ("urso", 1.3),
    ("tigre", 0.95),
    ("owlbear", 1.5),
    ("lobo", 2.8),
];

/// As cores da faixa do saquinho, por tier do item: as mesmas do cristal da
/// pedra (quem le' a pedra le' o saque), e dourada pra ouro e pocao, que nao
/// tem tier. Quatro tons do claro ao escuro, um por indice 241-244.
pub fn variantes_do_saque() -> Vec<(&'static str, [[u8; 3]; 4])> {
    let rampa = |c: (u8, u8, u8)| {
        let t = |f: f32| {
            [
                (c.0 as f32 * f) as u8,
                (c.1 as f32 * f) as u8,
                (c.2 as f32 * f) as u8,
            ]
        };
        [t(1.0), t(0.82), t(0.66), t(0.5)]
    };
    let cristal = |t: u8| shared::terreno::cristal_do_tier(t).rgb();
    vec![
        ("0", rampa((235, 190, 50))),
        ("1", rampa(cristal(1))),
        ("2", rampa(cristal(2))),
        ("3", rampa(cristal(3))),
        ("4", rampa(cristal(4))),
    ]
}

/// Modelos de gente, desenhados na escala do corpo.
pub const MODELOS_DE_GENTE: [&str; 17] = [
    "player",
    "pistoleiro",
    "mago",
    "arqueiro",
    // Island variants (`RIGS_DE_GENTE`).
    "frost_archer",
    "frost_mage",
    "dune_raider",
    "sand_archer",
    "sun_mage",
    "cliff_archer",
    "storm_mage",
    "gunner_bot",
    "laser_sentry",
    "tesla_unit",
    "drowned_pirate",
    "fishman_harpooner",
    "merfolk_mage",
];

/// Os NPCs da vila, um rig de dez pecas por oficio (`tools/voxrender/npcs.py`).
pub const MODELOS_DE_NPC: [&str; 16] = [
    "npcs/alquimista",
    "npcs/ferreiro",
    "npcs/armeiro",
    "npcs/taberneiro",
    "npcs/alfaiate",
    "npcs/treinador",
    "npcs/identificador",
    "npcs/cartografo",
    "npcs/estivador",
    "npcs/capitao",
    "npcs/mestre_missoes",
    "npcs/mercador",
    "npcs/aldeao_1",
    "npcs/aldeao_2",
    "npcs/aldeao_3",
    "npcs/motorista",
];

/// `Papel::Missoes`: o proximo depois de `Alquimista` no enum do shared.
/// Constante aqui porque o modelo existe antes da variante — o teste
/// `o_papel_de_missoes_e_o_seguinte` amarra os dois.
pub const PAPEL_MISSOES: u8 = shared::construcao::Papel::Alquimista as u8 + 1;

/// Sem arma pra desenhar: nenhum conjunto tem este numero.
const SEM_ARMA: u8 = u8::MAX;

/// O rig do NPC pelo papel que veio no `kind` (`shared::npc_papel_de_kind`).
///
/// Papel sem modelo proprio — morador, naufrago, ou um que o cliente ainda
/// nao conhece — cai num aldeao, escolhido pelo id pra dois vizinhos nao
/// sairem iguais.
pub fn rig_do_npc(papel: u8, id: u64) -> &'static str {
    use shared::construcao::Papel as P;
    const ALDEOES: [&str; 3] = ["npcs/aldeao_1", "npcs/aldeao_2", "npcs/aldeao_3"];
    match papel {
        p if p == P::Alquimista as u8 => "npcs/alquimista",
        p if p == P::Ferreiro as u8 => "npcs/ferreiro",
        p if p == P::Armas as u8 || p == P::Armaduras as u8 => "npcs/armeiro",
        p if p == P::Taberna as u8 => "npcs/taberneiro",
        p if p == P::Alfaiate as u8 => "npcs/alfaiate",
        p if p == P::Treinador as u8 => "npcs/treinador",
        p if p == P::Identificador as u8 => "npcs/identificador",
        p if p == P::Cartografo as u8 => "npcs/cartografo",
        p if p == P::Deposito as u8 => "npcs/estivador",
        p if p == P::Estaleiro as u8 => "npcs/capitao",
        p if p == P::Itens as u8 || p == P::Mercador as u8 => "npcs/mercador",
        PAPEL_MISSOES => "npcs/mestre_missoes",
        p if p == P::Motorista as u8 => "npcs/motorista",
        p if p == P::Submarino as u8 => "npcs/capitao",
        _ => ALDEOES[(id % ALDEOES.len() as u64) as usize],
    }
}

/// A PORÃO's entrance: a PORTAL. It was a stone door until 30/09/2026 — the
/// owner: "instead of dungeun gates to enter i want portals, whit animation and
/// visual effects".
///
/// Drawn with cubes (this runs with the solid material on, where a
/// `draw_line_3d` draws nothing), five in the whole game, one per landmark.
/// `perto` (within opening range) speeds everything up and brightens it: the
/// same "this is interactive" the chest says.
pub fn desenha_porta_do_porao(p: Vec3, conteudo: u16, perto: bool) {
    desenha_portal(p, cor_do_portal(conteudo), perto, get_time() as f32);
}

/// The portal's colour: the Porão's theme (`shared::planta::Tema`), so the
/// portal already says what is on the other side.
pub fn cor_do_portal(conteudo: u16) -> Color {
    use shared::planta::Tema;
    match shared::planta::da(conteudo).map(|p| p.tema) {
        Some(Tema::Caverna) => Color::from_rgba(70, 220, 190, 255),
        Some(Tema::Tijolo) => Color::from_rgba(240, 170, 70, 255),
        Some(Tema::Gelo) => Color::from_rgba(120, 200, 255, 255),
        Some(Tema::Arenito) => Color::from_rgba(255, 140, 60, 255),
        Some(Tema::Castelo) => Color::from_rgba(170, 110, 255, 255),
        None => Color::from_rgba(200, 160, 255, 255),
    }
}

/// A stone platform, a standing ring of stone with runes lighting up in turn,
/// a swirling core of light, and sparks rising around it.
pub fn desenha_portal(p: Vec3, cor: Color, perto: bool, t: f32) {
    use std::f32::consts::TAU;
    let pedra = Color::from_rgba(92, 94, 106, 255);
    let pedra_escura = Color::from_rgba(62, 64, 74, 255);
    let (vel, brilho) = if perto { (2.4, 1.0) } else { (1.0, 0.7) };
    let com = |a: f32| Color::new(cor.r, cor.g, cor.b, a);
    // The platform: a ring of flagstones and a step.
    draw_cube(p + vec3(0.0, 0.1, 0.0), vec3(3.6, 0.2, 1.6), None, pedra_escura);
    for k in 0..14 {
        let a = k as f32 / 14.0 * TAU;
        let q = p + vec3(a.cos() * 1.9, 0.14, a.sin() * 0.95);
        draw_cube(q, vec3(0.62, 0.28, 0.5), None, if k % 2 == 0 { pedra } else { pedra_escura });
    }
    // The glow pooled on the platform.
    let pulso = 0.5 + 0.5 * (t * 2.0 * vel).sin();
    draw_cube(p + vec3(0.0, 0.25, 0.0), vec3(2.6, 0.03, 1.0), None, com(0.25 + 0.2 * pulso * brilho));
    // The standing ring, facing the camera (in the XY plane): stone blocks,
    // every third a rune that lights up as the light runs round.
    let centro = p + vec3(0.0, 1.85, 0.0);
    let n = 22;
    for k in 0..n {
        let a = k as f32 / n as f32 * TAU;
        let q = centro + vec3(a.cos() * 1.45, a.sin() * 1.45, 0.0);
        let runa = k % 3 == 0;
        let aceso = runa && (t * 1.6 * vel - a / TAU * 3.0).rem_euclid(1.0) < 0.25;
        let c = if aceso {
            com(1.0)
        } else if runa {
            Color::new(cor.r * 0.45, cor.g * 0.45, cor.b * 0.45, 1.0)
        } else {
            pedra
        };
        draw_cube(q, vec3(0.44, 0.44, 0.5), None, c);
    }
    // The core: layers of light, and a spiral of motes turning inward.
    for k in 0..4 {
        let r = 1.15 - k as f32 * 0.26;
        let a = (0.10 + 0.07 * k as f32) * brilho * (0.8 + 0.2 * pulso);
        draw_cube(centro, vec3(r * 2.0, r * 2.0, 0.04 + k as f32 * 0.02), None, com(a));
    }
    let motes = if perto { 36 } else { 24 };
    for k in 0..motes {
        let f = k as f32 / motes as f32;
        let fase = (f + t * 0.35 * vel).rem_euclid(1.0);
        let raio = 1.2 * (1.0 - fase);
        let a = f * TAU * 3.0 + t * 2.2 * vel + fase * 4.0;
        let q = centro + vec3(a.cos() * raio, a.sin() * raio, 0.06);
        let lado = 0.07 + 0.1 * (1.0 - fase);
        let branco = fase * 0.6;
        draw_cube(
            q,
            vec3(lado, lado, lado),
            None,
            Color::new(
                cor.r + (1.0 - cor.r) * branco,
                cor.g + (1.0 - cor.g) * branco,
                cor.b + (1.0 - cor.b) * branco,
                0.9 * brilho,
            ),
        );
    }
    // Sparks rising round the platform.
    let faiscas = if perto { 16 } else { 10 };
    for k in 0..faiscas {
        let f = k as f32 / faiscas as f32;
        let subida = (t * 0.45 * vel + f * 1.7).rem_euclid(1.0);
        let a = f * TAU + t * 0.5;
        let q = p + vec3(a.cos() * 1.8, 0.3 + subida * 3.4, a.sin() * 0.8);
        let lado = 0.09 * (1.0 - subida) + 0.03;
        draw_cube(q, vec3(lado, lado, lado), None, com((1.0 - subida) * brilho));
    }
}

/// ENTERING a portal: light spiralling up around the character, faster and
/// tighter as `progresso` (0..1) runs out, before the trip.
pub fn desenha_entrada_no_portal(p: Vec3, cor: Color, progresso: f32, t: f32) {
    use std::f32::consts::TAU;
    let n = 40;
    for k in 0..n {
        let f = k as f32 / n as f32;
        let subida = (f + t * (0.8 + 2.0 * progresso)).rem_euclid(1.0);
        let raio = 1.4 * (1.0 - progresso * 0.8) * (1.0 - subida * 0.4);
        let a = f * TAU * 4.0 + t * (4.0 + 10.0 * progresso);
        let q = p + vec3(a.cos() * raio, subida * 2.6, a.sin() * raio);
        let lado = 0.08 + 0.08 * progresso;
        let branco = progresso * 0.7;
        draw_cube(
            q,
            vec3(lado, lado, lado),
            None,
            Color::new(
                cor.r + (1.0 - cor.r) * branco,
                cor.g + (1.0 - cor.g) * branco,
                cor.b + (1.0 - cor.b) * branco,
                0.9,
            ),
        );
    }
    // A column of light closing in on the body.
    let a = 0.15 + 0.5 * progresso;
    let lado = 1.6 * (1.0 - progresso * 0.7);
    draw_cube(p + vec3(0.0, 1.3, 0.0), vec3(lado, 2.6, lado), None, Color::new(cor.r, cor.g, cor.b, a));
}

/// O bau da dungeon: madeira, faixas de ouro e um anel que pulsa no chao.
fn desenha_bau(p: Vec3) {
    desenha_bau_de_cor(p, None);
}

/// The chest, with its bands and glow in the color of a Stormkeep chest
/// (`shared::forte`, 2 green .. 4 purple); `None` = the dungeon's gold one.
fn desenha_bau_de_cor(p: Vec3, cor: Option<u8>) {
    let t = get_time() as f32;
    let madeira = Color::from_rgba(132, 86, 44, 255);
    let escura = Color::from_rgba(92, 56, 28, 255);
    let metal = match cor {
        Some(2) => Color::from_rgba(96, 200, 96, 255),
        Some(3) => Color::from_rgba(80, 150, 240, 255),
        Some(4) => Color::from_rgba(186, 104, 240, 255),
        _ => Color::from_rgba(236, 190, 84, 255),
    };
    // Every piece sits ON or OUTSIDE the box's faces — a band drawn inside
    // the wood z-fights and reads as broken (the owner: "the chest visual is
    // bugged").
    const L: f32 = 1.2; // width (x)
    const P: f32 = 0.8; // depth (z)
    const A: f32 = 0.7; // body height
    const E: f32 = 0.03; // how far a band stands out
    draw_cube(p + vec3(0.0, A * 0.5, 0.0), vec3(L, A, P), None, madeira);
    // The lid: a wider slab on top, then a narrower crown.
    draw_cube(p + vec3(0.0, A + 0.08, 0.0), vec3(L + 0.08, 0.16, P + 0.08), None, escura);
    draw_cube(p + vec3(0.0, A + 0.22, 0.0), vec3(L - 0.1, 0.12, P - 0.16), None, escura);
    // Two bands over the whole chest, standing out of every face.
    for dx in [-0.38f32, 0.38] {
        draw_cube(p + vec3(dx, A * 0.5, 0.0), vec3(0.12, A + E, P + 2.0 * E), None, metal);
        draw_cube(p + vec3(dx, A + 0.08, 0.0), vec3(0.12, 0.16 + 2.0 * E, P + 0.08 + 2.0 * E), None, metal);
        draw_cube(p + vec3(dx, A + 0.22, 0.0), vec3(0.12, 0.12 + 2.0 * E, P - 0.16 + 2.0 * E), None, metal);
    }
    // The lock plate, on the front face.
    draw_cube(p + vec3(0.0, A - 0.1, P * 0.5 + E), vec3(0.22, 0.26, 0.04), None, metal);
    draw_cube(p + vec3(0.0, A - 0.16, P * 0.5 + 2.0 * E), vec3(0.08, 0.1, 0.03), None, escura);
    // The ring on the ground and a faint beam: seen from across the arena.
    let a = 0.55 + 0.35 * (t * 3.0).sin();
    draw_ring(p, 1.0 + 0.08 * (t * 3.0).sin(), Color::new(metal.r, metal.g, metal.b, a));
    if cor.is_some() {
        draw_cube(
            p + vec3(0.0, 2.6, 0.0),
            vec3(0.18, 3.6, 0.18),
            None,
            Color::new(metal.r, metal.g, metal.b, 0.22 + 0.1 * (t * 2.0).sin()),
        );
    }
}

/// Escala de desenho do corpo: 1 pro bicho comum, o fator do CHEFE pra ele, e
/// a escala da criatura pro PET.
///
/// O pet precisa entrar aqui porque a altura do modelo e' assada no
/// carregamento (`vox::load_bicho`), uma vez por ARQUIVO — e o mesmo arquivo
/// serve o pet e a montaria. Sem este fator, o filhote de dragao desenhava
/// com os 2,4 do dragao adulto, do mesmo tamanho da montaria; a montaria
/// escapava porque `desenha_bicho_montaria` sempre recebeu a escala de fora.
pub(crate) fn escala_de_chefe(e: &crate::world::Ent) -> f32 {
    if e.meta.tag == shared::EntityTag::Enemy && e.state.flags & shared::ent_flags::BOSS != 0 {
        shared::bosses::chefe(e.meta.kind).map_or(1.0, |c| c.escala)
            * crate::bicho::fator_do_modelo_de_chefe(e.meta.kind)
    } else if e.meta.tag == shared::EntityTag::Pet {
        shared::pets::de_item(e.meta.kind).map_or(1.0, |(c, _)| c.escala)
    } else {
        1.0
    }
}

/// Altura do corpo na tela (u), pra sombra, aura, placa e poeira.
pub(crate) fn altura_de_chefe(e: &crate::world::Ent) -> f32 {
    crate::bicho::do_mob(e.meta.tag, e.meta.kind, true)
        .or_else(|| crate::bicho::do_pet(e.meta.tag, e.meta.kind))
        .map_or(1.95, |(_, a)| a)
        * if e.meta.tag == shared::EntityTag::Pet {
            // `do_pet` ja' devolve a altura COM a escala: nao multiplicar de
            // novo, senao a sombra encolhe ao quadrado.
            1.0
        } else {
            escala_de_chefe(e)
        }
}

/// O tombo do chefe: o dobro do tempo do tombo comum e o mesmo quique — o
/// corpo grande cai pesado, e a poeira sai quando ele bate (`chefe_anim`).
fn queda_de_chefe(t: f32) -> f32 {
    queda(t * 0.5)
}

fn rig_do_humanoide(e: &crate::world::Ent) -> Option<&'static str> {
    if e.meta.tag != shared::EntityTag::Enemy {
        return None;
    }
    // Chefe: so' o feito de gente anda neste rig (o kind do corpo preset).
    let chefe = e.state.flags & shared::ent_flags::BOSS != 0;
    if chefe
        && !matches!(
            shared::bosses::chefe(e.meta.kind).map(|c| c.corpo),
            Some(shared::bosses::Corpo::Gente(_))
        )
    {
        return None;
    }
    rig_de_gente(shared::bosses::kind_do_corpo(e.meta.kind))
}

/// The rig file of each human mob (`tools/voxrender/humanoides.py`): the
/// three Morganeers and their island variants (`shared::bestiary`).
pub const RIGS_DE_GENTE: [(u16, &str); 18] = [
    (2, "humanoides/pistoleiro"),
    (4, "humanoides/mago"),
    (6, "humanoides/arqueiro"),
    (30, "humanoides/frost_archer"),
    (31, "humanoides/frost_mage"),
    (33, "humanoides/dune_raider"),
    (34, "humanoides/sand_archer"),
    (35, "humanoides/sun_mage"),
    (37, "humanoides/cliff_archer"),
    (39, "humanoides/storm_mage"),
    (42, "humanoides/seraph_archer"),
    (44, "humanoides/seraph_mage"),
    // Kōgen-tō's robots (`humanoides.py: capacete`).
    (57, "humanoides/gunner_bot"),
    (59, "humanoides/laser_sentry"),
    (61, "humanoides/tesla_unit"),
    (71, "humanoides/drowned_pirate"),
    (73, "humanoides/fishman_harpooner"),
    (75, "humanoides/merfolk_mage"),
];

pub fn rig_de_gente(kind: u16) -> Option<&'static str> {
    RIGS_DE_GENTE.iter().find(|(k, _)| *k == kind).map(|(_, n)| *n)
}

pub fn draw_entities(
    world: &mut World,
    vox: &VoxCache,
    target: Option<shared::EntityId>,
    vista: &Vista,
) {
    draw_entities_com_sombras(
        world,
        vox,
        target,
        vista,
        crate::config_graficos::Sombras::Leves,
    );
}

pub fn draw_entities_com_sombras(
    world: &mut World,
    vox: &VoxCache,
    target: Option<shared::EntityId>,
    vista: &Vista,
    sombras: crate::config_graficos::Sombras,
) {
    let order: Vec<_> = world.draw_order().to_vec();
    if sombras != crate::config_graficos::Sombras::Desligadas {
        desenha_sombras(world, &order, vista, sombras);
    }
    // Os rastros sao transparentes: vao depois de tudo que e' solido, senao
    // o que fosse desenhado atras deles depois nao apareceria atraves.
    let mut rastros = Vec::new();
    let mut brilhos: Vec<Brilho> = Vec::new();
    let self_id = world.self_id;
    for id in order {
        let Some(e) = world.ents.get_mut(&id) else {
            continue;
        };
        let p = vista.pos_de(e);

        // Marca do alvo: anel no chao, que e' como MMO de target sinaliza.
        if Some(id) == target {
            draw_ring(p, 0.55, Color::from_rgba(241, 200, 112, 255));
        }

        let boss = e.state.flags & shared::ent_flags::BOSS != 0;
        // Chefe vivo: sombra larga e aura na cor do elemento.
        if boss && e.meta.tag == shared::EntityTag::Enemy && e.morte.is_none() {
            crate::chefe_anim::presenca(
                e.meta.id.0 as u64,
                e.meta.kind,
                p,
                altura_de_chefe(e),
                get_time() as f32,
                get_frame_time(),
            );
        }
        if let Some(corpo) = rig_do_humanoide(e).and_then(|nome| vox.rig(nome)) {
            let veste = Vestimenta::nua(corpo);
            brilhos.extend(desenha_personagem(e, &veste, vox, vista, false));
            continue;
        }
        // NPC da vila: o rig do OFICIO dele. Sem o arquivo, cai no corpo de
        // gente abaixo, como antes.
        if e.meta.tag == shared::EntityTag::Npc {
            // Bau de conclusao da dungeon: caixa com tampa e fecho, pulsando.
            if shared::npc_papel_de_kind(e.meta.kind) == shared::dungeon::PAPEL_BAU {
                desenha_bau(p);
                continue;
            }
            if let Some(cor) = shared::forte::cor_do_papel(shared::npc_papel_de_kind(e.meta.kind)) {
                desenha_bau_de_cor(p, Some(cor));
                continue;
            }
            let nome = rig_do_npc(shared::npc_papel_de_kind(e.meta.kind), e.meta.id.0 as u64);
            if let Some(corpo) = vox.rig(nome) {
                let veste = Vestimenta::nua(corpo);
                brilhos.extend(desenha_personagem(e, &veste, vox, vista, false));
                continue;
            }
        }
        // Chefe pirata: o corpo do personagem com o chapeu, na escala de chefe.
        if boss
            && shared::bosses::chefe(e.meta.kind)
                .is_some_and(|c| c.corpo == shared::bosses::Corpo::Pirata)
        {
            if let Some(corpo) = vox.rig(RIG_CORPO) {
                let mut veste = Vestimenta::nua(corpo);
                veste.cabelo = vox.rig(RIG_CHAPEU);
                brilhos.extend(desenha_personagem(e, &veste, vox, vista, false));
                continue;
            }
        }
        // Gente (jogador, NPC) e' desenhada em PECAS, com a pose do quadro.
        if matches!(
            e.meta.tag,
            shared::EntityTag::Player | shared::EntityTag::Npc
        ) {
            if let Some(corpo) = vox.rig(RIG_CORPO) {
                // A APARÊNCIA veio na meta (`EntityMeta::aparencia`). Zero é
                // o corpo de sempre, que é o que NPC manda.
                let mut veste = vestimenta_de(vox, e.meta.aparencia).unwrap_or_else(|| {
                    let mut v = Vestimenta::nua(corpo);
                    v.cabelo = vox.rig(RIG_CHAPEU);
                    v
                });
                veste.skins = e.meta.skins;
                brilhos.extend(desenha_personagem(
                    e,
                    &veste,
                    vox,
                    vista,
                    self_id == Some(id),
                ));
                continue;
            }
        }
        // Bicho de quatro patas anda em PECAS (`bicho.rs`). Sem o arquivo,
        // cai no modelo inteiro abaixo, parado.
        if let Some((arquivo, _)) = crate::bicho::do_mob(e.meta.tag, e.meta.kind, boss)
            .or_else(|| crate::bicho::do_pet(e.meta.tag, e.meta.kind))
        {
            if let Some(b) = vox.bicho(arquivo) {
                if let Some(r) = desenha_bicho(e, b, vista) {
                    rastros.push(r);
                }
                continue;
            }
        }
        if e.meta.tag == shared::EntityTag::Projectile {
            desenha_projetil(e, p);
            continue;
        }
        let drawn = model_for(e.meta.tag, boss, e.meta.kind)
            .and_then(|name| vox.peek(name))
            .map(|meshes| {
                // A malha nasce centrada em X/Z e apoiada em Y=0; girar em
                // torno de Y bastaria, mas a macroquad nao transforma malha —
                // entao a rotacao vira quando houver malha por direcao.
                // Saque nao anda nem vira: sem isto todo saquinho do chao
                // olharia pro mesmo lado, e o monte leria como carimbo.
                let yaw = if e.meta.tag == shared::EntityTag::Loot {
                    (e.meta.id.0 as f32 * 2.399).rem_euclid(std::f32::consts::TAU)
                } else {
                    e.yaw
                };
                for m in meshes {
                    draw_mesh_at(m, p, yaw);
                }
            })
            .is_some();

        if !drawn {
            use shared::EntityTag as T;
            let c = match e.meta.tag {
                T::Enemy => Color::from_rgba(200, 85, 61, 255),
                T::Projectile => Color::from_rgba(255, 240, 150, 255),
                T::Loot => Color::from_rgba(150, 220, 120, 255),
                _ => Color::from_rgba(160, 160, 170, 255),
            };
            draw_cube(p + vec3(0.0, 0.35, 0.0), vec3(0.6, 0.7, 0.6), None, c);
        }
    }
    for (base, r) in &rastros {
        desenha_rastro(base, r);
    }
    let agora = get_time() as f32;
    for b in &brilhos {
        match b {
            Brilho::Fita(f) => desenha_fita(f, agora),
            Brilho::Clarao(p, u) => desenha_clarao(*p, *u),
            Brilho::Circulo(pulso, t, tema) => desenha_circulo(*pulso, *t, tema),
            Brilho::Mao(p, forca, t, tema) => desenha_mao(*p, *forca, *t, tema),
        }
    }
    // Lascas e faiscas da coleta: um pool so', avancado uma vez por quadro.
    crate::lascas::avanca_e_desenha(get_frame_time());
    crate::auras::desenha();
}

/// Sombras de contato baratas: um disco suave por corpo, em uma malha por lote.
/// A borda consulta o relevo para acompanhar encostas sem textura nem luz extra.
fn desenha_sombras(
    world: &World,
    order: &[shared::EntityId],
    vista: &Vista,
    modo: crate::config_graficos::Sombras,
) {
    const LADOS: usize = 12;
    const LIMITE: usize = 1800;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for id in order {
        let Some(e) = world.ents.get(id) else {
            continue;
        };
        if e.morte.is_some()
            || !matches!(
                e.meta.tag,
                shared::EntityTag::Player
                    | shared::EntityTag::Enemy
                    | shared::EntityTag::Npc
                    | shared::EntityTag::Pet
            )
        {
            continue;
        }
        let escala = escala_de_chefe(e).clamp(0.5, 4.0);
        let raio = if e.meta.tag == shared::EntityTag::Enemy {
            0.72
        } else {
            0.55
        } * escala;
        let lobos: &[(f32, f32, f32, f32, u8)] =
            if modo == crate::config_graficos::Sombras::Bonitas {
                &[(0.0, 0.0, 1.0, 0.72, 80), (0.8, -0.46, 1.55, 0.92, 65)]
            } else {
                &[(0.12, 0.08, 1.0, 0.72, 72)]
            };
        for &(dx, dz, rx, rz, alfa) in lobos {
            let x = e.render_pos.x + dx * escala;
            let z = e.render_pos.y + dz * escala;
            let chao = vista.chao_em(e.render_pos.x, e.render_pos.y);
            let centro_chao = vista.chao_em(x, z);
            // A projecao deslocada da sombra nunca atravessa um barranco.
            if (centro_chao - chao).abs() > 0.6 {
                continue;
            }
            if vertices.len() + LADOS + 1 > LIMITE {
                draw_mesh(&Mesh {
                    vertices: std::mem::take(&mut vertices),
                    indices: std::mem::take(&mut indices),
                    texture: None,
                });
            }
            let base = vertices.len() as u16;
            vertices.push(vtx(vec3(x, centro_chao + 0.035, z), [13, 17, 25, alfa]));
            let mut alturas = [0.0; LADOS];
            for i in 0..LADOS {
                let a = i as f32 * std::f32::consts::TAU / LADOS as f32;
                let px = x + a.cos() * raio * rx;
                let pz = z + a.sin() * raio * rz;
                alturas[i] = vista.chao_em(px, pz);
                vertices.push(vtx(vec3(px, alturas[i] + 0.035, pz), [13, 17, 25, 0]));
            }
            for i in 0..LADOS {
                let proximo = (i + 1) % LADOS;
                if (alturas[i] - centro_chao).abs() <= 0.6
                    && (alturas[proximo] - centro_chao).abs() <= 0.6
                {
                    indices.extend_from_slice(&[
                        base,
                        base + 1 + proximo as u16,
                        base + 1 + i as u16,
                    ]);
                }
            }
        }
    }
    if !vertices.is_empty() {
        draw_mesh(&Mesh {
            vertices,
            indices,
            texture: None,
        });
    }
}

/// Os arquivos de pecas do personagem (docs/character create.md).
pub const RIG_CORPO: &str = "personagem/corpo";
pub const RIG_CHAPEU: &str = "personagem/cabelo_01";

/// Os rostos, os cabelos e os chapeus (tools/voxrender/personagem.py).
///
/// Vao todos no boot: sao 14 arquivos pequenos, contra os 19 rigs que o boot
/// ja' carrega. So' skin de ROUPA e' que carrega preguiçosa — ela e' o eixo
/// que cresce com o catalogo.
/// TODO índice que o seletor de cabelo oferece — cabelos e chapéus.
///
/// Existe pra o BOOT e o SELETOR lerem a mesma coisa. Quando o boot tinha o
/// seu próprio `0..CABELOS` escrito à mão, os chapéus (que ficam acima de
/// `CABELOS`) nunca eram carregados: o arquivo existia, passava no teste de
/// contrato, e mesmo assim a cabeça saía pelada — porque `vox.rig` devolvia
/// `None` e o slot ficava vazio.
pub fn rigs_da_cabeca() -> Vec<String> {
    let total = shared::aparencia::CABELOS + 1 + shared::aparencia::CHAPEUS.len() as u8;
    (0..shared::aparencia::ROSTOS)
        .map(rig_do_rosto)
        .chain((0..total).filter_map(rig_do_cabelo))
        .collect()
}

pub fn rig_do_rosto(i: u8) -> String {
    format!(
        "personagem/rostos/rosto_{:02}",
        i.min(shared::aparencia::ROSTOS - 1) + 1
    )
}

pub fn rig_do_cabelo(i: u8) -> Option<String> {
    // Acima dos cabelos vem o "no hair" e depois os CHAPEUS: os tres
    // dividem a junta da cabeca, entao dividem o campo.
    if let Some(id) = shared::aparencia::chapeu_do_cabelo(i) {
        let arq = shared::aparencia::CHAPEUS
            .get((id - shared::aparencia::CHAPEU_BASE) as usize)?
            .0;
        return Some(format!("personagem/chapeus/{arq}"));
    }
    if i >= shared::aparencia::CABELOS {
        return None;
    }
    Some(format!("personagem/cabelos/cabelo_{:02}", i + 1))
}

/// O rig de uma skin de ROUPA.
pub fn rig_da_roupa(id: u16) -> Option<String> {
    shared::aparencia::arquivo_da_roupa(id).map(|a| format!("personagem/skins/{a}"))
}

/// A VESTIMENTA de uma entidade, a partir do `u32` da meta.
pub fn vestimenta_de<'a>(vox: &'a VoxCache, aparencia: u32) -> Option<Vestimenta<'a>> {
    let a = shared::aparencia::Aparencia::desempacota(aparencia).saneada();
    let mut v = Vestimenta::nua(vox.rig(RIG_CORPO)?);
    v.rosto = vox.rig(&rig_do_rosto(a.rosto));
    v.cabelo = rig_do_cabelo(a.cabelo).and_then(|n| vox.rig(&n));
    // A ROUPA e' o eixo que cresce com o catalogo, entao ela e' a unica que
    // carrega preguicosa: `rig_ou_pede` anota a falta e devolve `None`, o
    // laco principal atende UM por quadro, e ate' la' aparece o corpo padrao.
    v.roupa = rig_da_roupa(a.roupa).and_then(|n| vox.rig_ou_pede(&n));
    v.pele = Some(cores_da_pele(a.pele));
    v.cor_cabelo = Some(cores_do_cabelo(a.cor_cabelo));
    Some(v)
}

/// As rampas de `tools/voxrender/npcs.py` (`PELES` e `CABELOS`), como
/// `(claro, escuro)` em 0..1. Os dois extremos bastam: o shader interpola, e
/// a arte de hoje usa 3 dos 4 degraus da pele e 2 dos 4 do cabelo.
pub fn cores_da_pele(i: u8) -> [[f32; 3]; 2] {
    const R: [[[u8; 3]; 2]; 4] = [
        [[252, 222, 192], [180, 132, 104]], // clara
        [[226, 184, 140], [140, 98, 70]],   // media
        [[176, 124, 86], [94, 60, 40]],     // morena
        [[126, 88, 62], [62, 40, 26]],      // escura
    ];
    em_fracao(R[(i as usize).min(R.len() - 1)])
}

pub fn cores_do_cabelo(i: u8) -> [[f32; 3]; 2] {
    const R: [[[u8; 3]; 2]; 6] = [
        [[122, 86, 54], [54, 36, 22]],      // castanho
        [[58, 52, 50], [20, 18, 17]],       // preto
        [[196, 104, 52], [104, 48, 22]],    // ruivo
        [[236, 206, 128], [150, 118, 56]],  // loiro
        [[200, 200, 204], [110, 110, 118]], // grisalho
        [[244, 244, 246], [168, 168, 176]], // branco
    ];
    em_fracao(R[(i as usize).min(R.len() - 1)])
}

fn em_fracao(p: [[u8; 3]; 2]) -> [[f32; 3]; 2] {
    p.map(|c| c.map(|v| v as f32 / 255.0))
}

/// O tombo de quem morre: vai a 90 graus acelerando, como quem cai de
/// verdade, quica um pouco no chao e fica.
fn queda(t: f32) -> f32 {
    let u = (t / 0.5).clamp(0.0, 1.0);
    let caindo = std::f32::consts::FRAC_PI_2 * u * u;
    let quique = if t > 0.5 {
        -0.12 * (-(t - 0.5) * 7.0).exp() * ((t - 0.5) * 22.0).sin().abs()
    } else {
        0.0
    };
    caindo + quique
}

/// Quanto o rastro da lamina dura.
const VIDA_DO_RASTRO: f32 = 0.16;

/// O clarao do golpe: o modelo pisca na hora do acerto — branco no bicho,
/// vermelho em gente. E' o quadro que diz "pegou".
fn clarao(e: &crate::world::Ent, eu: bool) -> Option<([f32; 3], f32)> {
    let t = e.ferido?;
    if t > 0.14 {
        return None;
    }
    let k = (1.0 - t / 0.14) * 0.8;
    let cor = if e.meta.tag == shared::EntityTag::Enemy {
        [1.0, 1.0, 1.0]
    } else if eu {
        [1.0, 0.25, 0.2]
    } else {
        [1.0, 0.55, 0.45]
    };
    Some((cor, k))
}

/// O personagem em pecas: pose do quadro, a inercia das molas por cima, uma
/// matriz por peca. Devolve o rastro da lamina, se ele esta' cortando.
fn desenha_personagem(
    e: &mut crate::world::Ent,
    veste: &Vestimenta<'_>,
    vox: &VoxCache,
    vista: &Vista,
    eu: bool,
) -> Vec<Brilho> {
    let p = vista.pos_de(e);
    let (sin, cos) = e.yaw.sin_cos();
    // Quanto o chao sob cada pe' esta' acima da base do corpo, em voxels. O pe'
    // direito fica 2 voxels pro -X do rig (a direita do boneco), o esquerdo
    // pro +X; girado pela direcao da entidade.
    let voando = e.voando;
    let degrau = |dx: f32| -> f32 {
        if voando {
            return 0.0;
        }
        let ox = dx * VOXEL;
        let (x, z) = (p.x + ox * cos, p.z - ox * sin);
        (vista.chao_em(x, z) - p.y) / VOXEL
    };
    let pes = [degrau(-2.0), degrau(2.0)];
    // o golpe empurra pra LONGE de quem bateu: do mundo pro espaco do boneco
    let recuo = Quat::from_rotation_y(-e.yaw) * vec3(-e.golpe_de.x, 0.0, -e.golpe_de.y);
    let entrada = crate::rig::Entrada {
        fase: e.fase,
        andar: e.andar,
        correr: e.correr,
        tempo: get_time() as f32,
        ar: e.ar,
        degrau: pes,
        combate: crate::rig::Combate {
            conjunto: shared::components::acao::conjunto(e.state.acao),
            sacada: e.sacada,
            golpe: e.combo,
            golpe_ant: e.combo_ant,
            skill: e.skill,
            ferido: e.ferido,
            recuo,
            coleta: e
                .coleta
                .filter(|_| e.morte.is_none())
                .map(|t| (t, e.coleta_t)),
        },
    };
    let mut entrada = entrada;
    let dash = e.morte.is_none()
        && (e.dash_visual_ate > get_time() || e.state.flags & shared::ent_flags::DASHING != 0);
    if dash {
        entrada.combate.golpe = None;
        entrada.combate.golpe_ant = None;
        entrada.combate.skill = None;
        entrada.combate.ferido = None;
        entrada.combate.coleta = None;
        entrada.ar = 0.0;
    }
    // Montado (docs/MONTARIAS.md): o bicho da montaria EQUIPADA por baixo e o
    // cavaleiro sentado na sela, sem passada propria. O `kind` da meta e' o
    // item_id da montaria: especie e cor saem dele.
    let montaria = if e.meta.tag == shared::EntityTag::Player
        && e.state.flags & shared::ent_flags::MONTADO != 0
        && e.morte.is_none()
    {
        shared::montarias::de_item(e.meta.kind)
            .and_then(|(esp, grau)| bicho_da_montaria(vox, esp, e.meta.skins).map(|b| (esp, grau, b)))
    } else {
        None
    };
    if montaria.is_some() {
        entrada.andar = 0.0;
        entrada.correr = 0.0;
        entrada.ar = 0.0;
        entrada.degrau = [0.0; 2];
        entrada.combate.coleta = None;
    }
    let humanoide = rig_do_humanoide(e).is_some();
    if humanoide {
        entrada.combate.conjunto = if shared::bestiary::species_of(shared::bosses::kind_do_corpo(e.meta.kind)) == 4 {
            3
        } else {
            2
        };
        entrada.combate.sacada = 1.0;
        entrada.combate.golpe = e.ataque_mob.map(|(_, t, impacto)| {
            let relogio = if t <= impacto {
                t / impacto.max(0.01) * crate::rig::IMPACTO
            } else {
                crate::rig::IMPACTO + t - impacto
            };
            (0, relogio)
        });
    }
    // NPC nao anda armado: o `acao` dele e' zero, que e' o conjunto de espada
    // e escudo, e ele saia com a espada no quadril e o escudo nas costas. O que
    // ele segura (martelo, caneca, livro) ja' vem no modelo.
    if e.meta.tag == shared::EntityTag::Npc {
        entrada.combate.conjunto = SEM_ARMA;
    }
    // Chefe de gente com golpe telegrafado: o braco arma na carga e completa
    // no impacto; o corpo inteiro ganha o ajuste de `chefe_anim` la' embaixo.
    let chefe =
        e.meta.tag == shared::EntityTag::Enemy && e.state.flags & shared::ent_flags::BOSS != 0;
    let agora = get_time();
    let carga = e.carga_chefe.filter(|_| chefe && e.morte.is_none());
    let ajuste = carga.and_then(|c| crate::chefe_anim::ajuste(&c, agora));
    if let (Some(c), Some(_)) = (carga, ajuste) {
        entrada.combate.golpe = Some((
            0,
            crate::chefe_anim::relogio_do_braco(&c, agora, crate::rig::IMPACTO),
        ));
    }
    // Morto: sem passo, sem golpe, sem tranco — so' o tombo.
    let cai = e
        .morte
        .map_or(0.0, |t| if chefe { queda_de_chefe(t) } else { queda(t) });
    if e.morte.is_some() {
        entrada.andar = 0.0;
        entrada.correr = 0.0;
        entrada.ar = 0.0;
        entrada.combate.golpe = None;
        entrada.combate.skill = None;
        entrada.combate.ferido = None;
    }
    let mut pose = crate::rig::pose(&entrada);
    if montaria.is_some() {
        crate::rig::aplica_montado(&mut pose, entrada.tempo,
            entrada.combate.sacada > 0.0 || entrada.combate.golpe.is_some());
    }
    if e.skill.is_some_and(|(id, _, _)| id == 9) {
        pose.na_mao = false;
    }
    if humanoide && shared::bestiary::species_of(e.meta.kind) == 6 {
        crate::rig::aplica_arqueiro(&mut pose, entrada.combate.golpe.map(|(_, t)| t));
    }
    e.molas.segue(&mut pose, get_frame_time());
    if dash {
        crate::rig::aplica_dash(&mut pose);
        let frente = vec3(sin, 0.0, cos);
        let lado = vec3(cos, 0.0, -sin);
        for i in [-1.0, 1.0] {
            let inicio = p + vec3(0.0, 0.65, 0.0) + lado * (i * 0.28);
            draw_line_3d(
                inicio,
                inicio - frente * 1.1,
                Color::new(0.55, 0.85, 1.0, 0.45),
            );
        }
    }
    let s = if e.morte.is_some() {
        0.0
    } else {
        e.ferido.map_or(0.0, crate::rig::esmagamento)
    };
    // Cai de COSTAS girando em volta do pe': deitado, as costas ficariam 3
    // voxels abaixo do chao, entao o corpo sobe isso junto com o tombo.
    let sobe = cai / std::f32::consts::FRAC_PI_2 * 3.5 * VOXEL;
    let esc = escala_de_chefe(e);
    let (desloca, giro, inclina, agacha) = match (ajuste, carga) {
        (Some(a), Some(c)) => {
            let alt = 1.95 * esc;
            let tz = crate::chefe_anim::tremor_xz(&a, agora as f32, alt);
            (
                vec3(
                    c.dir.x * a.desloca + tz.x,
                    a.voa * alt + a.sobe * alt * 0.5,
                    c.dir.y * a.desloca + tz.y,
                ),
                a.giro,
                a.pitch * 0.6,
                a.agacha,
            )
        }
        _ => (Vec3::ZERO, 0.0, 0.0, 0.0),
    };
    let s = (s + agacha).min(0.45);
    if chefe {
        crate::chefe_anim::poeira_da_queda(e.morte, get_frame_time(), p, 1.95 * esc, e.meta.kind);
    }
    let sela = match montaria {
        Some((m, s, b)) => {
            let matriz = desenha_montaria(e, m, s, b, p);
            crate::auras::animal(e.meta.auras, shared::auras::MONTARIA, matriz, b, m.bicho,
                if eu { 0.0 } else { p.distance(vista.cam.target) }, e.meta.id.0);
            let frente = vec3(e.yaw.sin(), 0.0, e.yaw.cos());
            // A sela e' o LOMBO DO MODELO vezes a escala da especie, e nao um
            // numero escrito na tabela: numero a mao nao acompanha a escala, e
            // foi assim que o cavaleiro do cervo acabou boiando na altura da
            // cabeca do bicho (`vox::lombo_medido`).
            let sela = b.anat.lombo * m.escala;
            Vec3::Y * (sela - crate::rig::altura_do_quadril(VOXEL)) + frente * m.sela_frente
        }
        None => Vec3::ZERO,
    };
    let base = Mat4::from_translation(p + vec3(0.0, sobe, 0.0) + desloca + sela)
        * Mat4::from_scale(Vec3::splat(esc))
        * Mat4::from_rotation_y(e.yaw + giro)
        * Mat4::from_rotation_x(-cai + inclina)
        * Mat4::from_scale(vec3(1.0 + 0.5 * s, 1.0 - s, 1.0 + 0.5 * s));
    let (mats, armas) = desenha_rig(base, &pose, veste, vox, clarao(e, eu));
    if e.meta.tag == shared::EntityTag::Player && e.morte.is_none() {
        crate::auras::personagem(e.meta.auras, &mats, &armas, vox,
            if eu { 0.0 } else { p.distance(vista.cam.target) }, e.meta.id.0 as u32,
            pose.ferramenta.is_some(), e.combo.is_some() || e.skill.is_some());
    }
    e.emissores = crate::rig::palmas(&mats, VOXEL);
    // Coleta: a rajada de lascas no quadro em que a cabeca da ferramenta bate.
    if let Some((tipo, _)) = entrada.combate.coleta {
        let u = (e.coleta_t.max(0.0) / crate::rig::PERIODO_DA_COLETA).fract();
        if crate::lascas::cruzou(e.coleta_u_ant, u, crate::rig::fase_do_impacto(tipo)) {
            let nome = crate::rig::ferramenta_de(tipo);
            if let Some((_, m)) = armas.iter().find(|(n, _)| *n == nome) {
                let ponto = m.transform_point3(crate::rig::cabeca_da_ferramenta(tipo) * VOXEL);
                let semente = (e.meta.id.0 as u32).wrapping_mul(31) ^ (e.coleta_t * 997.0) as u32;
                crate::lascas::impacto(ponto, tipo, semente);
            }
        }
        e.coleta_u_ant = u;
    } else {
        e.coleta_u_ant = 0.0;
    }
    if crate::rig::e_pistolas(&pose) && pose.na_mao {
        for (i, (_, m)) in armas
            .iter()
            .filter(|(n, _)| *n == "pistola")
            .take(2)
            .enumerate()
        {
            e.emissores[i] = m.transform_point3(vec3(0.0, 2.5 * VOXEL, 9.5 * VOXEL));
        }
    }
    if humanoide && shared::bestiary::species_of(e.meta.kind) == 6 {
        let maos = crate::rig::palmas(&mats, VOXEL);
        let centro = maos[1];
        let frente = vec3(e.yaw.sin(), 0.0, e.yaw.cos());
        let ponta = |u: f32| centro + Vec3::Y * (u * 0.65) + frente * ((1.0 - u * u) * 0.18);
        for k in 0..16 {
            let a = ponta(k as f32 / 8.0 - 1.0);
            let b = ponta((k + 1) as f32 / 8.0 - 1.0);
            draw_cube(a.lerp(b, 0.5), vec3(0.055, 0.095, 0.055), None, BROWN);
        }
        let puxada = if e.ataque_mob.is_some() {
            maos[0]
        } else {
            centro
        };
        draw_line_3d(ponta(-1.0), puxada, LIGHTGRAY);
        draw_line_3d(puxada, ponta(1.0), LIGHTGRAY);
        if e.ataque_mob.is_some_and(|(_, t, impacto)| t < impacto) {
            draw_line_3d(puxada, centro + frente * 0.7, BEIGE);
        }
    }
    let mut brilhos = Vec::new();

    // O rastro da lamina: base e ponta a cada quadro enquanto o golpe corre.
    // (onde a lamina comeca e termina, em voxels a partir da pega)
    let agora = get_time() as f32;
    if e.combo.is_some()
        || e.skill
            .is_some_and(|(_, t, impacto)| t > impacto - 0.16 && t < impacto + 0.12)
    {
        let lamina = armas.iter().find_map(|(n, m)| match *n {
            "espada" => Some((*m, 5.0, 22.0)),
            "katana" => Some((*m, 6.0, 26.0)),
            _ => None,
        });
        if let Some((m, de, ate)) = lamina {
            e.rastro.push((
                m.transform_point3(vec3(0.0, 0.0, de * VOXEL)),
                m.transform_point3(vec3(0.0, 0.0, ate * VOXEL)),
                agora,
            ));
        }
    }
    e.rastro.retain(|(_, _, t)| agora - t < VIDA_DO_RASTRO);
    if e.rastro.len() >= 2 {
        brilhos.push(Brilho::Fita(e.rastro.clone()));
    }

    if let Some((passo, t)) = entrada.combate.golpe {
        // Pistolas: o clarao do cano no instante do tiro — da direita no
        // primeiro, da esquerda no segundo, das duas no terceiro.
        if crate::rig::e_pistolas(&pose) {
            let dt = t - crate::rig::IMPACTO;
            if (0.0..0.08).contains(&dt) {
                let pistolas: Vec<Mat4> = armas
                    .iter()
                    .filter(|(n, _)| *n == "pistola")
                    .map(|(_, m)| *m)
                    .collect();
                let quais: &[usize] = match passo {
                    0 => &[0],
                    1 => &[1],
                    _ => &[0, 1],
                };
                for &i in quais {
                    if let Some(m) = pistolas.get(i) {
                        brilhos.push(Brilho::Clarao(
                            m.transform_point3(vec3(0.0, 2.5 * VOXEL, 9.5 * VOXEL)),
                            dt / 0.08,
                        ));
                    }
                }
            }
        }
        // Anel: o circulo no pulso da mao que empurra (as duas no segundo).
        if crate::rig::e_anel(&pose) {
            let pulsos = crate::rig::pulsos(&mats, VOXEL);
            let quais: &[usize] = if passo == 1 { &[0, 1] } else { &[0] };
            for &i in quais {
                brilhos.push(Brilho::Circulo(pulsos[i], t, tema_do_anel(veste.skins)));
            }
        }
    }
    // Anel: as maos BRILHAM enquanto ele esta' em uso — fraco com ele
    // guardado, forte em combate, e um pico no instante do golpe. Anel nao
    // tem modelo (de cima ele e' um voxel): o brilho e' como ele aparece.
    if crate::rig::e_anel(&pose) {
        let pico = e.combo.map_or(0.0, |(_, t)| {
            (1.0 - (t - crate::rig::IMPACTO).abs() / 0.12).max(0.0)
        });
        let forca = 0.55 + 0.45 * e.sacada.clamp(0.0, 1.0) + 0.8 * pico;
        for (i, p) in crate::rig::palmas(&mats, VOXEL).iter().enumerate() {
            brilhos.push(Brilho::Mao(*p, forca, agora + i as f32 * 1.7, tema_do_anel(veste.skins)));
        }
    }
    brilhos
}

/// O brilho do anel numa mao: um nucleo claro, dois halos violeta pulsando
/// e tres faiscas orbitando. O nucleo vai PRIMEIRO: o halo por cima dele o
/// deixa passar; na ordem contraria o halo escreveria profundidade na frente
/// e o nucleo sumiria.
fn desenha_mao(p: Vec3, forca: f32, t: f32, tema: &TemaDoAnel) {
    let pulso = 1.0 + 0.15 * (t * 6.0).sin();
    let alfa = |x: f32| (x * forca.min(1.0)).clamp(0.0, 255.0) as u8;
    let c = |rgb: [u8; 3], a: u8| [rgb[0], rgb[1], rgb[2], a];
    octaedro(p, 0.045 * pulso * forca.max(0.6), c(tema.nucleo, alfa(255.0)));
    octaedro(p, 0.10 * pulso * forca, c(tema.halo, alfa(120.0)));
    octaedro(p, 0.19 * pulso * forca, c(tema.halo_fora, alfa(45.0)));
    for k in 0..3 {
        let ang = t * 4.0 + k as f32 * 2.094;
        let q = p + vec3(
            ang.cos() * 0.16,
            (t * 3.0 + k as f32).sin() * 0.06,
            ang.sin() * 0.16,
        ) * forca.max(0.7);
        octaedro(q, 0.02, c(tema.faisca, alfa(220.0)));
    }
}

/// O que brilha por cima do mundo, transparente, depois de todo mundo: o
/// rastro da lamina, o clarao do cano e o circulo do anel.
pub enum Brilho {
    Fita(Vec<(Vec3, Vec3, f32)>),
    /// Posicao e 0..1 da vida do clarao.
    Clarao(Vec3, f32),
    /// O pulso e o instante do golpe.
    Circulo(Mat4, f32, TemaDoAnel),
    /// O brilho do anel na mao: onde, com que forca e a fase do pulso.
    Mao(Vec3, f32, f32, TemaDoAnel),
}

/// The colours of the magic ring — its skin. The ring has no model (from
/// above it would be one voxel): what shows is the glow on the hands and the
/// circle at the wrist, so a ring skin is those, plus spokes or an inner ring
/// to change the circle's shape and not only its colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TemaDoAnel {
    pub nucleo: [u8; 3],
    pub halo: [u8; 3],
    pub halo_fora: [u8; 3],
    pub faisca: [u8; 3],
    pub circulo: [u8; 3],
    pub onda: [u8; 3],
    /// Spokes across the circle (0 = none).
    pub raios: u8,
    /// A second, smaller ring inside the circle.
    pub interno: bool,
}

/// The default violet, then one theme per ring skin (`SKINS_DE_ARMA`,
/// conjunto 3, by `sufixo`).
pub fn tema_do_anel(skins: u64) -> TemaDoAnel {
    let a = shared::aparencia::Aparencia::default().com_skins(skins);
    let t = |nucleo, halo, halo_fora, faisca, circulo, onda, raios, interno| TemaDoAnel {
        nucleo, halo, halo_fora, faisca, circulo, onda, raios, interno,
    };
    match shared::aparencia::sufixo_da_arma(&a, 3) {
        Some("brasa") => t([255, 246, 220], [255, 150, 60], [230, 70, 30], [255, 210, 120],
            [255, 120, 40], [255, 180, 80], 0, false),
        Some("gelo") => t([240, 252, 255], [140, 220, 255], [80, 170, 240], [220, 246, 255],
            [120, 210, 255], [190, 240, 255], 6, false),
        Some("verdejante") => t([240, 255, 230], [140, 230, 110], [60, 170, 80], [210, 255, 170],
            [110, 220, 100], [180, 250, 150], 0, true),
        Some("solar") => t([255, 255, 235], [255, 220, 90], [240, 170, 40], [255, 240, 170],
            [255, 200, 60], [255, 230, 140], 12, false),
        Some("vazio") => t([70, 24, 100], [200, 60, 255], [90, 20, 160], [255, 120, 255],
            [150, 40, 230], [230, 100, 255], 4, true),
        _ => t([255, 240, 255], [205, 150, 255], [170, 110, 255], [235, 210, 255],
            [190, 130, 255], [215, 170, 255], 0, false),
    }
}

/// Malha transparente das duas faces (o descarte de face de costas esta'
/// ligado e o efeito e' visto dos dois lados).
fn dupla(vertices: Vec<Vertex>, tris: Vec<[u16; 3]>) {
    let mut indices = Vec::with_capacity(tris.len() * 6);
    for [a, b, c] in tris {
        indices.extend_from_slice(&[a, b, c, a, c, b]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

fn vtx(p: Vec3, cor: [u8; 4]) -> Vertex {
    Vertex {
        position: p,
        uv: vec2(0.0, 0.0),
        color: cor,
        normal: Vec4::ZERO,
    }
}

/// O clarao do tiro: tres quadrados cruzados, amarelo quente, que encolhem
/// e somem em 0,08 s.
fn desenha_clarao(p: Vec3, u: f32) {
    let s = (1.0 - u) * 0.20 + 0.04;
    let a = ((1.0 - u) * 235.0) as u8;
    let mut v = Vec::new();
    let mut t = Vec::new();
    for (ea, eb) in [(Vec3::X, Vec3::Y), (Vec3::Y, Vec3::Z), (Vec3::X, Vec3::Z)] {
        let b0 = v.len() as u16;
        for q in [
            p - ea * s - eb * s,
            p + ea * s - eb * s,
            p + ea * s + eb * s,
            p - ea * s + eb * s,
        ] {
            v.push(vtx(q, [255, 232, 160, a]));
        }
        t.push([b0, b0 + 1, b0 + 2]);
        t.push([b0, b0 + 2, b0 + 3]);
    }
    dupla(v, t);
}

/// Uma faixa circular no plano XZ de `m`, a `y` do centro.
fn faixa_circular(m: Mat4, y: f32, raio: f32, largura: f32, cor: [u8; 4]) {
    const N: u16 = 28;
    let mut v = Vec::new();
    let mut t = Vec::new();
    for i in 0..=N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        for r in [raio - largura * 0.5, raio + largura * 0.5] {
            v.push(vtx(
                m.transform_point3(vec3(a.cos() * r, y, a.sin() * r)),
                cor,
            ));
        }
    }
    for i in 0..N {
        let (a0, a1, b0, b1) = (i * 2, i * 2 + 1, i * 2 + 2, i * 2 + 3);
        t.push([a0, a1, b0]);
        t.push([a1, b1, b0]);
    }
    dupla(v, t);
}

/// O anel magico: o circulo aceso em volta do pulso enquanto o golpe arma, e
/// no impacto um segundo circulo SAI da mao pra frente, crescendo e sumindo —
/// o golpe projetado atraves do anel (docs/PERSONAGEM.md).
fn desenha_circulo(pulso: Mat4, t: f32, tema: &TemaDoAnel) {
    let imp = crate::rig::IMPACTO;
    let aceso = if t < imp {
        t / imp
    } else {
        (1.0 - (t - imp) / 0.25).max(0.0)
    };
    let c = |rgb: [u8; 3], a: f32| [rgb[0], rgb[1], rgb[2], a.clamp(0.0, 255.0) as u8];
    faixa_circular(pulso, 0.0, 7.0 * VOXEL, 2.0 * VOXEL, c(tema.circulo, aceso * 230.0));
    if tema.interno {
        faixa_circular(pulso, 0.0, 3.6 * VOXEL, 1.0 * VOXEL, c(tema.onda, aceso * 210.0));
    }
    // Spokes: thin quads from the inner edge out past the ring, turning.
    for k in 0..tema.raios {
        let ang = k as f32 / tema.raios as f32 * std::f32::consts::TAU + t * 1.5;
        let (dir, lado) = (vec3(ang.cos(), 0.0, ang.sin()), vec3(-ang.sin(), 0.0, ang.cos()));
        let (r0, r1, w) = (2.5 * VOXEL, 9.5 * VOXEL, 0.35 * VOXEL);
        let q = [dir * r0 - lado * w, dir * r1 - lado * w * 0.3, dir * r1 + lado * w * 0.3, dir * r0 + lado * w];
        let cor = c(tema.onda, aceso * 200.0);
        dupla(q.iter().map(|p| vtx(pulso.transform_point3(*p), cor)).collect(), vec![[0, 1, 2], [0, 2, 3]]);
    }
    if t > imp {
        let u = ((t - imp) / 0.22).min(1.0);
        faixa_circular(
            pulso,
            -(3.0 + 18.0 * u) * VOXEL,
            (7.0 + 12.0 * u) * VOXEL,
            (2.2 * (1.0 - u) + 0.5) * VOXEL,
            c(tema.onda, (1.0 - u) * 200.0),
        );
    }
}

/// Octaedro: o menor solido que le' como bola de longe.
fn octaedro(c: Vec3, r: f32, cor: [u8; 4]) {
    let v = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
        .map(|d| vtx(c + d * r, cor))
        .to_vec();
    let t = vec![
        [0, 2, 4],
        [2, 1, 4],
        [1, 3, 4],
        [3, 0, 4],
        [2, 0, 5],
        [1, 2, 5],
        [3, 1, 5],
        [0, 3, 5],
    ];
    dupla(v, t);
}

/// O projetil: bala e' um ponto quente com um risco atras; magia e' um orbe
/// violeta com halo. Vem o tipo no `kind` da meta.
fn desenha_projetil(e: &crate::world::Ent, p: Vec3) {
    let alto = p + vec3(0.0, 1.05, 0.0);
    let v = e.state.vel_f32();
    let dir = vec3(v.x, 0.0, v.y).normalize_or_zero();
    if e.meta.kind == 2 {
        // Vento Cortante: uma meia-lua clara atravessando o ar.
        let lado = dir.cross(Vec3::Y).normalize_or_zero();
        for i in 0..12 {
            let a = -1.3 + i as f32 * 2.6 / 12.0;
            let b = -1.3 + (i + 1) as f32 * 2.6 / 12.0;
            let ponto = |ang: f32, r: f32| alto + lado * ang.sin() * r + dir * ang.cos() * r;
            dupla(
                vec![
                    vtx(ponto(a, 0.85), [155, 235, 255, 255]),
                    vtx(ponto(b, 0.85), [155, 235, 255, 255]),
                    vtx(ponto(b, 0.62), [90, 180, 255, 40]),
                    vtx(ponto(a, 0.62), [90, 180, 255, 40]),
                ],
                vec![[0, 1, 2], [0, 2, 3]],
            );
        }
    } else if e.meta.kind == 1 {
        // A player's orb carries the caster's skins: the ring skin colours it.
        let t = tema_do_anel(e.meta.skins);
        octaedro(alto, 0.15, [t.halo[0], t.halo[1], t.halo[2], 255]);
        octaedro(alto, 0.27, [t.halo_fora[0], t.halo_fora[1], t.halo_fora[2], 90]);
    } else {
        octaedro(alto, 0.06, [255, 244, 200, 255]);
        let lado = dir.cross(Vec3::Y).normalize_or_zero() * 0.035;
        let atras = alto - dir * 0.8;
        dupla(
            vec![
                vtx(alto + lado, [255, 214, 130, 180]),
                vtx(alto - lado, [255, 214, 130, 180]),
                vtx(atras - lado, [255, 214, 130, 0]),
                vtx(atras + lado, [255, 214, 130, 0]),
            ],
            vec![[0, 1, 2], [0, 2, 3]],
        );
    }
}

/// O boneco em pecas numa base qualquer — o mundo usa a posicao da entidade;
/// QUEM O BONECO E': as malhas que o vestem e as cores das faixas.
///
/// Entra no lugar dos dois `HashMap` soltos de antes. O que mudou de verdade
/// nao foi o numero de parametros: e' que `desenha_rig` tinha UM caso
/// especial cravado (`if nome == "cabelo"`) e nenhuma substituicao de peca. A
/// roupa precisa substituir as dez, e isso e' uma CADEIA, nao um `if`.
pub struct Vestimenta<'a> {
    /// O corpo base (o piratinha). Sempre presente: e' o fallback de tudo, e
    /// e' o que aparece se a roupa nao tiver aquela peca.
    pub corpo: &'a std::collections::HashMap<String, Vec<Mesh>>,
    /// A roupa: cada peca dela SUBSTITUI a do corpo. `None` = piratinha.
    pub roupa: Option<&'a std::collections::HashMap<String, Vec<Mesh>>>,
    /// O rosto: manda na peca `cabeca`.
    pub rosto: Option<&'a std::collections::HashMap<String, Vec<Mesh>>>,
    /// Cabelo, chapeu, capuz ou elmo: todos mandam no slot `cabelo`.
    pub cabelo: Option<&'a std::collections::HashMap<String, Vec<Mesh>>>,
    /// `(claro, escuro)` em 0..1. `None` = a cor que veio no arquivo.
    pub pele: Option<[[f32; 3]; 2]>,
    pub cor_cabelo: Option<[[f32; 3]; 2]>,
    pub tier: Option<[[f32; 3]; 2]>,
    /// Weapon and mount skins (`Aparencia::empacota_skins`). 0 = defaults.
    pub skins: u64,
}

impl<'a> Vestimenta<'a> {
    /// So' o corpo, sem nada por cima: o NPC, o chefe pirata e as previas.
    pub fn nua(corpo: &'a std::collections::HashMap<String, Vec<Mesh>>) -> Self {
        Self {
            corpo,
            roupa: None,
            rosto: None,
            cabelo: None,
            pele: None,
            cor_cabelo: None,
            tier: None,
            skins: 0,
        }
    }

    /// Quem responde por uma peca. A ordem E' a regra: rosto e cabelo mandam
    /// nos slots deles, a roupa manda no resto, e o corpo responde por ultimo.
    fn peca(&self, nome: &str) -> Option<&'a Vec<Mesh>> {
        let camadas: [Option<&'a std::collections::HashMap<String, Vec<Mesh>>>; 3] = match nome {
            "cabeca" => [self.rosto, self.roupa, Some(self.corpo)],
            // O slot do cabelo NAO cai no corpo: o `corpo.vox` nao tem peca
            // `cabelo`, e quem nao escolheu chapeu nem cabelo anda de cabeca
            // descoberta — nao herda o tricornio do piratinha.
            "cabelo" => [self.cabelo, self.roupa, None],
            _ => [self.roupa, Some(self.corpo), None],
        };
        camadas.into_iter().flatten().find_map(|c| c.get(nome))
    }

    fn faixas(&self) -> crate::gpu_estatica::Faixas {
        crate::gpu_estatica::Faixas::nova(self.tier, self.cor_cabelo, self.pele)
    }
}

/// a bolsa, a origem do retrato. Devolve onde ficaram a espada e o escudo.
pub fn desenha_rig(
    base: Mat4,
    pose: &crate::rig::Pose,
    veste: &Vestimenta<'_>,
    vox: &VoxCache,
    tinta: Option<([f32; 3], f32)>,
) -> ([Mat4; crate::rig::N], Vec<(&'static str, Mat4)>) {
    let mats = crate::rig::matrizes(pose, base, VOXEL);
    let faixas = veste.faixas();
    for (i, (nome, _, _)) in crate::rig::PECAS.iter().enumerate() {
        for m in veste.peca(nome).into_iter().flatten() {
            draw_mesh_mat_faixas(m, &mats[i], tinta, faixas);
        }
    }
    // As armas do conjunto: na mao em combate, guardadas fora dele — e o que
    // se veste junto (bainha, coldres).
    let armas = crate::rig::armas(pose, &mats, VOXEL);
    for (nome, mat) in &armas {
        for m in arma_vestida(vox, nome, veste.skins).into_iter().flatten() {
            draw_mesh_mat(m, mat);
        }
    }
    (mats, armas)
}

/// The weapon model in the skin worn for its set, once loaded; until then
/// (and with no skin) the default one. Same grip marker, so the hands and
/// the sheath fit the same.
pub fn arma_vestida<'v>(vox: &'v VoxCache, nome: &str, skins: u64) -> Option<&'v Vec<Mesh>> {
    let conjunto = match nome {
        "espada" | "escudo" => 0,
        "katana" | "bainha" => 1,
        "pistola" | "coldre" => 2,
        _ => return vox.arma(nome),
    };
    let a = shared::aparencia::Aparencia::default().com_skins(skins);
    shared::aparencia::sufixo_da_arma(&a, conjunto)
        .and_then(|s| vox.arma_ou_pede(&format!("{nome}_{s}")))
        .or_else(|| vox.arma(nome))
}

/// The mount's creature in the coat worn (`SKINS_DE_MONTARIA`), once loaded;
/// until then the species' own.
pub fn bicho_da_montaria<'v>(
    vox: &'v VoxCache,
    especie: &shared::montarias::Especie,
    skins: u64,
) -> Option<&'v crate::bicho::Bicho> {
    let a = shared::aparencia::Aparencia::default().com_skins(skins);
    shared::aparencia::sufixo_da_montaria(&a)
        .and_then(|s| {
            let altura = crate::bicho::BICHOS.iter().find(|(n, _)| *n == especie.bicho)?.1;
            vox.bicho_ou_pede(&format!("{}_{s}", especie.bicho), altura)
        })
        .or_else(|| vox.bicho(especie.bicho))
}

/// O rastro da lamina: uma fita entre a base e a ponta de cada amostra, que
/// some com a idade. E' o que o olho le' como velocidade do corte.
fn desenha_fita(amostras: &[(Vec3, Vec3, f32)], agora: f32) {
    let mut vertices = Vec::with_capacity(amostras.len() * 2);
    for (base, ponta, t) in amostras {
        let u = ((agora - t) / VIDA_DO_RASTRO).clamp(0.0, 1.0);
        let a = (1.0 - u).powi(2) * 170.0;
        for (q, alfa) in [(*base, a / 3.0), (*ponta, a)] {
            vertices.push(Vertex {
                position: q,
                uv: vec2(0.0, 0.0),
                color: [235, 244, 255, alfa as u8],
                normal: Vec4::ZERO,
            });
        }
    }
    let mut indices = Vec::new();
    for i in 0..(amostras.len() as u16).saturating_sub(1) {
        let (a0, a1, b0, b1) = (i * 2, i * 2 + 1, i * 2 + 2, i * 2 + 3);
        indices.extend_from_slice(&[a0, a1, b0, a1, b1, b0, a0, b0, a1, a1, b0, b1]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

/// Um bicho em pecas: a pose sai da passada e do golpe (`bicho.rs`), e cada
/// peca gira em volta do proprio pivo.
///
/// Duas bases: as PATAS ficam no chao, e o tronco (com cabeca e cauda)
/// empina, torce e avanca por cima delas — na patada e no golpe recebido. O
/// achatamento do golpe vai no bicho inteiro. Devolve o rastro das garras,
/// desenhado depois de todo mundo (ver `desenha_rastro`).
fn desenha_bicho(
    e: &crate::world::Ent,
    b: &crate::bicho::Bicho,
    vista: &Vista,
) -> Option<(Mat4, crate::bicho::Rastro)> {
    let p = vista.pos_de(e);
    // Morto: as patas param, a cauda para, e ele TOMBA de lado (o lado sai do
    // id, pra dois corpos vizinhos nao caírem iguais).
    let morto = e.morte.is_some();
    let agora = get_time();
    let boss = e.state.flags & shared::ent_flags::BOSS != 0;
    // Chefe com golpe telegrafado: a pose da carga/golpe/recuperacao por cima.
    let carga = e.carga_chefe.filter(|_| boss && !morto);
    let ajuste = carga.and_then(|c| crate::chefe_anim::ajuste(&c, agora));
    let entrada = crate::bicho::Entrada {
        passada: e.fase,
        vel: if morto {
            0.0
        } else {
            e.andar * shared::PLAYER_SPEED
        },
        tempo: if morto { 0.0 } else { agora as f32 },
        golpe: if morto {
            99.0
        } else {
            carga
                .and_then(|c| crate::chefe_anim::relogio_da_pata(&c, agora))
                .unwrap_or(e.golpe)
        },
        semente: e.meta.id.0 as f32,
        ferido: if morto { None } else { e.ferido },
        recuo: Quat::from_rotation_y(-e.yaw) * vec3(-e.golpe_de.x, 0.0, -e.golpe_de.y),
    };
    let mut c = crate::bicho::corpo(&entrada, &b.anat);
    let esc = escala_de_chefe(e);
    let (mut desloca, mut giro) = (Vec3::ZERO, 0.0);
    if let (Some(a), Some(cg)) = (ajuste, carga) {
        c.pitch += a.pitch;
        c.sobe_tronco += a.sobe * b.anat.altura;
        c.avanca += a.avanca * b.anat.altura;
        c.esmaga = (c.esmaga + a.agacha).min(0.45);
        let tz = crate::chefe_anim::tremor_xz(&a, agora as f32, b.anat.altura * esc);
        desloca = vec3(
            cg.dir.x * a.desloca + tz.x,
            a.voa * b.anat.altura * esc,
            cg.dir.y * a.desloca + tz.y,
        );
        giro = a.giro;
    }
    let lado = if e.meta.id.0 % 2 == 0 { 1.0 } else { -1.0 };
    let cai = e
        .morte
        .map_or(0.0, |t| if boss { queda_de_chefe(t) } else { queda(t) });
    if boss {
        crate::chefe_anim::poeira_da_queda(
            e.morte,
            get_frame_time(),
            p,
            b.anat.altura * esc,
            e.meta.kind,
        );
    }
    let chao = Mat4::from_translation(p + desloca)
        * Mat4::from_scale(Vec3::splat(esc))
        * Mat4::from_rotation_y(e.yaw + giro + crate::bicho::yaw_lateral(&b.anat, &entrada))
        * Mat4::from_rotation_z(lado * cai)
        * Mat4::from_scale(vec3(
            1.0 + 0.5 * c.esmaga,
            1.0 - c.esmaga,
            1.0 + 0.5 * c.esmaga,
        ));
    let patas = chao * Mat4::from_translation(vec3(0.0, c.sobe, 0.0));
    let tronco = patas
        * Mat4::from_translation(vec3(c.lado, c.sobe_tronco, c.avanca))
        * Mat4::from_rotation_y(c.torce)
        * Mat4::from_rotation_x(c.pitch);
    // o corpo escurece um pouco: de longe, morto nao se confunde com vivo
    //
    // Morto vence tudo; fora isso, so' o clarao do golpe tinge. Nao ha' mais
    // TINTA DE ESPECIE: o Urso Branco e o Tigre Branco tem `.vox` proprio,
    // com a paleta do PELO trocada (`bichos.py: PELAGENS`), porque tinta por
    // cima clareia tambem o que nao e' pelo — a listra, o nariz, a boca.
    let tinta = if morto {
        Some(([0.0, 0.0, 0.0], 0.3))
    } else {
        clarao(e, false)
    };
    for peca in &b.pecas {
        let (giro, desloca) = crate::bicho::peca(peca.junta, &entrada, &b.anat, peca.pivo);
        let base = if matches!(peca.junta, crate::bicho::Junta::Pata { .. }) {
            patas
        } else {
            tronco
        };
        let mat = base
            * Mat4::from_translation(peca.pivo + desloca)
            * Mat4::from_quat(giro)
            * Mat4::from_translation(-peca.pivo);
        for m in &peca.malhas {
            draw_mesh_mat_tinta(m, &mat, tinta);
        }
    }
    if !morto {
        if let Some((modelo,_)) = crate::bicho::do_pet(e.meta.tag,e.meta.kind) {
            crate::auras::animal(e.meta.auras,shared::auras::PET,tronco,b,modelo,p.distance(vista.cam.target),e.meta.id.0);
        }
    }
    crate::bicho::rastro(&entrada, &b.anat).map(|r| (patas, r))
}

/// A montaria por baixo do cavaleiro: o bicho em pecas na escala da especie,
/// tingido pela COR dela, com a passada na velocidade de quem monta.
fn desenha_montaria(
    e: &crate::world::Ent,
    esp: &shared::montarias::Especie,
    grau: u8,
    b: &crate::bicho::Bicho,
    p: Vec3,
) -> Mat4 {
    let vel = e.andar * shared::PLAYER_SPEED * shared::montarias::velocidade(grau);
    // A fase do cavaleiro anda com a DISTANCIA, mas dividida pela passada de
    // GENTE; o bicho tem passada de outro tamanho. Converter pela razao entre
    // as duas poe a pata no chao que passa: sem isso ela patina (o passo do
    // tigre corria no ritmo da perna de quem monta).
    // Igual ao mob: a fase JA' acumulou com o ciclo deste bicho
    // (`world::anda_a_fase` via `bicho::da_montaria`) e a marcha sai da
    // velocidade real. Sem conversao e sem teto no meio do caminho.
    desenha_bicho_montaria(
        b,
        esp.escala,
        tinta_do_grau(grau),
        p,
        e.yaw,
        vel,
        e.fase,
        get_time() as f32,
        e.meta.id.0 as f32,
    )
}

/// Personagem no painel de aparência, pelo mesmo caminho de viewport das
/// vitrines: funciona também no OpenGL ES, sem framebuffer de profundidade.
/// A look as a small ICON (a skin item in the bag): closer than
/// `vitrine_aparencia`, framed on the body, no platform — at the panel's
/// framing the figure was a speck in a bag slot.
pub fn vitrine_aparencia_icone(
    vox: &VoxCache,
    aparencia: shared::aparencia::Aparencia,
    r: Rect,
    yaw: f32,
    solido: &Material,
) -> bool {
    let Some(mut veste) = vestimenta_de(vox, aparencia.empacota()) else { return false; };
    veste.skins = aparencia.empacota_skins();
    let Some(vp) = viewport_na_tela(r) else { return false; };
    let cam = Camera3D {
        position: vec3(0.0, 1.1, 3.3),
        target: vec3(0.0, 0.9, 0.0),
        up: Vec3::Y,
        fovy: 38f32.to_radians(),
        aspect: Some(vp.2 as f32 / vp.3 as f32),
        viewport: Some(vp),
        render_target: alvo(),
        ..Default::default()
    };
    set_camera(&cam);
    limpa_so_profundidade();
    gl_use_material(solido);
    solido.set_uniform("Crop", Vec3::ZERO);
    let mut pose = crate::rig::pose(&crate::rig::Entrada {
        fase: 0.0, andar: 0.0, correr: 0.0, tempo: get_time() as f32,
        ar: 0.0, degrau: [0.0, 0.0],
        combate: crate::rig::Combate::default(),
    });
    pose.armado = false;
    desenha_rig_com_auras(Mat4::from_rotation_y(yaw), &pose, &veste, vox, 0);
    gl_use_default_material();
    camera_padrao();
    true
}

pub fn vitrine_aparencia(
    vox: &VoxCache,
    aparencia: shared::aparencia::Aparencia,
    arma: u16,
    auras: u64,
    r: Rect,
    yaw: f32,
    solido: &Material,
) -> bool {
    let Some(mut veste) = vestimenta_de(vox, aparencia.empacota()) else { return false; };
    veste.skins = aparencia.empacota_skins();
    let Some(vp) = viewport_na_tela(r) else { return false; };
    let aspecto = vp.2 as f32 / vp.3 as f32;
    let cam = Camera3D {
        position: vec3(0.0, 1.35, 4.7 * (0.62 / aspecto).max(1.0)),
        target: vec3(0.0, 0.88, 0.0),
        up: Vec3::Y,
        fovy: 34f32.to_radians(),
        aspect: Some(aspecto),
        viewport: Some(vp),
        render_target: alvo(),
        ..Default::default()
    };
    set_camera(&cam);
    limpa_so_profundidade();
    draw_cylinder(vec3(0.0, -0.07, 0.0), 0.9, 0.93, 0.06, None,
        Color::new(0.12, 0.16, 0.21, 1.0));
    gl_use_material(solido);
    solido.set_uniform("Crop", Vec3::ZERO);
    let mut pose = crate::rig::pose(&crate::rig::Entrada {
        fase: 0.0, andar: 0.0, correr: 0.0, tempo: get_time() as f32,
        ar: 0.0, degrau: [0.0, 0.0],
        combate: crate::rig::Combate {
            conjunto: shared::skills::Conjunto::da_arma(arma) as u8,
            ..Default::default()
        },
    });
    pose.armado &= arma != 0;
    desenha_rig_com_auras(Mat4::from_rotation_y(yaw), &pose, &veste, vox, auras);
    gl_use_default_material();
    camera_padrao();
    true
}

/// A RING skin: its magic circle facing the camera, cycling through the
/// cast (lights up, flashes, the wave goes out), with the hand's glow in the
/// middle. The ring has no model, so this is what it looks like.
pub fn vitrine_anel(skin: u16, r: Rect, yaw: f32) -> bool {
    use shared::aparencia as ap;
    let Some(s) = ap::skin_de_arma(skin).filter(|s| s.conjunto == 3) else { return false; };
    let mut a = ap::Aparencia::default();
    a.armas[3] = (skin - ap::ARMA_SKIN_BASE + 1) as u8;
    let _ = s;
    let tema = tema_do_anel(a.empacota_skins());
    let Some(vp) = viewport_na_tela(r) else { return false; };
    let aspecto = vp.2 as f32 / vp.3 as f32;
    let meia = 12.0 * VOXEL * (1.0 / aspecto).max(1.0);
    let fovy = 30f32.to_radians();
    let cam = Camera3D {
        position: vec3(0.0, 0.0, meia / (fovy * 0.5).tan()),
        target: Vec3::ZERO,
        up: Vec3::Y,
        fovy,
        aspect: Some(aspecto),
        viewport: Some(vp),
        render_target: alvo(),
        ..Default::default()
    };
    set_camera(&cam);
    limpa_so_profundidade();
    gl_use_default_material();
    let agora = get_time() as f32;
    // The circle's plane is the wrist's XZ; stood up to face the camera,
    // tilted a little so it reads as a ring and not a flat sticker.
    let pulso = Mat4::from_rotation_y(yaw * 0.3)
        * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2 - 0.35)
        * Mat4::from_translation(vec3(0.0, 3.0 * VOXEL, 0.0));
    let ciclo = (agora * 0.7).fract() * (crate::rig::IMPACTO + 0.12);
    let t = ciclo.max(crate::rig::IMPACTO * 0.6);
    desenha_circulo(pulso, t, &tema);
    desenha_mao(Vec3::ZERO, 1.0, agora, &tema);
    camera_padrao();
    true
}

/// Any wardrobe skin in a small frame: outfits and hats on `base` (the
/// viewer's own look, or the default), a weapon skin up close, a mount coat
/// on `montaria` (a mount item). The shop card, the shop detail and the bag
/// icon all show skins through here.
pub fn vitrine_de_skin(
    vox: &VoxCache,
    skin: u16,
    base: shared::aparencia::Aparencia,
    montaria: u16,
    r: Rect,
    yaw: f32,
    solido: &Material,
) -> bool {
    use shared::aparencia as ap;
    if ap::skin_de_arma(skin).is_some() {
        return vitrine_arma(vox, skin, r, yaw, solido);
    }
    if ap::skin_de_montaria(skin).is_some() {
        let a = ap::Aparencia { montaria: (skin - ap::MONTARIA_SKIN_BASE + 1) as u8, ..Default::default() };
        return vitrine_montaria_com_skins(vox, montaria, a.empacota_skins(), r, yaw, solido);
    }
    let mut a = base;
    match ap::cabelo_do_chapeu(skin) {
        Some(cabelo) => a.cabelo = cabelo,
        None => a.roupa = skin,
    }
    vitrine_aparencia_icone(vox, a, r, yaw, solido)
}

/// A WEAPON SKIN up close: its two pieces (sword and shield, katana and
/// sheath, pistol and holster) standing side by side, each turning on its own
/// axis. On the character the weapon is a few pixels; this is what the shop
/// card, the bag icon and the wardrobe show. `false` = not loaded yet.
pub fn vitrine_arma(vox: &VoxCache, skin: u16, r: Rect, yaw: f32, solido: &Material) -> bool {
    let Some(s) = shared::aparencia::skin_de_arma(skin) else { return false; };
    if s.conjunto == 3 {
        return vitrine_anel(skin, r, yaw);
    }
    let pecas = match s.conjunto {
        0 => ["espada", "escudo"],
        1 => ["katana", "bainha"],
        _ => ["pistola", "coldre"],
    };
    let mut a = shared::aparencia::Aparencia::default();
    a.armas[s.conjunto as usize] = (skin - shared::aparencia::ARMA_SKIN_BASE + 1) as u8;
    let skins = a.empacota_skins();
    let nome = |p: &str| format!("{p}_{}", s.sufixo);
    // Only the skin's own meshes: the default model standing in would
    // show the wrong weapon on a card that sells this one.
    let malhas: Vec<(&str, &Vec<Mesh>)> = pecas
        .iter()
        .filter_map(|p| arma_vestida(vox, p, skins).filter(|_| vox.arma(&nome(p)).is_some()).map(|m| (*p, m)))
        .collect();
    if malhas.len() < pecas.len() {
        return false;
    }
    let Some(vp) = viewport_na_tela(r) else { return false; };
    // Blade along voxel +Y = world +Z; stood up, it points to +Y. The shield
    // turns a quarter more so its painted face looks at the camera.
    let em_pe = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let caixas: Vec<(Mat4, Vec3, Vec3)> = malhas
        .iter()
        .map(|(p, ms)| {
            let giro = if *p == "escudo" { Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2) } else { Mat4::IDENTITY };
            let m = giro * em_pe;
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for v in ms.iter().flat_map(|x| &x.vertices) {
                let q = m.transform_point3(v.position);
                lo = lo.min(q);
                hi = hi.max(q);
            }
            (m, lo, hi)
        })
        .collect();
    let vao = 0.12;
    let larguras: Vec<f32> = caixas.iter().map(|(_, lo, hi)| (hi.x - lo.x).max(hi.z - lo.z)).collect();
    let total = larguras.iter().sum::<f32>() + vao * (larguras.len() as f32 - 1.0);
    let altura = caixas.iter().map(|(_, lo, hi)| hi.y - lo.y).fold(0.0, f32::max);
    let aspecto = vp.2 as f32 / vp.3 as f32;
    let meia = (altura * 0.5).max(total * 0.5 / aspecto) * 1.18;
    let fovy = 30f32.to_radians();
    let cam = Camera3D {
        position: vec3(0.0, 0.0, meia / (fovy * 0.5).tan()),
        target: Vec3::ZERO,
        up: Vec3::Y,
        fovy,
        aspect: Some(aspecto),
        viewport: Some(vp),
        render_target: alvo(),
        ..Default::default()
    };
    set_camera(&cam);
    limpa_so_profundidade();
    gl_use_material(solido);
    solido.set_uniform("Crop", Vec3::ZERO);
    let mut x = -total * 0.5;
    for ((_, ms), ((m, lo, hi), w)) in malhas.iter().zip(caixas.iter().zip(&larguras)) {
        let centro = (*lo + *hi) * 0.5;
        let mat = Mat4::from_translation(vec3(x + w * 0.5, 0.0, 0.0))
            * Mat4::from_rotation_y(yaw)
            * Mat4::from_translation(-centro)
            * *m;
        for malha in ms.iter() {
            draw_mesh_mat(malha, &mat);
        }
        x += w + vao;
    }
    gl_use_default_material();
    camera_padrao();
    true
}

/// The character as the WORLD draws it: the body, then the auras of the
/// equipped pieces (`shared::auras::equipamento`), flushed under the camera
/// that is set right now. The inventory portrait and the Appearance screen
/// draw through here, so neither can drift from what other players see.
pub fn desenha_rig_com_auras(
    base: Mat4,
    pose: &crate::rig::Pose,
    veste: &Vestimenta,
    vox: &VoxCache,
    auras: u64,
) {
    let (mats, armas) = desenha_rig(base, pose, veste, vox, None);
    if auras != 0 {
        crate::auras::personagem(auras, &mats, &armas, vox, 0.0, 0, pose.ferramenta.is_some(), false);
        crate::auras::desenha();
    }
}

/// A montaria parada num palco, girando em `yaw`: a vitrine da Loja.
/// Desenhada DIRETO na tela num viewport (sem render target com
/// profundidade, que o iPhone recusa — ver `viewport_em_pixels`). `false` =
/// sem modelo ou sem espaco.
pub fn vitrine_montaria(
    vox: &crate::vox::VoxCache,
    item_id: u16,
    r: Rect,
    yaw: f32,
    solido: &Material,
) -> bool {
    vitrine_montaria_com_skins(vox, item_id, 0, r, yaw, solido)
}

/// `vitrine_montaria` wearing a mount skin (`Aparencia::empacota_skins`):
/// the shop card and the wardrobe preview of a coat.
pub fn vitrine_montaria_com_skins(
    vox: &crate::vox::VoxCache,
    item_id: u16,
    skins: u64,
    r: Rect,
    yaw: f32,
    solido: &Material,
) -> bool {
    let Some((especie, grau)) = shared::montarias::de_item(item_id) else {
        return false;
    };
    let Some(b) = bicho_da_montaria(vox, especie, skins) else {
        return false;
    };
    if r.w < 8.0 || r.h < 8.0 {
        return false;
    }
    let Some(vp) = viewport_na_tela(r) else {
        return false;
    };
    let cam = camera_da_vitrine(b, especie.escala, 1.25, vp);
    set_camera(&cam);
    limpa_so_profundidade();
    macroquad::material::gl_use_material(solido);
    // Vitrine: a montaria esta' PARADA no palco (so' gira em `yaw`).
    desenha_bicho_montaria(
        b,
        especie.escala,
        tinta_do_grau(grau),
        Vec3::ZERO,
        yaw,
        0.0,
        0.0,
        get_time() as f32,
        7.0,
    );
    macroquad::material::gl_use_default_material();
    camera_padrao();
    true
}

/// O PET num palco, girando em `yaw` — e' o icone dele na bolsa e na loja
/// (docs/PETS.md). Mesma vitrine da montaria; o que muda e' a escala da
/// especie e a tinta, que aqui e' a COR DO GRAU: e' assim que se ve' de
/// relance que o bichinho e' roxo e nao verde.
///
/// O cinza nao leva tinta: ele e' o bicho na cor dele.
pub fn vitrine_pet(
    vox: &crate::vox::VoxCache,
    item_id: u16,
    r: Rect,
    yaw: f32,
    solido: &Material,
) -> bool {
    let Some((especie, grau)) = shared::pets::de_item(item_id) else {
        return false;
    };
    let Some(b) = vox.bicho(especie.bicho) else {
        return false;
    };
    if r.w < 8.0 || r.h < 8.0 {
        return false;
    }
    let Some(vp) = viewport_na_tela(r) else {
        return false;
    };
    // A escala do catalogo e' a do bichinho no MUNDO, ao lado do jogador: um
    // caranguejinho ficaria minusculo na vitrine e a corujinha, enorme. Aqui
    // todo pet e' enquadrado do mesmo jeito, no tamanho natural do modelo —
    // a vitrine mostra O BICHO, nao o tamanho dele no mapa.
    let cam = camera_da_vitrine(b, 1.0, 1.35, vp);
    set_camera(&cam);
    limpa_so_profundidade();
    macroquad::material::gl_use_material(solido);
    desenha_bicho_montaria(
        b,
        1.0,
        tinta_do_grau(grau),
        Vec3::ZERO,
        yaw,
        0.0,
        0.0,
        get_time() as f32,
        item_id as f32,
    );
    macroquad::material::gl_use_default_material();
    camera_padrao();
    true
}

/// A creature by its file (`bicho::BICHOS`), on a stage: previews of models
/// not yet tied to a mob kind.
pub fn vitrine_bicho(vox: &crate::vox::VoxCache, nome: &str, r: Rect, yaw: f32, solido: &Material) -> bool {
    let Some(b) = vox.bicho(nome) else { return false; };
    let Some(vp) = viewport_na_tela(r) else { return false; };
    let cam = camera_da_vitrine(b, 1.0, 1.35, vp);
    set_camera(&cam);
    limpa_so_profundidade();
    macroquad::material::gl_use_material(solido);
    desenha_bicho_montaria(b, 1.0, None, Vec3::ZERO, yaw, 0.0, 0.0, get_time() as f32, 1.0);
    macroquad::material::gl_use_default_material();
    camera_padrao();
    true
}

/// Modelo real do inimigo no bestiário, usando a mesma malha do mundo.
pub fn vitrine_mob(vox: &crate::vox::VoxCache, kind: u16, chefe: bool,
    r: Rect, yaw: f32, solido: &Material) -> bool {
    if let Some((nome, _)) = crate::bicho::do_mob(shared::EntityTag::Enemy, kind, chefe) {
        let Some(b) = vox.bicho(nome) else { return false; };
        let Some(vp) = viewport_na_tela(r) else { return false; };
        let cam = camera_da_vitrine(b, 1.0, 1.35, vp);
        set_camera(&cam);
        limpa_so_profundidade();
        macroquad::material::gl_use_material(solido);
        desenha_bicho_montaria(b, 1.0, None, Vec3::ZERO, yaw,
            0.0, 0.0, get_time() as f32, kind as f32);
        macroquad::material::gl_use_default_material();
        camera_padrao();
        true
    } else {
        // People go on their rig file (`RIGS_DE_GENTE`). The whole model
        // (`modelo_do_mob`) has no named parts, and loaded as a rig it drew
        // only the hand-held weapon.
        let corpo = if chefe {
            match shared::bosses::chefe(kind).map(|c| c.corpo) {
                Some(shared::bosses::Corpo::Gente(k)) => k,
                Some(shared::bosses::Corpo::Pirata) => 2,
                _ => kind,
            }
        } else { kind };
        let nome = rig_de_gente(corpo).unwrap_or_else(|| modelo_do_mob(corpo));
        vitrine_rig(vox, nome, r, yaw, solido)
    }
}

/// A cor do grau como tinta — hoje SEM EFEITO, e de proposito.
///
/// Ela existia quando a mesma criatura vinha nas cinco cores: a tinta era o
/// unico jeito de ler o grau. Desde 20/09/2026 a cor E' a criatura (um cervo
/// nao e' um dragao verde), e tingir o filhote de lobo de verde so' estragava
/// o bicho. O grau continua legivel na borda da celula (`cor_do_tier`) e no
/// nome.
///
/// Mantida como ponto unico caso volte a fazer falta — e pra o motivo ficar
/// escrito onde alguem procuraria.
fn tinta_do_grau(_grau: u8) -> Option<([f32; 3], f32)> {
    None
}

#[allow(dead_code)]
fn tinta_do_grau_antiga(grau: u8) -> Option<([f32; 3], f32)> {
    if grau <= 1 {
        return None;
    }
    let h = shared::items::tier_color_hex(grau).trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
    Some((
        [
            ((v >> 16) & 0xff) as f32 / 255.0,
            ((v >> 8) & 0xff) as f32 / 255.0,
            (v & 0xff) as f32 / 255.0,
        ],
        0.45,
    ))
}

/// A camera da vitrine: tres-quartos de cima, como a do jogo, enquadrando o
/// maior lado do bicho com folga.
/// `folga` afasta a camera: 1.0 enquadra justo (a montaria, que tem palco
/// grande), mais que isso sobra borda — o pet mora num quadrado pequeno e
/// encostava no topo.
fn camera_da_vitrine(
    b: &crate::bicho::Bicho,
    escala: f32,
    folga: f32,
    vp: (i32, i32, i32, i32),
) -> Camera3D {
    let h = (b.anat.altura * escala).max(0.4);
    let comprido = (b.anat.frente.abs() * escala * 2.0).max(h);
    let aspecto = vp.2 as f32 / vp.3.max(1) as f32;
    let tamanho = h.max(comprido / aspecto.max(0.6)) * folga;
    Camera3D {
        position: vec3(0.0, h * 1.25, tamanho * 2.3),
        target: vec3(0.0, h * 0.45, 0.0),
        up: Vec3::Y,
        fovy: 30f32.to_radians(),
        aspect: Some(aspecto),
        viewport: Some(vp),
        render_target: alvo(),
        ..Default::default()
    }
}

/// Onde o CHAO sob a montaria (origem) cai na tela, dentro de `r`: e' ali
/// que a Loja poe o pedestal, pra montaria pisar nele.
/// Um RIG de NPC numa caixinha: o modelo do morador, girando devagar.
///
/// O dono, sobre contratar na Minha Ilha: "quando for comprar um trabalhador
/// ter o modelo 3D dele também bonitinho na direita". Cinco botões com as
/// três primeiras letras do ofício ("Len", "Min", "Mer") não dizem nada sobre
/// quem se está contratando — o modelo diz.
///
/// Mesmo enquadramento do retrato da bolsa, que é o que já se sabe que cabe.
/// `false` = o arquivo ainda não carregou (a carga é preguiçosa) ou a caixa é
/// pequena demais; quem chama desenha o que tinha antes.
pub fn vitrine_rig(vox: &VoxCache, nome: &str, r: Rect, yaw: f32, solido: &Material) -> bool {
    if r.w < 16.0 || r.h < 16.0 {
        return false;
    }
    let Some(corpo) = vox.rig_ou_pede(nome) else {
        return false;
    };
    let Some(vp) = viewport_na_tela(r) else {
        return false;
    };
    let cam = Camera3D {
        position: vec3(0.0, 1.15, 4.6),
        target: vec3(0.0, 0.92, 0.0),
        up: Vec3::Y,
        fovy: 30f32.to_radians(),
        aspect: Some(vp.2 as f32 / vp.3 as f32),
        viewport: Some(vp),
        render_target: alvo(),
        ..Default::default()
    };
    set_camera(&cam);
    limpa_so_profundidade();
    macroquad::material::gl_use_material(solido);
    let entrada = crate::rig::Entrada {
        fase: 0.0,
        andar: 0.0,
        correr: 0.0,
        tempo: get_time() as f32,
        ar: 0.0,
        degrau: [0.0; 2],
        combate: Default::default(),
    };
    let pose = crate::rig::pose(&entrada);
    desenha_rig(
        Mat4::from_rotation_y(yaw),
        &pose,
        &Vestimenta::nua(corpo),
        vox,
        None,
    );
    macroquad::material::gl_use_default_material();
    camera_padrao();
    true
}

pub fn vitrine_chao(vox: &crate::vox::VoxCache, item_id: u16, r: Rect) -> Option<Vec2> {
    let (especie, _) = shared::montarias::de_item(item_id)?;
    let b = vox.bicho(especie.bicho)?;
    let vp = viewport_na_tela(r)?;
    let cam = camera_da_vitrine(b, especie.escala, 1.25, vp);
    let clip = cam.matrix() * Vec3::ZERO.extend(1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    Some(vec2(
        r.x + (ndc.x * 0.5 + 0.5) * r.w,
        r.y + (1.0 - (ndc.y * 0.5 + 0.5)) * r.h,
    ))
}

/// O bicho da montaria com a skin: o do cavaleiro no mundo e o da vitrine.
#[allow(clippy::too_many_arguments)]
pub fn desenha_bicho_montaria(
    b: &crate::bicho::Bicho,
    escala: f32,
    // (cor 0..1, forca 0..1). `None` = o bicho na cor dele.
    tinta: Option<([f32; 3], f32)>,
    p: Vec3,
    yaw: f32,
    vel: f32,
    // Fase da passada, em radianos. Anda com a DISTANCIA percorrida (como
    // `world::anda_a_fase` faz pro bicho do mundo): multiplicar o relogio
    // pela velocidade tratava milhares de segundos como distancia, e a
    // montaria trotava muito mais rapido do que andava.
    passada: f32,
    tempo: f32,
    semente: f32,
) -> Mat4 {
    let entrada = crate::bicho::Entrada {
        passada,
        vel,
        tempo,
        golpe: 99.0,
        semente,
        ferido: None,
        recuo: Vec3::ZERO,
    };
    let c = crate::bicho::corpo(&entrada, &b.anat);
    let chao = Mat4::from_translation(p)
        * Mat4::from_scale(Vec3::splat(escala))
        * Mat4::from_rotation_y(yaw + crate::bicho::yaw_lateral(&b.anat, &entrada));
    let patas = chao * Mat4::from_translation(vec3(0.0, c.sobe, 0.0));
    let tronco = patas
        * Mat4::from_translation(vec3(c.lado, c.sobe_tronco, c.avanca))
        * Mat4::from_rotation_y(c.torce)
        * Mat4::from_rotation_x(c.pitch);
    for peca in &b.pecas {
        let (giro, desloca) = crate::bicho::peca(peca.junta, &entrada, &b.anat, peca.pivo);
        let base = if matches!(peca.junta, crate::bicho::Junta::Pata { .. }) {
            patas
        } else {
            tronco
        };
        let mat = base
            * Mat4::from_translation(peca.pivo + desloca)
            * Mat4::from_quat(giro)
            * Mat4::from_translation(-peca.pivo);
        for malha in &peca.malhas {
            draw_mesh_mat_tinta(malha, &mat, tinta);
        }
    }
    tronco
}

/// O rastro das garras: tres riscos finos acompanhando o arco, por cima de um
/// brilho largo e fraco.
///
/// Cada risco nasce fino na cauda, engrossa e termina em PONTA na cabeca, que
/// e' onde a garra esta' — e e' mais forte na cabeca, porque o que passou ha'
/// mais tempo ja' esta' sumindo. A fita fica inclinada a 45 graus, entre o
/// chao e a parede: deitada ela sumiria de camera baixa, em pe' sumiria de
/// cima. Vai com as duas faces, porque o descarte de face de costas esta'
/// ligado e a camera ve' a fita pelos dois lados.
fn desenha_rastro(base: &Mat4, r: &crate::bicho::Rastro) {
    const N: usize = 20;
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u16> = Vec::new();
    let riscos: [(f32, f32, [u8; 3], f32); 4] = [
        (0.0, 3.2, [255, 196, 120], 0.22),
        (-r.vao, 1.0, [255, 244, 222], 0.95),
        (0.0, 1.0, [255, 244, 222], 0.95),
        (r.vao, 1.0, [255, 244, 222], 0.95),
    ];
    for (dr, larg, cor, forca) in riscos {
        let b0 = vertices.len() as u16;
        for i in 0..=N {
            let v = i as f32 / N as f32;
            let phi = r.de + (r.ate - r.de) * v;
            let raio = r.raio + dr;
            let radial = vec3(r.lado * phi.sin(), 0.0, phi.cos());
            let meio = r.centro + radial * raio;
            let w = r.largura * larg * v.powf(0.6) * (1.0 - v.powi(6));
            let meia = (radial + Vec3::Y).normalize() * (w * 0.5);
            let a = (255.0 * r.forca * forca * (0.2 + 0.8 * v)).clamp(0.0, 255.0) as u8;
            for q in [meio + meia, meio - meia] {
                vertices.push(Vertex {
                    position: base.transform_point3(q),
                    uv: vec2(0.0, 0.0),
                    color: [cor[0], cor[1], cor[2], a],
                    normal: Vec4::ZERO,
                });
            }
        }
        for i in 0..N as u16 {
            let (a0, a1, b1, c1) = (b0 + i * 2, b0 + i * 2 + 1, b0 + i * 2 + 2, b0 + i * 2 + 3);
            indices.extend_from_slice(&[a0, a1, b1, a1, c1, b1, a0, b1, a1, a1, b1, c1]);
        }
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

/// Desenha uma malha com uma matriz de mundo inteira. Mesma conta do
/// `draw_mesh_at` — vertice transformado na CPU —, so' que com rotacao em
/// qualquer eixo, que e' o que uma peca do rig precisa.
fn draw_mesh_mat(m: &Mesh, mat: &Mat4) {
    draw_mesh_mat_tinta(m, mat, None);
}

/// A mesma coisa, puxando a cor de cada vertice pra `tinta` na fracao pedida
/// — e' o clarao do golpe.
fn draw_mesh_mat_tinta(m: &Mesh, mat: &Mat4, tinta: Option<([f32; 3], f32)>) {
    draw_mesh_mat_faixas(m, mat, tinta, crate::gpu_estatica::Faixas::default())
}

/// Como a de cima, e ainda tinge as FAIXAS de paleta (pele, cabelo, tier).
/// E' o caminho do personagem; todo o resto passa `Faixas::default()`.
fn draw_mesh_mat_faixas(
    m: &Mesh,
    mat: &Mat4,
    tinta: Option<([f32; 3], f32)>,
    faixas: crate::gpu_estatica::Faixas,
) {
    // Na GPU: a malha do voxel sobe uma vez e a matriz e a tinta vao por
    // uniforme. Recopiar os vertices todo quadro era o que pesava com bicho
    // em volta (ver `gpu_estatica`).
    let tinta = tinta.map_or([0.0; 4], |(c, k)| [c[0], c[1], c[2], k]);
    crate::gpu_estatica::desenha_voxel(m, *mat, tinta, faixas);
}

/// Desenha uma malha girada em Y e deslocada — com a matriz de modelo no
/// shader (`gpu_estatica::desenha_voxel`), sem recopiar vertice.
fn draw_mesh_at(m: &Mesh, at: Vec3, yaw: f32) {
    // Mesma conta de antes (y pra cima, giro em Y), como matriz.
    let mat = Mat4::from_translation(at) * Mat4::from_rotation_y(yaw);
    crate::gpu_estatica::desenha_voxel(m, mat, [0.0; 4], crate::gpu_estatica::Faixas::default());
}

/// Projeta um ponto do mundo pra pixel de tela.
///
/// Necessario pra mirar com o mouse: a selecao de alvo compara a distancia em
/// PIXELS entre o cursor e cada entidade, que e' o que o jogador enxerga.
/// Retangulo da tela (em PONTOS, origem em cima) → viewport do GL (em PIXELS,
/// origem EMBAIXO). `escala` e' o `dpi_scale` (iPhone 2x/3x com high_dpi) e
/// `alto_px` a altura do framebuffer em pixels. `None` se nao sobra area.
///
/// E' como se desenha um boneco 3D DENTRO de um painel (previa da criacao, da
/// lista, retrato da bolsa) sem render target com profundidade: a miniquad
/// 0.4.11 cria essa profundidade com `GL_DEPTH_COMPONENT` sem tamanho, que o
/// OpenGL ES do iPhone recusa — o framebuffer fica incompleto e o boneco some.
pub fn viewport_em_pixels(r: Rect, escala: f32, alto_px: f32) -> Option<(i32, i32, i32, i32)> {
    let x = (r.x * escala).round().max(0.0);
    let topo = (r.y * escala).round().max(0.0);
    let base = ((r.y + r.h) * escala).round().min(alto_px);
    let w = (r.w * escala).round();
    let h = base - topo;
    if w < 1.0 || h < 1.0 {
        return None;
    }
    Some((x as i32, (alto_px - base) as i32, w as i32, h as i32))
}

// ─────────────────────────── alvo de captura ───────────────────────────
//
// So' a PREVIA usa: desenhar numa textura em vez da tela. `get_screen_data`
// le' o buffer da FRENTE, e sem tela (Xvfb + llvmpipe) ele nunca e'
// resolvido — a captura saia branca e do tamanho da janela, nao do pedido.
// Com alvo, o tamanho da imagem e' o do alvo e o conteudo existe sempre.
// No jogo o alvo e' `None` e nada muda.

thread_local! {
    static ALVO: std::cell::RefCell<Option<RenderTarget>> = const { std::cell::RefCell::new(None) };
}

/// Liga (ou desliga) a captura em textura. So' a previa chama.
pub fn define_alvo(rt: Option<RenderTarget>) {
    ALVO.with(|c| *c.borrow_mut() = rt);
}

pub fn alvo() -> Option<RenderTarget> {
    ALVO.with(|c| c.borrow().clone())
}

/// O tamanho LOGICO de desenho: o do alvo quando ha' captura, senao a tela.
/// Todo layout que se ancora em canto passa por aqui.
pub fn tela() -> (f32, f32) {
    match alvo() {
        Some(rt) => (rt.texture.width(), rt.texture.height()),
        None => (screen_width(), screen_height()),
    }
}

/// O `set_default_camera` que respeita o alvo: volta pro 2D da tela ou do
/// alvo. Sem isto, cada `set_default_camera` no meio do desenho jogaria o
/// resto do quadro na tela e a captura sairia pela metade.
pub fn camera_padrao() {
    match alvo() {
        Some(rt) => {
            let (w, h) = (rt.texture.width(), rt.texture.height());
            let mut c = Camera2D::from_display_rect(Rect::new(0.0, 0.0, w, h));
            c.render_target = Some(rt);
            set_camera(&c);
        }
        None => set_default_camera(),
    }
}

/// O viewport de `r` na tela (ou no alvo de captura), com o dpi atual.
pub fn viewport_na_tela(r: Rect) -> Option<(i32, i32, i32, i32)> {
    if let Some(rt) = alvo() {
        // No alvo nao ha' dpi: a textura ja' esta' em pixels.
        return viewport_em_pixels(r, 1.0, rt.texture.height());
    }
    let s = macroquad::miniquad::window::dpi_scale();
    viewport_em_pixels(r, s, screen_height() * s)
}

/// Depois do `set_camera` 3D de um viewport: limpa so' a PROFUNDIDADE (a cor
/// do painel ja' desenhado fica), pro boneco nao brigar com o depth do mundo.
pub fn limpa_so_profundidade() {
    unsafe { get_internal_gl() }
        .quad_context
        .clear(None, Some(1.0), None);
}

pub fn world_to_screen(cam: &Camera3D, p: Vec3) -> Option<Vec2> {
    let clip = cam.matrix() * p.extend(1.0);
    if clip.w <= 0.0 {
        return None; // atras da camera
    }
    let ndc = clip.truncate() / clip.w;
    Some(vec2(
        (ndc.x * 0.5 + 0.5) * screen_width(),
        (1.0 - (ndc.y * 0.5 + 0.5)) * screen_height(),
    ))
}

/// Entidade mais perto do cursor, dentro de um raio em pixels.
///
/// Ignora o proprio player: em MMO de target, clicar em si mesmo nunca e' a
/// intencao de atacar.
pub fn pick(world: &World, vista: &Vista, mouse: Vec2, raio_px: f32) -> Option<shared::EntityId> {
    let mut melhor: Option<(f32, shared::EntityId)> = None;
    for (id, e) in &world.ents {
        // morto nao se seleciona: o corpo fica, o alvo nao
        if e.is_self() || e.morte.is_some() {
            continue;
        }
        let Some(sp) = vista.na_tela(vista.mira_de(e)) else {
            continue;
        };
        let d = sp.distance(mouse);
        if d <= raio_px && melhor.map_or(true, |(bd, _)| d < bd) {
            melhor = Some((d, *id));
        }
    }
    melhor.map(|(_, id)| id)
}

/// Anel de alvo: uma faixa de TRIANGULOS no chao, e nao linhas.
///
/// Era `draw_line_3d`, e linha desenhada com o material do mundo (o que
/// descarta face de costas e faz o recorte) estragava o desenho do proprio
/// alvo: o modelo selecionado — urso ou jogador — saia como um emaranhado de
/// arestas. O anel era a UNICA coisa que mudava quando havia alvo, e e' o que
/// saiu. Triangulo e' o que todo o resto do mundo ja' desenha.
fn draw_ring(center: Vec3, r: f32, color: Color) {
    const N: usize = 32;
    const LARGURA: f32 = 0.06;
    let cor: [u8; 4] = color.into();
    let ponto = |a: f32, raio: f32| center + vec3(a.cos() * raio, 0.02, a.sin() * raio);
    let mut vertices = Vec::with_capacity(N * 2);
    for i in 0..N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        for raio in [r + LARGURA, r - LARGURA] {
            vertices.push(Vertex {
                position: ponto(a, raio),
                uv: vec2(0.0, 0.0),
                color: cor,
                normal: Vec4::ZERO,
            });
        }
    }
    let mut indices = Vec::with_capacity(N * 6);
    for i in 0..N as u16 {
        let (o0, i0) = (i * 2, i * 2 + 1);
        let (o1, i1) = (((i + 1) % N as u16) * 2, ((i + 1) % N as u16) * 2 + 1);
        for tri in [[o0, i0, o1], [i0, i1, o1]] {
            // A face tem que olhar pra CIMA: com o descarte de face de costas
            // ligado, enrolada ao contrario ela some vista de cima.
            let p = |k: u16| vertices[k as usize].position;
            let n = (p(tri[1]) - p(tri[0])).cross(p(tri[2]) - p(tri[0]));
            if n.y >= 0.0 {
                indices.extend_from_slice(&tri)
            } else {
                indices.extend_from_slice(&[tri[0], tri[2], tri[1]])
            }
        }
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

#[cfg(test)]
mod testes_da_vestimenta {
    use super::*;

    fn pecas(nomes: &[&str]) -> std::collections::HashMap<String, Vec<Mesh>> {
        nomes.iter().map(|n| (n.to_string(), Vec::new())).collect()
    }

    /// A CADEIA: rosto e cabelo mandam nos slots deles, a roupa no resto, e o
    /// corpo responde por último.
    ///
    /// Antes disto havia um `if nome == "cabelo"` cravado e nenhuma
    /// substituição de peça — a roupa era impossível de escrever.
    #[test]
    fn a_roupa_vence_o_corpo_e_o_rosto_vence_a_roupa() {
        let corpo = pecas(&["torso", "cabeca", "braco_d"]);
        let roupa = pecas(&["torso", "cabeca"]);
        let rosto = pecas(&["cabeca"]);
        let cabelo = pecas(&["cabelo"]);
        let mut v = Vestimenta::nua(&corpo);
        assert!(v.peca("torso").is_some(), "sem roupa, o torso é o do corpo");
        assert!(
            v.peca("cabelo").is_none(),
            "de cabeça descoberta, nada no slot"
        );

        v.roupa = Some(&roupa);
        assert!(std::ptr::eq(
            v.peca("torso").unwrap(),
            roupa.get("torso").unwrap()
        ));
        assert!(
            std::ptr::eq(v.peca("braco_d").unwrap(), corpo.get("braco_d").unwrap()),
            "peça que a roupa não tem cai no corpo"
        );
        assert!(std::ptr::eq(
            v.peca("cabeca").unwrap(),
            roupa.get("cabeca").unwrap()
        ));

        v.rosto = Some(&rosto);
        assert!(
            std::ptr::eq(v.peca("cabeca").unwrap(), rosto.get("cabeca").unwrap()),
            "o rosto vence a roupa na cabeça"
        );

        v.cabelo = Some(&cabelo);
        assert!(std::ptr::eq(
            v.peca("cabelo").unwrap(),
            cabelo.get("cabelo").unwrap()
        ));
    }

    /// O slot do cabelo NÃO cai no corpo.
    ///
    /// Se caísse, quem escolhesse "sem chapéu" herdaria o tricórnio do
    /// piratinha — e não haveria como andar de cabeça descoberta.
    #[test]
    fn sem_chapeu_e_sem_cabelo_a_cabeca_fica_descoberta() {
        let corpo = pecas(&["torso", "cabeca", "cabelo"]);
        let v = Vestimenta::nua(&corpo);
        assert!(
            v.peca("cabelo").is_none(),
            "o slot do cabelo não herda do corpo, mesmo que ele tenha a peça"
        );
    }

    /// Vestimenta sem cor = faixas desligadas = o desenho de sempre.
    #[test]
    fn a_vestimenta_nua_nao_tinge_nada() {
        let corpo = pecas(&["torso"]);
        assert_eq!(
            Vestimenta::nua(&corpo).faixas(),
            crate::gpu_estatica::Faixas::default()
        );
    }
}

/// A MAQUETE da ilha: a colônia inteira girando dentro do painel.
///
/// A ilha deixou de ser uma zona em que se anda (decisão do dono, 22/09/2026)
/// e virou painel. O que ela perdeu em caminhada precisa voltar como VISTA:
/// um painel de números sobre uma ilha que ninguém vê não é uma ilha, é uma
/// planilha com tema.
///
/// Reusa o mesmo `Terreno` e as mesmas `Construcoes` que desenhavam a ilha
/// quando se andava nela — o assentamento que aparece aqui é, voxel por
/// voxel, o que estava no chão. Uma segunda representação seria uma segunda
/// fonte de verdade pro mesmo lugar.
///
/// Devolve `false` quando não deu (célula pequena demais, relevo ainda
/// assando): o painel desenha o resto e tenta de novo no quadro seguinte.
/// O fundo da maquete: escuro e liso, pra peça se destacar.
///
/// Era um azul de mar. A maquete virou DIORAMA — uma peça flutuando, não uma
/// ilha no mundo — e água em volta contaria a história errada.
///
/// **A prévia só mostra isto se limpar com esta cor.** O retângulo é 2D e vai
/// pro alvo corrente, que na prévia é a tela — não o render target que vira
/// PNG. Medi a cor "de fundo" da prévia uma vez achando que era esta, e era o
/// `clear_background` dela.
/// O quanto a câmera da maquete pode subir e descer, em radianos.
///
/// Presa em cima porque de cima a ilhota lê como mapa, e presa em baixo
/// porque ela não tem fundo: passar do horizonte mostra o vazio das colunas.
pub const ELEV_MIN: f32 = 0.12;
pub const ELEV_MAX: f32 = 1.45;
/// A elevação de abertura: rasante, que é onde o relevo se lê pelo perfil.
pub const ELEV_PADRAO: f32 = 0.55;

pub const COR_DO_FUNDO_DA_MAQUETE: Color = Color::new(0.09, 0.12, 0.17, 1.0);

/// Um morador da maquete: quem ele é, onde mora e onde trabalha.
pub struct Morador {
    pub oficio: shared::colonia::Profissao,
    /// Em frente à porta do prédio do ofício dele.
    pub casa: Vec2,
    /// O ponto lá fora, na mata, onde ele colhe / minera / caça.
    pub trabalho: Vec2,
}

/// Onde o morador está AGORA e o que ele está fazendo.
pub struct PassoDoMorador {
    pub onde: Vec2,
    pub yaw: f32,
    /// 0 parado .. 1 andando.
    pub andar: f32,
    /// Distância já caminhada no ciclo, pra fase do passo. A perna anda com
    /// a DISTÂNCIA e não com o relógio — parado, ela para junto.
    pub avanco: f32,
    /// Hora de bater o machado / a picareta.
    pub trabalhando: bool,
}

/// Adianta o relógio da maquete, em segundos. SÓ a prévia mexe nisto.
///
/// A rotina dura 26 s e a prévia grava dois quadros por vista: sem adiantar,
/// todo PNG pega os moradores no mesmo instante do ciclo e eu não teria como
/// ver se eles andam. Fora da prévia vale zero e some na otimização.
static ADIANTA_O_RELOGIO: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn adianta_o_relogio_da_maquete(s: f32) {
    ADIANTA_O_RELOGIO.store(s.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

fn relogio_da_maquete() -> f32 {
    get_time() as f32 + f32::from_bits(ADIANTA_O_RELOGIO.load(std::sync::atomic::Ordering::Relaxed))
}

/// Quanto dura a ida-trabalho-volta de um morador, em segundos.
const CICLO_DO_MORADOR: f32 = 26.0;

/// A ROTINA: sai da casa, vai até o ponto de trabalho, trabalha, volta.
///
/// O dono: "eles tão parados no centro da ilha e não andando entre as
/// árvores e voltando pro centro como se tivesse coletando e voltando, ou
/// minerando e voltando, ou caçando e voltando". Antes o morador era um
/// boneco fixo na porta, girando devagar em torno do próprio eixo.
///
/// Pura de propósito: é geometria e relógio, e é isso que deixa testar sem
/// abrir o jogo.
pub fn rotina_do_morador(m: &Morador, i: usize, agora: f32) -> PassoDoMorador {
    // As quatro partes do ciclo, em fração dele.
    const IDA: f32 = 0.26;
    const TRABALHO: f32 = 0.60;
    const VOLTA: f32 = 0.86;
    // A fase de cada um é diferente: com todos no mesmo compasso a ilha vira
    // um desfile, que é outro jeito de parecer maquete parada.
    let fase = (agora / CICLO_DO_MORADOR + i as f32 * 0.37).rem_euclid(1.0);
    let caminho = m.trabalho - m.casa;
    let dist = caminho.length();
    // Suaviza a partida e a chegada: em linear o boneco arranca e trava num
    // estalo, e a perna do rig pede aceleração pra ler direito.
    let suave = |u: f32| {
        let u = u.clamp(0.0, 1.0);
        u * u * (3.0 - 2.0 * u)
    };
    let (onde, andar, avanco, trabalhando, rumo) = if fase < IDA {
        let u = suave(fase / IDA);
        (
            m.casa + caminho * u,
            (u * 4.0).min(1.0).min((1.0 - u) * 4.0).max(0.0),
            dist * u,
            false,
            caminho,
        )
    } else if fase < TRABALHO {
        (m.trabalho, 0.0, dist, true, caminho)
    } else if fase < VOLTA {
        let u = suave((fase - TRABALHO) / (VOLTA - TRABALHO));
        (
            m.trabalho - caminho * u,
            (u * 4.0).min(1.0).min((1.0 - u) * 4.0).max(0.0),
            dist * (1.0 + u),
            false,
            -caminho,
        )
    } else {
        (m.casa, 0.0, dist * 2.0, false, -caminho)
    };
    PassoDoMorador {
        onde,
        // `dir.x.atan2(dir.y)` é a mesma conversão que o mundo usa pra virar
        // velocidade em yaw (`world.rs`).
        yaw: if rumo.length_squared() > 1e-6 {
            rumo.x.atan2(rumo.y)
        } else {
            0.0
        },
        andar,
        avanco,
        trabalhando,
    }
}

/// A distância de câmera que põe a ilhota INTEIRA dentro do quadro.
///
/// Sai de MEDIR, não de uma fórmula fechada. A fórmula anterior (raio ×
/// margem contra a meia-tangente do campo) errou três vezes seguidas na
/// prévia — ilha cortada em cima, embaixo e nos dois lados — e sempre pelo
/// mesmo motivo: ela é simétrica e a perspectiva não é. Com `fovy` 0,85 e a
/// câmera a ~140 u de um objeto de 97 de raio, a borda de perto fica a ~60 u
/// da lente e a de longe a ~220, então a metade de baixo projeta três vezes
/// maior que a de cima. Nenhum seno conta isso.
///
/// Então projeta a silhueta (a costa, no nível do mar e no do platô), olha o
/// ponto que mais escapa, e aproxima. A escala da projeção anda com 1/d, o
/// que faz multiplicar a distância pelo excesso convergir em três passos.
///
/// A matriz é montada aqui, e não por `Camera3D::matrix`, por duas razões:
/// aquela lê `render_target`/`screen_*` e não roda fora de um quadro (o teste
/// não teria como existir), e este é o mesmo par perspectiva × look-at que
/// ela monta — conferido na fonte da macroquad 0.4.16.
fn dist_que_enquadra(centro: Vec2, alto: f32, elevacao: f32, ang: f32, aspecto: f32) -> f32 {
    /// O quanto do quadro a peça ocupa no maior eixo. 1,0 seria encostar.
    const OCUPA: f32 = 0.96;
    let mira = vec3(
        centro.x,
        alto - shared::colonia::ALTURA_TOPO * 0.45,
        centro.y,
    );
    let borda: Vec<Vec3> = (0..48)
        .flat_map(|i| {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            let rr = shared::colonia::raio_na_direcao(a);
            [0.0f32, shared::colonia::ALTURA_TOPO]
                .map(|h| vec3(centro.x + a.cos() * rr, h, centro.y + a.sin() * rr))
        })
        .collect();
    let mut dist = shared::colonia::RAIO * 3.0;
    for _ in 0..4 {
        let m = matriz_da_maquete(centro, alto, elevacao, ang, aspecto, dist);
        let mut pior: f32 = 0.0;
        for p in &borda {
            let clip = m * p.extend(1.0);
            if clip.w <= 0.001 {
                pior = pior.max(4.0);
                continue;
            }
            pior = pior
                .max((clip.x / clip.w).abs())
                .max((clip.y / clip.w).abs());
        }
        if pior <= 0.0001 {
            break;
        }
        dist *= pior / OCUPA;
    }
    dist
}

/// Onde a câmera da maquete fica, a essa distância.
fn olho_da_maquete(centro: Vec2, alto: f32, elevacao: f32, ang: f32, dist: f32) -> Vec3 {
    vec3(
        centro.x + ang.cos() * dist * elevacao.cos(),
        alto + dist * elevacao.sin(),
        centro.y + ang.sin() * dist * elevacao.cos(),
    )
}

/// A MESMA matriz que `Camera3D::matrix` monta pra maquete, sem precisar de
/// janela. Os valores de `z_near`/`z_far` são os do `Camera3D::default`.
fn matriz_da_maquete(
    centro: Vec2,
    alto: f32,
    elevacao: f32,
    ang: f32,
    aspecto: f32,
    dist: f32,
) -> Mat4 {
    const FOVY: f32 = 0.85;
    Mat4::perspective_rh_gl(FOVY, aspecto, 0.01, 10000.0)
        * Mat4::look_at_rh(
            olho_da_maquete(centro, alto, elevacao, ang, dist),
            vec3(
                centro.x,
                alto - shared::colonia::ALTURA_TOPO * 0.45,
                centro.y,
            ),
            Vec3::Y,
        )
}

/// A faixa, dentro do retângulo dado, com a proporção da ilhota deitada.
///
/// Só encolhe: se o retângulo já for mais largo que a peça, ele passa
/// inteiro e quem limita volta a ser a altura.
fn faixa_da_peca(r: Rect) -> Rect {
    // Largura ÷ altura da ilhota como a câmera a vê.
    //
    // Cheguei a 1,5 por conta (o diâmetro contra o diâmetro achatado pelo
    // seno da elevação) e a prévia mostrou a ilha CORTADA em cima e embaixo.
    // A conta simétrica está errada: com `fovy` 0,85 e a câmera a 139 u de
    // um objeto de 97 de raio, a borda de perto fica a ~60 u da lente e a de
    // longe a ~220 — a metade de baixo projeta três vezes maior que a de
    // cima. Perspectiva não se estima por seno.
    //
    // 1,25 é o número que a prévia aprova, e a folga extra é justamente a
    // assimetria que a conta não vê.
    const PROPORCAO: f32 = 1.25;
    let alt = (r.w / PROPORCAO).min(r.h);
    Rect::new(r.x, r.y + (r.h - alt) * 0.5, r.w, alt)
}

pub fn maquete_da_ilha(
    terreno: &crate::terreno::Terreno,
    construcoes: &crate::construcoes::Construcoes,
    r: Rect,
    centro: Vec2,
    yaw: f32,
    // Quanto a câmera sobe, em radianos acima do horizonte. Era uma
    // constante (0,55); o dono pediu "que a câmera não ficasse presa só
    // direita e esquerda e fosse 360 em torno do centro".
    elevacao: f32,
    zoom: f32,
    solido: &Material,
    moradores: &[Morador],
    vox: &VoxCache,
) -> bool {
    if r.w < 40.0 || r.h < 40.0 {
        return false;
    }
    // A FAIXA DE VISÃO, e não o retângulo inteiro que me deram.
    //
    // A coluna da esquerda é ALTA E ESTREITA (~0,72 de proporção) e a ilhota
    // é o contrário: 190 u de largura por uns 110 de altura na tela, deitada
    // pela câmera rasante. Enquadrar as duas coisas na mesma caixa significa
    // encostar na largura e sobrar metade da altura vazia — e foi isso que o
    // dono viu: "a ilhota fica num quadrado que achata ela".
    //
    // A conta não tem escolha: `dist` é o MAIOR entre a restrição horizontal
    // e a vertical, então numa caixa mais alta que o objeto quem manda é
    // sempre a largura, e a altura sobra. Em vez de brigar com isso, a peça
    // ganha uma faixa da proporção DELA, centrada na vertical. Sem moldura
    // desenhada, o que sobra em cima e embaixo é só o painel.
    let r = faixa_da_peca(r);
    let Some(vp) = viewport_na_tela(r) else {
        return false;
    };
    // Enquadra a ILHOTA INTEIRA, e não só o assentamento.
    //
    // Antes era o assentamento, porque a ilha tinha 140 u de raio e de longe
    // o bastante pra ela caber as casas viravam pontinhos. Desde 22/09/2026
    // a colônia é uma ilhota desenhada, pequena de propósito — o dono pediu
    // "quase que dá pra ver tudo em 360" —, e agora cabe.
    //
    // A distância sai do RAIO da ilhota e do FORMATO do painel.
    //
    // A primeira versão só olhou a vertical e a ilha vazou pela borda de
    // baixo — visto na prévia (`MMO_PREVIA_COLONIA`), não deduzido. Num painel
    // ALTO E ESTREITO quem limita é a HORIZONTAL: o `fovy` é vertical, e o
    // ângulo horizontal é ele multiplicado pelo aspecto, que ali é menor que
    // 1. Com aspecto 0,7 o campo horizontal é 30% do vertical, e a conta que
    // ignora isso deixa a câmera três vezes perto demais.
    //
    // Então as duas restrições entram e a maior manda. Derivado de
    // `colonia::RAIO` e do retângulo, para mexer em qualquer um dos dois
    // continuar enquadrando certo.
    const FOVY: f32 = 0.85;
    // A elevação vem de fora agora, mas presa entre o rasante e o quase-zênite.
    //
    // O limite de baixo NÃO é gosto: a ilhota não tem face embaixo (a saia de
    // terra saiu a pedido do dono), então a câmera abaixo do horizonte vê o
    // interior vazio das colunas — tela preta. E de cima demais ela vira um
    // disco chapado, que é o defeito que o rasante veio consertar.
    let elevacao = elevacao.clamp(ELEV_MIN, ELEV_MAX);
    let alto = terreno.altura(centro.x, centro.y);
    // O QUARTO DE VOLTA que faltava.
    //
    // Com `yaw = 0` a câmera nascia em +X olhando pra -X, e a praça da colônia
    // é montada olhando pro -Z (`construcao::frente_de`): a maquete abria de
    // perfil pras casas. O dono: "tá 90 graus pro lado errado".
    let ang = yaw + std::f32::consts::FRAC_PI_2;
    let mira = vec3(
        centro.x,
        alto - shared::colonia::ALTURA_TOPO * 0.45,
        centro.y,
    );
    let monta = |d: f32| Camera3D {
        position: olho_da_maquete(centro, alto, elevacao, ang, d),
        target: mira,
        up: Vec3::Y,
        fovy: FOVY,
        // O ASPECTO É O DO VIEWPORT, e dizer isso é obrigatório.
        //
        // `Camera3D::matrix` da macroquad 0.4.16 tira o aspecto do ALVO
        // INTEIRO (`render_target` ou a tela), nunca do viewport:
        //
        //     let (width, height) = if let Some(rt) = &self.render_target {
        //         (rt.texture.width(), rt.texture.height())
        //     } else { (screen_width(), screen_height()) };
        //     let aspect = self.aspect.unwrap_or(width / height);
        //
        // O viewport só recorta e ESTICA: ele mapeia o NDC [-1,1] no
        // retângulo dado. Com a tela em 1,6 e a faixa em 1,25, a peça era
        // projetada larga e espremida num quadro estreito — é esse, e não a
        // moldura, o "quadrado que achata ela" que o dono viu. Estava assim
        // desde que a maquete nasceu.
        aspect: Some(r.w / r.h.max(1.0)),
        viewport: Some(vp),
        render_target: alvo(),
        ..Default::default()
    };
    // A DISTÂNCIA SAI DE MEDIR A SILHUETA, e não de uma fórmula. Ver
    // `dist_que_enquadra`.
    let mut dist = dist_que_enquadra(centro, alto, elevacao, ang, r.w / r.h.max(1.0));
    // O zoom do jogador entra DEPOIS: acima de 1 ele corta de propósito.
    let dist = dist / zoom.max(0.05);
    let cam = monta(dist);
    // O MAR DE FUNDO, em 2D, antes de tudo.
    //
    // `agua::desenha` cobre só os pedaços de terreno carregados, então o mar
    // acaba num losango e fora dele aparecia o cartão do painel — a ilha
    // parecia flutuar num diamante. Visto na prévia. Pintar o retângulo
    // inteiro antes custa um quad e resolve.
    // O FUNDO da peça: escuro e liso. Não é mar — o diorama flutua, e água
    // em volta contaria outra história (a de uma ilha no mundo, que é
    // justamente o que ela deixou de ser).
    // SEM FUNDO PRÓPRIO: a peça fica solta no painel.
    //
    // Havia um retângulo escuro aqui e um cartão atrás dele, e o dono: "a
    // ilhota fica num quadrado que achata ela; ela nem precisava ficar dentro
    // de um quadrante assim, poderia ser livre". O quadrado é que dava a
    // impressão de achatamento — a ilha não mudou de forma, mudou de moldura.
    //
    // O `set_camera` daqui só limpa PROFUNDIDADE, então o painel que já foi
    // desenhado continua aparecendo por baixo. Por isso dá pra simplesmente
    // não pintar nada.
    // A SOMBRA vem ANTES do 3D, e é isso que a põe ATRÁS da peça.
    //
    // Desenhada depois, ela ficava POR CIMA da ilha — invisível de longe e
    // uma mancha cinza no meio da grama quando o jogador dava zoom. Visto na
    // prévia, nas vistas de zoom. Ela é o que assenta o diorama: sem sombra,
    // objeto flutuando lê como recorte colado no fundo.
    //
    // E ela ACOMPANHA A ELEVAÇÃO. Uma elipse fixa é convincente com a câmera
    // rasante e denuncia o truque quando o jogador sobe: vista de cima a
    // sombra de um disco é quase um círculo, e ela fica sob a peça, não
    // abaixo dela. Então a altura da elipse cresce com o seno da elevação e
    // o centro sobe junto.
    let sub = elevacao.sin();
    let c = vec2(r.x + r.w * 0.5, r.y + r.h * (0.78 - 0.26 * sub));
    // E ela SOME no rasante: com a câmera quase no horizonte o chão não
    // aparece, e uma elipse escura solta embaixo da peça lê como mancha.
    let forca = (sub * 1.9).min(1.0);
    for (k, a) in [(1.00f32, 0.22f32), (0.72, 0.16), (0.46, 0.12)] {
        let a = a * forca;
        draw_ellipse(
            c.x,
            c.y,
            r.w * 0.32 * k,
            r.w * 0.075 * k * (0.35 + 1.75 * sub),
            0.0,
            Color::new(0.0, 0.0, 0.0, a),
        );
    }
    set_camera(&cam);
    limpa_so_profundidade();
    macroquad::material::gl_use_material(solido);
    let n = terreno.desenha(&cam, Vec3::ZERO, 0.0);
    construcoes.desenha(&cam, None, Vec3::ZERO, 0.0);
    // O `solido` SAI antes dos moradores.
    //
    // O rig desenha por `gpu_estatica::desenha_voxel`, que tem material
    // próprio: é ele quem conhece as FAIXAS de cor (pele, cabelo, tier) que o
    // vértice carrega em `normal.yzw`. Com o `solido` preso, o mesmo vértice
    // era lido por um shader que não sabe das faixas, e cada morador saía
    // como uma silhueta PRETA — o terreno e as casas não, porque a faixa
    // deles é zero. Visto na prévia.
    macroquad::material::gl_use_default_material();
    // OS MORADORES, em pé na porta do ofício de cada um.
    //
    // O dono: "dando pra ver visualmente os trabalhadores trabalhando na
    // ilha". Sem eles a maquete é um condomínio vazio — as casas nascem por
    // morador contratado e não havia ninguém dentro delas.
    //
    // Vão aqui, e não no `assar_colonia`: aquilo roda numa thread de fundo e
    // devolve malha estática, e o rig precisa do `VoxCache`, que é da thread
    // do quadro.
    for (i, mor) in moradores.iter().enumerate() {
        let oficio = &mor.oficio;
        let nome = rig_do_npc(oficio.papel() as u8, i as u64);
        let Some(corpo) = vox.rig_ou_pede(nome) else {
            continue;
        };
        let passo = rotina_do_morador(mor, i, relogio_da_maquete());
        let onde = &passo.onde;
        let y = terreno.altura(onde.x, onde.y);
        // Uma balançada lenta, com fase por morador: parados e idênticos eles
        // leriam como estátua.
        let t = get_time() as f32 + i as f32 * 1.7;
        // TRABALHANDO, e não só de pé.
        //
        // O dono pediu "ver visualmente os trabalhadores TRABALHANDO na
        // ilha". O rig já sabe a animação de coleta — machado e picareta,
        // com a ferramenta na mão (`rig::ferramenta_de`) —, então quem colhe
        // colhe. O mercenário fica de arma sacada, que é o ofício dele; o
        // curtidor e o alquimista trabalham dentro de casa e ficam de pé,
        // respirando.
        //
        // A fase por morador tira o efeito de fileira de bonecos batendo no
        // mesmo compasso.
        //
        // E SÓ NO PONTO DE TRABALHO: batendo machado enquanto caminha, o
        // boneco lê como quebrado. A caminho e na volta ele só anda.
        use shared::colonia::Profissao as P;
        let combate = match (oficio, passo.trabalhando) {
            (P::Lenhador, true) => crate::rig::Combate {
                coleta: Some((0, t)),
                ..Default::default()
            },
            (P::Minerador, true) => crate::rig::Combate {
                coleta: Some((1, t)),
                ..Default::default()
            },
            // O mercenário anda de arma sacada o tempo todo: é o ofício dele,
            // e é o que diz que aquele ali é a guarda e não mais um colhedor.
            (P::Mercenario, _) => crate::rig::Combate {
                conjunto: 0,
                sacada: 1.0,
                ..Default::default()
            },
            _ => Default::default(),
        };
        let entrada = crate::rig::Entrada {
            // A fase do passo anda com a DISTÂNCIA percorrida, que é o que o
            // rig pede: com o relógio no lugar dela, a perna continua
            // pedalando com o boneco parado.
            fase: passo.avanco * 0.55,
            andar: passo.andar,
            correr: 0.0,
            tempo: t,
            ar: 0.0,
            degrau: [0.0; 2],
            combate,
        };
        let pose = crate::rig::pose(&entrada);
        // MAIOR QUE A VIDA, e de propósito.
        //
        // Enquadrada a ilhota inteira, um morador em escala real tem uns
        // quatro pixels. A 3× ele já aparece, mas como um risco escuro — a
        // roupa do NPC é preta, e num risco de dez pixels não há rosto nem
        // mão que salve. A 26× (só pra olhar) dá pra ver que o modelo está
        // certo: rosto, mãos e ferramenta, tudo no lugar.
        //
        // Sete é onde ele vira gente sem virar gigante. Uma maquete é um
        // modelo, e num modelo as figuras são exageradas justamente para
        // serem lidas.
        // Um e meio, e não dois e meio.
        //
        // A escala do boneco anda junto com o resto: sete foi calibrado com a
        // câmera longe, dois e meio quando ela chegou perto, e agora a ilha
        // ficou maior no quadro (o aspecto do viewport foi corrigido) e mais
        // limpa (metade das árvores). O dono: "eles tão gigantes comparados
        // com a ilha que hoje tá simplificada". A 2,5 um morador tinha metade
        // da altura da casa ao lado dele.
        const ESCALA_DO_MORADOR: f32 = 1.5;
        // E ele OLHA PRA ONDE ANDA. Antes girava devagar em torno do próprio
        // eixo (`t * 0,25`), que é o truque de quem não tem rumo nenhum.
        let base = Mat4::from_translation(vec3(onde.x, y, onde.y))
            * Mat4::from_rotation_y(passo.yaw)
            * Mat4::from_scale(Vec3::splat(ESCALA_DO_MORADOR));
        // PELE E CABELO PRECISAM DE COR.
        //
        // O rig do NPC pinta pele e cabelo nas FAIXAS (`tools/voxrender/
        // npcs.py`), e `Vestimenta::nua` manda as faixas com alfa 0 — que
        // significa "sem cor" e sai PRETO. Na maquete os quatro moradores
        // apareciam como silhuetas negras; visto na prévia, e confirmado
        // pintando-os de vermelho num render (aí apareceram vermelhos, então
        // o desenho estava certo e a cor é que faltava).
        //
        // O mundo não tropeça nisso porque quem desenha gente lá passa por
        // `vestimenta_de`, que preenche as duas faixas.
        //
        // O índice dá a variação: quatro moradores idênticos leriam como
        // quatro cópias do mesmo boneco.
        let mut veste = Vestimenta::nua(corpo);
        veste.pele = Some(cores_da_pele((i % 4) as u8));
        veste.cor_cabelo = Some(cores_do_cabelo((i % 6) as u8));
        // E a faixa TIER, que no NPC é a ROUPA. As três faixas juntas cobrem
        // o modelo inteiro: deixar uma sem cor pinta aquela parte de preto.
        const PANOS: [[[f32; 3]; 2]; 4] = [
            [[0.58, 0.42, 0.28], [0.34, 0.24, 0.16]], // couro
            [[0.40, 0.48, 0.62], [0.22, 0.28, 0.38]], // azul
            [[0.52, 0.56, 0.44], [0.30, 0.33, 0.25]], // verde-oliva
            [[0.62, 0.36, 0.34], [0.36, 0.20, 0.19]], // terracota
        ];
        veste.tier = Some(PANOS[i % PANOS.len()]);
        desenha_rig(base, &pose, &veste, vox, None);
    }
    macroquad::material::gl_use_default_material();
    camera_padrao();
    n > 0
}

#[cfg(test)]
mod testes_da_maquete {
    use super::*;

    /// Os pontos da costa, nos dois níveis, como `dist_que_enquadra` os vê.
    fn silhueta(centro: Vec2) -> Vec<Vec3> {
        (0..96)
            .flat_map(|i| {
                let a = i as f32 / 96.0 * std::f32::consts::TAU;
                let rr = shared::colonia::raio_na_direcao(a);
                [0.0f32, shared::colonia::ALTURA_TOPO]
                    .map(|h| vec3(centro.x + a.cos() * rr, h, centro.y + a.sin() * rr))
            })
            .collect()
    }

    /// O pior ponto da ilhota, em NDC. Acima de 1 ela está CORTADA.
    fn pior_ndc(elevacao: f32, ang: f32, aspecto: f32) -> f32 {
        let centro = Vec2::ZERO;
        let alto = shared::colonia::ALTURA_TOPO;
        let d = dist_que_enquadra(centro, alto, elevacao, ang, aspecto);
        let m = matriz_da_maquete(centro, alto, elevacao, ang, aspecto, d);
        silhueta(centro).iter().fold(0.0f32, |pior, p| {
            let c = m * p.extend(1.0);
            if c.w <= 0.001 {
                return 9.0;
            }
            pior.max((c.x / c.w).abs()).max((c.y / c.w).abs())
        })
    }

    /// A ilhota CABE no quadro, em toda a órbita e em toda proporção de
    /// painel.
    ///
    /// Este teste existe porque eu entreguei três enquadramentos seguidos com
    /// a ilha cortada — em cima, embaixo e nos dois lados — e só descobri
    /// abrindo o PNG. A conta fechada que eu usava era simétrica e a
    /// perspectiva não é.
    #[test]
    fn a_ilhota_nunca_sai_do_quadro() {
        for &elev in &[ELEV_MIN, 0.3, ELEV_PADRAO, 0.9, 1.2, ELEV_MAX] {
            for giro in 0..8 {
                let ang = giro as f32 / 8.0 * std::f32::consts::TAU;
                // Do celular em pé ao painel largo do desktop.
                for &asp in &[0.62f32, 0.815, 1.25, 1.6, 2.2] {
                    let pior = pior_ndc(elev, ang, asp);
                    assert!(
                        pior <= 1.0,
                        "cortada: elev {elev:.2} ang {ang:.2} aspecto {asp:.2} -> pior NDC {pior:.3}"
                    );
                }
            }
        }
    }

    /// E ela OCUPA o quadro: enquadrar de longe demais é o outro jeito de
    /// errar, e foi o defeito que o dono viu como "a ilha fica pequena".
    #[test]
    fn a_ilhota_enche_o_quadro() {
        for &elev in &[ELEV_MIN, ELEV_PADRAO, ELEV_MAX] {
            for &asp in &[0.62f32, 0.815, 1.25, 1.6] {
                let pior = pior_ndc(elev, 0.9, asp);
                assert!(
                    pior >= 0.80,
                    "sobrou quadro demais: elev {elev:.2} aspecto {asp:.2} -> pior NDC {pior:.3}"
                );
            }
        }
    }

    fn zé() -> Morador {
        Morador {
            oficio: shared::colonia::Profissao::Lenhador,
            casa: vec2(4.0, -3.0),
            trabalho: vec2(50.0, 35.0),
        }
    }

    /// Ele SAI, TRABALHA e VOLTA — e o ciclo fecha em casa.
    ///
    /// O dono: "eles tão parados no centro da ilha e não andando entre as
    /// árvores e voltando". O teste varre o ciclo inteiro e exige as quatro
    /// coisas que faltavam.
    #[test]
    fn o_morador_sai_trabalha_e_volta() {
        let m = zé();
        let dist = (m.trabalho - m.casa).length();
        let (mut longe, mut perto, mut andou, mut trabalhou) = (0.0f32, f32::MAX, false, false);
        for k in 0..=260 {
            let t = k as f32 / 260.0 * CICLO_DO_MORADOR;
            let p = rotina_do_morador(&m, 0, t);
            let d = (p.onde - m.casa).length();
            longe = longe.max(d);
            perto = perto.min(d);
            andou |= p.andar > 0.9;
            trabalhou |= p.trabalhando;
            // Nunca sai da linha entre a casa e o ponto de trabalho.
            let na_linha = (p.onde - m.casa).length() + (p.onde - m.trabalho).length();
            assert!(na_linha <= dist + 0.01, "saiu do caminho em t={t:.1}");
            // Batendo machado só parado, e parado só no fim de um trecho.
            if p.trabalhando {
                assert!(p.andar < 0.01, "trabalhando em movimento em t={t:.1}");
                assert!(
                    (p.onde - m.trabalho).length() < 0.01,
                    "trabalhando longe do ponto"
                );
            }
        }
        assert!(
            longe > dist - 0.01,
            "nunca chegou ao trabalho (só {longe:.1} de {dist:.1})"
        );
        assert!(
            perto < 0.01,
            "nunca voltou pra casa (o mais perto foi {perto:.1})"
        );
        assert!(andou, "nunca andou de verdade");
        assert!(trabalhou, "nunca trabalhou");
    }

    /// O ciclo é FECHADO: onde ele está em t e em t+26 s é o mesmo lugar.
    #[test]
    fn o_ciclo_do_morador_fecha() {
        let m = zé();
        for k in 0..40 {
            let t = k as f32 / 40.0 * CICLO_DO_MORADOR;
            let a = rotina_do_morador(&m, 2, t).onde;
            let b = rotina_do_morador(&m, 2, t + CICLO_DO_MORADOR).onde;
            assert!((a - b).length() < 0.01, "o ciclo não fecha em t={t:.1}");
        }
    }

    /// Ele OLHA PRA ONDE ANDA: indo, pro trabalho; voltando, pra casa.
    ///
    /// Antes o boneco girava em torno do próprio eixo (`t * 0,25`) — o que
    /// nenhuma pessoa faz enquanto caminha.
    #[test]
    fn o_morador_olha_pra_onde_anda() {
        let m = zé();
        let ida = (m.trabalho - m.casa).normalize();
        for k in 0..=260 {
            let t = k as f32 / 260.0 * CICLO_DO_MORADOR;
            let p = rotina_do_morador(&m, 0, t);
            if p.andar < 0.2 {
                continue;
            }
            let olha = vec2(p.yaw.sin(), p.yaw.cos());
            // Indo, ele se afasta de casa; voltando, se aproxima.
            let indo = (p.onde - m.casa).length()
                < (rotina_do_morador(&m, 0, t + 0.2).onde - m.casa).length();
            let esperado = if indo { ida } else { -ida };
            assert!(
                olha.dot(esperado) > 0.99,
                "olhando torto em t={t:.1}: {olha:?} contra {esperado:?}"
            );
        }
    }

    /// Dois moradores não andam no mesmo compasso.
    #[test]
    fn os_moradores_nao_marcham_juntos() {
        let m = zé();
        let a = rotina_do_morador(&m, 0, 3.0).onde;
        let b = rotina_do_morador(&m, 1, 3.0).onde;
        assert!((a - b).length() > 1.0, "moradores 0 e 1 no mesmo ponto");
    }

    /// A faixa nunca estoura o retângulo que o painel deu, e fica centrada.
    #[test]
    fn a_faixa_cabe_no_retangulo_dado() {
        for (w, h) in [
            (512.0, 628.0),
            (300.0, 200.0),
            (900.0, 300.0),
            (120.0, 900.0),
        ] {
            let r = Rect::new(40.0, 70.0, w, h);
            let f = faixa_da_peca(r);
            assert!(
                f.h <= r.h + 0.01 && f.w <= r.w + 0.01,
                "{f:?} nao cabe em {r:?}"
            );
            assert!(
                (f.center().y - r.center().y).abs() < 0.01,
                "fora do centro: {f:?}"
            );
        }
    }
}
