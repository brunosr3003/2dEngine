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
/// Acima disto, entre dois ticks, a posicao nova e' teleporte e nao passo.
/// Montado a toda (7,5 u/s) com um segundo de engasgo de rede da' 7,5.
pub const SALTO_DE_TELEPORTE: f32 = 15.0;
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
    pub dash_visual_ate: f64,
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
    /// Fase do passo, em radianos. Anda com a DISTANCIA percorrida e nao com
    /// o tempo: parado, a perna para junto; correndo, ela nao dispara na
    /// mesma proporcao da velocidade. Ver `rig::pose`.
    pub fase: f32,
    /// 0 parado .. 1 andando, suavizado — o passo entra e sai sem estalo.
    pub andar: f32,
    /// Segundos desde que o mob comecou o ultimo golpe (a patada, ver
    /// `bicho::golpe`). Grande = sem golpe.
    pub golpe: f32,
    /// 0 = arma guardada .. 1 = na mao. Persegue o `acao::em_combate` do
    /// servidor no tempo de sacar (`rig::TEMPO_DE_SACAR`).
    pub sacada: f32,
    /// Golpe do combo do JOGADOR em curso: (passo, segundos). E o que ele
    /// interrompeu, congelado no instante da troca (`rig::Combate`).
    pub combo: Option<(u8, f32)>,
    /// Id, tempo desde o inicio e instante de impacto da skill confirmada.
    pub skill: Option<(u32, f32, f32)>,
    pub combo_ant: Option<(u8, f32)>,
    /// Segundos desde o ultimo dano recebido.
    pub ferido: Option<f32>,
    /// De onde veio o ultimo golpe recebido (aponta pro atacante), em mundo.
    pub golpe_de: Vec2,
    /// A inercia de cada peca do boneco (`rig::Molas`).
    pub molas: crate::rig::Molas,
    /// O rastro da lamina: (base, ponta, quando), em mundo.
    pub rastro: Vec<(macroquad::prelude::Vec3, macroquad::prelude::Vec3, f32)>,
    /// Pra onde olhar enquanto golpeia: (posicao do alvo, segundos). Sem
    /// isto o boneco olhava pra onde ANDAVA, e golpeava de costas.
    pub mira: Option<(Vec2, f32)>,
    pub ataque_mob: Option<(Option<EntityId>, f32, f32)>,
    /// Palmas/canos calculados pelo rig no quadro, usados pelos efeitos 3D.
    pub emissores: [Vec3; 2],
    /// Segundos desde que morreu (ou caiu). Morto fica na tela — o corpo
    /// tomba e fica —, mas nao e' mais selecionavel.
    pub morte: Option<f32>,
    /// 0 andando .. 1 correndo, suavizado.
    pub correr: f32,
    /// 0 no chao .. 1 no ar, suavizado: descer um degrau tira o pe' do chao
    /// por um instante, e a pose nao pode trocar num estalo.
    pub ar: f32,
    /// Coletando: o TIPO que o fio disse (`acao::tipo_da_coleta`: 0 madeira,
    /// 1..4 pedra pela cor). Escolhe a ferramenta e o gesto.
    pub coleta: Option<u8>,
    /// Segundos desde que a coleta deste tipo comecou: o relogio do gesto.
    pub coleta_t: f32,
    /// Fase (0..1) do golpe de coleta no quadro anterior: e' por ela que o
    /// desenho sabe que a ferramenta ACABOU de bater (`lascas::cruzou`).
    pub coleta_u_ant: f32,
    /// Chefe: o golpe telegrafado carregando/batendo agora (`chefe_anim`).
    pub carga_chefe: Option<crate::chefe_anim::Carga>,
}

impl Ent {
    pub fn is_self(&self) -> bool {
        self.state.flags & ent_flags::SELF != 0
    }
}

