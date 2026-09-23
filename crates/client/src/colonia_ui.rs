//! O painel da COLONIA (docs/COLONIA.md).
//!
//! Desde 22/09/2026 a ilha NAO E' MAIS UMA ZONA: ela e' este painel. Nao ha'
//! viagem, mural nem cais — ha' a MAQUETE (o relevo e o assentamento de
//! verdade, girando), os tres eixos, os moradores e o bau.
//!
//! A maquete existe porque um painel de numeros sobre uma ilha que ninguem ve'
//! nao e' uma ilha, e' uma planilha com tema. Ela e' o MESMO `Terreno` e as
//! MESMAS `Construcoes` de quando se andava nela — uma segunda representacao
//! seria uma segunda fonte de verdade pro mesmo lugar.
//!
//! Quem decide tudo e' o servidor (`shared::colonia`): aqui so' se desenha o
//! estado que ele mandou e se devolve o pedido que o dedo encostou.

use macroquad::prelude::*;
use shared::colonia::{eixo, PedidoColonia, EIXOS, NIVEL_MAX};

use crate::hud_estilo as estilo;

/// O estado que o servidor mandou, do jeito que ele mandou.
#[derive(Debug, Clone)]
pub struct Estado {
    pub niveis: [u8; EIXOS],
    pub horas: f32,
    pub colheita: Vec<(u16, u32)>,
    pub custos: Vec<Vec<(u16, u32)>>,
    pub banco: u8,
    /// O que esta' NO BAU da ilha. A colheita cai aqui.
    pub bau: Vec<shared::InventorySlot>,
    /// Os moradores, na ordem das vagas.
    pub trabalhadores: Vec<shared::colonia::Profissao>,
    /// Quantas vagas o assentamento sustenta. Maior que `trabalhadores.len()`
    /// = ha' casa vazia esperando alguem.
    pub vagas: u8,
}

pub struct ColoniaUi {
    estado: Option<Estado>,
    /// O relevo e o assentamento da ilha, pra maquete. Montados quando o
    /// servidor manda `AvisoColonia::Terreno` — e nao a cada abertura: assar
    /// a vegetacao custa alguns milissegundos, e o painel abre muito.
    maquete: Option<Maquete>,
    /// O giro da maquete. Anda sozinho devagar; o dedo manda quando encosta.
    giro: f32,
    /// Arrastando a maquete: onde o dedo estava no quadro passado.
    ///
    /// O dono: "a colônia 3D fica literalmente um 3D na lateral esquerda que
    /// você consegue movimentar para direita e esquerda para mudar a câmera
    /// de lugar". Enquanto o dedo está nela o giro automático para — senão a
    /// ilha escorregaria por baixo do dedo.
    arrasto: Option<f32>,
    /// A coluna da direita rola: ela tem colheita, baú, três eixos e até seis
    /// moradores, e isso não cabe em tela de celular.
    rolagem: crate::rolagem::Rolagem,
    /// Distância entre os dois dedos no quadro passado (pinça).
    pinca: Option<f32>,
    /// ZOOM da maquete. 1 = a ilhota enquadrada; acima, mais perto.
    ///
    /// Nasce em 1,35 porque o dono: "tá mostrando muito longe, não precisa
    /// mostrar o oceano". A ilhota passa a encher o quadro e a costa sai
    /// pelas bordas, que é o que se quer numa maquete — ela É o assunto.
    zoom: f32,
}

impl Default for ColoniaUi {
    fn default() -> Self {
        Self {
            estado: None,
            maquete: None,
            giro: 0.0,
            arrasto: None,
            rolagem: Default::default(),
            pinca: None,
            // Começa APERTADO: a ilhota enche o quadro e a costa sai pelas
            // bordas. O dono: "tá mostrando muito longe, não precisa mostrar
            // o oceano".
            zoom: 1.35,
        }
    }
}

struct Maquete {
    terreno: crate::terreno::Terreno,
    construcoes: crate::construcoes::Construcoes,
    /// Centro do assentamento: e' pra ele que a camera olha.
    centro: Vec2,
    /// Cada morador e o lugar onde ele fica em pe', na porta do oficio dele.
    ///
    /// Calculado aqui e nao no desenho porque sai da VILA, e a vila e' uma
    /// varredura: refazer por quadro seria pagar a mesma conta trinta vezes
    /// por segundo pra um resultado que so' muda quando alguem e' contratado.
    moradores: Vec<(shared::colonia::Profissao, Vec2)>,
}

