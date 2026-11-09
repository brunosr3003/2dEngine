//! Ponte pro Google Play Billing (compra real de TP no Android).
//!
//! So' compila em `target_os = "android"`. As outras plataformas (Linux,
//! Windows, Mac, iOS, servidor de teste) continuam 100% no fluxo simulado
//! de `crates/server/src/loja.rs` — este modulo nunca participa delas.
//!
//! O lado Java (`crates/client/android/TempestBilling.java`) fala com a
//! `BillingClient` do Google e chama de volta `onPurchaseFinished` (JNI),
//! que so' enfileira o resultado. Quem consome a fila e' quem estiver no
//! loop do jogo, via `poll_result()` — chamar uma vez por frame.
//!
//! IMPORTANTE: o cliente NUNCA credita TP sozinho so' porque recebeu um
//! `purchase_token`. Esse token vai pro servidor, que confere na Android
//! Publisher API antes de creditar (mesmo design do `confirmar_pagamento`
//! existente, so' que com um provedor novo que verifica em vez de aprovar
//! na hora). Ver docs/LOJA.md.
//!
//! TODO(billing): o acoplamento com `ClientMessage::Loja` ainda nao foi
//! fechado aqui — falta decidir, junto com o lado servidor
//! (`crates/shared/src/loja.rs::PedidoLoja::ComprarTp`), que campos novos
//! carregam `product_id`/`purchase_token` ate' o servidor. Quando isso
//! estiver definido, o ponto de chamada e' `crates/client/src/loja_tp.rs`
//! (busca por `TODO(billing)` la').

use std::sync::Mutex;

/// Resultado de uma tentativa de compra via Play Billing.
#[derive(Debug, Clone)]
pub struct ResultadoCompra {
    /// Product id cadastrado na Play Console, ex. "tp_pacote_2" — o sufixo
    /// numerico e' o mesmo `id` de `shared::loja::PacoteTp`. Convencao do
    /// SERVIDOR (`loja_google_play::id_do_produto`), que recalcula esse id
    /// sozinho a partir do `pacote` e nunca confia no que o cliente manda —
    /// este formato so' precisa bater com o que esta' cadastrado na Play
    /// Console.
    pub product_id: String,
    /// Token de compra opaco. So' tem valor se `codigo == 0`; o SERVIDOR e'
    /// quem confere ele na Android Publisher API antes de creditar TP.
    pub purchase_token: String,
    /// `BillingClient.BillingResponseCode` do Google (0 = OK). Usamos -1
    /// pra "nao veio nenhuma compra" quando o Java so' tem um erro de setup
    /// pra reportar e nenhum produto/token fazem sentido.
    pub codigo: i32,
}

static FILA: Mutex<Vec<ResultadoCompra>> = Mutex::new(Vec::new());

/// Dispara o fluxo de compra real da Play Store pro pacote de TP
/// `pacote_id` (1..=4, mesmo `id` de `shared::loja::PacoteTp` —
/// `shared::loja::PACOTES`). So' pede pro Java comecar; o resultado chega
/// depois, assincrono, em `poll_result`.
pub fn launch_purchase(pacote_id: u16) {
    launch_product(&format!("tp_pacote_{pacote_id}"));
}

/// Any Play product: TP packs (`tp_pacote_N`) and flash offers (`oferta_*`).
pub fn launch_product(product_id: &str) {
    unsafe { jni::chama_estatico("launchPurchase", Some(product_id)) };
}

/// Re-reports purchases Google still lists as owned (paid, not consumed by
/// the server yet). Call once after entering the world.
pub fn recover() {
    unsafe { jni::chama_estatico("recover", None) };
}

/// The order id of a Google Play purchase, DERIVED FROM ITS TOKEN: every
/// re-send of the same purchase (lost reply, `recover` at login) is the same
/// order, so the server answers "already credited" and consumes it instead of
/// crediting twice or refusing it.
pub fn pedido_do_token(token: &str) -> String {
    fn fnv(dados: &[u8], semente: u64) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64 ^ semente;
        for b in dados {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    }
    let b = token.as_bytes();
    format!("gp-{:016x}{:016x}", fnv(b, 0), fnv(b, 0x9e37_79b9_7f4a_7c15))
}

/// Chamar uma vez por frame (loop principal) pra saber se alguma compra
/// terminou desde a ultima chamada. `None` na maioria dos frames.
pub fn poll_result() -> Option<ResultadoCompra> {
    FILA.lock().unwrap().pop()
}

mod jni {
    use super::{ResultadoCompra, FILA};
    use macroquad::miniquad::native::android::{attach_jni_env, ndk_sys, ACTIVITY};
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    /// Chamado pelo Java: `quad_native.TempestBilling.onPurchaseFinished`.
    /// So' enfileira — nada de logica de credito aqui (isso e' do servidor).
    #[allow(non_snake_case)]
    #[no_mangle]
    pub unsafe extern "system" fn Java_quad_1native_TempestBilling_onPurchaseFinished(
        env: *mut ndk_sys::JNIEnv,
        _class: ndk_sys::jclass,
        product_id: ndk_sys::jstring,
        purchase_token: ndk_sys::jstring,
        codigo: ndk_sys::jint,
    ) {
        let resultado = ResultadoCompra {
            product_id: jstring_para_string(env, product_id),
            purchase_token: jstring_para_string(env, purchase_token),
            codigo: codigo as i32,
        };
        FILA.lock().unwrap().push(resultado);
    }

