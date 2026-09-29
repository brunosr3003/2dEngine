//! The little that depends on the platform: opening a URL in the browser and
//! the iPhone's on-screen keyboard.
//!
//! Keyboard: miniquad 0.4.11 creates a hidden `UITextField` and reads the
//! characters from it (`char_event`, Enter and Backspace already arrive by the
//! normal `entrada::Teclado` path), but the `show_keyboard` request falls into
//! a `_ => {}` on iOS and never calls `becomeFirstResponder`. Here the field
//! is found in the app's view and the request is made directly, by
//! Objective-C FFI, with no new dependency.

/// Does this platform have an on-screen keyboard (and does the panel need to rise)?
pub const TECLADO_NA_TELA: bool = cfg!(any(target_os = "ios", target_os = "android"));

/// Opens `url` in the system browser. `false` if it could not.
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
    // Web: not yet.
    #[cfg(not(any(unix, windows)))]
    {
        let _ = url;
        false
    }
    #[cfg(target_os = "android")]
    {
        android_abrir_url(url)
    }
}

/// An ACTION_VIEW intent using miniquad's Activity. A local frame releases the
/// JNI references; an exception (no browser/store) becomes a visible UI error.
#[cfg(target_os = "android")]
fn android_abrir_url(url: &str) -> bool {
    use macroquad::miniquad::native::android::{attach_jni_env, ndk_sys::jvalue, ACTIVITY};
    let Ok(url) = std::ffi::CString::new(url) else { return false; };
    unsafe {
        let env = attach_jni_env();
        let j = &**env;
        if (j.PushLocalFrame.unwrap())(env, 16) != 0 { return false; }
        let resultado = (|| {
            let uri_class = (j.FindClass.unwrap())(env, b"android/net/Uri\0".as_ptr().cast());
            let intent_class = (j.FindClass.unwrap())(env, b"android/content/Intent\0".as_ptr().cast());
            let activity_class = (j.GetObjectClass.unwrap())(env, ACTIVITY);
            if uri_class.is_null() || intent_class.is_null() || activity_class.is_null() { return false; }
            let parse = (j.GetStaticMethodID.unwrap())(env, uri_class, b"parse\0".as_ptr().cast(), b"(Ljava/lang/String;)Landroid/net/Uri;\0".as_ptr().cast());
            let ctor = (j.GetMethodID.unwrap())(env, intent_class, b"<init>\0".as_ptr().cast(), b"(Ljava/lang/String;Landroid/net/Uri;)V\0".as_ptr().cast());
            let start = (j.GetMethodID.unwrap())(env, activity_class, b"startActivity\0".as_ptr().cast(), b"(Landroid/content/Intent;)V\0".as_ptr().cast());
            if parse.is_null() || ctor.is_null() || start.is_null() { return false; }
            let texto = (j.NewStringUTF.unwrap())(env, url.as_ptr());
            let action = (j.NewStringUTF.unwrap())(env, b"android.intent.action.VIEW\0".as_ptr().cast());
            if texto.is_null() || action.is_null() { return false; }
            let uri = (j.CallStaticObjectMethodA.unwrap())(env, uri_class, parse, [jvalue { l: texto }].as_ptr());
            if uri.is_null() { return false; }
            let intent = (j.NewObjectA.unwrap())(env, intent_class, ctor, [jvalue { l: action }, jvalue { l: uri }].as_ptr());
            if intent.is_null() { return false; }
            (j.CallVoidMethodA.unwrap())(env, ACTIVITY, start, [jvalue { l: intent }].as_ptr());
            (j.ExceptionCheck.unwrap())(env) == 0
        })();
        if (j.ExceptionCheck.unwrap())(env) != 0 { (j.ExceptionClear.unwrap())(env); }
        (j.PopLocalFrame.unwrap())(env, std::ptr::null_mut());
        resultado
    }
}

/// Shows or hides the on-screen keyboard. Does nothing on desktop.
pub fn teclado_virtual(mostrar: bool) {
    #[cfg(target_os = "ios")]
    ios::teclado(mostrar);
    #[cfg(target_os = "android")]
    macroquad::miniquad::window::show_keyboard(mostrar);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = mostrar;
}

/// Screen always awake while playing: on automatic nobody touches it and the
/// iPhone would turn the screen off (and pause the app). Does nothing on desktop.
pub fn manter_tela_acesa(sim: bool) {
    #[cfg(target_os = "ios")]
    ios::manter_tela_acesa(sim);
    #[cfg(not(target_os = "ios"))]
    let _ = sim;
}

/// The screen's safe area, in macroquad px: (top, left, bottom, right). On an
/// iPhone in landscape the notch/Dynamic Island eats one side, the corners are
/// rounded and the home bar sits at the bottom. Outside iOS, zero.
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

    // arm64: objc_msgSend called with the exact signature of each use.
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

    /// The `UITextField` miniquad hung on the view controller's view.
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

    /// The window's `safeAreaInsets` (points) converted to macroquad px by the
    /// screen-width / window-width ratio.
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
            // Breathing room: 4 pt beyond the inset and never less than 16 pt — in
            // landscape the top has a zero inset but the rounded corner cuts.
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