impl ColoniaUi {
    /// O relevo mudou (ou o jogador entrou): remonta a maquete.
    pub fn define_maquete(
        &mut self,
        plato: f32,
        trabalhadores: Vec<shared::colonia::Profissao>,
    ) {
        let terreno = crate::terreno::Terreno::da_colonia(plato);
        let ger = shared::terreno::Gerador::da_colonia(plato);
        let centro = ger.cidade().map(|c| c.centro()).unwrap_or_default();
        let moradores = onde_ficam(&ger, &trabalhadores);
        self.maquete = Some(Maquete {
            terreno,
            construcoes: crate::construcoes::Construcoes::da_colonia(plato, trabalhadores),
            centro: vec2(centro.x, centro.y),
            moradores,
        });
    }


    pub fn abrir(&mut self, e: Estado) {
        self.estado = Some(e);
    }

    /// Atualiza SEM abrir: colher e melhorar mandam estado novo, e se isso
    /// abrisse o painel ele reapareceria sozinho no porto.
    pub fn atualizar(&mut self, e: Estado) {
        if self.estado.is_some() {
            self.estado = Some(e);
        }
    }

    pub fn fechar(&mut self) {
        self.estado = None;
    }

    pub fn aberto(&self) -> bool {
        self.estado.is_some()
    }

    /// Desenha; devolve o pedido que o jogador fez.
    pub fn desenha(
        &mut self,
        nome_item: &dyn Fn(u16) -> String,
        solido: &Material,
        vox: &crate::vox::VoxCache,
    ) -> Option<PedidoColonia> {
        // A maquete vive mesmo com o painel fechado? Nao: assar e' barato o
        // bastante pra refazer, e manter o relevo na memoria o tempo todo
        // custaria a ilha inteira por jogador — o que foi justamente o que a
        // colonia deixou de fazer quando virou uma so'.
        if let Some(mq) = &mut self.maquete {
            mq.construcoes.acompanhar();
            let c = mq.centro;
            // RAIO 6, e não 3. Um pedaço tem 32 blocos = 16 u, então raio 3
            // cobre ±56 u — e a ilhota tem 85 de raio. Um terço dela nunca
            // era gerado: a maquete mostrava um quadrado de terra com a costa
            // cortada, boiando no mar. Era parte do "o 3D tá MUITO feio".
            //
            // Raio 6 cobre ±104 u, a ilhota inteira com folga. São 169
            // pedaços; o orçamento de 24 por quadro monta tudo em 7 quadros,
            // e depois o laço não faz mais nada.
            //
            // (Minha prévia não pegou isso porque ela chamava `atualiza` com
            // raio 5 e orçamento 400 — validei com número diferente do que o
            // jogo usa, que é o mesmo que não validar.)
            mq.terreno.atualiza(c, 6, 24);
        }
        // O giro automático só corre quando ninguém está arrastando.
        if self.arrasto.is_none() {
            self.giro += get_frame_time() * 0.12;
        }
        // O painel cresceu: era 600x660 e o dono disse que "o menu da Minha
        // Ilha está muito feio, pra começar que está pequeno". Agora ele
        // ocupa a tela com folga, que é o que permite as duas colunas.
        estilo::no_painel(estilo::escala_do_painel(1040.0, 720.0), || {
            self.desenha_na_escala(nome_item, solido, vox)
        })
    }

    fn desenha_na_escala(
        &mut self,
        nome_item: &dyn Fn(u16) -> String,
        solido: &Material,
        vox: &crate::vox::VoxCache,
    ) -> Option<PedidoColonia> {
        let e = self.estado.as_ref()?.clone();
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let linha_h = 86.0 * f;
        // DUAS COLUNAS, e o painel grande. Era uma coluna só de 600 px com a
        // maquete numa faixa de 200 px no topo — o dono: "o menu da Minha
        // Ilha está muito feio, pra começar que está pequeno; a minha ideia
        // era a colônia 3D ficar literalmente um 3D na lateral esquerda que
        // você consegue movimentar, e os upgrades na direita".
        let p = janela(seguro, f);
        crate::hud_layout::escurece(0.45);
        estilo::painel_destaque(p, estilo::ACENTO);
        let m = Vec2::from(mouse_position());
        let x0 = p.x + 20.0 * f;
        estilo::texto_forte(x0, p.y + 36.0 * f, "Minha Ilha", 20, estilo::OURO);
        estilo::texto(x0, p.y + 60.0 * f, &resumo(&e), 14, estilo::SUAVE);

        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );

        let mut pedido = None;

        // ── ESQUERDA: a ilhota, girando, do tamanho que merece ──
        //
        // Ela vem antes de tudo porque é ela que diz ao jogador que aquilo é
        // um LUGAR, e não uma planilha com tema. Numa faixa de 200 px no topo
        // isso não acontecia.
        let (mr, dir) = colunas(p, f);
        let topo = mr.y;
        let alturac = mr.h;
        estilo::cartao(mr, false, false);
        let desenhou = match &self.maquete {
            Some(mq) => crate::render3d::maquete_da_ilha(
                &mq.terreno,
                &mq.construcoes,
                mr,
                mq.centro,
                self.giro,
                self.zoom,
                solido,
                &mq.moradores,
                vox,
            ),
            None => false,
        };
        if !desenhou {
            estilo::texto_centro(
                mr.center().x,
                mr.center().y,
                "montando a ilha…",
                13,
                estilo::SUAVE,
            );
        }
        // ARRASTAR GIRA, e o toque tem que ser pego NO PONTO DO DEDO.
        //
        // Antes o teste era `mr.contains(mouse_position())` no quadro do
        // aperto — e nesse quadro o mouse simulado ainda aponta pro TOQUE
        // ANTERIOR (a mesma armadilha que já mordeu o botão de criar
        // personagem e o do minimapa). Quem abria o painel tocava no menu, à
        // direita, e o arrasto na maquete nunca começava: "nem dá pra mover".
        if let Some(q) = apertou_em() {
            self.arrasto = mr.contains(q).then_some(q.x);
        }
        if !is_mouse_button_down(MouseButton::Left) && touches().is_empty() {
            self.arrasto = None;
        }
        if let Some(antes) = self.arrasto {
            self.giro -= (m.x - antes) * 0.008;
            self.arrasto = Some(m.x);
        }
        // PINÇA e RODA dão zoom. A maquete não tinha zoom nenhum.
        let dedos: Vec<Vec2> = touches()
            .iter()
            .filter(|t| t.phase != TouchPhase::Ended)
            .map(|t| t.position)
            .collect();
        if dedos.len() >= 2 {
            self.arrasto = None; // dois dedos não giram
            let d = dedos[0].distance(dedos[1]);
            if let Some(antes) = self.pinca.replace(d) {
                if antes > 1.0 {
                    self.zoom = (self.zoom * (d / antes)).clamp(ZOOM_MIN, ZOOM_MAX);
                }
            }
        } else {
            self.pinca = None;
        }
        let roda = mouse_wheel().1;
        if roda != 0.0 && mr.contains(m) {
            self.zoom = (self.zoom * if roda > 0.0 { 1.12 } else { 1.0 / 1.12 })
                .clamp(ZOOM_MIN, ZOOM_MAX);
        }
        estilo::texto_centro(
            mr.center().x,
            mr.y + mr.h - 10.0 * f,
            "arraste para girar · pinça dá zoom",
            12,
            estilo::SUAVE,
        );

        // ── DIREITA: o que se faz com a ilha ──

        let vagas_h = if e.vagas > 0 {
            32.0 * f + 78.0 * f * e.vagas as f32
        } else {
            0.0
        };
        let total = 56.0 * f + 12.0 * f + 52.0 * f + 12.0 * f
            + linha_h * EIXOS as f32
            + vagas_h
            + 12.0 * f;
        // A rolagem é quem decide o que é clique e o que é arrasto: sem isso,
        // rolar a lista com o dedo contrataria um morador no caminho.
        let clique = self.rolagem.quadro(dir, total, linha_h);
        let clicou_em = |r: Rect| clique.is_some_and(|c| r.contains(c));
        crate::rolagem::recortar(Some(dir));
        let cx = dir.x;
        let cw = dir.w - 10.0 * f;
        let cr = Rect::new(cx, dir.y - self.rolagem.pos, cw, 56.0 * f);
        estilo::cartao(cr, false, !e.colheita.is_empty());
        estilo::texto(
            cr.x + 14.0 * f,
            cr.y + 24.0 * f,
            &colheita_em_texto(&e, nome_item),
            14,
            if e.colheita.is_empty() {
                estilo::SUAVE
            } else {
                estilo::TEXTO
            },
        );
        estilo::texto(
            cr.x + 14.0 * f,
            cr.y + 44.0 * f,
            &aviso_do_relogio(e.horas),
            12,
            cor_do_relogio(e.horas),
        );
        if !e.colheita.is_empty() {
            let b = Rect::new(
                cr.x + cr.w - 136.0 * f,
                cr.y + (cr.h - 40.0 * f) * 0.5,
                124.0 * f,
                40.0 * f,
            );
            crate::foco::marca(crate::foco::chave::ILHA_COLHER, b);
            estilo::botao(b, "Colher", estilo::estado_de(b, false, false), true);
            if clicou_em(b) {
                pedido = Some(PedidoColonia::Colher);
            }
        }

