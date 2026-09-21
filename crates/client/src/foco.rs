//! O foco do tutorial: escurece a tela, deixa claro só onde se deve tocar, e
//! **o toque só passa ali**.
//!
//! Sem isso o passo virava um texto no rastreador que o jogador lia e não
//! achava: "toque em COMBATE" não diz onde COMBATE fica. Com o resto da tela
//! apagado e travado, não há como errar nem como se perder no meio.
//!
//! Quem desenha um botão que pode ser alvo de tutorial **marca** o retângulo
//! dele (`marca`); o passo ativo **pede** uma chave (`pede`). O botão marcado
//! neste quadro vira o buraco aceso no quadro SEGUINTE — assim nenhum painel
//! precisa saber em que ordem o outro desenha, e o `main` não precisa saber
//! onde ficam os botões de dentro dos painéis.
//!
//! O pedido é uma LISTA em ordem de profundidade: "o + da Ficha, senão a linha
//! Ficha do Menu, senão o botão MENU". O primeiro que existir na tela ganha,
//! e é assim que o foco anda sozinho de tela em tela até onde a ação acontece.
//!
//! A trava do toque é num lugar só: `clique()` e `segurando()` no lugar de
//! `is_mouse_button_pressed/down`. Travar pelo PONTO do dedo, e não pelo
//! retângulo de cada widget, é o que faz "o resto da tela não responde" valer
//! também pro mundo, pro joystick e pra quem eu esquecesse de converter.

use macroquad::prelude::*;
use std::cell::{Cell, RefCell};

/// As chaves dos passos do MEIO — os do fim usam a própria ação do tutorial
/// (`shared::quests::tutorial`), que vai de 1 a 8. Começam bem acima disso
/// para nunca colidirem com uma ação nova.
pub mod chave {
    /// Os alvos que SÃO a ação do tutorial: a chave é a própria ação, pra não
    /// existir uma segunda numeração pra manter em dia.
    pub use shared::quests::tutorial::{
        AUTO_COLETA, AUTO_COMBATE, MAPA_IR, POCAO_LIMIAR, SKILL_AUTO,
    };

    /// O botão MENU do HUD: o começo de quase todo caminho.
    pub const MENU: u16 = 100;
    /// A linha "Ficha" dentro do Menu.
    pub const MENU_FICHA: u16 = 101;
    /// A linha "Habilidades" dentro do Menu.
    pub const MENU_SKILLS: u16 = 102;
    /// O "+" de um atributo, na Ficha.
    pub const FICHA_MAIS: u16 = 103;
    /// O botão de evoluir, na tela de Habilidades.
    pub const SKILL_EVOLUIR: u16 = 104;
    /// O minimapa do HUD, quando o Mapa grande ainda está fechado.
    pub const MINIMAPA: u16 = 105;
}

/// Quantas chaves cabem num pedido. Três dá pro caminho mais fundo que existe
/// (botão do painel → linha do menu → MENU) com folga.
const FUNDO: usize = 4;

thread_local! {
    /// O buraco aceso deste quadro, se houver.
    static ALVO: Cell<Option<Rect>> = const { Cell::new(None) };
    /// O que foi apontado direto neste quadro — vira `ALVO` no quadro seguinte.
    static PENDENTE: Cell<Option<Rect>> = const { Cell::new(None) };
    /// As chaves pedidas neste quadro, da mais funda pra mais rasa.
    static PEDIDO: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    /// Os retângulos marcados neste quadro.
    static MARCAS: RefCell<Vec<(u16, Rect)>> = const { RefCell::new(Vec::new()) };
}

/// Um pouco de folga em volta do alvo: o dedo não acerta o pixel exato, e o
/// anel aceso fica feio colado na borda do botão.
const FOLGA: f32 = 10.0;

/// Aponta o foco direto pra `r` neste quadro. Chamar todo quadro enquanto o
/// passo estiver valendo — parar de chamar apaga o foco.
pub fn aponta(r: Rect) {
    PENDENTE.with(|p| p.set(Some(com_folga(r))));
}

