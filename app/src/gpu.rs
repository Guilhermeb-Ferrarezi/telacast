//! Encoder H.264 por GPU (NVENC). A GPU converte BGRA -> YUV e codifica,
//! então a CPU só entrega os pixels capturados.
use nvidia_video_codec_sdk::{
    sys::nvEncodeAPI::{
        NV_ENC_BUFFER_FORMAT, NV_ENC_CODEC_H264_GUID, NV_ENC_PARAMS_RC_MODE, NV_ENC_PIC_TYPE,
        NV_ENC_PRESET_P1_GUID, NV_ENC_TUNING_INFO,
    },
    Encoder, EncoderInitParams, ErrorKind,
};
use std::{thread, time::Duration};

/// Falha ao iniciar a GPU (permite cair para o encoder de software).
#[derive(Debug)]
pub struct InitError(pub String);
impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NVENC indisponível: {}", self.0)
    }
}
impl std::error::Error for InitError {}

fn init<E: std::fmt::Debug>(e: E) -> InitError {
    InitError(format!("{e:?}"))
}

/// Codifica quadros BGRA (w*h*4 bytes) recebidos de `recv` até ele devolver `None`.
/// `send(key, h264_annexb)` recebe cada quadro codificado.
pub fn encode_loop(
    w: u32,
    h: u32,
    fps: u32,
    kbps: u32,
    mut recv: impl FnMut() -> Option<Vec<u8>>,
    mut send: impl FnMut(bool, &[u8]) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let ctx = cudarc::driver::CudaContext::new(0).map_err(init)?;
    let encoder = Encoder::initialize_with_cuda(ctx).map_err(init)?;

    let mut preset = encoder
        .get_preset_config(
            NV_ENC_CODEC_H264_GUID,
            NV_ENC_PRESET_P1_GUID,
            NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY,
        )
        .map_err(init)?;
    let cfg = &mut preset.presetCfg;
    let gop = fps * 2;
    cfg.gopLength = gop;
    cfg.frameIntervalP = 1; // sem quadros B: latência mínima
    let bps = kbps * 1000;
    cfg.rcParams.rateControlMode = NV_ENC_PARAMS_RC_MODE::NV_ENC_PARAMS_RC_CBR;
    cfg.rcParams.averageBitRate = bps;
    cfg.rcParams.maxBitRate = bps;
    cfg.rcParams.vbvBufferSize = bps / fps.max(1);
    unsafe {
        let c = &mut cfg.encodeCodecConfig.h264Config;
        c.set_repeatSPSPPS(1); // todo IDR leva SPS/PPS: o espectador decodifica de qualquer um
        c.idrPeriod = gop;
    }

    let mut params = EncoderInitParams::new(NV_ENC_CODEC_H264_GUID, w, h);
    params
        .preset_guid(NV_ENC_PRESET_P1_GUID)
        .tuning_info(NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY)
        .encode_config(cfg)
        .framerate(fps, 1)
        .enable_picture_type_decision();
    // NVENC "ARGB" little-endian = bytes B,G,R,A: o mesmo layout do X11.
    let session = encoder
        .start_session(NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_ARGB, params)
        .map_err(init)?;
    let mut input = session.create_input_buffer().map_err(init)?;
    let mut output = session.create_output_bitstream().map_err(init)?;

    let row = w as usize * 4;
    while let Some(frame) = recv() {
        unsafe { input.lock()?.write_pitched(&frame, row) };
        loop {
            match session.encode_picture(&mut input, &mut output, Default::default()) {
                Ok(()) => break,
                Err(e) if e.kind() == ErrorKind::EncoderBusy => {
                    thread::sleep(Duration::from_millis(1))
                }
                Err(e) if e.kind() == ErrorKind::NeedMoreInput => break,
                Err(e) => return Err(e.into()),
            }
        }
        let lock = output.lock()?;
        let key = matches!(
            lock.picture_type(),
            NV_ENC_PIC_TYPE::NV_ENC_PIC_TYPE_IDR | NV_ENC_PIC_TYPE::NV_ENC_PIC_TYPE_I
        );
        if !lock.data().is_empty() {
            send(key, lock.data())?;
        }
    }
    Ok(())
}
