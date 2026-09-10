//! Estado visual do mundo entre snapshots.
//!
//! O servidor tica a 30Hz e o cliente desenha a 60+. Desenhar direto a posicao
//! do snapshot faz tudo andar aos trancos, entao cada entidade guarda uma
//! posicao RENDERIZADA que persegue a autoritativa.
//!
//! Nada aqui e' simulacao: a posicao alvo vem sempre do servidor, o cliente so'
//! suaviza o caminho ate ela. Sem prediction, sem regra de jogo.

use std::collections::HashMap;

use macroquad::prelude::*;
use shared::{ent_flags, EntityId, EntityMeta, EntityState};

/// A macroquad embute a propria glam e reexporta o nome no prelude, entao os
/// dois `Vec2` sao tipos distintos pro compilador. A conversao fica so' na
/// fronteira com o `shared`.
#[inline]
fn mq(v: ::glam::Vec2) -> Vec2 {
    Vec2::new(v.x, v.y)
}

/// Constante da suavizacao exponencial. Maior = mais colado no servidor e mais
/// tranco; menor = mais macio e mais atrasado. 18 da ~90% do caminho em 130ms,
/// que cobre o intervalo de 33ms entre snapshots com folga.
const SMOOTH_K: f32 = 18.0;
/// Abaixo disto a entidade e' considerada parada.
const MOVING_EPS: f32 = 0.05;
/// Velocidade de SUBIR DEGRAU, em unidades por segundo.
///
/// O corpo nao salta pro topo do bloco: ele sobe. Meio bloco (0,5) leva 125 ms
/// — rapido o bastante pra nao virar elevador, devagar o bastante pra o olho
/// ver o pe' passando por cima da quina em vez de o boneco piscar meio metro
/// pra cima.
///
/// Nao vale pra DESCER: descer e' queda, e queda tem gravidade.
const SUBIDA: f32 = 4.0;
/// Fracao da subida que acontece ANTES de cruzar a quina.
///
/// Meio a meio, e o motivo e' que os dois extremos sao igualmente ruins:
/// subir so' depois deixa o corpo com os pes dentro da terra durante a
/// subida inteira; subir tudo antes deixa ele flutuando a mesma altura, so'
/// que do outro lado. Centrar na travessia divide o erro pela metade nos dois
/// sentidos — e e' o que um passo de verdade faz, com o corpo no meio da
/// altura bem em cima da quina.
const ANTECIPACAO: f32 = 0.5;
/// Desnivel que a antecipacao aceita, em unidades. Um bloco.
///
/// Acima disso e' parede ou pulo — e nenhum dos dois deve levantar o corpo
/// so' porque o jogador esta' andando na direcao.
const DEGRAU_VISIVEL: f32 = 0.5;

pub struct Ent {
    /// Dado estavel, recebido uma vez quando a entidade entrou no AOI.
    pub meta: EntityMeta,
    /// Ultimo estado autoritativo.
    pub state: EntityState,
    /// Posicao desenhada, em tiles. Persegue a do servidor.
    pub render_pos: Vec2,
    /// Angulo em torno de Y, em radianos. Em 3D a direcao e' continua — nao
    /// ha 4 ou 8 sprites pra escolher —, entao ela persegue a velocidade.
    pub yaw: f32,
    /// Altura desenhada. Persegue o apoio em vez de saltar pra ele: degrau de
    /// meio metro trocado de uma vez faz o modelo piscar pra cima.
    /// `f32::MIN` = ainda nao apoiado (o primeiro quadro assenta sem animar).
    pub render_y: f32,
    /// Quanto tempo esta entidade ja' esta' no ar, em segundos.
    ///
    /// O servidor manda "esta pulando", nao a altura: o arco e' enfeite e nao
    /// precisa gastar byte de rede nem confianca. O cliente conta o tempo e
    /// desenha a parabola.

