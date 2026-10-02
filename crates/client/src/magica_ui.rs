//! O painel da ILHA MÁGICA (`shared::magica`).
//!
//! Duas telas na mesma janela, porque são duas perguntas diferentes:
//!
//! - **Fora** da ilha: "quantos passes eu tenho e quanto tempo compro com
//!   eles?" — e o botão de ir.
//! - **Dentro**: "quanto falta e o que esta ilhota me paga?" — e o botão de
//!   sair antes da hora.
//!
//! Quem decide tudo é o servidor. Aqui só se desenha o estado que ele mandou
//! e se devolve o pedido que o dedo encostou.

use macroquad::prelude::*;
use shared::magica::{AvisoMagica, Bonus, PedidoMagica};

use crate::hud_estilo as estilo;
use crate::ui;

/// A janela, em unidades de `fator_texto`. `escala_do_painel` encolhe isto
/// até caber na tela — e era o passo que faltava.
const LARGURA: f32 = 460.0;
/// NAO BAIXAR SEM MEXER NO RODAPE JUNTO.
///
/// Tentado 380 -> 344 em 29/09/2026 pra fechar o vao entre a linha do saldo e
/// os botoes. O vao fecha, mas o rodape e' ancorado EMBAIXO: encolher a janela
/// sobe os botoes por cima da lista de ilhotas, e "Islet of the Colossus" e
/// "Islet of Spoils" ficam atras dos botoes I/II/III. Os testes de layout daqui
/// passaram assim mesmo — eles cuidam da tarja e das bordas, nao da lista
/// contra os botoes —, entao quem for tentar de novo confere na imagem.
const ALTURA: f32 = 380.0;

const OURO: Color = Color::new(0.93, 0.76, 0.33, 1.0);
const VERMELHO: Color = Color::new(0.95, 0.45, 0.40, 1.0);

/// A JANELA, medida a partir da tela. Fora do desenho pra poder ser testada.
fn janela(seguro: Rect, f: f32) -> Rect {
    let w = (LARGURA * f).min(seguro.w - 16.0);
    let h = (ALTURA * f).min(seguro.h - 16.0);
    Rect::new(
        seguro.center().x - w * 0.5,
        seguro.center().y - h * 0.5,
        w,
        h,
    )
}

#[cfg(debug_assertions)]
pub async fn previa() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-magica-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    crate::render3d::define_alvo(Some(rt.clone()));
    let mut ui = MagicaUi::default();
    ui.na_zona_magica = true;
    ui.recebe(
        AvisoMagica::Estado {
            grau_maximo: 2,
            grau_atual: 1,
            meu_nivel: 30,
            passes: 0,
            gratis: 0,
            fim_unix: 1600,
            dentro: true,
            bonus: 255,
        },
        0.0,
    );
    ui.abrir();
    for _ in 0..3 {
        crate::render3d::camera_padrao();
        clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
        ui.desenha(0.0, 1000);
        unsafe { get_internal_gl().flush() };
        rt.texture
            .get_texture_data()
            .export_png(&format!("{saida}/magica.png"));
        next_frame().await;
    }
}

/// O RODAPÉ — onde mora o botão de entrar —, ancorado no fundo da janela.
///
/// Fora do desenho porque é a medida que quebrou: com a janela em pixels
/// crus e o texto em `f`, o conteúdo empurrava o botão pra fora e não havia
/// como entrar na ilha. Ancorado ao fundo e testado, ele não tem como sair.
fn rodape_de(p: Rect, f: f32, dentro: bool) -> Rect {
    let _ = dentro;
    let alt = 104.0 * f;
    Rect::new(
        p.x + 16.0 * f,
        p.y + p.h - alt - 10.0 * f,
        p.w - 32.0 * f,
        alt,
    )
}

/// A cor de cada bônus.
///
/// São sete ilhotas, e ler o nome de cada uma no meio de uma briga de PvP não
/// acontece. A cor é o que o olho separa sem ler — e é a mesma no HUD e na
/// lista do painel, senão seriam dois códigos para a mesma coisa.
/// A cor de cada bônus. UMA só no jogo inteiro: a tarja do HUD e os nomes no
/// mapa usam esta mesma, pra o jogador aprender o par cor↔ilhota uma vez.
pub(crate) fn cor_do_bonus(b: Bonus) -> Color {
    match b {
        Bonus::Xp => Color::new(0.55, 0.78, 0.98, 1.0),
        Bonus::DropDeMob => Color::new(0.72, 0.86, 0.45, 1.0),
        Bonus::Ouro => Color::new(0.97, 0.82, 0.35, 1.0),
        Bonus::DropDeChefe => Color::new(0.95, 0.52, 0.45, 1.0),
        Bonus::Coleta(0) => Color::new(0.76, 0.58, 0.36, 1.0),
        Bonus::Coleta(5) => Color::new(0.68, 0.60, 0.98, 1.0),
        Bonus::Coleta(_) => Color::new(0.72, 0.74, 0.80, 1.0),
    }
}
const VERDE: Color = Color::new(0.55, 0.85, 0.50, 1.0);
const SUAVE: Color = Color::new(0.72, 0.74, 0.80, 1.0);

#[derive(Debug, Clone, Default)]
pub struct Estado {
    /// Maior degrau liberado (0 = nenhum) e em qual ele está.
    grau_maximo: u8,
    grau_atual: u8,
    /// O nível do personagem, pro aviso de "você está abaixo desta ilha".
    meu_nivel: u32,
    pub passes: u32,
    /// Entradas de graça que ainda há hoje (`magica::GRATIS_POR_DIA`).
    pub gratis: u8,
    pub fim_unix: i64,
    pub dentro: bool,
    pub bonus: u8,
}