        // O BAU DA ILHA, logo abaixo da colheita: e' onde ela cai, e nao na
        // bolsa. Sem esta linha o jogador colhia e nao achava nada.
        let mut y = cr.y + cr.h + 12.0 * f;
        {
            let br = Rect::new(cx, y, cw, 52.0 * f);
            estilo::cartao(br, false, !e.bau.is_empty());
            let dentro: Vec<(u16, u32)> = e.bau.iter().map(|s| (s.item_id, s.qty)).collect();
            estilo::texto_forte(
                br.x + 14.0 * f,
                br.y + 22.0 * f,
                &format!("Baú da ilha · {}/{} espaços", e.bau.len(), e.banco),
                14,
                estilo::OURO,
            );
            estilo::texto(
                br.x + 14.0 * f,
                br.y + 42.0 * f,
                &if dentro.is_empty() {
                    "Vazio — a colheita cai aqui.".to_string()
                } else {
                    lista(&dentro, nome_item)
                },
                12,
                if dentro.is_empty() { estilo::SUAVE } else { estilo::TEXTO },
            );
            if !e.bau.is_empty() {
                let b = Rect::new(
                    br.x + br.w - 136.0 * f,
                    br.y + (br.h - 38.0 * f) * 0.5,
                    124.0 * f,
                    38.0 * f,
                );
                crate::foco::marca(crate::foco::chave::ILHA_RETIRAR, b);
                estilo::botao(b, "Retirar", estilo::estado_de(b, false, false), false);
                if clicou_em(b) {
                    pedido = Some(PedidoColonia::Retirar);
                }
            }
            y += br.h + 12.0 * f;
        }

        // Os tres eixos.
        for i in 0..EIXOS {
            let r = Rect::new(cx, y, cw, linha_h - 10.0 * f);
            let no_maximo = e.niveis[i] >= NIVEL_MAX;
            estilo::cartao(r, false, false);
            estilo::texto_forte(
                r.x + 14.0 * f,
                r.y + 24.0 * f,
                &format!("{} {}/{}", eixo::NOMES[i], e.niveis[i], NIVEL_MAX),
                16,
                estilo::TEXTO,
            );
            // AGORA em cima, DEPOIS embaixo: o jogador lê o que tem antes de
            // ler o que ganharia.
            estilo::texto(
                r.x + 14.0 * f,
                r.y + 44.0 * f,
                &agora(i, e.niveis[i]),
                12,
                estilo::TEXTO,
            );
            let custo = e.custos.get(i).cloned().unwrap_or_default();
            estilo::texto(
                r.x + 14.0 * f,
                r.y + 62.0 * f,
                &if no_maximo {
                    "No máximo.".to_string()
                } else {
                    format!("{} · custa {}", depois(i, e.niveis[i]), lista(&custo, nome_item))
                },
                12,
                estilo::SUAVE,
            );
            if !no_maximo {
                let b = Rect::new(
                    r.x + r.w - 136.0 * f,
                    r.y + (r.h - 40.0 * f) * 0.5,
                    124.0 * f,
                    40.0 * f,
                );
                if i == eixo::ASSENTAMENTO {
                    crate::foco::marca(crate::foco::chave::ILHA_ASSENTAMENTO, b);
                }
                estilo::botao(b, "Melhorar", estilo::estado_de(b, false, false), false);
                if clicou_em(b) {
                    pedido = Some(PedidoColonia::Melhorar { eixo: i as u8 });
                }
            }
            y += linha_h;
        }

