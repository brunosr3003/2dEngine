//! O pouco que depende da plataforma: abrir URL no navegador e o teclado
//! virtual do iPhone.
//!
//! Teclado: a miniquad 0.4.11 cria um `UITextField` escondido e le' dele os
//! caracteres (`char_event`, Enter e Backspace ja' chegam pelo caminho normal
//! do `entrada::Teclado`), mas o pedido `show_keyboard` cai num `_ => {}` no
//! iOS e nunca chama `becomeFirstResponder`. Aqui o campo e' achado na view do
//! app e o pedido e' feito direto, por FFI Objective-C, sem dependencia nova.

/// Esta plataforma tem teclado na tela (e o painel precisa subir)?
pub const TECLADO_NA_TELA: bool = cfg!(any(target_os = "ios", target_os = "android"));

/// Abre `url` no navegador do sistema. `false` se nao conseguiu.
pub fn abrir_url(url: &str) -> bool {
    #[cfg(target_os = "ios")]
    {
        ios::abrir_url(url)
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn().is_ok()
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .is_ok()
    }
    #[cfg(all(
        unix,
        not(any(target_os = "ios", target_os = "macos", target_os = "android"))
    ))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .is_ok()
    }
    // Android e web: ainda nao.
    #[cfg(not(any(unix, windows)))]
    {
        let _ = url;
        false
    }
    #[cfg(target_os = "android")]
    {
        let _ = url;
        false
    }
}

/// Mostra ou esconde o teclado na tela. No desktop nao faz nada.
pub fn teclado_virtual(mostrar: bool) {
    #[cfg(target_os = "ios")]
    ios::teclado(mostrar);
    #[cfg(target_os = "android")]
    macroquad::miniquad::window::show_keyboard(mostrar);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = mostrar;
}

/// Tela sempre acesa enquanto joga: no automatico ninguem toca e o iPhone
/// apagaria a tela (e pausaria o app). No desktop nao faz nada.
pub fn manter_tela_acesa(sim: bool) {
    #[cfg(target_os = "ios")]
    ios::manter_tela_acesa(sim);
    #[cfg(not(target_os = "ios"))]
    let _ = sim;
}

/// Area segura da tela, em px da macroquad: (topo, esquerda, baixo, direita).
/// No iPhone em paisagem o notch/Dynamic Island come um lado, os cantos sao
/// arredondados e a barra do home fica embaixo. Fora do iOS, zero.
pub fn area_segura() -> [f32; 4] {
    #[cfg(target_os = "ios")]
    {
        ios::area_segura()
    }
    #[cfg(not(target_os = "ios"))]
    {
        [0.0; 4]
    }
}

#[cfg(target_os = "ios")]
mod ios {
    use std::ffi::{c_char, c_void, CString};