/// As peças da tarja do HUD, já medidas. Ver `pecas_da_tarja`.
pub(crate) struct Pecas {
    pub relogio: Rect,
    pub ilhota: Rect,
    pub barra: Rect,
    pub mais: Rect,
    pub sair: Rect,
}

impl Pecas {
    pub fn todas(&self) -> [(&'static str, Rect); 5] {
        [
            ("clock", self.relogio),
            ("ilhota", self.ilhota),
            ("barra", self.barra),
            ("+", self.mais),
            ("sair", self.sair),
        ]
    }
}

#[derive(Default)]
pub struct MagicaUi {
    estado: Option<Estado>,
    aberto: bool,
    /// O jogador PEDIU pra abrir e o estado ainda não voltou do servidor.
    ///
    /// Existe porque a abertura tem que vir do PEDIDO, e não de o estado ser
    /// desconhecido. A versão anterior abria em `estado.is_none()` — ou seja,
    /// só na primeira vez da sessão. Da segunda em diante o jogador tocava
    /// "Magic Island" no menu, o pedido saía, o servidor respondia, e nada
    /// acontecia na tela. O dono: "clico em ilha mágica mas não abre nada e
    /// me deixa travado na quest".
    pedida: bool,
    /// Quantas entradas o jogador escolheu gastar (1..3).
    entradas: u8,
    /// O degrau escolhido (`NivelMagico::grau`). 0 = ainda não escolheu, e aí
    /// vale o maior liberado — que é o que quase todo mundo quer.
    grau: u8,
    aviso: Option<(String, f64)>,
    /// O cliente está CONECTADO numa zona de Ilha Mágica agora?
    ///
    /// Vem do nome da zona, e não de mensagem nenhuma. O `dentro` do estado
    /// chega do processo da ilha e fica VELHO quando o jogador sai: a saída é
    /// um handoff pra zona de origem, e lá ninguém manda estado novo. O
    /// resultado era a tarja continuar na tela depois de sair, com o relógio
    /// correndo — o dono: "a HUD da ilha mágica tá aparecendo até depois de
    /// sair dela com countdown e tudo".
    ///
    /// Amarrar na zona conserta sem depender de mensagem chegar: se o cliente
    /// não está falando com um processo de ilha, não há ilha.
    na_zona_magica: bool,
}

impl MagicaUi {
    pub fn aberto(&self) -> bool {
        self.aberto
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    /// O que o servidor mandou. `Estado` abre o painel só quando ele foi
    /// PEDIDO; dentro da ilha ele chega sozinho (a cada troca de ilhota) e aí
    /// serve só pro HUD — abrir um painel por cima de quem está lutando seria
    /// o melhor jeito de matar o jogador.
    pub fn recebe(&mut self, aviso: AvisoMagica, agora: f64) {
        match aviso {
            AvisoMagica::AbrirPainel => {
                self.grau = self.estado.as_ref().map_or(0, |e| e.grau_atual);
                self.aberto = true;
            }
            AvisoMagica::Estado {
                grau_maximo,
                grau_atual,
                meu_nivel,
                passes,
                gratis,
                fim_unix,
                dentro,
                bonus,
            } => {
                let e = Estado {
                    grau_maximo,
                    grau_atual,
                    meu_nivel,
                    passes,
                    gratis,
                    fim_unix,
                    dentro,
                    bonus,
                };
                // Abre se FOI PEDIDO, ou na primeira notícia de fora da
                // ilha (que é o caso de quem chega pelo tutorial).
                if !dentro && (self.pedida || self.estado.is_none()) {
                    self.aberto = true;
                }
                self.pedida = false;
                self.entradas = self.entradas.clamp(1, 3);
                self.estado = Some(e);
            }
            AvisoMagica::Recusa(t) => self.aviso = Some((t, agora)),
        }
    }

    /// Pedido ao abrir pelo menu.
    pub fn abrir(&mut self) {
        self.aberto = true;
    }

    /// O jogador tocou "Magic Island": a próxima notícia de estado ABRE.
    ///
    /// Chamado junto do envio do pedido, nunca sozinho — é o par do
    /// `PedidoMagica::Painel`.
    pub fn pedir_abertura(&mut self) {
        self.pedida = true;
    }

    /// Quanto falta, em segundos, com o relógio do cliente.
    pub fn resta(&self, agora_unix: i64) -> i64 {
        self.estado
            .as_ref()
            .map_or(0, |e| shared::magica::resta(e.fim_unix, agora_unix))
    }

    /// Em que zona o cliente está. Uma vez por quadro, do nome da zona.
    pub fn atualiza_zona(&mut self, zona: Option<&str>) {
        self.na_zona_magica = zona.is_some_and(shared::magica::e_magica);
    }

    /// O cliente está numa zona de Ilha Mágica? (Sem olhar o relógio.)
    pub fn na_ilha(&self) -> bool {
        self.na_zona_magica
    }

    /// Está dentro da ilha com tempo valendo?
    ///
    /// As TRÊS condições: a zona é de ilha (senão a tarja sobrevive à saída),
    /// o servidor disse que está dentro, e ainda há tempo.
    pub fn dentro(&self, agora_unix: i64) -> bool {
        self.na_zona_magica
            && self.estado.as_ref().is_some_and(|e| e.dentro)
            && self.resta(agora_unix) > 0
    }

    /// O bônus da ilhota de agora.
    pub fn bonus(&self) -> Option<Bonus> {
        self.estado.as_ref().and_then(|e| Bonus::do_indice(e.bonus))
    }

    /// A tarja do HUD, dentro da ilha: o relógio e a ilhota.
    ///
    /// Fica no HUD e não no painel porque é informação que muda a decisão
    /// enquanto se joga — "faltam 4 minutos e eu estou na ilhota errada" é
    /// exatamente o que o jogador precisa saber sem abrir nada.
    /// Onde fica o botão "+" de estender, dada a tarja.
    ///
    /// Fora do desenho pra poder ser medido: ele divide a tarja com o relógio
    /// e a ilhota, e medida escrita à mão nesta tela já pôs o "Enter" fora
    /// da janela uma vez.
    /// A faixa dos degraus: (y da linha de detalhe, os três botões).
    ///
    /// Fora do desenho pra poder ser medida. A primeira versão desta faixa
    /// saiu com os rótulos sobrepostos no emulador, e medida escrita à mão
    /// nesta tela já pôs o "Enter" fora da janela uma vez.
    pub(crate) fn faixa_dos_degraus(rod: Rect, f: f32) -> (f32, Vec<Rect>) {
        // 34 em pixels CRUS no mínimo: `area_de_toque` cresce o alvo até o
        // dedo, mas com teto (+14). Num `f` pequeno, 34 × 0,8 = 27 e nem com
        // o crescimento chega aos 44 pt da Apple — o teste pegou em f=0,8.
        let alto = (34.0 * f).max(34.0);
        let y = rod.y - 104.0 * f;
        let folga = 8.0 * f;
        // One button per tier (`magica::NIVEIS`): it was a fixed three, and
        // the fourth tier crashed the panel on an out-of-bounds index.
        let n = shared::magica::NIVEIS.len();
        let larg = (rod.w - folga * (n as f32 - 1.0)) / n as f32;
        (
            y + alto + 15.0 * f,
            (0..n).map(|k| Rect::new(rod.x + k as f32 * (larg + folga), y, larg, alto)).collect(),
        )
    }

    /// TODAS as peças da tarja, de uma vez.
    ///
    /// Uma função só porque o defeito era entre peças: os botões eram postos
    /// dentro da tarja, em cima da barra de tempo, que ocupa a largura toda.
    /// O teste que existia media só os dois botões ENTRE SI e por isso passou
    /// com a sobreposição na tela. Medindo tudo junto, o teste consegue exigir
    /// que nada encoste em nada.
    pub(crate) fn pecas_da_tarja(tarja: Rect, f: f32) -> Pecas {
        let pad = 12.0 * f;
        // 32 no MÍNIMO, em pixels crus: `area_de_toque` cresce o alvo até o
        // dedo, mas com teto (+14). Num `f` pequeno, 30 × 0,8 = 24 e nem com
        // o crescimento chega aos 44 pt da Apple.
        let alto = (32.0 * f).max(32.0);
        let lado = alto;
        let larg_sair = (64.0 * f).max(58.0);
        let by = tarja.y + (tarja.h - alto) * 0.5;
        // Os botões ancoram na DIREITA e o texto ocupa o que sobra. O
        // contrário (texto primeiro) faria o "Leave" andar conforme o nome da
        // ilhota, e botão que muda de lugar é botão que se erra.
        let sair = Rect::new(tarja.x + tarja.w - pad - larg_sair, by, larg_sair, alto);
        let mais = Rect::new(sair.x - 8.0 * f - lado, by, lado, alto);
        // O texto acaba ONDE OS BOTÕES COMEÇAM, e a barra acompanha o texto.
        let fim = mais.x - 10.0 * f;
        let relogio = Rect::new(tarja.x + pad, tarja.y + 6.0 * f, 92.0 * f, 26.0 * f);
        let ilhota = Rect::new(
            relogio.x + relogio.w + 8.0 * f,
            tarja.y + 6.0 * f,
            (fim - (relogio.x + relogio.w + 8.0 * f)).max(0.0),
            30.0 * f,
        );
        let barra = Rect::new(
            tarja.x + pad,
            tarja.y + tarja.h - 12.0 * f,
            (fim - tarja.x - pad).max(0.0),
            5.0 * f,
        );
        Pecas {
            relogio,
            ilhota,
            barra,
            mais,
            sair,
        }
    }

    /// A FAIXA DE PVP, entre a área e o minimapa.
    ///
    /// O dono: "tem que indicar essa safe zone / PvP on em algum lugar; a
    /// minha recomendação é ali abaixo do ilha_magica SA01 CH magica1, entre
    /// isso e o minimapa".
    ///
    /// Ela existe porque a regra MUDA DE ILHOTA PRA ILHOTA: a da chegada é
    /// porto seguro e as outras seis são PvP aberto. Uma regra que muda
    /// enquanto se anda e não aparece em lugar nenhum é uma armadilha — o
    /// jogador só descobre qual valia depois de morrer.
    ///
    /// Quem decide é `shared::magica::e_porto_seguro`, a MESMA função que o
    /// servidor usa pra recusar o dano. Duas cópias divergiriam, e a tela
    /// diria "seguro" enquanto o servidor deixava bater.
    pub fn desenha_faixa_pvp(&self, eu: Option<Vec2>) {
        if !self.na_zona_magica {
            return;
        }
        let z = crate::hud_layout::atual();
        let r = crate::hud_layout::faixa_pvp_rect(&z);
        // O Vec2 do shared é de outra versão do glam.
        let seguro =
            eu.is_some_and(|p| shared::magica::e_porto_seguro(::glam::Vec2::new(p.x, p.y)));
        let (texto, cor) = if seguro {
            ("Safe zone · no PvP", VERDE)
        } else {
            ("PvP open", VERMELHO)
        };
        let f = estilo::fator_texto();
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.55));
        draw_rectangle(r.x, r.y, 3.0 * f, r.h, cor);
        // The coloured bar on the left edge carries the state; no dot before
        // the word (owner, 30/09/2026: "take out the lil balls in front of
        // texts").
        estilo::texto(r.x + 12.0 * f, r.y + r.h * 0.5 + 5.0 * f, texto, 13, cor);
    }