#[derive(Default)]
pub struct World {
    pub ents: HashMap<EntityId, Ent>,
    /// Numeros e faiscas em curso (`efeitos`).
    pub efeitos: Vec<Efeito>,
    /// 0..1: a borda vermelha de quando o PROPRIO jogador apanha.
    pub dor: f32,
    /// Quem me ACERTOU e quando. Ver `auto_combate::AGRESSOR_S`.
    pub agressores: std::collections::HashMap<EntityId, f64>,
    /// O alvo do jogador local (espelho do `Jogo::alvo`).
    pub alvo: Option<EntityId>,
    /// Ordem estavel de desenho (y crescente) recalculada por quadro.
    order: Vec<EntityId>,
    pub self_id: Option<EntityId>,
    /// O proprio jogador foi TELEPORTADO neste tick (renascer, dungeon, troca
    /// de area): o `main` mostra o carregando. Quem le' zera.
    pub salto_do_eu: bool,
}

/// Distancia, em unidades, de um ciclo inteiro de passo (dois passos).
///
/// Andando a 5 u/s da' ~2,3 ciclos por segundo. Correndo, a passada abre: sem
/// isso o ciclo iria junto com os 1,65x da velocidade e as pernas virariam
/// um borrao.
const PASSADA_ANDANDO: f32 = 2.2;
const PASSADA_CORRENDO: f32 = 3.0;

/// Quanto uma PESSOA anda num ciclo inteiro de passada (u), entre o passo e
/// a corrida. E' o divisor de `Ent::fase`, e quem monta precisa dele pra
/// converter a propria fase na do bicho, que tem passada de outro tamanho.
pub fn passada_de_gente(correr: f32) -> f32 {
    PASSADA_ANDANDO + (PASSADA_CORRENDO - PASSADA_ANDANDO) * correr
}

/// Avanca a fase do passo pela distancia que o corpo DESENHADO andou, e
/// suaviza "andando" e "correndo" a partir da velocidade que sai dela.
///
/// Vem da posicao desenhada, nao da velocidade do servidor: e' ela que o
/// jogador ve' andar, e se as duas discordassem o pe' patinaria no chao.
fn anda_a_fase(ent: &mut Ent, andou: f32, dt: f32) {
    if dt <= 0.0 {
        return;
    }
    let vel = andou / dt;
    let k = 1.0 - (-8.0 * dt).exp();
    let alvo_andar = (vel / shared::PLAYER_SPEED).clamp(0.0, 1.0);
    let alvo_correr =
        ((vel - shared::PLAYER_SPEED * 1.1) / (shared::PLAYER_SPEED * 0.45)).clamp(0.0, 1.0);
    ent.andar += (alvo_andar - ent.andar) * k;
    ent.correr += (alvo_correr - ent.correr) * k;
    let boss = ent.state.flags & ent_flags::BOSS != 0;
    // Montado, quem anda e' o BICHO: a fase acumula com o ciclo dele, igual
    // a' de um mob. Antes acumulava com a passada de gente e o desenho
    // convertia por uma razao — e o pe' da montaria nunca casava com o chao.
    let montado = ent.state.flags & ent_flags::MONTADO != 0;
    let passada = match crate::bicho::da_montaria(ent.meta.tag, ent.meta.kind, montado) {
        Some((_, altura)) => crate::bicho::ciclo(altura, vel),
        None => match crate::bicho::do_mob(ent.meta.tag, ent.meta.kind, boss)
            .or_else(|| crate::bicho::do_pet(ent.meta.tag, ent.meta.kind))
        {
            // bicho: o ciclo casa com a viagem do pe' (ver `bicho`), senao a
            // pata patina no chao
            Some((_, altura)) => crate::bicho::ciclo(altura, ent.andar * shared::PLAYER_SPEED),
            None => passada_de_gente(ent.correr),
        },
    };
    ent.fase =
        (ent.fase + andou / passada * std::f32::consts::TAU) % (std::f32::consts::TAU * 64.0);
}

/// Um golpe que acertou alguem, na tela: o numero e a faisca.
#[derive(Clone, Copy, Debug)]
pub struct Efeito {
    pub alvo: EntityId,
    pub dano: i32,
    pub critico: bool,
    /// Foi no proprio jogador (vermelho, e a tela doi junto).
    pub eu: bool,
    pub t: f32,
    /// 0..1: espalha numeros seguidos e gira a faisca.
    pub semente: f32,
}

