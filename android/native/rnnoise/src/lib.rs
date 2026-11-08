//! Ponte JNI mínima para o RNNoise (`nnnoiseless`), o mesmo supressor de ruído do cliente desktop.
//!
//! API (classe Kotlin `app.telacast.voz.RnnoiseNativo`, um `object` com métodos `@JvmStatic external`):
//! - `criar(): Long`: novo estado do denoiser (um por chamada de voz), devolvido como ponteiro opaco;
//! - `processar(estado: Long, entrada: FloatArray, saida: FloatArray): Float`: um quadro de 480 amostras (10 ms a 48 kHz)
//!   em escala de 16 bits (-32768..32767, como o RNNoise espera); devolve a probabilidade de voz (0..1), ou -1 se algo
//!   deu errado (ponteiro nulo, arrays curtos, pânico);
//! - `destruir(estado: Long)`.
//!
//! Nada aqui aloca por quadro (buffers na pilha) e nenhum pânico atravessa a fronteira JNI.

use jni::objects::{JClass, JFloatArray};
use jni::sys::{jfloat, jlong};
use jni::JNIEnv;
use nnnoiseless::DenoiseState;
use std::panic::{catch_unwind, AssertUnwindSafe};

const FRAME: usize = DenoiseState::FRAME_SIZE;

#[no_mangle]
pub extern "system" fn Java_app_telacast_voz_RnnoiseNativo_criar(_env: JNIEnv, _class: JClass) -> jlong {
    catch_unwind(|| Box::into_raw(DenoiseState::new()) as jlong).unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_app_telacast_voz_RnnoiseNativo_processar(
    env: JNIEnv,
    _class: JClass,
    estado: jlong,
    entrada: JFloatArray,
    saida: JFloatArray,
) -> jfloat {
    if estado == 0 {
        return -1.0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let mut ent = [0.0f32; FRAME];
        let mut sai = [0.0f32; FRAME];
        if env.get_float_array_region(&entrada, 0, &mut ent).is_err() {
            return -1.0;
        }
        // SAFETY: `estado` veio de `criar` e só a thread de captura o usa (o Kotlin garante um dono só).
        let den = unsafe { &mut *(estado as *mut DenoiseState<'static>) };
        let prob = den.process_frame(&mut sai, &ent);
        if env.set_float_array_region(&saida, 0, &sai).is_err() {
            return -1.0;
        }
        if prob.is_finite() { prob } else { 0.0 }
    }))
    .unwrap_or(-1.0)
}

#[no_mangle]
pub extern "system" fn Java_app_telacast_voz_RnnoiseNativo_destruir(_env: JNIEnv, _class: JClass, estado: jlong) {
    if estado != 0 {
        // SAFETY: ponteiro criado por `criar`, destruído uma única vez pelo Kotlin.
        let _ = catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(estado as *mut DenoiseState<'static>) })));
    }
}