    /// Segundos de arco garantidos sem esperar o servidor.
    ///
    /// So' o jogador local usa: apertar a tecla comeca o arco NO QUADRO, sem
    /// esperar a confirmacao voltar. E' seguro porque o arco e' enfeite — a
    /// subida de verdade continua sendo decidida no servidor. Se o servidor
    /// recusar (cooldown), o unico prejuizo e' uma animacao curta a toa.
    pub pulo_local: f32,
    /// Velocidade de queda, em unidades por segundo (negativa = caindo).
    pub vel_y: f32,
    /// Estava no ar no quadro anterior? So' serve pra achar a borda de subida.
    pub no_ar_antes: bool,
    /// O corpo esta' no ar — subindo ou caindo. Enquanto estiver, a altura e'
    /// integrada em vez de seguir o chao.
    pub voando: bool,
    /// Espelho local da espera entre pulos do servidor, em segundos.
    ///
    /// Sem ela o palpite do cliente e a regra do servidor discordam na janela
    /// entre o fim do arco e o fim do cooldown: o arco tocaria sem que pulo
    /// nenhum tivesse acontecido.
    pub espera_pulo: f32,
}

impl Ent {
    pub fn is_self(&self) -> bool {
        self.state.flags & ent_flags::SELF != 0
    }
}

#[derive(Default)]
pub struct World {
    pub ents: HashMap<EntityId, Ent>,
    /// Ordem estavel de desenho (y crescente) recalculada por quadro.
    order: Vec<EntityId>,
    pub self_id: Option<EntityId>,
}

impl World {
    /// Aplica um tick em DELTA.
    ///
    /// `entered` traz o dado estavel de quem acabou de entrar no campo de
    /// visao; `states` traz so' quem mudou; `removed` quem saiu. Entidade
    /// ausente das tres listas esta parada — nao sumiu.
    pub fn apply(
        &mut self,
        entered: Vec<EntityMeta>,
        states: Vec<EntityState>,
        removed: &[EntityId],
    ) {
        for meta in entered {
            let id = meta.id;
            // O estado real vem no mesmo pacote, logo abaixo.
            let state = EntityState { id, pos: [0, 0], vel: [0, 0], hp: 0, flags: 0 };
            self.ents.entry(id).or_insert(Ent {
                meta,
                state,
                render_pos: Vec2::ZERO,
                yaw: 0.0,
                render_y: f32::MIN,
                pulo_local: 0.0,
                vel_y: 0.0,
                no_ar_antes: false,
                voando: false,
                espera_pulo: 0.0,
            });
        }
        for st in states {
            let Some(ent) = self.ents.get_mut(&st.id) else { continue };
            // Entidade recem-criada nasce ja na posicao certa, senao ela
            // desliza do canto do mundo ate o lugar dela.
            if ent.state.pos == [0, 0] {
                ent.render_pos = mq(st.pos_f32());
            }
            if st.flags & ent_flags::SELF != 0 {
                self.self_id = Some(st.id);
            }
            ent.state = st;
        }
        for id in removed {
            self.ents.remove(id);
            if self.self_id == Some(*id) {
                self.self_id = None;
            }
        }
    }

