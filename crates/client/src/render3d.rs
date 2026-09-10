//! Desenho 3D com camera de cima. Nenhuma decisao de jogo mora aqui.
//!
//! A vista e' a de MMO top-down (MIR4): camera alta, inclinada, seguindo o
//! player. Como o mundo nao precisa ser complexo, o chao e' reconstruido por
//! quadro so' com os tiles visiveis — algumas centenas de quads, mais barato
//! que manter chunk em cache e invalidar.

use macroquad::prelude::*;
use macroquad::models::{Mesh, Vertex};
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
    /// Tempo aproximado pra fechar a distancia. Mais alto e' mais macio.
    const TEMPO: f32 = 0.34;
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
            assert!((d - base).abs() < 1e-3, "pitch {p} mudou a distancia: {d} vs {base}");
        }
    }

    /// Afastar tem que FECHAR a inclinacao: deitar a camera e afastar sao a
    /// mesma vontade de ver mais longe, e as duas juntas passam da borda do
    /// mundo carregado.
    #[test]
    fn afastar_fecha_a_inclinacao() {
        let perto = pitch_min_para(ZOOM_MIN);
        let longe = pitch_min_para(ZOOM_MAX);
        assert!((perto - PITCH_MIN).abs() < 1e-5, "colado tem que deitar ate' o limite");
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
            assert!(p >= anterior - 1e-6, "piso caiu de {anterior} pra {p} no zoom {z}");
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
            assert!(p > anterior, "afastar tem que LEVANTAR: {anterior} -> {p} no zoom {z}");
            anterior = p;
        }
        // E tem que sobrar espaco pra mao nos dois sentidos.
        for z in [ZOOM_MIN, 1.0, ZOOM_MAX] {
            let p = pitch_do_zoom(z);
            assert!(p - pitch_min_para(z) > 0.05, "zoom {z}: sem espaco pra deitar na mao");
            assert!(PITCH_MAX - p > 0.05, "zoom {z}: sem espaco pra levantar na mao");
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
        assert!(
            h1 - 12.0 < 0.5 * 0.02,
            "arrancou {:.4} de 0,5 no primeiro quadro", h1 - 12.0
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
        assert!((h - 12.5).abs() < 0.01, "dois segundos depois estava em {h}");
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
        assert!(
            quadro_do_pico >= 8,
            "a camera correu mais no quadro {quadro_do_pico} — arrancou em vez de acelerar"
        );
    }

    /// A vista de repouso tem que ser EXATAMENTE a de antes de a banda
    /// existir: a banda foi pra dar liberdade, nao pra mudar o padrao.
    #[test]
    fn a_vista_de_repouso_e_a_de_sempre() {
        let cam = camera(Vec2::ZERO, 0.0, 0.0, 1.0, pitch_padrao());
        assert!((cam.position.y - CAM_HEIGHT).abs() < 1e-3, "altura {}", cam.position.y);
        assert!((cam.position.z - CAM_BACK).abs() < 1e-3, "recuo {}", cam.position.z);
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
            assert!(perto(obtido, frente), "yaw {yaw}: W deu {obtido:?}, esperado {frente:?}");
            // D e' a direita DA TELA. Com o eixo Z crescendo pra baixo na
            // tela, a direita de `(fx, fz)` e' `(-fz, fx)` — a mao troca em
            // relacao a' convencao 3D, e foi ai' que eu errei o primeiro
            // assert (o codigo estava certo, o teste e' que nao).
            let dir_d = input_para_mundo(d, yaw);
            let direita = vec2(-frente.y, frente.x);
            assert!(perto(dir_d, direita), "yaw {yaw}: D deu {dir_d:?}, esperado {direita:?}");
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

impl<'a> Vista<'a> {
    /// Monta a vista do quadro: camera girada, levantada pra o relevo nao
    /// tapar o jogador, e a funcao de chao que todo o resto vai consultar.
    pub fn nova(
        alvo: Vec2,
        yaw: f32,
        zoom: f32,
        pitch: f32,
        apoio: f32,
        chao: &'a dyn Fn(f32, f32) -> f32,
    ) -> Self {
        let sobe = altura_livre(alvo, apoio, yaw, zoom, pitch, chao);
        Self { cam: camera(alvo, apoio + sobe, yaw, zoom, pitch), chao }
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

/// Sobe a camera ate' o jogador ficar visivel.
///
/// Com a camera girando, qualquer morro entre ela e o jogador tapa a vista, e
/// perder o boneco atras do relevo e' pior que qualquer outra falha de camera.
/// Em vez de raycast contra a malha, amostra a ALTURA ao longo da linha —
/// o campo de altura ja' esta' ali, e e' consulta O(1) por amostra.
pub fn altura_livre(
    target: Vec2,
    chao: f32,
    yaw: f32,
    zoom: f32,
    pitch: f32,
    altura_em: &dyn Fn(f32, f32) -> f32,
) -> f32 {
    let z = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let (recuo, altura) = recuo_e_altura(z, pitch);
    let mut extra: f32 = 0.0;
    const AMOSTRAS: i32 = 10;
    for i in 1..=AMOSTRAS {
        let t = i as f32 / AMOSTRAS as f32;
        let x = target.x - yaw.sin() * recuo * t;
        let zz = target.y + yaw.cos() * recuo * t;
        // Altura da linha camera→alvo neste ponto, se a camera nao subisse.
        let linha = chao + altura * t;
        let solo = altura_em(x, zz);
        // Uma folga acima do solo: rasar o morro deixa a camera dentro dele.
        extra = extra.max(solo + 1.5 - linha);
    }
    extra.max(0.0)
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
            verts.push(Vertex { position: v, uv: vec2(0.0, 0.0), color, normal: Vec4::ZERO });
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
        draw_mesh(&Mesh { vertices: verts, indices: idx, texture: None });
    }
}

/// Modelo `.vox` de cada entidade.
///
/// Orcamento de arte: **mob comum e' barato, boss e' caro**. O `lobo.vox`
/// original da 14.112 triangulos — isso e' modelo de boss, do qual ha um na
/// tela. Mob comum usa a versao reduzida (`tools/voxrender/voxsimplify.py`),
/// menor e com uma fracao dos triangulos, porque ha dezenas deles.
fn model_for(tag: shared::EntityTag, boss: bool) -> Option<&'static str> {
    use shared::EntityTag as T;
    match tag {
        T::Player | T::Npc => Some("player"),
        T::Enemy if boss => Some("lobo"),
        T::Enemy => Some("lobo_pequeno"),
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
    for id in order {
        let Some(e) = world.ents.get(&id) else { continue };
        let p = vista.pos_de(e);

        // Marca do alvo: anel no chao, que e' como MMO de target sinaliza.
        if Some(id) == target {
            draw_ring(p, 0.55, Color::from_rgba(241, 200, 112, 255));
        }

        let boss = e.state.flags & shared::ent_flags::BOSS != 0;
        let drawn = model_for(e.meta.tag, boss)
            .and_then(|name| vox.peek(name))
            .map(|meshes| {
                // A malha nasce centrada em X/Z e apoiada em Y=0; girar em
                // torno de Y bastaria, mas a macroquad nao transforma malha —
                // entao a rotacao vira quando houver malha por direcao.
                for m in meshes {
                    draw_mesh_at(m, p, e.yaw);
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
}

/// Desenha uma malha girada em Y e deslocada.
///
/// A macroquad nao tem transform por malha, entao a matriz e' aplicada nos
/// vertices na CPU. Cabe porque os modelos sao pequenos depois do greedy
/// meshing (o player tem 356 triangulos); se o bestiario crescer, o caminho e'
/// um shader com uniform de modelo.
fn draw_mesh_at(m: &Mesh, at: Vec3, yaw: f32) {
    let (sin, cos) = yaw.sin_cos();
    let moved = Mesh {
        vertices: m
            .vertices
            .iter()
            .map(|v| {
                let q = v.position;
                Vertex {
                    position: vec3(
                        q.x * cos + q.z * sin + at.x,
                        q.y + at.y,
                        -q.x * sin + q.z * cos + at.z,
                    ),
                    ..*v
                }
            })
            .collect(),
        indices: m.indices.clone(),
        texture: None,
    };
    draw_mesh(&moved);
}

/// Projeta um ponto do mundo pra pixel de tela.
///
/// Necessario pra mirar com o mouse: a selecao de alvo compara a distancia em
/// PIXELS entre o cursor e cada entidade, que e' o que o jogador enxerga.
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
        if e.is_self() {
            continue;
        }
        let Some(sp) = vista.na_tela(vista.mira_de(e)) else { continue };
        let d = sp.distance(mouse);
        if d <= raio_px && melhor.map_or(true, |(bd, _)| d < bd) {
            melhor = Some((d, *id));
        }
    }
    melhor.map(|(_, id)| id)
}

fn draw_ring(center: Vec3, r: f32, color: Color) {
    const N: usize = 24;
    for i in 0..N {
        let a0 = i as f32 / N as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / N as f32 * std::f32::consts::TAU;
        draw_line_3d(
            center + vec3(a0.cos() * r, 0.02, a0.sin() * r),
            center + vec3(a1.cos() * r, 0.02, a1.sin() * r),
            color,
        );
    }
}