        // OS MORADORES. Uma linha por vaga: quem esta' nela, ou o convite
        // pra alguem. Vem DEPOIS dos eixos porque o assentamento e' quem abre
        // as vagas — a ordem da tela e' a ordem em que se faz a coisa.
        if e.vagas > 0 {
            estilo::texto_forte(cx, y + 20.0 * f, "Moradores", 16, estilo::OURO);
            y += 32.0 * f;
            for vaga in 0..e.vagas as usize {
                let r = Rect::new(cx, y, cw, 72.0 * f);
                let quem = e.trabalhadores.get(vaga).copied();
                estilo::cartao(r, false, quem.is_some());
                // O RETRATO 3D de quem mora aí — ou de quem o dedo está
                // prestes a contratar.
                //
                // O dono: "quando for comprar um trabalhador ter o modelo 3D
                // dele também bonitinho na direita". Cinco botões com três
                // letras ("Len", "Min", "Mer") não dizem nada sobre quem se
                // está contratando; o modelo diz. Na casa vazia ele mostra o
                // ofício sob o dedo, então passar por cima dos cinco é um
                // desfile dos candidatos.
                let retrato = Rect::new(r.x + 6.0 * f, r.y + 4.0 * f, 64.0 * f, 64.0 * f);
                let sob_o_dedo = shared::colonia::Profissao::TODAS
                    .iter()
                    .copied()
                    .enumerate()
                    .find(|(i, _)| {
                        let bw = 62.0 * f;
                        let n = shared::colonia::Profissao::TODAS.len() as f32;
                        let bx = r.x + r.w - bw * n - 8.0 * f + *i as f32 * bw;
                        Rect::new(bx, r.y + 20.0 * f, bw - 4.0 * f, 32.0 * f).contains(m)
                    })
                    .map(|(_, op)| op);
                if let Some(mostrar) = quem.or(sob_o_dedo) {
                    let nome_do_rig =
                        crate::render3d::rig_do_npc(mostrar.papel() as u8, vaga as u64);
                    crate::render3d::vitrine_rig(
                        vox,
                        nome_do_rig,
                        retrato,
                        (get_time() as f32 * 0.5).sin() * 0.9,
                        solido,
                    );
                }
                let texto = match quem {
                    Some(q) => {
                        let (item, qtd) = shared::colonia::por_hora_do_trabalhador(
                            q,
                            e.niveis[eixo::RECURSOS],
                        );
                        format!("{} · {} {}/h", q.nome(), qtd, nome_item(item))
                    }
                    None => match sob_o_dedo {
                        Some(op) => {
                            let (item, qtd) = shared::colonia::por_hora_do_trabalhador(
                                op,
                                e.niveis[eixo::RECURSOS],
                            );
                            format!("{} daria {} {}/h", op.nome(), qtd, nome_item(item))
                        }
                        None => "Casa vazia — escolha um ofício".to_string(),
                    },
                };
                estilo::texto(
                    r.x + 78.0 * f,
                    r.y + 30.0 * f,
                    &texto,
                    13,
                    if quem.is_some() { estilo::TEXTO } else { estilo::SUAVE },
                );
                // Um botao por oficio, do lado direito. Sao cinco e cabem:
                // uma lista suspensa esconderia a escolha atras de um toque.
                let bw = 62.0 * f;
                let total = shared::colonia::Profissao::TODAS.len() as f32;
                let mut bx = r.x + r.w - bw * total - 8.0 * f;
                for op in shared::colonia::Profissao::TODAS {
                    let b = Rect::new(bx, r.y + 20.0 * f, bw - 4.0 * f, 32.0 * f);
                    let posto = quem == Some(op);
                    estilo::botao(
                        b,
                        &op.nome()[..3.min(op.nome().len())],
                        estilo::estado_de(b, posto, false),
                        posto,
                    );
                    // O foco aponta pro PRIMEIRO ofício de uma casa vazia:
                    // qualquer um serve pro tutorial, e apontar pros cinco
                    // seria não apontar pra nenhum.
                    if quem.is_none() && op == shared::colonia::Profissao::TODAS[0] {
                        crate::foco::marca(crate::foco::chave::ILHA_CONTRATAR, b);
                    }
                    if clicou_em(b) {
                        pedido = Some(if posto {
                            PedidoColonia::Demitir { vaga: vaga as u8 }
                        } else {
                            PedidoColonia::Contratar {
                                vaga: vaga as u8,
                                oficio: op.indice(),
                            }
                        });
                    }
                    bx += bw;
                }
                y += 78.0 * f;
            }
        }