impl World {
    /// Os acertos do tick (`WorldSnapshot::acertos`): quem apanhou da' o
    /// tranco, e o golpe vira numero e faisca. Vem do servidor com o dano
    /// REAL, e nao da queda de vida — no modo imortal a vida nao cai e o
    /// golpe tem que aparecer do mesmo jeito.
    pub fn acertos(&mut self, lista: &[shared::protocol::Acerto], agora: f64) {
        for a in lista {
            let eu = self.self_id == Some(a.alvo);
            if let Some(e) = self.ents.get(&a.alvo) {
                let som = if eu {crate::sons::Som::Dor} else if a.critico {crate::sons::Som::Critico} else if a.alvo.0 % 2 == 0 {crate::sons::Som::Acerto} else {crate::sons::Som::Acerto2};
                crate::sons::mundo(som,e.render_pos,self.self_pos(),eu || self.self_id == Some(a.atacante));
                if e.meta.tag == shared::EntityTag::Enemy && e.state.hp > 0 && a.dano > 0 {
                    crate::sons::mob(crate::sons::Som::MobDor,e.render_pos,self.self_pos(),self.self_id==Some(a.atacante));
                }
            }
            if let Some(ent) = self.ents.get_mut(&a.alvo) {
                ent.ferido = Some(0.0);
                ent.golpe_de = Vec2::new(a.de[0] as f32, a.de[1] as f32) / 127.0;
            }
            // quem bateu vira pra quem apanhou
            let onde = self.ents.get(&a.alvo).map(|e| e.render_pos);
            if let (Some(p), Some(at)) = (onde, self.ents.get_mut(&a.atacante)) {
                at.mira = Some((p, 0.6));
            }
            if eu {
                self.dor = 1.0;
                // QUEM ME BATEU FICA MARCADO.
                //
                // É o que permite "revidar em quem começou" sem abrir PvP
                // geral, e o que dá ao auto combate a noção de agressor. O
                // prazo é curto (`auto_combate::AGRESSOR_S`): um tiro perdido
                // de um minuto atrás não pode marcar alguém pra sessão toda.
                self.agressores.insert(a.atacante, agora);
            }
            let h = (a.alvo.0 as u64 ^ (self.efeitos.len() as u64).wrapping_mul(0x9E37_79B9))
                .wrapping_mul(2_654_435_761);
            self.efeitos.push(Efeito {
                alvo: a.alvo,
                dano: a.dano,
                critico: a.critico,
                eu,
                t: 0.0,
                semente: (h % 1000) as f32 / 1000.0,
            });
        }
    }
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
            // Ja' conhecida: a meta veio de novo porque mudou (skin de
            // montaria). So' troca a meta; o resto do corpo fica.
            if let Some(e) = self.ents.get_mut(&id) {
                e.meta = meta;
                continue;
            }
            // O estado real vem no mesmo pacote, logo abaixo.
            let state = EntityState {
                id,
                pos: [0, 0],
                vel: [0, 0],
                hp: 0,
                flags: 0,
                acao: 0,
                rumo: 0,
            };
            // NPC de porta vem com o rumo no `kind`: nasce olhando pra fora e,
            // parado, o tick nao mexe no yaw.
            let yaw = if meta.tag == shared::EntityTag::Npc {
                shared::npc_yaw_de_kind(meta.kind).unwrap_or(0.0)
            } else {
                0.0
            };
            self.ents.entry(id).or_insert(Ent {
                meta,
                state,
                render_pos: Vec2::ZERO,
                yaw,
                render_y: f32::MIN,
                pulo_local: 0.0,
                dash_visual_ate: 0.0,
                vel_y: 0.0,
                no_ar_antes: false,
                voando: false,
                espera_pulo: 0.0,
                fase: 0.0,
                andar: 0.0,
                golpe: 99.0,
                sacada: 0.0,
                combo: None,
                skill: None,
                combo_ant: None,
                ferido: None,
                golpe_de: Vec2::ZERO,
                molas: Default::default(),
                rastro: Vec::new(),
                mira: None,
                ataque_mob: None,
                emissores: [Vec3::ZERO; 2],
                morte: None,
                correr: 0.0,
                ar: 0.0,
                coleta: None,
                coleta_t: 0.0,
                coleta_u_ant: 0.0,
                carga_chefe: None,
            });
        }
        let ouvinte = self.self_pos();
        for st in states {
            let Some(ent) = self.ents.get_mut(&st.id) else {
                continue;
            };
            // Entidade recem-criada nasce ja na posicao certa, senao ela
            // desliza do canto do mundo ate o lugar dela.
            if ent.state.pos == [0, 0] {
                ent.render_pos = mq(st.pos_f32());
            } else if mq(st.pos_f32()).distance(ent.render_pos) > SALTO_DE_TELEPORTE {
                // Longe demais pra ser andar: e' teleporte. Vai direto e no
                // CHAO — deslizando, o boneco atravessava o mapa voando e caia
                // do alto de onde estava.
                ent.render_pos = mq(st.pos_f32());
                ent.render_y = f32::MIN;
                ent.voando = false;
                ent.vel_y = 0.0;
                ent.rastro.clear();
                if st.flags & ent_flags::SELF != 0 {
                    self.salto_do_eu = true;
                }
            }
            if st.flags & ent_flags::SELF != 0 {
                self.self_id = Some(st.id);
            }
            // O servidor segura o bit do golpe uns quadros; a patada comeca
            if st.flags & ent_flags::DASHING != 0 {
                ent.combo = None;
                ent.combo_ant = None;
                ent.skill = None;
                ent.ferido = None;
            }
            // O servidor segura o bit do golpe uns quadros; a patada comeca
            // quando ele ACENDE.
            if st.flags & ent_flags::ATACANDO != 0 && ent.state.flags & ent_flags::ATACANDO == 0 {
                ent.golpe = 0.0;
            }
            // Golpe do combo: comeca quando o gesto ACENDE ou quando o passo
            // muda — golpes seguidos a 0,25 s nem sempre apagam o bit no meio.
            {
                use shared::components::acao;
                let (g, v) = (acao::gesto(st.acao), acao::variante(st.acao));
                let (ga, va) = (acao::gesto(ent.state.acao), acao::variante(ent.state.acao));
                if g == acao::GOLPE && (ga != acao::GOLPE || va != v) {
                    let som = match acao::conjunto(st.acao) {2=>crate::sons::Som::Disparo,3=>crate::sons::Som::Magia,_=>if v%2==0 {crate::sons::Som::Golpe} else {crate::sons::Som::Golpe2}};
                    if ent.state.pos != [0,0] {crate::sons::mundo(som,ent.render_pos,ouvinte,self.self_id==Some(st.id));}
                    ent.combo_ant = ent.combo;
                    ent.combo = Some((v, 0.0));
                    // quem golpeia ja' esta' com a arma: nao espera sacar
                    ent.sacada = 1.0;
                }
                // Coletando: o tipo escolhe ferramenta e gesto; trocar de tipo
                // (ou comecar) zera o relogio do gesto.
                let tipo = acao::tipo_da_coleta(st.acao);
                if tipo != ent.coleta {
                    ent.coleta_t = 0.0;
                }
                ent.coleta = tipo;
            }
            // Morreu (ou caiu): conta o tempo do tombo. Quem ja' chega morto no
            // campo de visao nasce no chao, sem cair de novo na frente de todo
            // mundo.
            {
                let caido = st.hp == 0 || st.flags & ent_flags::DOWNED != 0;
                let tem_vida = matches!(
                    ent.meta.tag,
                    shared::EntityTag::Enemy | shared::EntityTag::Player
                );
                if tem_vida && caido {
                    if ent.morte.is_none() {
                        if ent.state.hp > 0 && ent.state.pos != [0,0] {
                            if ent.meta.tag == shared::EntityTag::Enemy {
                                let perto=ouvinte.is_some_and(|p|p.distance(ent.render_pos)<12.0);
                                crate::sons::mob(crate::sons::Som::MobMorte,ent.render_pos,ouvinte,perto);
                            } else {
                                crate::sons::mundo(crate::sons::Som::Morte,ent.render_pos,ouvinte,self.self_id == Some(st.id));
                            }
                        }
                        ent.morte = Some(if ent.state.pos == [0, 0] { 9.0 } else { 0.0 });
                    }
                } else {
                    ent.morte = None;
                }
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
        let posicoes: HashMap<_, _> = self
            .ents
            .iter()
            .map(|(&id, e)| (id, e.render_pos))
            .collect();
        for ef in &mut self.efeitos {
            ef.t += dt;
        }
        self.efeitos.retain(|ef| ef.t < 1.1);
        self.dor = (self.dor - dt * 2.2).max(0.0);
        let alvo_pos = self
            .alvo
            .and_then(|a| self.ents.get(&a))
            .map(|e| e.render_pos);
        // Peso da suavizacao independente do frame rate.
        let a = 1.0 - (-SMOOTH_K * dt).exp();

        for ent in self.ents.values_mut() {
            let target = mq(ent.state.pos_f32());
            let antes = ent.render_pos;
            ent.render_pos += (target - ent.render_pos) * a;
            anda_a_fase(ent, (ent.render_pos - antes).length(), dt);
            ent.golpe = (ent.golpe + dt).min(99.0);
            if let Some((alvo, t, impacto)) = ent.ataque_mob.as_mut() {
                *t += dt;
                if let Some(p) = alvo.and_then(|id| posicoes.get(&id)) {
                    ent.mira = Some((*p, 0.2));
                }
                if *t > *impacto + 0.3 || ent.morte.is_some() {
                    ent.ataque_mob = None;
                }
            }
            if let Some((_, t, _)) = ent.skill.as_mut() {
                *t += dt;
            }
            if ent
                .skill
                .is_some_and(|(_, t, impacto)| t > impacto + shared::skills::RECUPERACAO_S)
                || ent.morte.is_some()
            {
                ent.skill = None;
            }
            let saca = if shared::components::acao::em_combate(ent.state.acao) {
                1.0
            } else {
                0.0
            };
            let passo = dt / crate::rig::TEMPO_DE_SACAR;
            ent.sacada += (saca - ent.sacada).clamp(-passo, passo);
            if let Some((_, t)) = ent.combo.as_mut() {
                *t += dt;
            }
            if ent
                .combo
                .map_or(false, |(_, t)| t > crate::rig::DURACAO_DO_GOLPE)
            {
                ent.combo = None;
                ent.combo_ant = None;
            }
            if ent.coleta.is_some() {
                ent.coleta_t += dt;
            } else {
                ent.coleta_t = 0.0;
            }
            if let Some(t) = ent.ferido.as_mut() {
                *t += dt;
            }
            if let Some(t) = ent.morte.as_mut() {
                *t = (*t + dt).min(99.0);
            }
            if ent.ferido.map_or(false, |t| t > 1.0) {
                ent.ferido = None;
            }
            let alvo_ar = if ent.voando { 1.0 } else { 0.0 };
            ent.ar += (alvo_ar - ent.ar) * (1.0 - (-12.0 * dt).exp());

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
                if a > apoio && a - apoio <= DEGRAU_VISIVEL {
                    a
                } else {
                    apoio
                }
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
                ent.render_y = (ent.render_y + SUBIDA * dt).min(alvo_alto).max(piso);
            }

            // Quem esta' golpeando olha pra quem golpeia, mesmo andando de
            // lado; fora disso, pra onde anda.
            if ent.is_self() && ent.combo.is_some() {
                if let Some(p) = alvo_pos {
                    ent.mira = Some((p, 0.6));
                }
            }
            if let Some((_, t)) = ent.mira.as_mut() {
                *t -= dt;
            }
            if ent.mira.map_or(false, |(_, t)| t <= 0.0) {
                ent.mira = None;
            }
            let vel = mq(ent.state.vel_f32());
            let olhar = match ent.mira {
                Some((p, _)) => Some(p - ent.render_pos),
                None if vel.length_squared() > MOVING_EPS * MOVING_EPS => Some(vel),
                None => None,
            };
            // Os OUTROS olham pra onde o servidor diz (`EntityState::rumo`):
            // parado coletando, mirando ou mordendo, a velocidade nao sabe. O
            // proprio personagem continua na previsao local, sem atraso.
            let rede = if ent.is_self() {
                None
            } else {
                shared::yaw_de_rumo(ent.state.rumo)
            };
            // O modelo nasce olhando pro +Z do mundo.
            let quer = rede.or_else(|| {
                olhar
                    .filter(|d| d.length_squared() > 1e-4)
                    .map(|dir| dir.x.atan2(dir.y))
            });
            if let Some(want) = quer {
                // Caminho mais curto no circulo, senao ele gira 350 graus pra
                // virar 10.
                let mut d = want - ent.yaw;
                while d > std::f32::consts::PI {
                    d -= std::f32::consts::TAU;
                }
                while d < -std::f32::consts::PI {
                    d += std::f32::consts::TAU;
                }
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
        self.self_id
            .and_then(|id| self.ents.get(&id))
            .map(|e| e.render_pos)
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

    /// O OUTRO jogador olha pro rumo que veio no fio, parado, com suavizacao;
    /// o proprio personagem ignora o rumo (previsao local).
    #[test]
    fn rumo_do_fio_vira_os_outros_suave_e_nao_o_proprio() {
        let q = std::f32::consts::FRAC_PI_2;
        let mut w = World::default();
        for (id, flags) in [(EntityId(1), 0u8), (EntityId(2), ent_flags::SELF)] {
            w.apply(
                vec![EntityMeta {
                    pk: Default::default(),
                    auras: 0,
                    id,
                    tag: EntityTag::Player,
                    name: None,
                    hp_max: 100,
                    faction: None,
                    kind: 0,
                    nivel: 1,
                    desafio: None,
            aparencia: 0,
                }],
                vec![EntityState {
                    id,
                    pos: [16, 16],
                    vel: [0, 0],
                    hp: 100,
                    flags,
                    acao: 0,
                    rumo: shared::rumo_de_yaw(q),
                }],
                &[],
            );
        }
        w.tick(1.0 / 60.0, &|_, _| 0.0);
        let um_quadro = w.ents[&EntityId(1)].yaw;
        assert!(
            um_quadro > 0.0 && um_quadro < q * 0.9,
            "vira suave, nao de estalo: {um_quadro}"
        );
        for _ in 0..120 {
            w.tick(1.0 / 60.0, &|_, _| 0.0);
        }
        assert!(
            (w.ents[&EntityId(1)].yaw - q).abs() < 0.05,
            "chegou no rumo do fio"
        );
        assert!(
            w.ents[&EntityId(2)].yaw.abs() < 1e-4,
            "o proprio nao segue o fio"
        );
    }

    /// Teleporte (renascer, dungeon, troca de area) vai DIRETO e no chao, e o
    /// do proprio jogador avisa o carregando. Passo normal continua suave.
    #[test]
    fn teleporte_nao_desliza_e_avisa_o_carregando() {
        let mut w = World::default();
        let id = EntityId(7);
        let estado = |x: f32| {
            let mut st =
                EntityState::quantize(id, ::glam::Vec2::new(x, 0.0), ::glam::Vec2::ZERO, 100, 0);
            st.flags = ent_flags::SELF;
            st
        };
        w.apply(
            vec![EntityMeta {
                pk: Default::default(),
                auras: 0,
                id,
                tag: EntityTag::Player,
                name: None,
                hp_max: 100,
                faction: None,
                kind: 0,
                nivel: 1,
                desafio: None,
            aparencia: 0,
            }],
            vec![estado(1.0)],
            &[],
        );
        for _ in 0..30 {
            w.tick(1.0 / 60.0, &|_, _| 0.0);
        }
        assert!(
            !std::mem::take(&mut w.salto_do_eu),
            "entrar no mundo nao e' teleporte"
        );
        // Um passo: suave.
        w.apply(vec![], vec![estado(2.0)], &[]);
        w.tick(1.0 / 60.0, &|_, _| 0.0);
        assert!(
            w.ents[&id].render_pos.x < 1.9 && !w.salto_do_eu,
            "passo curto desliza"
        );
        // Teleporte: direto, no chao do lugar novo.
        for _ in 0..60 {
            w.tick(1.0 / 60.0, &|_, _| 0.0);
        }
        w.apply(vec![], vec![estado(300.0)], &[]);
        assert!(w.salto_do_eu, "teleporte do proprio avisa o carregando");
        w.tick(1.0 / 60.0, &|_, _| 12.0);
        let e = &w.ents[&id];
        assert!(
            (e.render_pos.x - 300.0).abs() < 0.1,
            "foi direto: {:?}",
            e.render_pos
        );
        assert!(
            (e.render_y - 12.0).abs() < 0.01 && !e.voando,
            "no chao, sem cair: {}",
            e.render_y
        );
    }

    #[test]
    fn mob_em_strafe_mira_no_alvo_durante_todo_o_golpe() {
        let mut w = World::default();
        for (id, tag, pos) in [
            (EntityId(1), EntityTag::Enemy, [16, 16]),
            (EntityId(2), EntityTag::Player, [16, 48]),
        ] {
            w.apply(
                vec![EntityMeta {
                    pk: Default::default(),
                    auras: 0,
                    id,
                    tag,
                    name: None,
                    hp_max: 100,
                    faction: None,
                    kind: 2,
                    nivel: 1,
                    desafio: None,
            aparencia: 0,
                }],
                vec![EntityState {
                    id,
                    pos,
                    vel: [16, 0],
                    hp: 100,
                    flags: 0,
                    acao: 0,
                    rumo: 0,
                }],
                &[],
            );
        }
        w.ents.get_mut(&EntityId(1)).unwrap().ataque_mob = Some((Some(EntityId(2)), 0.0, 0.46));
        for _ in 0..26 {
            w.tick(1.0 / 60.0, &|_, _| 0.0);
        }
        assert!(
            w.ents[&EntityId(1)].yaw.abs() < 0.01,
            "virou para a velocidade lateral"
        );
        w.ents.get_mut(&EntityId(2)).unwrap().render_pos = vec2(3.0, 1.0);
        w.ents.get_mut(&EntityId(2)).unwrap().state.pos = [48, 16];
        for _ in 0..16 {
            w.tick(1.0 / 60.0, &|_, _| 0.0);
        }
        assert!((w.ents[&EntityId(1)].yaw - std::f32::consts::FRAC_PI_2).abs() < 0.02);
    }

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
            vec![EntityMeta {
                pk: Default::default(),
                auras: 0,
                id,
                tag: EntityTag::Player,
                name: None,
                hp_max: 100,
                faction: None,
                kind: 0,
                nivel: 0,
                desafio: None,
            aparencia: 0,
            }],
            vec![EntityState {
                id,
                pos: [16, 16],
                vel: [0, 0],
                hp: 100,
                flags: ent_flags::SELF,
                acao: 0,
                rumo: 0,
            }],
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
                | if (0.05..voo).contains(&t) {
                    ent_flags::PULANDO
                } else {
                    0
                };
            w.apply(
                Vec::new(),
                vec![EntityState {
                    id,
                    pos: [16, 16],
                    vel: [0, 0],
                    hp: 100,
                    flags,
                    acao: 0,
                    rumo: 0,
                }],
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
        assert!(
            alturas.last().is_some_and(|&h| h == 0.0),
            "tem que acabar no chao"
        );
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
            acao: 0,
            rumo: 0,
        };
        w.apply(
            vec![EntityMeta {
                pk: Default::default(),
                auras: 0,
                id,
                tag: EntityTag::Player,
                name: None,
                hp_max: 100,
                faction: None,
                kind: 0,
                nivel: 0,
                desafio: None,
            aparencia: 0,
            }],
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
            "terminou em {:.3} em vez de 0,5",
            w.ents[&id].render_y
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
            vec![EntityMeta {
                pk: Default::default(),
                auras: 0,
                id,
                tag: EntityTag::Player,
                name: None,
                hp_max: 100,
                faction: None,
                kind: 0,
                nivel: 0,
                desafio: None,
            aparencia: 0,
            }],
            vec![EntityState {
                id,
                pos: [16, 16],
                vel: [0, 0],
                hp: 100,
                flags: ent_flags::SELF,
                acao: 0,
                rumo: 0,
            }],
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
        assert_eq!(
            arcos(&alturas),
            1,
            "martelar a tecla tem que dar um arco so'"
        );
    }
}