    pub fn desenha_hud(&mut self, agora_unix: i64) -> Option<PedidoMagica> {
        if !self.dentro(agora_unix) {
            return None;
        }
        let resta = self.resta(agora_unix);
        let z = crate::hud_layout::atual();
        let f = estilo::fator_texto();
        let urgente = resta <= 60;

        // NA ESQUERDA, ABAIXO DO RASTREADOR — e não mais no topo do meio.
        //
        // O dono: "eu disse abaixo do resumo das missões, na esquerda". E faz
        // sentido: qual ilhota e quanto falta se lê junto da missão que se
        // está fazendo. No topo do meio a tarja disputava o lugar onde o olho
        // procura o alvo.
        //
        // A largura é a da coluna (a do rastreador), não a do texto: saindo
        // do texto, a tarja mudava de tamanho a cada travessia de ponte e o
        // relógio nunca ficava onde se aprendeu a procurar.
        let r = crate::hud_layout::tarja_magica_rect(&z);
        estilo::painel(r);
        let p = Self::pecas_da_tarja(r, f);

        // O RELÓGIO, grande, à esquerda. É o número que decide se vale
        // atravessar ou não.
        let cor = if urgente { VERMELHO } else { OURO };
        let relogio = format!("{}:{:02}", resta / 60, resta % 60);
        estilo::texto_forte(p.relogio.x, p.relogio.y + 22.0 * f, &relogio, 26, cor);

        // A BARRA, por baixo: o relógio em número diz quanto falta, a barra
        // diz quanto falta COMPARADO ao que cabe (1h30). Um vê-se lendo, a
        // outra vê-se de canto de olho no meio de uma briga. Ela para onde os
        // botões começam — antes atravessava por baixo deles.
        let frac = (resta as f32 / shared::magica::TETO_S as f32).clamp(0.0, 1.0);
        draw_rectangle(
            p.barra.x,
            p.barra.y,
            p.barra.w,
            p.barra.h,
            Color::new(1.0, 1.0, 1.0, 0.13),
        );
        draw_rectangle(p.barra.x, p.barra.y, p.barra.w * frac, p.barra.h, cor);

        // A ILHOTA, NA COR DELA. São sete bônus; cor é o que o olho separa
        // sem ler. O nome e o multiplicador na mesma linha, porque a tarja
        // agora é larga e baixa em vez de estreita e alta.
        let (nome, mult, c) = match self.bonus() {
            Some(b) => (
                b.nome(),
                format!("×{:.2}", b.multiplicador()),
                cor_do_bonus(b),
            ),
            None => ("Bridge", "no bonus".to_string(), SUAVE),
        };
        // The name in its bonus colour, with no dot in front of it.
        let nx = p.ilhota.x + 2.0 * f;
        estilo::texto_ajustado(
            nome,
            nx,
            p.ilhota.y + 15.0 * f,
            p.ilhota.w - 14.0 * f,
            15,
            c,
        );
        estilo::texto_forte(nx, p.ilhota.y + 31.0 * f, &mult, 15, c);

        // ESTENDER SEM SAIR DO JOGO.
        //
        // O dono: "countdown de tempo que você ainda tem na ilha, com a
        // possibilidade de aumentar caso tenha tickets".
        //
        // Aqui e não só no painel porque é aqui que a decisão acontece: o
        // relógio virando vermelho é o que faz a pessoa querer mais tempo, e
        // mandá-la abrir menu no meio de uma briga pra isso é perder a ilha
        // enquanto se procura o botão.
        //
        // Só aparece com passe na mão e com espaço no teto — botão que não
        // faz nada é pior que botão nenhum.
        let e = self.estado.clone().unwrap_or_default();
        // `pode_entrar` já cobre as duas condições (passe na mão e espaço no
        // teto de 1h30): reimplementá-las aqui daria duas regras pro mesmo
        // assunto, e a da tela ficaria velha.
        let total = e.passes + e.gratis as u32;
        let mut pedido = None;
        if shared::magica::pode_entrar(e.fim_unix, agora_unix, total, 1).is_ok()
            && crate::ui::botao(p.mais, "+", true)
        {
            pedido = Some(PedidoMagica::Entrar {
                entradas: 1,
                grau: e.grau_atual,
            });
        }
        // SAIR DA ILHA, daqui mesmo.
        //
        // O dono: "tem que ter botão de sair da ilha". Antes só dava pelo
        // painel — e o painel NÃO abre de dentro (o estado chega a cada
        // travessia de ponte, e abri-lo sozinho atrapalharia quem está
        // lutando). Ou seja: de dentro não havia saída nenhuma na interface.
        //
        // O relógio CONTINUA correndo depois de sair, e isso é regra de
        // `PedidoMagica::Sair`: senão o jogador sairia no primeiro susto e
        // voltaria com o tempo intacto, e a ilha deixaria de ter hora.
        if crate::ui::botao(p.sair, "Leave", true) {
            pedido = Some(PedidoMagica::Sair);
        }
        pedido
    }