    unsafe fn jstring_para_string(env: *mut ndk_sys::JNIEnv, s: ndk_sys::jstring) -> String {
        if s.is_null() {
            return String::new();
        }
        let get = (**env).GetStringUTFChars.unwrap();
        let release = (**env).ReleaseStringUTFChars.unwrap();
        let mut is_copy: ndk_sys::jboolean = 0;
        let ptr = get(env, s, &mut is_copy);
        if ptr.is_null() {
            return String::new();
        }
        let out = CStr::from_ptr(ptr as *const c_char)
            .to_string_lossy()
            .into_owned();
        release(env, s, ptr);
        out
    }

    /// `ACTIVITY.getClassLoader().loadClass(nome)`; null (with the Java
    /// exception cleared) when the class is missing.
    unsafe fn classe_do_app(env: *mut ndk_sys::JNIEnv, nome: &str) -> ndk_sys::jclass {
        let limpa = |env: *mut ndk_sys::JNIEnv| {
            if (**env).ExceptionCheck.unwrap()(env) != 0 {
                (**env).ExceptionClear.unwrap()(env);
                true
            } else {
                false
            }
        };
        let cls_activity = (**env).GetObjectClass.unwrap()(env, ACTIVITY);
        let m_loader = (**env).GetMethodID.unwrap()(
            env,
            cls_activity,
            c"getClassLoader".as_ptr(),
            c"()Ljava/lang/ClassLoader;".as_ptr(),
        );
        if limpa(env) || m_loader.is_null() {
            return std::ptr::null_mut();
        }
        let loader = (**env).CallObjectMethodA.unwrap()(env, ACTIVITY, m_loader, std::ptr::null());
        if limpa(env) || loader.is_null() {
            return std::ptr::null_mut();
        }
        let cls_loader = (**env).GetObjectClass.unwrap()(env, loader);
        let m_load = (**env).GetMethodID.unwrap()(
            env,
            cls_loader,
            c"loadClass".as_ptr(),
            c"(Ljava/lang/String;)Ljava/lang/Class;".as_ptr(),
        );
        if limpa(env) || m_load.is_null() {
            return std::ptr::null_mut();
        }
        let jnome = {
            let c = CString::new(nome).unwrap();
            (**env).NewStringUTF.unwrap()(env, c.as_ptr())
        };
        let args = [ndk_sys::jvalue { l: jnome }];
        let class = (**env).CallObjectMethodA.unwrap()(env, loader, m_load, args.as_ptr());
        if limpa(env) {
            return std::ptr::null_mut();
        }
        class as ndk_sys::jclass
    }

    /// `TempestBilling.<metodo>(ACTIVITY[, arg])` on the Java side, with the
    /// Activity miniquad already cached (`native::android::ACTIVITY`).
    pub unsafe fn chama_estatico(metodo: &str, arg: Option<&str>) {
        let env = attach_jni_env();

        // Not `FindClass`: the game loop runs on a native thread, and there
        // JNI's FindClass uses the system class loader, which only knows the
        // Android framework - it never finds the app's own classes. The
        // Activity's class loader does, from any thread.
        let class = classe_do_app(env, "quad_native.TempestBilling");
        if class.is_null() {
            (**env).ExceptionClear.unwrap()(env);
            macroquad::prelude::error!(
                "TempestBilling: classe quad_native.TempestBilling nao encontrada no APK"
            );
            return;
        }
        let method_name = CString::new(metodo).unwrap();
        let sig = if arg.is_some() {
            c"(Landroid/app/Activity;Ljava/lang/String;)V"
        } else {
            c"(Landroid/app/Activity;)V"
        };
        let method = (**env).GetStaticMethodID.unwrap()(env, class, method_name.as_ptr(), sig.as_ptr());
        if method.is_null() {
            (**env).ExceptionClear.unwrap()(env);
            macroquad::prelude::error!("TempestBilling: metodo {metodo} nao encontrado");
            return;
        }
        let mut args = vec![ndk_sys::jvalue { l: ACTIVITY }];
        if let Some(a) = arg {
            let c = CString::new(a).unwrap_or_default();
            args.push(ndk_sys::jvalue { l: (**env).NewStringUTF.unwrap()(env, c.as_ptr()) });
        }
        (**env).CallStaticVoidMethodA.unwrap()(env, class, method, args.as_ptr());
        if (**env).ExceptionCheck.unwrap()(env) != 0 {
            (**env).ExceptionClear.unwrap()(env);
        }
    }
}
