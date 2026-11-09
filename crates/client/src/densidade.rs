//! Pixels per millimetre of the real screen.
//!
//! Touch targets have to be sized in MILLIMETRES, not pixels: a phone packs
//! 10 to 18 pixels into each millimetre, and the game draws in raw pixels
//! there (`high_dpi`). 44 px — the old finger target — came out as 3 mm on a
//! Moto G, about a third of a fingertip (owner, 08/11/2026: "the clickable
//! icons are terrible, way too small").
//!
//! - Android: `DisplayMetrics.xdpi/ydpi` over JNI, checked against the
//!   `densityDpi` bucket (some phones report a bogus xdpi).
//! - iOS: the game draws in POINTS, ~1/160 inch each (the screen scale only
//!   matters for pixels, which this never sees).
//! - Desktop: `None` — a mouse pointer doesn't need it.

use std::sync::atomic::{AtomicU32, Ordering};

const MM_POR_POLEGADA: f32 = 25.4;

/// Cached result as f32 bits; 0 = not measured yet.
static CACHE: AtomicU32 = AtomicU32::new(0);

/// Pixels per millimetre of this screen, when known (phones only).
pub fn px_por_mm() -> Option<f32> {
    let b = CACHE.load(Ordering::Relaxed);
    if b != 0 {
        return Some(f32::from_bits(b));
    }
    let v = medir()?;
    if !(v.is_finite() && v > 1.0 && v < 200.0) {
        return None;
    }
    CACHE.store(v.to_bits(), Ordering::Relaxed);
    Some(v)
}

/// Picks the dpi to trust: the measured one, unless it is far from the bucket
/// Android itself uses for layout (then the bucket wins).
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
fn dpi_confiavel(xdpi: f32, ydpi: f32, balde: f32) -> f32 {
    let medido = (xdpi + ydpi) * 0.5;
    if balde > 0.0 && (medido < balde * 0.7 || medido > balde * 1.4 || !medido.is_finite()) {
        balde
    } else {
        medido
    }
}

#[cfg(target_os = "android")]
fn medir() -> Option<f32> {
    let (x, y, balde) = unsafe { android::metricas()? };
    Some(dpi_confiavel(x, y, balde) / MM_POR_POLEGADA)
}

#[cfg(target_os = "ios")]
fn medir() -> Option<f32> {
    let b = macroquad::miniquad::native::ios::ESCALA_TELA.load(Ordering::Relaxed);
    if b == 0 {
        return None; // first frame not drawn yet; ask again later
    }
    // The game draws in POINTS on iOS (not pixels): an iPhone point is
    // ~1/160 inch whatever the screen scale. Multiplying by the scale (as it
    // did) made the 9 mm target three times too big — a corner cog ended up
    // covering the whole button under it.
    let _ = f32::from_bits(b);
    Some(160.0 / MM_POR_POLEGADA)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn medir() -> Option<f32> {
    let _ = MM_POR_POLEGADA;
    None
}

#[cfg(target_os = "android")]
mod android {
    use macroquad::miniquad::native::android::{attach_jni_env, ndk_sys, ACTIVITY};

    /// `(xdpi, ydpi, densityDpi)` from `getResources().getDisplayMetrics()`.
    pub unsafe fn metricas() -> Option<(f32, f32, f32)> {
        let env = attach_jni_env();
        let f = &**env;
        let falhou = |env: *mut ndk_sys::JNIEnv| {
            if (**env).ExceptionCheck.unwrap()(env) != 0 {
                (**env).ExceptionClear.unwrap()(env);
                true
            } else {
                false
            }
        };
        let cls_act = f.GetObjectClass.unwrap()(env, ACTIVITY);
        let m_res = f.GetMethodID.unwrap()(
            env,
            cls_act,
            c"getResources".as_ptr(),
            c"()Landroid/content/res/Resources;".as_ptr(),
        );
        if falhou(env) || m_res.is_null() {
            return None;
        }
        let res = f.CallObjectMethodA.unwrap()(env, ACTIVITY, m_res, std::ptr::null());
        if falhou(env) || res.is_null() {
            return None;
        }
        let cls_res = f.GetObjectClass.unwrap()(env, res);
        let m_dm = f.GetMethodID.unwrap()(
            env,
            cls_res,
            c"getDisplayMetrics".as_ptr(),
            c"()Landroid/util/DisplayMetrics;".as_ptr(),
        );
        if falhou(env) || m_dm.is_null() {
            return None;
        }
        let dm = f.CallObjectMethodA.unwrap()(env, res, m_dm, std::ptr::null());
        if falhou(env) || dm.is_null() {
            return None;
        }
        let cls_dm = f.GetObjectClass.unwrap()(env, dm);
        let campo_f = |nome: &std::ffi::CStr| -> Option<f32> {
            let id = (**env).GetFieldID.unwrap()(env, cls_dm, nome.as_ptr(), c"F".as_ptr());
            if falhou(env) || id.is_null() {
                return None;
            }
            Some((**env).GetFloatField.unwrap()(env, dm, id))
        };
        let x = campo_f(c"xdpi")?;
        let y = campo_f(c"ydpi")?;
        let id = f.GetFieldID.unwrap()(env, cls_dm, c"densityDpi".as_ptr(), c"I".as_ptr());
        let balde = if falhou(env) || id.is_null() {
            0.0
        } else {
            f.GetIntField.unwrap()(env, dm, id) as f32
        };
        Some((x, y, balde))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn dpi_maluco_cai_no_balde_do_android() {
        // Honest phone: measured value wins.
        assert_eq!(dpi_confiavel(268.0, 270.0, 280.0), 269.0);
        // Bogus xdpi (some vendors report 160 on a 420 dpi panel): bucket wins.
        assert_eq!(dpi_confiavel(160.0, 160.0, 420.0), 420.0);
        // No bucket: whatever we measured.
        assert_eq!(dpi_confiavel(300.0, 300.0, 0.0), 300.0);
    }
}