    pub fn desenha(&mut self, agora: f64, agora_unix: i64) -> Option<PedidoMagica> {
        if !self.aberto {
            return None;
        }
        estilo::no_painel(estilo::escala_do_painel(LARGURA, ALTURA), || {
            self.desenha_na_escala(agora, agora_unix)
        })
    }

    fn desenha_na_escala(&mut self, agora: f64, agora_unix: i64) -> Option<PedidoMagica> {
        let e = self.estado.clone().unwrap_or_default();
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();

        // A JANELA É MEDIDA EM `f`, e era esse o defeito.
        //
        // Antes ela vinha de `ui::painel(480, 430)`, que monta o retângulo em
        // PIXELS CRUS — e o texto dentro dele escala por `fator_texto()`. No
        // celular o conteúdo media ~443 px numa área útil de 346: o botão
        // "Enter" caía FORA da janela. O dono: "a HUD de entrar na Ilha
        // Mágica tá um lixo, nem consigo entrar".
        //
        // Agora tudo multiplica por `f`, e `escala_do_painel` (que recebe a
        // base de verdade) encolhe o conjunto até caber na tela.
        let p = janela(seguro, f);
        crate::hud_layout::escurece(0.5);
        estilo::painel_destaque(p, OURO);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let x = p.x + 16.0 * f;
        let larg = p.w - 32.0 * f;
        let mut pedido = None;

        estilo::texto_forte(x, p.y + 32.0 * f, "Magic Island", 20, OURO);
        let fechar = Rect::new(p.x + p.w - 44.0 * f, p.y + 8.0 * f, 36.0 * f, 34.0 * f);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 6.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );

        estilo::texto(
            x,
            p.y + 54.0 * f,
            "Seven islets joined by bridges · PvP open · dying sends you back to the landing",
            13,
            SUAVE,
        );

        // ── O RODAPÉ É ANCORADO NO FUNDO, e desenhado primeiro ──
        //
        // Ele é a razão de a janela existir: entrar. Ancorado, ele não pode
        // ser empurrado pra fora por nada que venha acima — que foi
        // exatamente o que aconteceu.
        let rod = rodape_de(p, f, e.dentro);
        let total = e.passes + e.gratis as u32;

        if e.dentro {
            let grau = if self.grau == 0 {
                e.grau_atual
            } else {
                self.grau
            };
            let (linha, fila) = Self::faixa_dos_degraus(rod, f);
            for (k, nv) in shared::magica::NIVEIS.iter().enumerate() {
                let liberado = e.grau_maximo >= nv.grau;
                if grau == nv.grau {
                    estilo::ret_arredondado(fila[k], 6.0, estilo::alfa(estilo::OURO, 0.22));
                }
                const ROMANO: [&str; 6] = ["I", "II", "III", "IV", "V", "VI"];
                if ui::botao(fila[k], ROMANO[k], liberado) && liberado {
                    self.grau = nv.grau;
                }
            }
            if let Some(nv) = shared::magica::NIVEIS.iter().find(|n| n.grau == grau) {
                estilo::texto_centro(
                    rod.center().x,
                    linha,
                    &format!(
                        "{} · mobs lv {}-{} · power {}",
                        nv.nome,
                        nv.mob.0,
                        nv.mob.1,
                        crate::bolsa::milhar(nv.poder() as u64)
                    ),
                    13,
                    SUAVE,
                );
            }
            let viajar = Rect::new(rod.x, rod.y + 5.0 * f, rod.w, 38.0 * f);
            let pode_viajar = grau != e.grau_atual && grau > 0 && grau <= e.grau_maximo;
            if ui::botao(viajar, "Switch island without spending a pass", pode_viajar) && pode_viajar {
                pedido = Some(PedidoMagica::Trocar { grau });
                self.aberto = false;
            }
            if ui::botao(
                Rect::new(rod.x, rod.y + 51.0 * f, rod.w, 34.0 * f),
                "Leave island",
                true,
            ) {
                pedido = Some(PedidoMagica::Sair);
                self.aberto = false;
            }
            estilo::texto(
                rod.x,
                rod.y + 100.0 * f,
                "Switching and leaving do not pause the clock.",
                12,
                SUAVE,
            );
        } else {
            let retomar = shared::magica::resta(e.fim_unix, agora_unix) > 0;
            // Escolher 1, 2 ou 3 antes de ir: acumular é decisão do jogador,
            // e gastar três de uma vez sem ter pedido seria roubo.
            let bw = (rod.w - 16.0 * f) / 3.0;
            for n in 1u8..=3 {
                let caixa = Rect::new(rod.x + (n - 1) as f32 * (bw + 8.0 * f), rod.y, bw, 32.0 * f);
                if retomar {
                    continue;
                }
                let pode = total >= n as u32;
                estilo::botao(
                    caixa,
                    &format!("{n} = {}min", 30 * n),
                    estilo::estado_de(caixa, false, self.entradas == n),
                    self.entradas == n,
                );
                if pode && clicou && caixa.contains(m) {
                    self.entradas = n;
                }
            }
            // ── OS DEGRAUS ──
            //
            // O dono: "a ilha mágica precisa ter níveis; você é nv 15, aí pode
            // entrar na ilha desbloqueada no nv 15 com poder recomendado
            // 1500; a próxima nv 30 etc — dessa forma todos os mobs ficam
            // padronizados".
            //
            // O BOTÃO É SÓ O ALGARISMO, e o detalhe vai numa linha abaixo.
            //
            // A primeira versão punha `"{nome}\npoder {n}"` dentro do rótulo.
            // `ui::botao` desenha uma linha só: o `\n` não quebrou nada, o
            // texto transbordou e os três se sobrepuseram — "Ilha Mágica
            // I⏎poder 75" invadindo "ha Mágica II". Visto em print do
            // emulador, que é o único jeito de ver isto.
            let grau = if self.grau == 0 {
                e.grau_maximo
            } else {
                self.grau
            };
            let (linha, fila) = Self::faixa_dos_degraus(rod, f);
            for (k, nv) in shared::magica::NIVEIS.iter().enumerate() {
                let r = fila[k];
                let liberado = e.grau_maximo >= nv.grau;
                if nv.grau == grau {
                    estilo::ret_arredondado(r, 6.0, estilo::alfa(estilo::OURO, 0.22));
                }
                const ROMANO: [&str; 6] = ["I", "II", "III", "IV", "V", "VI"];
                if ui::botao(r, ROMANO[k], liberado) && liberado {
                    self.grau = nv.grau;
                }
            }
            // A LINHA DE DETALHE do degrau escolhido — uma só, centrada.
            if let Some(nv) = shared::magica::NIVEIS.iter().find(|n| n.grau == grau) {
                let (txt, cor) = if e.grau_maximo < nv.grau {
                    (format!("Opens at level {}", nv.exige_nivel), estilo::SUAVE)
                } else if nv.acima_do_nivel(e.meu_nivel) {
                    // O aviso de que você está abaixo: é o que torna entrar no
                    // 15 numa ilha de 20-23 uma escolha, e não uma surpresa.
                    (
                        format!(
                            "Mobs lv {}-{} · power {} · you {}",
                            nv.mob.0,
                            nv.mob.1,
                            crate::bolsa::milhar(nv.poder() as u64),
                            e.meu_nivel
                        ),
                        estilo::VERMELHO,
                    )
                } else {
                    (
                        format!(
                            "Mobs lv {}-{} · power {}",
                            nv.mob.0,
                            nv.mob.1,
                            crate::bolsa::milhar(nv.poder() as u64)
                        ),
                        estilo::SUAVE,
                    )
                };
                estilo::texto_centro(rod.center().x, linha, &txt, 13, cor);
            }
            let n = self.entradas.max(1);
            let de_graca = (e.gratis as u32).min(n as u32);
            let rot = if de_graca == n as u32 {
                format!("Enter — {n} free")
            } else if de_graca > 0 {
                format!("Enter — {de_graca} free + {} pass", n as u32 - de_graca)
            } else {
                format!("Enter — {n} pass(es)")
            };
            // Sem degrau liberado não há entrada: o portão é o nível.
            let pode = (retomar || total >= n as u32) && grau > 0;
            let b = Rect::new(rod.x, rod.y + 40.0 * f, rod.w, 40.0 * f);
            // O destaque da trava de nível mira AQUI depois que o painel abre.
            crate::foco::marca(crate::foco::chave::MAGICA_ENTRAR, b);
            if ui::botao(
                b,
                if retomar {
                    "Go back without spending a pass"
                } else {
                    &rot
                },
                pode,
            ) && pode
            {
                pedido = Some(PedidoMagica::Entrar {
                    entradas: if retomar { 0 } else { n },
                    grau,
                });
            }
            estilo::texto(
                rod.x,
                rod.y + 96.0 * f,
                if retomar {
                    "Your time keeps running while you are off the island."
                } else if pode {
                    "Each entry is worth 30 min. The free ones come back at 4am."
                } else {
                    "No entry left: the 3 free ones come back at 4am, and passes drop from bosses."
                },
                12,
                SUAVE,
            );
        }