        // Nao ha' mais "voltar ao porto": a ilha e' um painel, e sair dele e'
        // fechar. O X e o clique por fora bastam.
        //
        // Colher e melhorar NAO fecham: o jogador quase sempre faz os dois
        // seguidos, e fechar a cada toque custa um toque a mais.
        crate::rolagem::recortar(None);
        self.rolagem.desenha(dir, total);

        // O X e o clique POR FORA fecham. Fora da rolagem de propósito: ela
        // já resolveu o que é arrasto, e arrastar a lista até passar da borda
        // não pode fechar o painel.
        if crate::foco::clique() && (fechar.contains(m) || !p.contains(m)) {
            self.fechar();
        }
        pedido
    }
}

/// Limites do zoom da maquete. O mínimo ainda mostra a ilhota inteira; o
/// máximo chega perto o bastante pra ver um morador trabalhando.
const ZOOM_MIN: f32 = 0.8;
const ZOOM_MAX: f32 = 3.5;

/// O ponto onde o dedo ENCOSTOU neste quadro.
///
/// No quadro do aperto o mouse simulado ainda aponta pro toque anterior, e
/// testar retângulo com ele erra. Igual ao de `mapa`.
fn apertou_em() -> Option<Vec2> {
    if let Some(t) = touches()
        .into_iter()
        .find(|t| t.phase == TouchPhase::Started)
    {
        return Some(t.position);
    }
    is_mouse_button_pressed(MouseButton::Left).then(|| Vec2::from(mouse_position()))
}

/// A JANELA do painel, medida a partir da tela.
///
/// Fora do desenho pra ser testada: foi uma conta de janela escrita à mão que
/// pôs o botão de entrar da Ilha Mágica fora da tela e prendeu o jogador lá.
fn janela(seguro: Rect, f: f32) -> Rect {
    let w = (1040.0 * f).min(seguro.w - 16.0);
    let h = (720.0 * f).min(seguro.h - 16.0);
    Rect::new(
        seguro.center().x - w * 0.5,
        seguro.center().y - h * 0.5,
        w,
        h,
    )
}

/// As duas colunas: (maquete à esquerda, controles à direita).
///
/// A da esquerda tem 46% do que sobra depois das margens; a da direita, o
/// resto. Numa janela estreita a maquete cede primeiro — ela é pra olhar, e
/// a da direita é onde se toca.
fn colunas(p: Rect, f: f32) -> (Rect, Rect) {
    let topo = p.y + 76.0 * f;
    let alt = (p.h - 92.0 * f).max(40.0);
    let util = (p.w - 56.0 * f).max(80.0);
    let esq = util * 0.46;
    let maquete = Rect::new(p.x + 20.0 * f, topo, esq, alt);
    let dir = Rect::new(maquete.x + esq + 16.0 * f, topo, util - esq, alt);
    (maquete, dir)
}

/// Onde cada morador fica em pé: na FRENTE do prédio do ofício dele.
///
/// Percorre os prédios na MESMA ordem e com a mesma contagem por papel que
/// `construcoes::assar_colonia` usa. Fosse outra varredura, um morador podia
/// aparecer na porta de uma casa que a maquete não desenhou.
///
/// Sem prédio do ofício, ele fica na praça: melhor um morador no centro que
/// um morador invisível.
fn onde_ficam(
    ger: &shared::terreno::Gerador,
    trabalhadores: &[shared::colonia::Profissao],
) -> Vec<(shared::colonia::Profissao, Vec2)> {
    use std::collections::HashMap;
    let vila = ger.vila();
    let praca = ger.cidade().map(|c| c.centro()).unwrap_or_default();
    let mut usado: HashMap<shared::construcao::Papel, usize> = HashMap::new();
    let mut saida = Vec::new();
    for t in trabalhadores {
        let papel = t.papel();
        let n = usado.entry(papel).or_default();
        let predio = vila
            .predios
            .iter()
            .filter(|p| p.papel == papel)
            .nth(*n)
            .map(|p| {
                // Dois metros À FRENTE da porta, e não dentro da parede: o
                // `yaw_q` é o quarto de volta que a frente olha.
                let frente = shared::construcao::frente_de(p.yaw_q);
                vec2(p.pos.x, p.pos.z) + vec2(frente.x, frente.y) * 2.2
            });
        *n += 1;
        saida.push((*t, predio.unwrap_or(vec2(praca.x, praca.y))));
    }
    saida
}

