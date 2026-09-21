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
    let z = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let t = (z - ZOOM_MIN) / (ZOOM_MAX - ZOOM_MIN);
    PITCH_MIN + (PITCH_MIN_LONGE - PITCH_MIN) * t
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
/// perseguicao serve pro degrau, pra escada e pra queda. Criticamente
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
    /// entao um degrau isolado saia macio e uma escada saia chicoteando.
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

    uniform mat4 Model;
    uniform mat4 Projection;
    // Clarao do golpe / corpo escurecido: puxa o rgb pra `Tinta.rgb` na
    // fracao `Tinta.a` (zero = cor do voxel). Antes era recopia na CPU.
    uniform vec4 Tinta;

    void main() {
        gl_Position = Projection * Model * vec4(position, 1);
        color = color0 / 255.0;
        color.rgb = mix(color.rgb, Tinta.rgb, Tinta.a);
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

    uniform sampler2D Texture;
    uniform highp vec3 Recorte;
    uniform highp float RecorteZ;

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

pub fn material_solido() -> Material {
    load_material(
        ShaderSource::Glsl {
            vertex: SOLIDO_VERTICE,
            fragment: SOLIDO_FRAGMENTO,
        },
        MaterialParams {
            uniforms: vec![
                UniformDesc::new("Recorte", UniformType::Float3),
                UniformDesc::new("RecorteZ", UniformType::Float1),
                UniformDesc::new("Tinta", UniformType::Float4),
            ],
            pipeline_params: params_solido(),
            ..Default::default()
        },
    )
    .expect("shader do mundo")
}

pub fn clear() {
    // Ceu, e nao quase-preto. O fundo aparece em todo horizonte e em todo vao
    // do relevo; escuro ele le' como buraco na malha — foi exatamente o que me
    // fez cacar bug de geometria por um bom tempo.
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
    match kind {
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
pub const MODELOS_DE_GENTE: [&str; 4] = ["player", "pistoleiro", "mago", "arqueiro"];

/// Os NPCs da vila, um rig de dez pecas por oficio (`tools/voxrender/npcs.py`).
pub const MODELOS_DE_NPC: [&str; 15] = [
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
        _ => ALDEOES[(id % ALDEOES.len() as u64) as usize],
    }
}

/// O bau da dungeon: madeira, faixas de ouro e um anel que pulsa no chao.
fn desenha_bau(p: Vec3) {
    let t = get_time() as f32;
    let madeira = Color::from_rgba(122, 78, 40, 255);
    let escura = Color::from_rgba(86, 52, 26, 255);
    let ouro = Color::from_rgba(236, 190, 84, 255);
    draw_cube(
        p + vec3(0.0, 0.38, 0.0),
        vec3(1.2, 0.76, 0.8),
        None,
        madeira,
    );
    draw_cube(
        p + vec3(0.0, 0.86, 0.0),
        vec3(1.26, 0.24, 0.86),
        None,
        escura,
    );
    for dx in [-0.42, 0.42] {
        draw_cube(p + vec3(dx, 0.5, 0.0), vec3(0.1, 0.98, 0.84), None, ouro);
    }
    draw_cube(
        p + vec3(0.0, 0.66, 0.42),
        vec3(0.18, 0.22, 0.06),
        None,
        ouro,
    );
    let a = 0.55 + 0.35 * (t * 3.0).sin();
    draw_ring(
        p,
        0.95 + 0.08 * (t * 3.0).sin(),
        Color::new(0.95, 0.78, 0.35, a),
    );
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
    match shared::bosses::kind_do_corpo(e.meta.kind) {
        2 => Some("humanoides/pistoleiro"),
        4 => Some("humanoides/mago"),
        6 => Some("humanoides/arqueiro"),
        _ => None,
    }
}

pub fn draw_entities(
    world: &mut World,
    vox: &VoxCache,
    target: Option<shared::EntityId>,
    vista: &Vista,
) {
    let order: Vec<_> = world.draw_order().to_vec();
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
            brilhos.extend(desenha_personagem(e, corpo, None, vox, vista, false));
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
            let nome = rig_do_npc(shared::npc_papel_de_kind(e.meta.kind), e.meta.id.0 as u64);
            if let Some(corpo) = vox.rig(nome) {
                brilhos.extend(desenha_personagem(e, corpo, None, vox, vista, false));
                continue;
            }
        }
        // Chefe pirata: o corpo do personagem com o chapeu, na escala de chefe.
        if boss
            && shared::bosses::chefe(e.meta.kind)
                .is_some_and(|c| c.corpo == shared::bosses::Corpo::Pirata)
        {
            if let Some(corpo) = vox.rig(RIG_CORPO) {
                brilhos.extend(desenha_personagem(
                    e,
                    corpo,
                    vox.rig(RIG_CHAPEU),
                    vox,
                    vista,
                    false,
                ));
                continue;
            }
        }
        // Gente (jogador, NPC) e' desenhada em PECAS, com a pose do quadro.
        if matches!(
            e.meta.tag,
            shared::EntityTag::Player | shared::EntityTag::Npc
        ) {
            if let Some(corpo) = vox.rig(RIG_CORPO) {
                brilhos.extend(desenha_personagem(
                    e,
                    corpo,
                    vox.rig(RIG_CHAPEU),
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
            Brilho::Circulo(pulso, t) => desenha_circulo(*pulso, *t),
            Brilho::Mao(p, forca, t) => desenha_mao(*p, *forca, *t),
        }
    }
    // Lascas e faiscas da coleta: um pool so', avancado uma vez por quadro.
    crate::lascas::avanca_e_desenha(get_frame_time());
}

/// Os arquivos de pecas do personagem (docs/character create.md).
pub const RIG_CORPO: &str = "personagem/corpo";
pub const RIG_CHAPEU: &str = "personagem/cabelo_01";

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
    corpo: &std::collections::HashMap<String, Vec<Mesh>>,
    chapeu: Option<&std::collections::HashMap<String, Vec<Mesh>>>,
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
    let dash = e.morte.is_none() && (e.dash_visual_ate > get_time()
        || e.state.flags & shared::ent_flags::DASHING != 0);
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
            .and_then(|(esp, grau)| vox.bicho(esp.bicho).map(|b| (esp, grau, b)))
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
        entrada.combate.conjunto = if shared::bosses::kind_do_corpo(e.meta.kind) == 4 {
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
        crate::rig::aplica_montado(&mut pose, entrada.tempo);
    }
    if e.skill.is_some_and(|(id, _, _)| id == 9) {
        pose.na_mao = false;
    }
    if humanoide && e.meta.kind == 6 {
        crate::rig::aplica_arqueiro(&mut pose, entrada.combate.golpe.map(|(_, t)| t));
    }
    e.molas.segue(&mut pose, get_frame_time());
    if dash {
        crate::rig::aplica_dash(&mut pose);
        let frente = vec3(sin, 0.0, cos);
        let lado = vec3(cos, 0.0, -sin);
        for i in [-1.0, 1.0] {
            let inicio = p + vec3(0.0, 0.65, 0.0) + lado * (i * 0.28);
            draw_line_3d(inicio, inicio - frente * 1.1,
                Color::new(0.55, 0.85, 1.0, 0.45));
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
            desenha_montaria(e, m, s, b, p);
            let frente = vec3(e.yaw.sin(), 0.0, e.yaw.cos());
            Vec3::Y * (m.sela - crate::rig::altura_do_quadril(VOXEL)) + frente * m.sela_frente
        }
        None => Vec3::ZERO,
    };
    let base = Mat4::from_translation(p + vec3(0.0, sobe, 0.0) + desloca + sela)
        * Mat4::from_scale(Vec3::splat(esc))
        * Mat4::from_rotation_y(e.yaw + giro)
        * Mat4::from_rotation_x(-cai + inclina)
        * Mat4::from_scale(vec3(1.0 + 0.5 * s, 1.0 - s, 1.0 + 0.5 * s));
    let (mats, armas) = desenha_rig(base, &pose, corpo, chapeu, vox, clarao(e, eu));
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
    if humanoide && e.meta.kind == 6 {
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
                brilhos.push(Brilho::Circulo(pulsos[i], t));
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
            brilhos.push(Brilho::Mao(*p, forca, agora + i as f32 * 1.7));
        }
    }
    brilhos
}

/// O brilho do anel numa mao: um nucleo claro, dois halos violeta pulsando
/// e tres faiscas orbitando. O nucleo vai PRIMEIRO: o halo por cima dele o
/// deixa passar; na ordem contraria o halo escreveria profundidade na frente
/// e o nucleo sumiria.
fn desenha_mao(p: Vec3, forca: f32, t: f32) {
    let pulso = 1.0 + 0.15 * (t * 6.0).sin();
    let alfa = |x: f32| (x * forca.min(1.0)).clamp(0.0, 255.0) as u8;
    octaedro(
        p,
        0.045 * pulso * forca.max(0.6),
        [255, 240, 255, alfa(255.0)],
    );
    octaedro(p, 0.10 * pulso * forca, [205, 150, 255, alfa(120.0)]);
    octaedro(p, 0.19 * pulso * forca, [170, 110, 255, alfa(45.0)]);
    for k in 0..3 {
        let ang = t * 4.0 + k as f32 * 2.094;
        let q = p + vec3(
            ang.cos() * 0.16,
            (t * 3.0 + k as f32).sin() * 0.06,
            ang.sin() * 0.16,
        ) * forca.max(0.7);
        octaedro(q, 0.02, [235, 210, 255, alfa(220.0)]);
    }
}

/// O que brilha por cima do mundo, transparente, depois de todo mundo: o
/// rastro da lamina, o clarao do cano e o circulo do anel.
pub enum Brilho {
    Fita(Vec<(Vec3, Vec3, f32)>),
    /// Posicao e 0..1 da vida do clarao.
    Clarao(Vec3, f32),
    /// O pulso e o instante do golpe.
    Circulo(Mat4, f32),
    /// O brilho do anel na mao: onde, com que forca e a fase do pulso.
    Mao(Vec3, f32, f32),
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
fn desenha_circulo(pulso: Mat4, t: f32) {
    let imp = crate::rig::IMPACTO;
    let aceso = if t < imp {
        t / imp
    } else {
        (1.0 - (t - imp) / 0.25).max(0.0)
    };
    faixa_circular(
        pulso,
        0.0,
        7.0 * VOXEL,
        2.0 * VOXEL,
        [190, 130, 255, (aceso * 230.0) as u8],
    );
    if t > imp {
        let u = ((t - imp) / 0.22).min(1.0);
        faixa_circular(
            pulso,
            -(3.0 + 18.0 * u) * VOXEL,
            (7.0 + 12.0 * u) * VOXEL,
            (2.2 * (1.0 - u) + 0.5) * VOXEL,
            [215, 170, 255, ((1.0 - u) * 200.0) as u8],
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
        octaedro(alto, 0.15, [205, 150, 255, 255]);
        octaedro(alto, 0.27, [170, 110, 255, 90]);
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
/// a bolsa, a origem do retrato. Devolve onde ficaram a espada e o escudo.
pub fn desenha_rig(
    base: Mat4,
    pose: &crate::rig::Pose,
    corpo: &std::collections::HashMap<String, Vec<Mesh>>,
    chapeu: Option<&std::collections::HashMap<String, Vec<Mesh>>>,
    vox: &VoxCache,
    tinta: Option<([f32; 3], f32)>,
) -> ([Mat4; crate::rig::N], Vec<(&'static str, Mat4)>) {
    let mats = crate::rig::matrizes(pose, base, VOXEL);
    for (i, (nome, _, _)) in crate::rig::PECAS.iter().enumerate() {
        let malhas = if *nome == "cabelo" {
            chapeu.and_then(|c| c.get("cabelo"))
        } else {
            corpo.get(*nome)
        };
        for m in malhas.into_iter().flatten() {
            draw_mesh_mat_tinta(m, &mats[i], tinta);
        }
    }
    // As armas do conjunto: na mao em combate, guardadas fora dele — e o que
    // se veste junto (bainha, coldres).
    let armas = crate::rig::armas(pose, &mats, VOXEL);
    for (nome, mat) in &armas {
        for m in vox.arma(nome).into_iter().flatten() {
            draw_mesh_mat(m, mat);
        }
    }
    (mats, armas)
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
) {
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
    );
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
    let Some((especie, grau)) = shared::montarias::de_item(item_id) else {
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
) {
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
    // Na GPU: a malha do voxel sobe uma vez e a matriz e a tinta vao por
    // uniforme. Recopiar os vertices todo quadro era o que pesava com bicho
    // em volta (ver `gpu_estatica`).
    let tinta = tinta.map_or([0.0; 4], |(c, k)| [c[0], c[1], c[2], k]);
    crate::gpu_estatica::desenha_voxel(m, *mat, tinta);
}

/// Desenha uma malha girada em Y e deslocada — com a matriz de modelo no
/// shader (`gpu_estatica::desenha_voxel`), sem recopiar vertice.
fn draw_mesh_at(m: &Mesh, at: Vec3, yaw: f32) {
    // Mesma conta de antes (y pra cima, giro em Y), como matriz.
    let mat = Mat4::from_translation(at) * Mat4::from_rotation_y(yaw);
    crate::gpu_estatica::desenha_voxel(m, mat, [0.0; 4]);
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
