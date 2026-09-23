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

/// O RODAPÉ — onde mora o botão de entrar —, ancorado no fundo da janela.
///
/// Fora do desenho porque é a medida que quebrou: com a janela em pixels
/// crus e o texto em `f`, o conteúdo empurrava o botão pra fora e não havia
/// como entrar na ilha. Ancorado ao fundo e testado, ele não tem como sair.
fn rodape_de(p: Rect, f: f32, dentro: bool) -> Rect {
    let alt = if dentro { 64.0 * f } else { 104.0 * f };
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
fn cor_do_bonus(b: Bonus) -> Color {
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
    pub passes: u32,
    /// Entradas de graça que ainda há hoje (`magica::GRATIS_POR_DIA`).
    pub gratis: u8,
    pub fim_unix: i64,
    pub dentro: bool,
    pub bonus: u8,
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
    /// "Ilha Mágica" no menu, o pedido saía, o servidor respondia, e nada
    /// acontecia na tela. O dono: "clico em ilha mágica mas não abre nada e
    /// me deixa travado na quest".
    pedida: bool,
    /// Quantas entradas o jogador escolheu gastar (1..3).
    entradas: u8,
    /// O degrau escolhido (`NivelMagico::grau`). 0 = ainda não escolheu, e aí
    /// vale o maior liberado — que é o que quase todo mundo quer.
    grau: u8,
    aviso: Option<(String, f64)>,
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
            AvisoMagica::Estado {
                grau_maximo,
                grau_atual,
                passes,
                gratis,
                fim_unix,
                dentro,
                bonus,
            } => {
                let e = Estado {
                    grau_maximo,
                    grau_atual,
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

    /// O jogador tocou "Ilha Mágica": a próxima notícia de estado ABRE.
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

    /// Está dentro da ilha com tempo valendo?
    pub fn dentro(&self, agora_unix: i64) -> bool {
        self.estado.as_ref().is_some_and(|e| e.dentro) && self.resta(agora_unix) > 0
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
    /// e a ilhota, e medida escrita à mão nesta tela já pôs o "Entrar" fora
    /// da janela uma vez.
    pub(crate) fn botoes_da_tarja(tarja: Rect, f: f32) -> (Rect, Rect) {
        // 32 no MÍNIMO, em pixels crus: `area_de_toque` cresce o alvo até o
        // dedo, mas com teto (+14). Num `f` pequeno, 30 × 0,8 = 24 e nem com
        // o crescimento chega aos 44 pt da Apple — foi o que o teste pegou.
        let lado = (30.0 * f).max(32.0);
        let y = tarja.y + tarja.h - lado - 4.0 * f;
        let meio = tarja.x + tarja.w * 0.5;
        // O "+" à esquerda do meio e o sair à direita: os dois moram na faixa
        // livre entre o relógio e a ilhota.
        (
            Rect::new(meio - lado - 3.0 * f, y, lado, lado),
            Rect::new(meio + 3.0 * f, y, lado, lado),
        )
    }

    pub fn desenha_hud(&mut self, agora_unix: i64) -> Option<PedidoMagica> {
        if !self.dentro(agora_unix) {
            return None;
        }
        let resta = self.resta(agora_unix);
        let s = crate::hud_layout::tela_segura();
        let f = estilo::fator_texto();
        let urgente = resta <= 60;

        // Largura FIXA. Antes ela saía do texto, então a tarja mudava de
        // tamanho a cada travessia de ponte — o olho via a coisa pular de
        // lugar e o relógio nunca ficava onde se aprendeu a procurar.
        let w = 268.0 * f;
        let h = 62.0 * f;
        let r = Rect::new(s.x + (s.w - w) * 0.5, s.y + 6.0 * f, w, h);
        estilo::painel(r);

        // O RELÓGIO, grande, à esquerda. É o número que decide se vale
        // atravessar ou não.
        let cor = if urgente { VERMELHO } else { OURO };
        let relogio = format!("{}:{:02}", resta / 60, resta % 60);
        estilo::texto_forte(r.x + 14.0 * f, r.y + 30.0 * f, &relogio, 26, cor);

        // A BARRA, por baixo: o relógio em número diz quanto falta, a barra
        // diz quanto falta COMPARADO ao que cabe (1h30). Um vê-se lendo, a
        // outra vê-se de canto de olho no meio de uma briga.
        let bx = r.x + 14.0 * f;
        let bw = w - 28.0 * f;
        let by = r.y + h - 16.0 * f;
        let frac = (resta as f32 / shared::magica::TETO_S as f32).clamp(0.0, 1.0);
        draw_rectangle(bx, by, bw, 5.0 * f, Color::new(1.0, 1.0, 1.0, 0.13));
        draw_rectangle(bx, by, bw * frac, 5.0 * f, cor);

        // A ILHOTA, à direita, NA COR DELA. São sete bônus; cor é o que o
        // olho separa sem ler.
        let (nome, mult, c) = match self.bonus() {
            Some(b) => (b.nome(), format!("×{:.2}", b.multiplicador()), cor_do_bonus(b)),
            None => ("Ponte", "sem bônus".to_string(), SUAVE),
        };
        let x = r.x + w - 14.0 * f;
        let tn = estilo::medir(nome, 15);
        estilo::texto(x - tn, r.y + 24.0 * f, nome, 15, c);
        let tm = estilo::medir(&mult, 17);
        estilo::texto_forte(x - tm, r.y + 44.0 * f, &mult, 17, c);
        // O ponto da cor, colado no nome: um rótulo colorido some no fundo
        // escuro; um disco cheio não.
        draw_circle(x - tn - 9.0 * f, r.y + 19.0 * f, 4.0 * f, c);

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
        let (mais, sair) = Self::botoes_da_tarja(r, f);
        let mut pedido = None;
        if shared::magica::pode_entrar(e.fim_unix, agora_unix, total, 1).is_ok()
            && crate::ui::botao(mais, "+", true)
        {
            pedido = Some(PedidoMagica::Entrar { entradas: 1, grau: e.grau_atual });
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
        if crate::ui::botao(sair, "Sair", true) {
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
        // "Entrar" caía FORA da janela. O dono: "a HUD de entrar na Ilha
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

        estilo::texto_forte(x, p.y + 32.0 * f, "Ilha Mágica", 20, OURO);
        let fechar = Rect::new(p.x + p.w - 44.0 * f, p.y + 8.0 * f, 36.0 * f, 34.0 * f);
        estilo::texto_centro(fechar.center().x, fechar.center().y + 6.0 * f, "X", 18, estilo::TEXTO);

        estilo::texto(
            x,
            p.y + 54.0 * f,
            "Sete ilhotas por pontes · PvP aberto · morrer volta à chegada",
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
            if ui::botao(Rect::new(rod.x, rod.y + 6.0 * f, rod.w, 38.0 * f), "Sair da ilha", true) {
                pedido = Some(PedidoMagica::Sair);
                self.aberto = false;
            }
            estilo::texto(
                rod.x,
                rod.y + 60.0 * f,
                "Sair não para o relógio: o tempo continua correndo.",
                12,
                SUAVE,
            );
        } else {
            // Escolher 1, 2 ou 3 antes de ir: acumular é decisão do jogador,
            // e gastar três de uma vez sem ter pedido seria roubo.
            let bw = (rod.w - 16.0 * f) / 3.0;
            for n in 1u8..=3 {
                let caixa = Rect::new(rod.x + (n - 1) as f32 * (bw + 8.0 * f), rod.y, bw, 32.0 * f);
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
            // O dono: "a ilha mágica precisa ter níveis; você é nv 20, aí pode
            // entrar na ilha desbloqueada no nv 20 com poder recomendado
            // 1500; a próxima nv 30 etc — dessa forma todos os mobs ficam
            // padronizados".
            //
            // Cada degrau é uma ZONA com faixa de mob estreita. O que está
            // TRAVADO aparece assim mesmo, com o nível que falta: saber que
            // existe um degrau adiante é metade do motivo de subir de nível.
            let grau = if self.grau == 0 { e.grau_maximo } else { self.grau };
            let linha = rod.y - 96.0 * f;
            let larg = (rod.w - 8.0 * f * 2.0) / 3.0;
            for (k, nv) in shared::magica::NIVEIS.iter().enumerate() {
                let r = Rect::new(rod.x + k as f32 * (larg + 8.0 * f), linha, larg, 46.0 * f);
                let liberado = e.grau_maximo >= nv.grau;
                if nv.grau == grau {
                    estilo::ret_arredondado(r, 6.0, estilo::alfa(estilo::OURO, 0.22));
                }
                let rot = if liberado {
                    format!("{}\npoder {}", nv.nome, crate::bolsa::milhar(nv.poder as u64))
                } else {
                    format!("{}\nnível {}", nv.nome, nv.exige_nivel)
                };
                if ui::botao(r, &rot, liberado) && liberado {
                    self.grau = nv.grau;
                }
            }
            let n = self.entradas.max(1);
            let de_graca = (e.gratis as u32).min(n as u32);
            let rot = if de_graca == n as u32 {
                format!("Entrar — {n} grátis")
            } else if de_graca > 0 {
                format!("Entrar — {de_graca} grátis + {} passe", n as u32 - de_graca)
            } else {
                format!("Entrar — {n} passe(s)")
            };
            // Sem degrau liberado não há entrada: o portão é o nível.
            let pode = total >= n as u32 && grau > 0;
            let b = Rect::new(rod.x, rod.y + 40.0 * f, rod.w, 40.0 * f);
            // O destaque da trava de nível mira AQUI depois que o painel abre.
            crate::foco::marca(crate::foco::chave::MAGICA_ENTRAR, b);
            if ui::botao(b, &rot, pode) && pode {
                pedido = Some(PedidoMagica::Entrar { entradas: n, grau });
            }
            estilo::texto(
                rod.x,
                rod.y + 96.0 * f,
                if pode {
                    "Cada entrada vale 30 min. As grátis voltam às 4h."
                } else {
                    "Sem entrada: as 3 grátis voltam às 4h, e o passe cai de chefes."
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
                "Grátis hoje {}/{}  ·  Passes {}{}",
                e.gratis,
                shared::magica::GRATIS_POR_DIA,
                e.passes,
                if resta > 0 {
                    format!("  ·  resta {}:{:02}", resta / 60, resta % 60)
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
            draw_circle(cx + 4.0 * f, cy - 4.0 * f, 3.5 * f, c);
            let v = format!("×{:.1}", i.bonus.multiplicador());
            let tv = estilo::medir(&v, 13);
            estilo::texto_ajustado(
                i.bonus.nome(),
                cx + 14.0 * f,
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

    /// O painel abre quando o jogador PEDE e não quando ele está jogando.
    ///
    /// Dentro da ilha o servidor manda `Estado` a cada troca de ilhota. Se
    /// isso abrisse o painel, o jogador atravessaria uma ponte no meio de uma
    /// briga de PvP e levaria uma janela na cara — que é o melhor jeito de
    /// morrer sem entender por quê.
    /// Tocar "Ilha Mágica" abre o painel TODA vez, não só na primeira.
    ///
    /// A versão anterior abria em `estado.is_none()`: da segunda vez em
    /// diante o jogador tocava no menu, o pedido saía, o servidor respondia e
    /// nada acontecia — e a missão que manda abrir o painel ficava
    /// impossível. O dono: "clico em ilha mágica mas não abre nada e me deixa
    /// travado na quest".
    /// Os botões da tarja cabem, não se encavalam e o dedo alcança.
    #[test]
    fn os_botoes_da_tarja_cabem_e_nao_se_encavalam() {
        for f in [0.8f32, 1.0, 1.5, 2.2] {
            let tarja = Rect::new(100.0, 10.0, 268.0 * f, 62.0 * f);
            let (mais, sair) = MagicaUi::botoes_da_tarja(tarja, f);
            assert!(mais.x + mais.w <= sair.x, "f={f}: o + e o sair se encavalam");
            for (b, nome) in [(mais, "+"), (sair, "sair")] {
            assert!(
                b.x >= tarja.x && b.x + b.w <= tarja.x + tarja.w,
                "f={f}: o {nome} vaza a tarja na horizontal"
            );
            assert!(
                b.y >= tarja.y && b.y + b.h <= tarja.y + tarja.h,
                "f={f}: o {nome} vaza a tarja na vertical"
            );
            assert!(
                crate::ui::area_de_toque(b).h >= 44.0,
                "f={f}: o {nome} é menor que um dedo"
            );
            }
        }
    }

    #[test]
    fn o_painel_abre_toda_vez_que_e_pedido() {
        let estado = |dentro: bool| AvisoMagica::Estado {
            grau_maximo: 1,
            grau_atual: 0,
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
        assert!(ui.aberto(), "o pedido tem que abrir, e era isto que faltava");

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