fn resumo(e: &Estado) -> String {
    format!(
        "Tamanho {} · Recursos {} · Banco {} espaços",
        e.niveis[eixo::TAMANHO],
        e.niveis[eixo::RECURSOS],
        e.banco
    )
}

/// O que o eixo DÁ HOJE — e é isto que faltava.
///
/// O painel só dizia o que a melhoria ia dar; o jogador não tinha como saber
/// o que já tinha. "Não tá mostrando o nível do que já existe upgradado na
/// ilha, só mostra pra melhorar mas não o que já tem" — e sem o AGORA, o
/// DEPOIS não quer dizer nada: subir de 3 pra 4 vagas e de 1 pra 4 são a
/// mesma frase.
///
/// A linha do TAMANHO ainda falava em "blocos de raio" de uma ilha que não
/// cresce mais desde que ela virou uma só (`colonia::RAIO_BLOCOS`).
fn agora(i: usize, n: u8) -> String {
    match i {
        eixo::ASSENTAMENTO => {
            let a = shared::colonia::Assentamento::do_nivel(n);
            let v = shared::colonia::vagas_de_trabalho(n);
            match v {
                0 => format!("{}, sem casa pra morador.", a.nome()),
                1 => format!("{}, 1 casa de ofício.", a.nome()),
                _ => format!("{}, {v} casas de ofício.", a.nome()),
            }
        }
        eixo::RECURSOS => {
            let (_, q) = shared::colonia::por_hora_do_trabalhador(
                shared::colonia::Profissao::Lenhador,
                n,
            );
            format!("Cada morador rende {q}/h do ofício dele.")
        }
        _ => format!("{} espaços no baú da ilha.", shared::colonia::espacos_do_bau(n)),
    }
}

/// O que a PRÓXIMA melhoria muda, em relação ao que se tem.
fn depois(i: usize, n: u8) -> String {
    if n >= NIVEL_MAX {
        return "No máximo.".into();
    }
    let p = n + 1;
    match i {
        eixo::ASSENTAMENTO => {
            let (a, b) = (
                shared::colonia::vagas_de_trabalho(n),
                shared::colonia::vagas_de_trabalho(p),
            );
            let nome = shared::colonia::Assentamento::do_nivel(p).nome();
            if b > a {
                format!("→ {nome}, {b} casas (+{})", b - a)
            } else {
                format!("→ {nome}, mais terreno plano")
            }
        }
        eixo::RECURSOS => {
            let ate = |x: u8| {
                shared::colonia::por_hora_do_trabalhador(
                    shared::colonia::Profissao::Lenhador,
                    x,
                )
                .1
            };
            format!("→ {}/h por morador (de {})", ate(p), ate(n))
        }
        _ => format!(
            "→ {} espaços (de {})",
            shared::colonia::espacos_do_bau(p),
            shared::colonia::espacos_do_bau(n)
        ),
    }
}