        // ── O SALDO, logo acima do rodapé ──
        let resta = shared::magica::resta(e.fim_unix, agora_unix);
        let saldo_y = rod.y - 34.0 * f;
        estilo::texto(
            x,
            saldo_y,
            &format!(
                "Free today {}/{}  ·  Passes {}{}",
                e.gratis,
                shared::magica::GRATIS_POR_DIA,
                e.passes,
                if resta > 0 {
                    format!("  ·  {}:{:02} left", resta / 60, resta % 60)
                } else {
                    String::new()
                }
            ),
            15,
            if total > 0 { estilo::TEXTO } else { SUAVE },
        );

        // ── AS SETE ILHOTAS, em DUAS COLUNAS, no espaço que sobrou ──
        //
        // Duas colunas porque sete linhas empurravam o rodapé pra fora da
        // tela. O que se perde é uma linha por ilhota; o que se ganha é o
        // botão de entrar existir.
        let topo = p.y + 74.0 * f;
        let disponivel = (saldo_y - 22.0 * f - topo).max(0.0);
        let ilhotas = shared::magica::ilhotas();
        let linhas = ilhotas.len().div_ceil(2);
        let passo = (disponivel / linhas as f32).min(24.0 * f);
        let col = larg * 0.5;
        for (k, i) in ilhotas.iter().enumerate() {
            let aqui = e.dentro && Bonus::do_indice(e.bonus) == Some(i.bonus);
            let c = cor_do_bonus(i.bonus);
            let cx = x + (k % 2) as f32 * col;
            let cy = topo + (k / 2) as f32 * passo + 12.0 * f;
            let v = format!("×{:.1}", i.bonus.multiplicador());
            let tv = estilo::medir(&v, 13);
            estilo::texto_ajustado(
                i.bonus.nome(),
                cx + 2.0 * f,
                cy,
                col - tv - 24.0 * f,
                13,
                if aqui { VERDE } else { c },
            );
            estilo::texto(cx + col - tv - 10.0 * f, cy, &v, 13, c);
        }

        // FECHAR: no X, ou tocando FORA. Sem a segunda, uma janela que
        // estoure a tela prende o jogador — e foi o que o dono viu.
        if clicou && (fechar.contains(m) || !p.contains(m)) {
            self.aberto = false;
        }
        if let Some((t, quando)) = &self.aviso {
            if agora - quando < 5.0 {
                estilo::texto_centro(p.x + p.w * 0.5, p.y + p.h - 4.0 * f, t, 13, OURO);
            }
        }
        pedido
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn guia_abre_painel_dentro_sem_abrir_a_cada_estado() {
        let mut ui = MagicaUi::default();
        ui.recebe(
            AvisoMagica::Estado {
                grau_maximo: 2,
                grau_atual: 1,
                meu_nivel: 30,
                passes: 0,
                gratis: 0,
                fim_unix: 1600,
                dentro: true,
                bonus: 255,
            },
            0.0,
        );
        assert!(!ui.aberto());
        ui.recebe(AvisoMagica::AbrirPainel, 0.0);
        assert!(ui.aberto());
        assert_eq!(ui.grau, 1);
    }