    type Id = *mut c_void;
    type Sel = *mut c_void;

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(nome: *const c_char) -> Id;
        fn sel_registerName(nome: *const c_char) -> Sel;
        fn objc_msgSend();
    }
    #[link(name = "UIKit", kind = "framework")]
    extern "C" {}
    #[link(name = "Foundation", kind = "framework")]
    extern "C" {}

    unsafe fn classe(nome: &str) -> Id {
        let c = CString::new(nome).unwrap();
        objc_getClass(c.as_ptr())
    }

    unsafe fn sel(nome: &str) -> Sel {
        let c = CString::new(nome).unwrap();
        sel_registerName(c.as_ptr())
    }

    // arm64: objc_msgSend chamado com a assinatura exata de cada uso.
    unsafe fn msg(obj: Id, s: &str) -> Id {
        let f: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(obj, sel(s))
    }

    unsafe fn msg_id(obj: Id, s: &str, a: Id) -> Id {
        let f: unsafe extern "C" fn(Id, Sel, Id) -> Id =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(obj, sel(s), a)
    }

    unsafe fn msg_bool(obj: Id, s: &str, a: Id) -> bool {
        let f: unsafe extern "C" fn(Id, Sel, Id) -> bool =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(obj, sel(s), a)
    }

    unsafe fn msg_n(obj: Id, s: &str) -> u64 {
        let f: unsafe extern "C" fn(Id, Sel) -> u64 =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(obj, sel(s))
    }

    unsafe fn msg_i(obj: Id, s: &str, i: u64) -> Id {
        let f: unsafe extern "C" fn(Id, Sel, u64) -> Id =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(obj, sel(s), i)
    }

    unsafe fn nsstring(s: &str) -> Id {
        let c = CString::new(s).unwrap_or_default();
        let f: unsafe extern "C" fn(Id, Sel, *const c_char) -> Id =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(classe("NSString"), sel("stringWithUTF8String:"), c.as_ptr())
    }

    pub fn abrir_url(url: &str) -> bool {
        unsafe {
            let nsurl = msg_id(classe("NSURL"), "URLWithString:", nsstring(url));
            if nsurl.is_null() {
                return false;
            }
            let app = msg(classe("UIApplication"), "sharedApplication");
            let opcoes = msg(classe("NSDictionary"), "dictionary");
            let f: unsafe extern "C" fn(Id, Sel, Id, Id, *mut c_void) =
                std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            f(
                app,
                sel("openURL:options:completionHandler:"),
                nsurl,
                opcoes,
                std::ptr::null_mut(),
            );
            true
        }
    }

    /// O `UITextField` que a miniquad pendurou na view do view controller.
    unsafe fn campo_escondido() -> Option<Id> {
        let tipo = classe("UITextField");
        let app = msg(classe("UIApplication"), "sharedApplication");
        let janelas = msg(app, "windows");
        for j in 0..msg_n(janelas, "count") {
            let janela = msg_i(janelas, "objectAtIndex:", j);
            let raiz = msg(janela, "rootViewController");
            if raiz.is_null() {
                continue;
            }
            let view = msg(raiz, "view");
            let filhos = msg(view, "subviews");
            for i in 0..msg_n(filhos, "count") {
                let v = msg_i(filhos, "objectAtIndex:", i);
                if msg_bool(v, "isKindOfClass:", tipo) {
                    return Some(v);
                }
            }
        }
        None
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Quatro(f64, f64, f64, f64);

    // UIEdgeInsets e CGRect: quatro f64, volta em registrador no arm64 (HFA),
    // entao o objc_msgSend comum serve.
    unsafe fn msg_quatro(obj: Id, s: &str) -> Quatro {
        let f: unsafe extern "C" fn(Id, Sel) -> Quatro =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(obj, sel(s))
    }

    /// `safeAreaInsets` da janela (pontos) convertido pra px da macroquad pela
    /// razao largura-da-tela / largura-da-janela.
    pub fn area_segura() -> [f32; 4] {
        unsafe {
            let app = msg(classe("UIApplication"), "sharedApplication");
            let janelas = msg(app, "windows");
            if janelas.is_null() || msg_n(janelas, "count") == 0 {
                return [0.0; 4];
            }
            let janela = msg_i(janelas, "objectAtIndex:", 0);
            let b = msg_quatro(janela, "bounds");
            if b.2 <= 1.0 {
                return [0.0; 4];
            }
            let i = msg_quatro(janela, "safeAreaInsets");
            let k = macroquad::window::screen_width() as f64 / b.2;
            // Respiro: 4 pt alem do inset e nunca menos de 16 pt — em paisagem
            // o topo tem inset zero mas o canto arredondado corta.
            let px = |v: f64| ((v.max(0.0) + 4.0).max(16.0) * k) as f32;
            [px(i.0), px(i.1), px(i.2), px(i.3)]
        }
    }

    /// `UIApplication.sharedApplication.idleTimerDisabled = sim`.
    pub fn manter_tela_acesa(sim: bool) {
        unsafe {
            let app = msg(classe("UIApplication"), "sharedApplication");
            let f: unsafe extern "C" fn(Id, Sel, bool) =
                std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            f(app, sel("setIdleTimerDisabled:"), sim);
        }
    }

    pub fn teclado(mostrar: bool) {
        unsafe {
            if let Some(campo) = campo_escondido() {
                msg(
                    campo,
                    if mostrar {
                        "becomeFirstResponder"
                    } else {
                        "resignFirstResponder"
                    },
                );
            }
        }
    }
}