fn lista(itens: &[(u16, u32)], nome_item: &dyn Fn(u16) -> String) -> String {
    if itens.is_empty() {
        return "nada".into();
    }
    itens
        .iter()
        .map(|(id, q)| format!("{q}× {}", nome_item(*id)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// O que há pra colher — e POR QUE não há, quando não há.
///
/// "Pronto para colher: nada" e "A ilha ainda não rendeu nada" dizem a mesma
/// coisa pro jogo e coisas opostas pro jogador: uma ilha sem MORADOR não vai
/// render por mais que ele espere, e uma ilha com morador só precisa de
/// tempo. O dono disse que estava "bem ruim de entender o que tá sendo
/// colhido"; a frase que faltava era a que separa esses dois casos.
fn colheita_em_texto(e: &Estado, nome_item: &dyn Fn(u16) -> String) -> String {
    if !e.colheita.is_empty() {
        return format!("Pronto para colher: {}", lista(&e.colheita, nome_item));
    }
    if e.trabalhadores.is_empty() {
        return if e.vagas == 0 {
            "Ninguém mora aqui. Melhore o Assentamento para abrir uma casa.".into()
        } else {
            "Ninguém mora aqui. Contrate um ofício numa casa vazia.".into()
        };
    }
    // Com morador, é só tempo: dizer QUANTO falta é o que transforma
    // "não rendeu" em "volte às tantas".
    let por_hora = shared::colonia::por_hora_dos_trabalhadores(
        &e.trabalhadores,
        e.niveis[eixo::RECURSOS],
    );
    match por_hora.first() {
        Some((id, q)) if *q > 0 => {
            let min = (60.0 / *q as f32).ceil() as u32;
            format!(
                "Rendendo {}/h. A primeira unidade sai em ~{min} min.",
                lista(&por_hora, nome_item)
            )
        }
        _ => "A ilha ainda não rendeu nada.".into(),
    }
}

/// O relogio e' a regra inteira da colonia numa linha, e por isso ele diz o
/// que esta' acontecendo AGORA e nao so' quantas horas passaram: e' a
/// diferenca entre "12h" e "passou das 12h, e desde entao rende metade".
fn aviso_do_relogio(horas: f32) -> String {
    let h = shared::colonia::HORAS_CHEIAS;
    let teto = shared::colonia::HORAS_TETO;
    if horas < h {
        format!("{horas:.0}h de {h:.0}h — rendendo cheio.")
    } else if horas < teto {
        format!("{horas:.0}h — passou das {h:.0}h e desde então rende metade.")
    } else {
        format!("{horas:.0}h — parou de render às {teto:.0}h. Colha.")
    }
}

fn cor_do_relogio(horas: f32) -> Color {
    if horas >= shared::colonia::HORAS_TETO {
        estilo::VERMELHO
    } else if horas >= shared::colonia::HORAS_CHEIAS {
        estilo::OURO
    } else {
        estilo::VERDE
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn e(horas: f32) -> Estado {
        Estado {
            niveis: [1; EIXOS],
            horas,
            colheita: vec![],
            custos: vec![],
            banco: 10,
            bau: vec![],
            trabalhadores: vec![],
            vagas: 0,
        }
    }

    /// As tres fases da regra tem que ser DISTINGUIVEIS na tela. Se as duas
    /// primeiras dissessem a mesma coisa, o jogador nao teria como aprender
    /// que voltar antes das 12h rende mais — que e' a unica decisao que a
    /// colonia pede dele.
    /// AS DUAS COLUNAS cabem na janela, e a janela na tela.
    ///
    /// Mesmo guarda que o painel da Ilha Mágica ganhou depois de prender o
    /// jogador. Este é o painel mais largo do jogo (1040x720) e o que tem
    /// mais chance de estourar num celular deitado.
    #[test]
    #[test]
    fn as_colunas_cabem_na_janela() {
        for (w, h) in [
            (734.0f32, 320.0f32),
            (812.0, 375.0),
            (1024.0, 768.0),
            (1920.0, 1080.0),
        ] {
            let seguro = Rect::new(0.0, 0.0, w, h);
            for f in [0.8f32, 1.0, 1.5, 2.2] {
                let p = janela(seguro, f);
                assert!(
                    p.w <= seguro.w && p.h <= seguro.h,
                    "{w}x{h} f={f}: a janela ({:.0}x{:.0}) passa da tela",
                    p.w,
                    p.h
                );
                let (esq, dir) = colunas(p, f);
                for (nome, c) in [("maquete", esq), ("controles", dir)] {
                    assert!(
                        c.w > 0.0 && c.h > 0.0,
                        "{w}x{h} f={f}: a coluna {nome} ficou sem tamanho ({:.0}x{:.0})",
                        c.w,
                        c.h
                    );
                    assert!(
                        c.x >= p.x - 0.01 && c.x + c.w <= p.x + p.w + 0.01,
                        "{w}x{h} f={f}: a coluna {nome} vaza de lado"
                    );
                    assert!(
                        c.y >= p.y && c.y + c.h <= p.y + p.h + 0.01,
                        "{w}x{h} f={f}: a coluna {nome} passa da altura da janela"
                    );
                }
                // Elas não se encavalam: a maquete desenha em 3D por cima de
                // tudo, e sobrepor comeria os botões da direita.
                assert!(
                    esq.x + esq.w <= dir.x + 0.01,
                    "{w}x{h} f={f}: as colunas se sobrepõem"
                );
            }
        }
    }

    #[test]
    fn o_relogio_conta_as_tres_fases() {
        let cheio = aviso_do_relogio(6.0);
        let meio = aviso_do_relogio(18.0);
        let parado = aviso_do_relogio(30.0);
        assert!(cheio.contains("cheio"), "{cheio}");
        assert!(meio.contains("metade"), "{meio}");
        assert!(parado.contains("Colha"), "{parado}");
        assert_ne!(cor_do_relogio(6.0), cor_do_relogio(18.0));
        assert_ne!(cor_do_relogio(18.0), cor_do_relogio(30.0));
    }
}