    /// O painel abre quando o jogador PEDE e não quando ele está jogando.
    ///
    /// Dentro da ilha o servidor manda `Estado` a cada troca de ilhota. Se
    /// isso abrisse o painel, o jogador atravessaria uma ponte no meio de uma
    /// briga de PvP e levaria uma janela na cara — que é o melhor jeito de
    /// morrer sem entender por quê.
    /// Tocar "Magic Island" abre o painel TODA vez, não só na primeira.
    ///
    /// A versão anterior abria em `estado.is_none()`: da segunda vez em
    /// diante o jogador tocava no menu, o pedido saía, o servidor respondia e
    /// nada acontecia — e a missão que manda abrir o painel ficava
    /// impossível. O dono: "clico em ilha mágica mas não abre nada e me deixa
    /// travado na quest".
    /// Os botões da tarja cabem, não se encavalam e o dedo alcança.
    /// Os degraus não se sobrepõem e cabem num dedo.
    ///
    /// Este teste existe porque a primeira versão saiu SOBREPOSTA no
    /// emulador: eu pus `"{nome}\npoder {n}"` no rótulo, `ui::botao` desenha
    /// uma linha só, e os três textos invadiram uns aos outros. O dono: "a
    /// HUD da ilha mágica ficou muito ruim agora, está tudo sobreposto".
    #[test]
    fn os_degraus_nao_se_sobrepoem() {
        for (w, f) in [(460.0f32, 1.0f32), (360.0, 1.4), (300.0, 2.2), (620.0, 0.8)] {
            let rod = Rect::new(20.0, 400.0, w, 40.0 * f);
            let (linha, fila) = MagicaUi::faixa_dos_degraus(rod, f);
            let n = fila.len();
            assert_eq!(n, shared::magica::NIVEIS.len());
            for i in 0..n {
                assert!(
                    fila[i].x >= rod.x - 0.01 && fila[i].x + fila[i].w <= rod.x + rod.w + 0.01,
                    "{w}x{f}: degrau {i} vaza a faixa"
                );
                assert!(
                    crate::ui::area_de_toque(fila[i]).h >= 44.0,
                    "{w}x{f}: degrau {i} é menor que um dedo"
                );
                for j in i + 1..n {
                    assert!(
                        fila[i].x + fila[i].w <= fila[j].x + 0.01,
                        "{w}x{f}: degrau {i} encosta no {j}"
                    );
                }
            }
            // A linha de detalhe fica ABAIXO dos botões e ACIMA do rodapé —
            // era ali que o texto batia no "Grátis hoje".
            let fim = fila[0].y + fila[0].h;
            assert!(linha > fim, "{w}x{f}: o detalhe sobe em cima dos botões");
            assert!(linha < rod.y, "{w}x{f}: o detalhe desce em cima do rodapé");
        }
    }

    /// NADA ENCOSTA EM NADA na tarja.
    ///
    /// O teste anterior media só os dois botões ENTRE SI, e por isso passou
    /// com a tela errada: os botões eram postos dentro da tarja, em cima da
    /// barra de tempo (que ocupa a largura toda) e ao lado do multiplicador.
    /// O dono: "os botões de + e sair tão em cima da hud, sobrepostos".
    ///
    /// Medir par a par é o que pega isso — sobreposição é uma relação, e uma
    /// peça sozinha nunca a revela.
    #[test]
    fn as_pecas_da_tarja_nao_se_sobrepoem() {
        for f in [0.8f32, 1.0, 1.5, 2.2] {
            // A largura é a da coluna da esquerda (a do rastreador).
            let tarja = Rect::new(20.0, 300.0, 440.0 * f, 58.0 * f);
            let p = MagicaUi::pecas_da_tarja(tarja, f);
            let todas = p.todas();
            for (i, (na, a)) in todas.iter().enumerate() {
                assert!(
                    a.w > 0.0 && a.h > 0.0,
                    "f={f}: a peça {na} ficou sem tamanho"
                );
                assert!(
                    a.x >= tarja.x - 0.01
                        && a.y >= tarja.y - 0.01
                        && a.x + a.w <= tarja.x + tarja.w + 0.01
                        && a.y + a.h <= tarja.y + tarja.h + 0.01,
                    "f={f}: a peça {na} vaza a tarja"
                );
                for (nb, b) in todas.iter().skip(i + 1) {
                    let cruza =
                        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
                    assert!(!cruza, "f={f}: {na} e {nb} se sobrepõem");
                }
            }
            for (b, nome) in [(p.mais, "+"), (p.sair, "sair")] {
                assert!(
                    crate::ui::area_de_toque(b).h >= 44.0,
                    "f={f}: o {nome} é menor que um dedo"
                );
            }
        }
    }

    /// A TARJA SOME QUANDO SE SAI DA ILHA.
    ///
    /// Sair é um handoff pra zona de origem, e lá ninguém manda estado novo —
    /// o `dentro: true` que veio do processo da ilha fica na memória do
    /// cliente pra sempre. O dono: "a HUD da ilha mágica tá aparecendo até
    /// depois de sair dela, com countdown e tudo".
    ///
    /// O relógio CONTINUA correndo depois de sair (é regra da ilha, pra não
    /// dar pra sair no primeiro susto e voltar com o tempo intacto), então
    /// esperar o tempo acabar não resolveria: a tarja ficaria até uma hora e
    /// meia na tela de quem já está em outro mapa.
    #[test]
    fn a_tarja_some_quando_se_sai_da_ilha() {
        let agora = 1_700_000_000i64;
        let mut ui = MagicaUi::default();
        ui.recebe(
            AvisoMagica::Estado {
                grau_maximo: 1,
                grau_atual: 1,
                meu_nivel: 20,
                passes: 0,
                gratis: 0,
                fim_unix: agora + 600,
                dentro: true,
                bonus: 0,
            },
            0.0,
        );
        ui.atualiza_zona(Some(shared::magica::ZONA));
        assert!(ui.dentro(agora), "dentro da ilha a tarja tem que aparecer");

        // Saiu: mesma mensagem velha na memória, mas outra zona.
        ui.atualiza_zona(Some("ilha_inicial"));
        assert!(!ui.dentro(agora), "a tarja sobreviveu à saída da ilha");

        // E sem zona nenhuma (entre um handoff e outro) também não.
        ui.atualiza_zona(None);
        assert!(!ui.dentro(agora), "a tarja apareceu sem zona nenhuma");
    }