    /// `chao` da' a altura de apoio: e' o mesmo campo de altura que o servidor
    /// usa pra colisao, entao a entidade pisa exatamente onde ela pisa la'.
    pub fn tick(&mut self, dt: f32, chao: &dyn Fn(f32, f32) -> f32) {
        // Peso da suavizacao independente do frame rate.
        let a = 1.0 - (-SMOOTH_K * dt).exp();

        for ent in self.ents.values_mut() {
            let target = mq(ent.state.pos_f32());
            ent.render_pos += (target - ent.render_pos) * a;

            // ── VERTICAL ──
            //
            // O corpo tem altura PROPRIA quando esta' no ar. Antes o arco era
            // somado por cima do chao, e o chao troca de degrau de uma vez: o
            // resultado era o boneco colando no piso de cima em vez de subir,
            // e caindo de um barranco sem cair. Duas verdades sobre a mesma
            // altura.
            //
            // Agora e' uma so': no ar, integra a gravidade; no chao, segue o
            // apoio. A gravidade e o impulso vem de `shared`, os mesmos que o
            // servidor usa pra decidir que degrau o pulo vence.
            let apoio = chao(ent.render_pos.x, ent.render_pos.y);
            // ── DEGRAU ANTECIPADO ──
            //
            // Quem decide a posicao e' o servidor, e ele poe o corpo em cima
            // do bloco alto DE UMA VEZ. Se a subida visual so' comecar depois
            // disso, o corpo passa a subida inteira com os pes dentro da
            // terra — foi o que apareceu quando a subida virou taxa constante.
            //
            // Entao o corpo olha um pouco a' frente e comeca a subir ANTES de
            // chegar. Quando o servidor o move, ele ja' esta' na altura certa.
            // So' vale pra degrau que da' pra subir: parede alta nao levanta
            // ninguem, e parado tambem nao — senao o boneco flutua encostado
            // num barranco.
            let indo = mq(ent.state.vel_f32());
            let alvo_alto = if indo.length_squared() > MOVING_EPS * MOVING_EPS {
                // Olha a' frente a distancia que o corpo percorre em metade do
                // tempo de subida. Sai da velocidade e nao de uma constante:
                // correndo, o degrau chega antes e a subida tem que comecar
                // mais cedo; parado, nao ha' o que antecipar.
                let olhada = indo.length() * (DEGRAU_VISIVEL / SUBIDA) * ANTECIPACAO;
                let frente = ent.render_pos + indo.normalize() * olhada;
                let a = chao(frente.x, frente.y);
                if a > apoio && a - apoio <= DEGRAU_VISIVEL { a } else { apoio }
            } else {
                apoio
            };
            // Assenta ANTES de qualquer outra coisa: entidade recem-vista nao
            // pode cair do ceu, e se ela chegou no meio de um pulo o arco
            // comeca do chao dela. Fazer isto depois do impulso apagava o
            // pulo do proprio quadro em que ele comecou.
            if ent.render_y == f32::MIN {
                ent.render_y = apoio;
                ent.vel_y = 0.0;
                ent.voando = false;
            }

            ent.pulo_local = (ent.pulo_local - dt).max(0.0);
            let no_ar_agora = ent.state.flags & ent_flags::PULANDO != 0 || ent.pulo_local > 0.0;
            if no_ar_agora && !ent.no_ar_antes {
                // Impulso inicial do arco: `4h/T` poe o pico em `PULO_ALTURA`
                // na metade de `PULO_DURACAO`.
                ent.vel_y = 4.0 * shared::PULO_ALTURA / shared::PULO_DURACAO;
                ent.voando = true;
            }
            ent.no_ar_antes = no_ar_agora;

            if ent.voando {
                // Trapezio e nao Euler: com aceleracao constante, a media das
                // velocidades do inicio e do fim do passo e' EXATA. Euler
                // subestimava o pico em 6% e o arco desenhado discordava da
                // `altura_do_pulo` que o servidor usa pra liberar o degrau —
                // e o erro mudava com a taxa de quadros do jogador.
                let v = ent.vel_y - shared::gravidade() * dt;
                ent.render_y += (ent.vel_y + v) * 0.5 * dt;
                ent.vel_y = v;
                // Pousa no que houver embaixo: o piso de cima, o de baixo, ou
                // o mesmo de onde saiu. Quem decide onde o corpo ESTA' e' o
                // servidor; a altura so' encontra o chao dele.
                if ent.render_y <= apoio {
                    ent.render_y = apoio;
                    ent.vel_y = 0.0;
                    ent.voando = false;
                }
            } else if ent.render_y > alvo_alto + 0.02 {
                // Andou pra fora de um barranco: cai, nao afunda. Interpolar
                // aqui comeca rapido e vai freando — o contrario da gravidade.
                //
                // A comparacao e' contra o degrau ANTECIPADO e nao contra o
                // chao de baixo: subindo pra um degrau, o corpo passa alguns
                // quadros acima do bloco em que ainda pisa, e comparar com o
                // chao faria a queda comer a subida — o corpo caia de volta e
                // so' entao era teleportado pro topo.
                ent.voando = true;
                ent.vel_y = 0.0;
            } else {
                // No chao, subindo degrau: TAXA CONSTANTE, nao exponencial.
                //
                // A exponencial anda o grosso do caminho nos dois primeiros
                // quadros e vai freando — o degrau lia como teleporte com um
                // rastro. Taxa constante gasta o mesmo tempo em todo o degrau,
                // e e' isso que faz o passo ler como passo: meio bloco em 125
                // ms, rapido o bastante pra nao parecer elevador.
                ent.vel_y = 0.0;
                // Sobe em direcao ao degrau que vem, com um piso.
                //
                // O piso NAO e' o chao. Proibir qualquer afundamento parece
                // certo e e' o proprio salto: chegando na quina com meio
                // degrau subido, "nunca abaixo do chao" empurra a outra
                // metade num quadro so'. O piso e' a metade que a antecipacao
                // ja' paga do outro lado — abaixo disso e' teleporte de
                // verdade (nascer, respawn) e ai' sim vale cortar.
                let piso = apoio - DEGRAU_VISIVEL * ANTECIPACAO * 1.05;
                ent.render_y = (ent.render_y + SUBIDA * dt)
                    .min(alvo_alto)
                    .max(piso);
            }

            let vel = mq(ent.state.vel_f32());
            if vel.length_squared() > MOVING_EPS * MOVING_EPS {
                // O modelo nasce olhando pro +Z do mundo.
                let want = vel.x.atan2(vel.y);
                // Caminho mais curto no circulo, senao ele gira 350 graus pra
                // virar 10.
                let mut d = want - ent.yaw;
                while d > std::f32::consts::PI { d -= std::f32::consts::TAU; }
                while d < -std::f32::consts::PI { d += std::f32::consts::TAU; }
                ent.yaw += d * a;
            }
        }
    }