/// "Este botão é a chave `k`". Quem desenha chama sempre, tenha tutorial ou
/// não — só guarda quem foi PEDIDO neste quadro.
///
/// Guardar só o pedido não é otimização: `pede` vem antes de todo `marca` no
/// quadro (é a primeira linha do desenho), e quem não filtrasse encheria a
/// lista para sempre nas prévias, que têm laço próprio e nunca chamam
/// `novo_quadro`.
pub fn marca(k: u16, r: Rect) {
    if PEDIDO.with(|p| p.borrow().contains(&k)) {
        MARCAS.with(|m| m.borrow_mut().push((k, r)));
    }
}

/// O passo ativo quer a primeira destas chaves que estiver na tela.
pub fn pede(chaves: &[u16]) {
    PEDIDO.with(|p| {
        let mut p = p.borrow_mut();
        p.clear();
        p.extend(chaves.iter().take(FUNDO).copied());
    });
}

/// Fecha o quadro: o que foi apontado (ou pedido e marcado) passa a valer, e o
/// que não foi some.
pub fn novo_quadro() {
    let direto = PENDENTE.with(|p| p.take());
    let pedido = PEDIDO.with(|p| std::mem::take(&mut *p.borrow_mut()));
    let marcas = MARCAS.with(|m| std::mem::take(&mut *m.borrow_mut()));
    let por_chave = pedido.iter().find_map(|k| {
        marcas
            .iter()
            .find(|(mk, _)| mk == k)
            .map(|(_, r)| com_folga(*r))
    });
    ALVO.with(|a| a.set(direto.or(por_chave)));
}

/// Há foco agora?
pub fn ativo() -> bool {
    ALVO.with(|a| a.get()).is_some()
}

/// O widget em `r` pode receber toque? Sem foco, tudo pode. Com foco, só
/// quem encosta no buraco.
pub fn passa(r: Rect) -> bool {
    match ALVO.with(|a| a.get()) {
        None => true,
        Some(alvo) => sobrepoe(alvo, r),
    }
}

/// O mesmo, para um ponto (toque no mundo, joystick).
pub fn passa_ponto(p: Vec2) -> bool {
    match ALVO.with(|a| a.get()) {
        None => true,
        Some(alvo) => alvo.contains(p),
    }
}

/// Houve clique aproveitável neste quadro? É o que a UI inteira usa no lugar
/// de `is_mouse_button_pressed(Left)`.
pub fn clique() -> bool {
    is_mouse_button_pressed(MouseButton::Left) && passa_ponto(mouse_position().into())
}

/// O dedo está segurando, e num lugar que o foco permite? No lugar de
/// `is_mouse_button_down(Left)`: sem isso dava pra arrastar o joystick e
/// andar com a tela travada.
pub fn segurando() -> bool {
    is_mouse_button_down(MouseButton::Left) && passa_ponto(mouse_position().into())
}

/// Escurece a tela inteira menos o buraco, e pisca um anel em volta dele.
/// Desenhado por cima de tudo, no fim do quadro.
pub fn desenha(agora: f64) {
    let Some(alvo) = ALVO.with(|a| a.get()) else {
        return;
    };
    let (w, h) = (screen_width(), screen_height());
    let escuro = Color::new(0.0, 0.0, 0.0, 0.72);
    // Quatro retângulos em volta do buraco: sem stencil e sem shader, o que
    // o iPhone aceita sem discussão.
    draw_rectangle(0.0, 0.0, w, alvo.y.max(0.0), escuro);
    let abaixo = alvo.y + alvo.h;
    draw_rectangle(0.0, abaixo, w, (h - abaixo).max(0.0), escuro);
    draw_rectangle(0.0, alvo.y, alvo.x.max(0.0), alvo.h, escuro);
    let direita = alvo.x + alvo.w;
    draw_rectangle(direita, alvo.y, (w - direita).max(0.0), alvo.h, escuro);

    // O anel pulsa devagar: chama o olho sem virar pisca-pisca.
    let pulso = 0.5 + 0.5 * ((agora * 2.2) as f32).sin();
    crate::hud_estilo::borda_arredondada(
        alvo,
        10.0,
        2.0 + pulso * 1.5,
        crate::hud_estilo::alfa(crate::hud_estilo::OURO, 0.55 + 0.45 * pulso),
    );
    // Uma mãozinha apontando logo abaixo, pra quem não achou o anel.
    let dedo = vec2(alvo.x + alvo.w * 0.5, alvo.y + alvo.h + 18.0 + pulso * 6.0);
    if dedo.y < h - 10.0 {
        crate::hud_estilo::texto_centro_forte(
            dedo.x,
            dedo.y,
            "▲",
            18,
            crate::hud_estilo::alfa(crate::hud_estilo::OURO, 0.6 + 0.4 * pulso),
        );
    }
}