    #[test]
    fn o_painel_abre_toda_vez_que_e_pedido() {
        let estado = |dentro: bool| AvisoMagica::Estado {
            grau_maximo: 1,
            grau_atual: 0,
            meu_nivel: 20,
            passes: 0,
            gratis: 3,
            fim_unix: 0,
            dentro,
            bonus: 255,
        };
        let mut ui = MagicaUi::default();
        // Primeira vez: abre sozinho (é o caminho de quem chega pelo tutorial).
        ui.recebe(estado(false), 0.0);
        assert!(ui.aberto(), "a primeira notícia tem que abrir");
        ui.fechar();
        assert!(!ui.aberto());

        // SEGUNDA vez, sem pedir: não abre. Estado chega o tempo todo dentro
        // da ilha, e abrir sozinho seria pior que não abrir.
        ui.recebe(estado(false), 1.0);
        assert!(!ui.aberto(), "estado sem pedido não pode abrir sozinho");

        // Segunda vez, PEDINDO: abre.
        ui.pedir_abertura();
        ui.recebe(estado(false), 2.0);
        assert!(
            ui.aberto(),
            "o pedido tem que abrir, e era isto que faltava"
        );

        // E o pedido é de uso único: não fica valendo pro estado seguinte.
        ui.fechar();
        ui.recebe(estado(false), 3.0);
        assert!(!ui.aberto(), "o pedido não pode ficar armado");
    }

    /// Dentro da ilha, nem o pedido abre o painel por cima do jogo.
    #[test]
    fn dentro_da_ilha_o_painel_nao_pula_na_tela() {
        let mut ui = MagicaUi::default();
        ui.pedir_abertura();
        ui.recebe(
            AvisoMagica::Estado {
                grau_maximo: 1,
                grau_atual: 1,
                meu_nivel: 20,
                passes: 0,
                gratis: 0,
                fim_unix: 0,
                dentro: true,
                bonus: 0,
            },
            0.0,
        );
        assert!(!ui.aberto(), "dentro da ilha o estado chega a cada ilhota");
    }

    #[test]
    fn o_estado_de_dentro_nao_abre_o_painel() {
        let mut ui = MagicaUi::default();
        ui.recebe(
            AvisoMagica::Estado {
                grau_maximo: 1,
                grau_atual: 1,
                meu_nivel: 20,
                passes: 0,
                gratis: 0,
                fim_unix: 1_000,
                dentro: true,
                bonus: Bonus::Xp.indice(),
            },
            0.0,
        );
        assert!(!ui.aberto(), "estado de dentro abriu o painel");
        assert_eq!(ui.bonus(), Some(Bonus::Xp));
        // `dentro` exige a zona além do estado: ver
        // `a_tarja_some_quando_se_sai_da_ilha`.
        ui.atualiza_zona(Some(shared::magica::ZONA));
        assert!(ui.dentro(900), "com tempo sobrando ele está dentro");
        assert!(!ui.dentro(1_001), "tempo vencido não conta como dentro");
    }

    /// O BOTÃO DE ENTRAR FICA DENTRO DA JANELA. Em toda tela.
    ///
    /// Era exatamente isto que estava quebrado: a janela vinha de
    /// `ui::painel(480, 430)`, em pixels CRUS, e o conteúdo escalava por
    /// `fator_texto()`. No celular o conteúdo media ~443 px numa área útil de
    /// 346 e o botão caía fora — o dono: "a HUD de entrar na Ilha Mágica tá
    /// um lixo, nem consigo entrar".
    ///
    /// As medidas rodam em telas de verdade, da menor à maior, e nas duas
    /// escalas que `escala_do_painel` pode devolver.
    #[test]
    fn o_botao_de_entrar_nunca_sai_da_janela() {
        // (largura, altura) de tela segura: iPhone deitado, tablet, desktop.
        for (w, h) in [
            (734.0, 320.0),
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
                for dentro in [false, true] {
                    let rod = rodape_de(p, f, dentro);
                    assert!(
                        rod.y >= p.y && rod.y + rod.h <= p.y + p.h,
                        "{w}x{h} f={f} dentro={dentro}: o rodapé vai de {:.0} a {:.0}, \
                         e a janela de {:.0} a {:.0}",
                        rod.y,
                        rod.y + rod.h,
                        p.y,
                        p.y + p.h
                    );
                    assert!(
                        rod.x >= p.x && rod.x + rod.w <= p.x + p.w,
                        "{w}x{h} f={f}: o rodapé vaza de lado"
                    );
                    // E o rodapé tem que caber na TELA, não só na janela: uma
                    // janela que já passou da tela levaria o botão junto.
                    assert!(
                        rod.y + rod.h <= seguro.y + seguro.h,
                        "{w}x{h} f={f}: o botão de entrar cai fora da tela"
                    );
                }
            }
        }
    }

    /// FORA, o primeiro estado abre — é a resposta ao clique do menu.
    #[test]
    fn o_primeiro_estado_de_fora_abre_o_painel() {
        let mut ui = MagicaUi::default();
        ui.recebe(
            AvisoMagica::Estado {
                grau_maximo: 1,
                grau_atual: 0,
                meu_nivel: 20,
                passes: 2,
                gratis: shared::magica::GRATIS_POR_DIA,
                fim_unix: 0,
                dentro: false,
                bonus: 255,
            },
            0.0,
        );
        assert!(ui.aberto());
        assert_eq!(ui.bonus(), None, "255 é 'nenhuma ilhota'");
        assert_eq!(ui.resta(0), 0);
    }
}