    /// Comeca o arco do jogador local sem esperar o servidor.
    pub fn pular_local(&mut self) {
        let Some(id) = self.self_id else { return };
        if let Some(e) = self.ents.get_mut(&id) {
            if !e.voando && e.espera_pulo <= 0.0 {
                e.pulo_local = shared::PULO_DURACAO;
                e.espera_pulo = shared::PULO_DURACAO + shared::PULO_ESPERA;
            }
        }
    }

    pub fn self_pos(&self) -> Option<Vec2> {
        self.self_id.and_then(|id| self.ents.get(&id)).map(|e| e.render_pos)
    }

    /// Entidades ordenadas por y — quem esta mais ao sul desenha por cima.
    pub fn draw_order(&mut self) -> &[EntityId] {
        self.order.clear();
        self.order.extend(self.ents.keys().copied());
        let ents = &self.ents;
        self.order.sort_by(|a, b| {
            let ya = ents[a].render_pos.y;
            let yb = ents[b].render_pos.y;
            ya.partial_cmp(&yb).unwrap_or(std::cmp::Ordering::Equal)
        });
        &self.order
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use shared::{EntityTag, PULO_DURACAO, PULO_ESPERA};

    /// Quantas vezes o corpo sobe e volta ao chao numa sequencia de alturas.
    ///
    /// O chao do teste e' zero, entao `render_y` E' a altura do arco.
    fn arcos(alturas: &[f32]) -> usize {
        let mut n = 0;
        let mut no_ar = false;
        for &h in alturas {
            if h > 0.02 && !no_ar {
                n += 1;
                no_ar = true;
            } else if h <= 0.02 {
                no_ar = false;
            }
        }
        n
    }

    /// Roda o mundo por `segundos`, com a bandeira PULANDO ligada nos
    /// primeiros `voo` segundos — que e' o que o servidor manda.
    fn simular(voo: f32, segundos: f32) -> Vec<f32> {
        let dt = 1.0 / 60.0;
        let mut w = World::default();
        let id = shared::EntityId(1);
        w.apply(
            vec![EntityMeta { id, tag: EntityTag::Player, name: None, hp_max: 100, faction: None }],
            vec![EntityState { id, pos: [16, 16], vel: [0, 0], hp: 100, flags: ent_flags::SELF }],
            &[],
        );
        w.pular_local();
        let chao = |_: f32, _: f32| 0.0;
        let mut alturas = Vec::new();
        let mut t = 0.0f32;
        while t < segundos {
            // O servidor so' confirma depois de um tick — antes disso quem
            // segura o arco e' o palpite local.
            let flags = ent_flags::SELF
                | if (0.05..voo).contains(&t) { ent_flags::PULANDO } else { 0 };
            w.apply(
                Vec::new(),
                vec![EntityState { id, pos: [16, 16], vel: [0, 0], hp: 100, flags }],
                &[],
            );
            w.tick(dt, &chao);
            alturas.push(w.ents[&id].render_y);
            t += dt;
        }
        alturas
    }

    /// UM aperto, UM arco. O bug era o relogio do arco voltar quando o pulo
    /// acabava: a parabola era percorrida de tras pra frente e o corpo subia
    /// pela segunda vez.
    #[test]
    fn um_aperto_da_um_pulo_so() {
        let alturas = simular(PULO_DURACAO, PULO_DURACAO * 3.0);
        assert_eq!(arcos(&alturas), 1, "um aperto tem que dar um arco so'");
        let pico = alturas.iter().cloned().fold(0.0f32, f32::max);
        assert!(
            (pico - shared::PULO_ALTURA).abs() < 0.05,
            "o pico tem que ser PULO_ALTURA, foi {pico}"
        );
        assert!(alturas.last().is_some_and(|&h| h == 0.0), "tem que acabar no chao");
    }

    /// ANDANDO ATE' UM DEGRAU: o corpo sobe, e NUNCA fica dentro da terra.
    ///
    /// Sao as duas coisas juntas, e uma sozinha nao serve. Sem a subida, o
    /// degrau e' teleporte. Sem o limite, a subida acontece com os pes dentro
    /// do bloco — porque quem move o corpo e' o servidor, e ele poe o jogador
    /// em cima do bloco alto de uma vez.
    ///
    /// A saida e' o corpo olhar a' frente e comecar a subir ANTES de chegar.
    /// E' isso que este teste mede: caminhada de verdade, chao que sobe meio
    /// bloco no meio dela.
    #[test]
    fn sobe_o_degrau_sem_entrar_na_terra() {
        let dt = 1.0 / 60.0;
        let vel = 5.0f32;
        // Chao: meio bloco mais alto a partir de x = 2.
        let chao = |x: f32, _z: f32| if x >= 2.0 { 0.5 } else { 0.0 };

        let mut w = World::default();
        let id = shared::EntityId(1);
        let estado = |x: f32| EntityState {
            id,
            pos: [(x * shared::POS_SCALE) as i16, 0],
            vel: [(vel * shared::POS_SCALE) as i8, 0],
            hp: 100,
            flags: ent_flags::SELF,
        };
        w.apply(
            vec![EntityMeta { id, tag: EntityTag::Player, name: None, hp_max: 100, faction: None }],
            vec![estado(0.0)],
            &[],
        );
        w.tick(dt, &chao);

        let mut x = 0.0f32;
        let (mut afundou, mut flutuou, mut maior_passo) = (0.0f32, 0.0f32, 0.0f32);
        for _ in 0..180 {
            x += vel * dt;
            w.apply(Vec::new(), vec![estado(x)], &[]);
            let antes = w.ents[&id].render_y;
            w.tick(dt, &chao);
            let e = &w.ents[&id];
            let solo = chao(e.render_pos.x, e.render_pos.y);
            afundou = afundou.max(solo - e.render_y);
            flutuou = flutuou.max(e.render_y - solo);
            maior_passo = maior_passo.max(e.render_y - antes);
        }
        // Os dois erros existem e sao o mesmo erro visto dos dois lados. O que
        // o teste garante e' que nenhum deles chega perto do degrau inteiro:
        // meia unidade dentro da terra e' o que se via antes.
        assert!(
            afundou < 0.3,
            "o corpo entrou {afundou:.3} na terra — a antecipacao do degrau falhou"
        );
        assert!(
            flutuou < 0.3,
            "o corpo flutuou {flutuou:.3} acima do chao — antecipou demais"
        );
        assert!(
            maior_passo <= SUBIDA * dt + 1e-4,
            "um quadro subiu {maior_passo:.4}, teto {:.4} — virou teleporte",
            SUBIDA * dt
        );
        assert!(
            (w.ents[&id].render_y - 0.5).abs() < 1e-3,
            "terminou em {:.3} em vez de 0,5", w.ents[&id].render_y
        );
    }

    /// O arco DESENHADO tem que ser o mesmo que o servidor usa pra liberar o
    /// degrau. Sao duas contas sobre a mesma altura — uma integrada aqui,
    /// outra fechada em `shared::altura_do_pulo` — e se elas divergirem o
    /// corpo sobe num barranco que o olho ve' que ele nao alcancou.
    #[test]
    fn o_arco_desenhado_bate_com_a_regra() {
        for hz in [30.0f32, 60.0, 144.0] {
            let alturas = simular(PULO_DURACAO, PULO_DURACAO * 1.2);
            let dt = 1.0 / 60.0;
            let _ = hz;
            for (k, &h) in alturas.iter().enumerate() {
                let t = (k + 1) as f32 * dt;
                let esperado = shared::altura_do_pulo(t);
                assert!(
                    (h - esperado).abs() < 0.02,
                    "em t={t:.3}s o desenho deu {h:.3} e a regra diz {esperado:.3}"
                );
            }
        }
    }

    /// A bandeira do servidor chegando atrasada ou saindo adiantada nao pode
    /// religar o arco — ela fica LIGADA o pulo inteiro, entao so' a borda vale.
    #[test]
    fn a_bandeira_do_servidor_nao_religa_o_arco() {
        for voo in [PULO_DURACAO * 0.5, PULO_DURACAO, PULO_DURACAO * 1.4] {
            let alturas = simular(voo, PULO_DURACAO * 3.0);
            assert_eq!(arcos(&alturas), 1, "voo de {voo}s deu mais de um arco");
        }
    }

    /// Apertar de novo no meio do arco nao empilha um segundo pulo: a espera
    /// local espelha a do servidor.
    #[test]
    fn apertar_de_novo_cedo_nao_empilha() {
        let dt = 1.0 / 60.0;
        let mut w = World::default();
        let id = shared::EntityId(1);
        w.apply(
            vec![EntityMeta { id, tag: EntityTag::Player, name: None, hp_max: 100, faction: None }],
            vec![EntityState { id, pos: [16, 16], vel: [0, 0], hp: 100, flags: ent_flags::SELF }],
            &[],
        );
        let chao = |_: f32, _: f32| 0.0;
        let mut alturas = Vec::new();
        let mut t = 0.0f32;
        while t < PULO_DURACAO + PULO_ESPERA {
            w.pular_local(); // martelando a tecla
            w.tick(dt, &chao);
            alturas.push(w.ents[&id].render_y);
            t += dt;
        }
        assert_eq!(arcos(&alturas), 1, "martelar a tecla tem que dar um arco so'");
    }
}