fn com_folga(r: Rect) -> Rect {
    Rect::new(r.x - FOLGA, r.y - FOLGA, r.w + FOLGA * 2.0, r.h + FOLGA * 2.0)
}

fn sobrepoe(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

#[cfg(test)]
mod testes {
    use super::*;

    fn limpa() {
        PENDENTE.with(|p| p.set(None));
        ALVO.with(|a| a.set(None));
        PEDIDO.with(|p| p.borrow_mut().clear());
        MARCAS.with(|m| m.borrow_mut().clear());
    }

    /// Sem foco nada e' bloqueado: o jogo normal nao muda.
    #[test]
    fn sem_foco_tudo_passa() {
        limpa();
        assert!(!ativo());
        assert!(passa(Rect::new(0.0, 0.0, 10.0, 10.0)));
        assert!(passa_ponto(vec2(500.0, 500.0)));
    }

    /// Com foco, so' o buraco recebe toque — e o alvo so' vale no quadro
    /// SEGUINTE, pra quem aponta nao depender da ordem de desenho.
    #[test]
    fn com_foco_so_o_buraco_passa() {
        limpa();
        let botao = Rect::new(100.0, 100.0, 60.0, 40.0);
        aponta(botao);
        assert!(!ativo(), "so' vale depois de fechar o quadro");
        novo_quadro();
        assert!(ativo());

        assert!(passa(botao), "o proprio alvo passa");
        assert!(passa_ponto(vec2(120.0, 120.0)), "o dedo no alvo passa");
        assert!(
            !passa(Rect::new(400.0, 400.0, 50.0, 50.0)),
            "botao longe fica travado"
        );
        assert!(!passa_ponto(vec2(10.0, 10.0)), "toque no mundo fica travado");

        // A folga deixa encostar um pouco fora do pixel exato.
        assert!(passa_ponto(vec2(botao.x - FOLGA * 0.5, botao.y)));
        assert!(!passa_ponto(vec2(botao.x - FOLGA * 3.0, botao.y)));

        // Parar de apontar apaga o foco no quadro seguinte.
        novo_quadro();
        assert!(!ativo());
        assert!(passa(Rect::new(400.0, 400.0, 50.0, 50.0)));
    }

    /// Um botao que ENCOSTA no alvo tambem passa: o buraco tem folga, e
    /// recusar quem toca a borda deixaria o alvo sem clique por um pixel.
    #[test]
    fn quem_encosta_no_buraco_passa() {
        limpa();
        aponta(Rect::new(100.0, 100.0, 40.0, 40.0));
        novo_quadro();
        assert!(passa(Rect::new(135.0, 100.0, 20.0, 20.0)), "encostado");
        assert!(!passa(Rect::new(200.0, 100.0, 20.0, 20.0)), "separado");
        limpa();
    }

    /// O pedido e' uma LISTA, e ganha o primeiro que estiver na tela: com a
    /// Ficha aberta o foco e' o "+", com ela fechada e' o MENU.
    #[test]
    fn o_mais_fundo_que_esta_na_tela_ganha() {
        limpa();
        let menu = Rect::new(900.0, 20.0, 50.0, 50.0);
        let mais = Rect::new(300.0, 400.0, 30.0, 30.0);

        // Ficha fechada: so' o MENU esta' marcado.
        pede(&[chave::FICHA_MAIS, chave::MENU]);
        marca(chave::MENU, menu);
        novo_quadro();
        assert!(passa_ponto(menu.center()), "o foco caiu no MENU");
        assert!(!passa_ponto(mais.center()));

        // Ficha aberta: os dois marcados, ganha o "+".
        pede(&[chave::FICHA_MAIS, chave::MENU]);
        marca(chave::MENU, menu);
        marca(chave::FICHA_MAIS, mais);
        novo_quadro();
        assert!(passa_ponto(mais.center()), "o foco andou pro +");
        assert!(!passa_ponto(menu.center()), "o MENU ficou travado");

        // Pedir chave que ninguem marcou nao acende nada: melhor sem foco do
        // que a tela inteira preta sem buraco.
        pede(&[chave::SKILL_EVOLUIR]);
        marca(chave::MENU, menu);
        novo_quadro();
        assert!(!ativo());
        limpa();
    }

    /// Sem pedido ninguem guarda nada. As previas tem laco proprio e nunca
    /// chamam `novo_quadro`: se `marca` guardasse a esmo, a lista crescia
    /// para sempre enquanto a janela estivesse aberta.
    #[test]
    fn sem_pedido_a_marca_nao_guarda() {
        limpa();
        for _ in 0..1000 {
            marca(chave::FICHA_MAIS, Rect::new(0.0, 0.0, 10.0, 10.0));
        }
        assert_eq!(MARCAS.with(|m| m.borrow().len()), 0);
        novo_quadro();
        assert!(!ativo());
        limpa();
    }

    /// O buraco cobre TODA escolha que o passo aceita.
    ///
    /// Dois passos ja' caíram nisto: "gaste um ponto" apontava so' o primeiro
    /// "+" e o jogador so' conseguia subir FOR; "evolua uma skill" apontava
    /// so' o botao da skill selecionada e so' dava pra subir a primeira da
    /// lista. Em ambos o foco nao estava errado — ele estava certo demais,
    /// apontando UMA das respostas certas e trancando as outras.
    #[test]
    fn o_buraco_cobre_a_escolha_inteira() {
        limpa();
        // Uma coluna de botoes: apontar so' o primeiro tranca o resto.
        let botoes: Vec<Rect> = (0..6)
            .map(|i| Rect::new(300.0, 45.0 + i as f32 * 48.0, 36.0, 36.0))
            .collect();
        pede(&[chave::FICHA_MAIS]);
        marca(chave::FICHA_MAIS, botoes[0]);
        novo_quadro();
        assert!(passa(botoes[0]));
        assert!(
            !passa(*botoes.last().unwrap()),
            "so' o primeiro marcado: o ultimo fica travado — era o defeito"
        );

        // A UNIAO deixa todos passarem.
        let uniao = botoes.iter().fold(botoes[0], |a, b| {
            let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
            Rect::new(
                x0,
                y0,
                (a.x + a.w).max(b.x + b.w) - x0,
                (a.y + a.h).max(b.y + b.h) - y0,
            )
        });
        pede(&[chave::FICHA_MAIS]);
        marca(chave::FICHA_MAIS, uniao);
        novo_quadro();
        for (i, b) in botoes.iter().enumerate() {
            assert!(passa(*b), "o botao {i} ficou de fora do buraco");
        }
        // E o que esta' FORA continua travado: o foco nao virou "tudo passa".
        assert!(!passa(Rect::new(900.0, 600.0, 50.0, 50.0)));
        limpa();
    }

    /// Marca de um quadro nao vaza pro seguinte: painel fechado apaga o foco.
    #[test]
    fn marca_nao_vaza_de_um_quadro_pro_outro() {
        limpa();
        let mais = Rect::new(300.0, 400.0, 30.0, 30.0);
        pede(&[chave::FICHA_MAIS]);
        marca(chave::FICHA_MAIS, mais);
        novo_quadro();
        assert!(ativo());
        // Painel fechou: ninguem marca, ninguem pede.
        novo_quadro();
        assert!(!ativo());
        limpa();
    }
}
