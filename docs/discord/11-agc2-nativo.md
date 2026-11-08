# 11 - AGC2 nativo do Discord (captura do microfone)

Alvo: `/snap/discord/320/usr/share/discord/resources/standalone_modules/discord_voice/discord_voice.node`
(ELF x86-64, 126 MB, sha256 `0cc5cb75...e9046caa`, WebRTC branch **m146**, com `.symtab` e DWARF; não é stripped).
Análise estática, só leitura. Nada foi executado.

## Método e limites

- Ferramentas: `inspect_with_objdump` do REA (evidência `ev_d8fd19a2...d1bc5c`, símbolo `GainController2::Validate`) e, para o volume grande, os mesmos GNU `objdump`, `nm`, `strings` e `gdb -batch ptype` (só lê DWARF) por linha de comando.
- Não abri o módulo no Ghidra/REA `open_binary`: 126 MB + ~100 MB de DWARF numa máquina de 15 GB com RAM apertada. Os símbolos existem, então o objdump com faixa de endereços bastou.
- Evidência de execução real: os logs do próprio Discord em `~/snap/discord/320/.config/discord/logs/discord-webrtc_0` imprimem o `AudioProcessing::Config` e o log `AGC2 adaptive digital | speech_dbfs | noise_dbfs | headroom_db | gain_db` (linhas lidas, nada alterado). Esses logs confirmam o decompilado.
- Confiança: ALTA = lido no binário e confirmado no log; MÉDIA = lido no binário, sem confirmação de execução; BAIXA/INFERIDO = deduzido (diz como).
- Endereços são VMA do ELF (`0x85....` = bloco `webrtc::` AGC2; `0x3829b0..` = código Discord).

## 1. Quem manda na configuração

O Discord **não usa os defaults do WebRTC diretamente**: o JS do cliente chama `SetAutomaticGainControlConfig({...})`, convertido em `NativeValueConverter<discord::media::GainControllerConfig>::Convert` (`0x329b50`). Campos lidos do objeto JS (strings em `0x18325d8..0x183265d`):

`enabled, useAGC2, enableAnalog, enableDigital, headroom_db, max_gain_db, initial_gain_db, max_gain_change_db_per_second, max_output_noise_level_dbfs, fixed_gain_db` (struct de 28 bytes, layout confirmado por DWARF).

Quando o JS **omite** um campo, o nativo usa estes defaults (rodata `0x1830178..0x1830188`, mais `xorps` para o fixed gain):

| Campo | Default se o JS omitir | Evidência | Confiança |
|---|---|---|---|
| `enabled` | true | `0x329bfe cmovb ...,1` | ALTA |
| `useAGC2` | **false** | `0x329c2a mov ebp,0x100` | ALTA |
| `enableAnalog` | **false** | `0x329c24 mov r12d,0x100` | ALTA |
| `enableDigital` | true | `0x329c6b cmovb r14d,1` | ALTA |
| `headroom_db` | 5.0 | float `0x1830178` | ALTA |
| `max_gain_db` | 50.0 | `0x183017c` | ALTA |
| `initial_gain_db` | 15.0 | `0x1830180` | ALTA |
| `max_gain_change_db_per_second` | 6.0 | `0x1830184` | ALTA |
| `max_output_noise_level_dbfs` | -50.0 | `0x1830188` | ALTA |
| `fixed_gain_db` | 0.0 | `xorps xmm0` em `0x32a249` | ALTA |

Os valores que o JS **realmente envia** não estão no `.node`. O que se observa nos logs desta máquina (7 nov 2026, 3 sessões): sempre 5 / 50 / 15 / 6 / -50 / fixed 0, ou seja, iguais aos defaults. Se o JS manda outro número em outra conta/experimento, não dá para saber só pelo nativo (INFERIDO: o cliente web decide `useAGC2` por experimento/servidor, não achei nada no `.node` que escolha sozinho).

## 2. Como o nativo monta o `AudioProcessing::Config`

`AudioEngine::SetAutomaticGainControl(bool)` `0x3829b0` e `SetAutomaticGainControlConfig` `0x382a70` fazem `GetConfig()` (vtable +0x108), alteram e `ApplyConfig` (vtable +0x30). Mapeamento (offsets do `Config` conferidos com `ptype /o`):

| Campo do APM | Valor | Evidência | Confiança |
|---|---|---|---|
| `gain_controller1.enabled` | `enabled && !useAGC2` | `0x3829e0..0x3829f5` | ALTA |
| `gain_controller1.mode` | AdaptiveAnalog (0) | `[rsp+0x48]=0x300000000` | ALTA |
| `gain_controller1.target_level_dbfs` | 3 (ou seja -3 dBFS) | idem | ALTA |
| `gain_controller1.compression_gain_db` | 9 | `[rsp+0x50]=9` | ALTA |
| `gain_controller1.enable_limiter` | true | `[rsp+0x54]=1` | ALTA |
| `analog_gain_controller` (AGC1) | enabled 1, startup_min_volume 0, clipped_level_min 70, enable_digital_adaptive 1, clipped_level_step 15, clipped_ratio_threshold 0.1, clipped_wait_frames 300, clipping_predictor desligado | `0x380e8f..0x380f4f` e log | ALTA |
| `gain_controller2.enabled` | `enabled && useAGC2` | `0x3829f5..0x382a10`, `[rsp+0x94]` | ALTA |
| `gain_controller2.input_volume_controller.enabled` | `enableAnalog` | `[rsp+0x95]` | ALTA |
| `gain_controller2.adaptive_digital.enabled` | `enableDigital` | `[rsp+0x98]` | ALTA |
| `adaptive_digital.headroom_db / max_gain_db / initial_gain_db / max_gain_change_db_per_second / max_output_noise_level_dbfs` | os do JS | `0x382a35..0x382a46` | ALTA |
| `fixed_digital.gain_db` | `fixed_gain_db` do JS | `[rbx+0x5c]` | ALTA |
| Estado inicial do engine (antes do JS) | enabled=1, useAGC2=0, analog=0, digital=1, 5/50/15/6/-50, fixed 0 | ctor `0x380586..0x38059e` | ALTA |
| `pre_amplifier` | desligado, fator 1.0 | `0x380e90..0x380eb1`, log | ALTA |
| `capture_level_adjustment` | desligado, pre/post 1.0, analog_mic_gain_emulation off (initial_level 255) | `0x380eb1..0x380ec2`, log | ALTA |
| `high_pass_filter` | **enabled = true**, apply_in_full_band = true | `AudioEngine::AudioEngine` `[rsp+0x74]=0x101`; log `high_pass_filter: { enabled: 1 }` | ALTA |
| `pipeline.maximum_internal_processing_rate` | **48000** (WebRTC padrão é 32000) | `0x380ad8 mov [rsp+0x48],0xbb80`; log | ALTA |
| `noise_suppression` | enabled = flag do usuário, **level = 2 (High)**; nível inicial Moderate (padrão) | `SetNoiseSuppression` `0x382e94 mov [rsp+0x38],2`; log `level: High` | ALTA |
| `echo_canceller` | enabled = flag; `enforce_high_pass_filtering` 1; fábrica própria `DiscordEchoCanceller3Factory` | `0x382880`, `0x3a2630` | ALTA |
| `transient_suppression` | desligado | log | ALTA |

Resultado nos logs (config efetiva em uso por esta conta): `gain_controller1.enabled: 0`, `gain_controller2.enabled: 1`, `adaptive_digital.enabled: 1`, `input_volume_control.enabled: 0`, `fixed_digital.gain_db: 0`, NS High, HPF on. Em sessões com AGC "clássico" o log mostra `gain_controller1.enabled: 1` e `gain_controller2.enabled: 0`.

`GainController2::Validate` `0x85b540` (confirmado via REA): `headroom_db >= 0`, `max_gain_db > 0`, `initial_gain_db >= 0`, `max_gain_change_db_per_second > 0`, `max_output_noise_level_dbfs <= 0`, `0 <= fixed_digital.gain_db < 50`.

### InputVolumeController (analógico do AGC2)

Só existe se `gain_controller2.enabled` (a construção é condicionada a isso em `InitializeGainController2` `0x8548a0`). A `InputVolumeController::Config` é montada na pilha com constantes fixas (`0x8548bd..0x8548ed`, rodata `0x1897bc0..0x1897be8`); o JS não altera nada disso.

| Campo | Valor | Evidência | Confiança |
|---|---|---|---|
| mínimo de volume | 20 | `0x1897bc0` | MÉDIA (ordem dos campos inferida do WebRTC aberto) |
| clipped_level_min / step / ratio / wait | 70 / 15 / 0.1 / 300 | `0x1897bc4..0x1897bd0` | ALTA |
| clipping predictor | habilitado na Config (`[rbp-0x44]=1`) | `0x8548d3` | BAIXA (campo identificado por posição) |
| faixa alvo | -30 / -12 / -50 dBFS, `update_input_volume_wait_frames` 100 | `0x1897bd8..0x8548d7 (0x64ffffffce)` | BAIXA (ordem exata dos campos não confirmada; -12 parece o "max speech level" experimental do field trial `WebRTC-Agc2MaxSpeechLevelExperimental`) |
| speech_probability_threshold / speech_ratio_threshold | 0.7 / 0.6 | `0x1897be0` | ALTA |

Como `enableAnalog` default é false, isso fica **desativado** na prática. Para o app Android o volume analógico do sistema não se aplica, ignore.

## 3. Algoritmo adaptativo digital (AGC2 puro)

Tudo abaixo bate com o `modules/audio_processing/agc2` aberto: **a lógica é a mesma do WebRTC**, só a configuração externa muda. Quadro de 10 ms (`kFrameDurationMs = 10`).

Ordem em `GainController2::Process` (`0x85af80`): VAD -> RMS e pico do quadro -> estimador de ruído -> estimador de nível de fala -> protetor de saturação -> `AdaptiveDigitalGainController::Process` -> ganho fixo -> `Limiter::Process`. A cada 3000 quadros (30 s) loga `[AGC2] limiter stats`.

### 3.1 Medição de nível

| Item | Valor | Evidência | Confiança |
|---|---|---|---|
| Escala dBFS | `20*log10(x) - 90.309` (amostras float em escala S16; se x <= 1, vale -90.309) | `0x1898780`=-90.3089981, `0x85b0f4..0x85b113` | ALTA |
| RMS | sqrt(soma(x^2)/n) tomada em todos os canais | `0x85b0b0..0x85b149` | ALTA |
| Pico | max(abs(x)) | `0x85b0bf` | ALTA |

### 3.2 VAD

É o **RNN VAD do WebRTC** (`rnn_vad::FeaturesExtractor`, `RnnVad`, camadas FC/GRU com pesos int8, `MonoVadImpl::Analyze` `0x85f9a0`). Reamostra para 24 kHz (`PushResampler` `0x85f8ba`). Reinício do estado a cada 1500 ms (`mov esi,0x5dc` em `0x85acd7`). O VAD **não aplica limiar**: devolve probabilidade 0..1. O único limiar é `kVadConfidenceThreshold = 0.95` (rodata `0x1841f58`), usado no estimador de fala, no protetor de saturação e no controlador de ganho (`0x85f3a4`, `0x85ede9`, `0x85b6d1`). Confiança ALTA. É o VAD interno do AGC2 (criado em `0x85acc7`), não um VAD separado.

### 3.3 SpeechLevelEstimator (`SpeechLevelEstimatorImpl`, `0x85f310`/`0x85f3a0`)

O estimador "Experimental" existe mas só é escolhido pelo field trial `WebRTC-Agc2SpeechLevelEstimatorExperimental`; o log mostra `AGC2 using SpeechLevelEstimator` (padrão).

| Parâmetro | Valor | Evidência | Confiança |
|---|---|---|---|
| Nível inicial | `clamp(-20 - initial_gain_db - headroom_db, -90, 30)` = **-40 dBFS** com os defaults | `0x1899750`=-20, `0x186e974`=30, `0x18989c0`=-90 | ALTA (log: `speech_dbfs: -40.0074` no início) |
| `kAdjacentSpeechFramesThreshold` | **12** quadros (120 ms) | `mov ecx,0xc` em `0x85acaa`, `0x85add1`; `mov edi,0xc` em `0x85ad96` | ALTA |
| Tempo até confiança | 400 ms (40 quadros de fala; decrementa 10 ms por quadro de fala) | `mov [rdi+0x18],0x190`; `add eax,-10` | ALTA |
| Atualização (quadro de fala, p >= 0.95) | `num = num*leak + rms_dbfs*p; den = den*leak + p`; `leak = 1.0` até encher o buffer de confiança, depois `0.9975` | `0x1830190`=1.0, `0x1899758`=0.9975 | ALTA |
| Nível publicado | `clamp(num/den, -90, 30)`, só atualizado quando já há >= 12 quadros de fala consecutivos | `0x85f429..0x85f456` | ALTA |
| Quadro sem fala | se havia >= 12 quadros de fala seguidos, confirma o estado preliminar; se havia 1..11, descarta (volta ao último estado confiável) | `0x85f3b4..0x85f482` | ALTA |
| `speech_level_reliable` | `reliable_state.time_to_confidence == 0` | `0x85f497..0x85f4a9` | MÉDIA |

### 3.4 SaturationProtector (margem entre pico e nível médio da fala)

`SaturationProtectorImpl::Analyze` `0x85ede0`, criado com margem inicial **20 dB** e `adjacent_speech_frames_threshold = 12` (`CreateSaturationProtector` `0x85ecf0`, `0x183c7d8`=20).

- Só atualiza em quadro com p >= 0.95.
- Mantém o máximo do pico dBFS por janela; a cada **> 400 ms** (`cmp eax,0x191`) empurra o máximo num buffer circular de **4** posições (`0x85efa9 cmp rax,4`) e zera o máximo para -90.31 (`0xc2b49eb8`).
- `delta = (oldest do buffer, ou o máximo corrente se vazio) - nivel_de_fala_dbfs`.
- Margem = `margem*A + delta*B`, com **ataque** (`delta > margem`): A=0.998849, B=0.001151; **liberação**: A=0.99977, B=0.00023 (`0x1899688`, `0x1899690`).
- Clamp **[12, 25] dB** (`0x1899684`=12, `0x1899680`=25).
- A margem publicada só muda quando há >= 12 quadros de fala seguidos.
- Evidência de execução: log `headroom_db: 20.2..22.7`. ALTA.

### 3.5 Estimador de ruído (`NoiseFloorEstimator`, `0x85ea10`/`0x85ea80`)

| Parâmetro | Valor | Evidência | Confiança |
|---|---|---|---|
| Energia por quadro | max entre canais de soma(x^2) | `0x85eb30..0x85ec18` | ALTA |
| Piso de energia (`kMinNoisePower`) | `sample_rate*2*2/100` por quadro, i.e. RMS 2.0 S16 = **-84.29 dBFS** | `imul ecx,eax,0x64`, init `0x44f00000`=1920 (48 kHz) | ALTA (log mostra `noise_dbfs` -83.5..-84.3) |
| Janela de estimação | 500 quadros (**5 s**) | `mov [rax+0x24],0x1f4` | ALTA |
| Regra | energia preliminar = mínimo dos quadros da janela; ao fim da janela: se `ruido < preliminar` então `ruido = 0.5*ruido + 0.5*preliminar`, senão `ruido = preliminar`; queda é imediata (mínimo corrente) | `0x85ec30..0x85ec9a`, `0x1830174`=0.5 | ALTA (estrutura) / MÉDIA (nome das variáveis) |
| Saída | `10*log10(energia/n_amostras) - 90.309` | `0x85ecc1..0x85ecd6` | ALTA |

### 3.6 AdaptiveDigitalGainController (`0x85b5a0` ctor, `0x85b630` Process)

Estado inicial: `last_gain_db = initial_gain_db` (15 dB; ganho linear inicial `10^(15/20) = 5.62`); `max_gain_change_db_per_frame = max_gain_change_db_per_second * 10 / 1000` = **0.06 dB/quadro** (`0x85b5f3..0x85b609`).

Por quadro, com `FrameInfo{p, speech_level_dbfs, reliable, noise_rms_dbfs, headroom_db(=margem 3.4), limiter_envelope_dbfs}`:

1. `input = speech_level_dbfs + margem`.
2. `ganho_alvo = ComputeGainDb(input)`: se `input < -(headroom_db + max_gain_db)` então `max_gain_db`; senão se `input < -headroom_db` então `-headroom_db - input`; senão 0. (`0x85b64f..0x85b682`)
3. Limite por ruído: `ganho_alvo = min(ganho_alvo, max(max_output_noise_level_dbfs - noise_rms_dbfs, 0))`. (`0x85b682..0x85b699`)
4. Limite por baixa confiança: se `!reliable` e `limiter_envelope_dbfs > -1.0` (`0x18342a4`=-1.0): `ganho_alvo = min(ganho_alvo, max(-1 - (limiter_envelope - last_gain), 0))`. (`0x85b69d..0x85b6cd`)
5. Contador de aumento: se `p >= 0.95` e contador > 0, decrementa; se `p < 0.95`, contador = 12. Aumento só é permitido com contador == 0; no quadro em que o contador chega a 0 o aumento máximo é `12*0.06 = 0.72 dB` (catch-up). (`0x85b6d1..0x85b99e`)
6. `delta = ganho_alvo - last_gain`; se aumento não permitido, `delta = min(delta, 0)`; `delta = clamp(delta, -0.06, +0.06)` (ou +0.72 no catch-up). Subida e descida têm o mesmo limite. (`0x85b70f..0x85b761`)
7. `last_gain += delta`; ganho linear = `10^(last_gain/20)` aplicado pelo `GainApplier` (rampa por amostra entre o ganho anterior e o novo, INFERIDO do WebRTC aberto, não li `0x85b9e0`).

Confirmação numérica com o log do Discord: `speech -40.0074, margem 22.5548 => input -17.4526 => ganho = 5 - 17.4526... = 12.4526` (log: `gain_db: 12.4526`); `speech -10.4297, margem 20.2329 => input +9.8 => ganho 0` (log: `gain_db: 0`). ALTA.

### 3.7 Ganho fixo e limitador

- `fixed_digital.gain_db` aplicado depois do adaptativo (`GainApplier::SetGainFactor`, `10^(g/20)`; `SetFixedGainDb` `0x85aef0`). Default 0.
- `Limiter` `0x84a720` = `FixedDigitalLevelEstimator` + `InterpolatedGainCurve`, nome "Agc2".
  - Estimador de nível: **20 subquadros** por quadro de 10 ms (`/20` em `0xad4146`, i.e. 0.5 ms a 48 kHz); nível = max(abs) do subquadro; **ataque instantâneo** e **liberação** com constante **0.997126** por subquadro (`0x18d5ac8`=0.002874, `0x18d5acc`=0.997126), que dá cerca de **-50 dB/s**. ALTA (constantes) / MÉDIA (interpretação).
  - Curva de ganho (tabelas `0x18d5ba0` x, `0x18d5c20` m, `0x18d5ca0` q; 32 pontos; ganho = `m*x + q`, x em escala S16): identidade (ganho 1) abaixo de x=30057 (-0.75 dBFS); joelho suave a partir daí; saída em -0.75 -> -0.75, -0.19 -> -0.31, +0.25 -> -0.15, +0.59 -> -0.08, +0.95 -> -0.01 dBFS (entrada -> saída); acima de x=36575 (+0.95 dBFS) o ganho satura para segurar em 0 dBFS. Corresponde a `kLimiterMaxInputLevelDbFs=1`, joelho 1 dB, razão 5:1 do WebRTC aberto. ALTA (tabelas lidas); a forma (razão 5:1) é INFERIDA.
  - Ganho por subquadro interpolado linearmente por amostra e clip final em S16 (INFERIDO do WebRTC aberto).
- Log do limitador nesta máquina: `identity: ~58736 por 30 s | knee: 101 | limiter: 65 | saturation: 1098`, ou seja, quase tudo passa em ganho 1.

## 4. O que é padrão e o que o Discord mudou

Padrão WebRTC (igual ao código aberto m146): todos os defaults numéricos do AGC2 (5 / 50 / 15 / 6 / -50 / fixed 0), o algoritmo inteiro de 3.1 a 3.7 (VAD RNN, 0.95, 12 quadros, -20 dB inicial, margem 12..25, ruído 5 s/-84.29 dBFS, limite 0.06 dB/quadro, tabelas do limitador), o AGC1 (AdaptiveAnalog, alvo 3, compressão 9, limitador on).

Mudanças/escolhas do Discord:

1. **AGC2 é opcional e substitui o AGC1**: `useAGC2` do JS liga `gain_controller2` e desliga `gain_controller1` (default do engine: AGC1 ligado, AGC2 desligado).
2. `enableAnalog` (InputVolumeController) e `enableDigital` vêm do JS; defaults false / true. Nos logs: analógico desligado, digital ligado.
3. Parâmetros adaptativos e `fixed_gain_db` são **enviados pelo JS** (default = padrão WebRTC se omitidos; nos logs vistos, são iguais ao padrão).
4. Pipeline fora do padrão: processamento interno 48 kHz (padrão 32 kHz), HPF ligado em banda cheia, NS no nível High (padrão Moderate), AEC com fábrica própria (`DiscordEchoCanceller3Factory`), `pre_amplifier` desligado.
5. Diferenças de código no AGC2 em si: nenhuma encontrada (todas as constantes lidas coincidem com o WebRTC aberto conhecido).

## 5. Como aplicar no Kotlin (ficha curta)

Constantes: `FRAME_MS = 10`, `HEADROOM = 5`, `MAX_GAIN = 50`, `INIT_GAIN = 15`, `MAX_CHANGE_DB_S = 6` (-> `STEP = 0.06 dB/quadro`), `MAX_NOISE = -50`, `VAD_THR = 0.95`, `ADJ = 12`, `MIN_DBFS = -90.309`.

```
dbfs(x)           = x > 1 ? 20*log10(x) - 90.309 : -90.309     // x em escala S16 (±32768)

// por quadro de 10 ms, p = prob. de fala (RNN VAD ou outro; só vale p >= 0.95)
levelEst: ini = clamp(-20 - INIT_GAIN - HEADROOM, -90, 30)       // = -40
  se p >= 0.95: n++;  leak = (tempoConf > 0) ? 1.0 : 0.9975;  tempoConf = max(0, tempoConf-10) // começa em 400
                num = num*leak + rms_dbfs*p;  den = den*leak + p
                se n >= 12: speech_dbfs = clamp(num/den, -90, 30)
  senão: (descartar o que veio de rajadas < 12 quadros; confirmar as >= 12); n = 0
  confiável = tempoConf == 0

margem (protetor): ini 20; só com p >= 0.95; maxPico = max(maxPico, pico_dbfs);
  a cada > 400 ms: fila4.push(maxPico); maxPico = -90.31
  ref = fila4.vazia ? maxPico : fila4.maisAntigo
  delta = ref - speech_dbfs
  margem = (delta > margem) ? 0.998849*margem + 0.001151*delta : 0.99977*margem + 0.00023*delta
  margem = clamp(margem, 12, 25)

ruido (dBFS): energia por quadro = soma(x^2); piso = (taxa/100)*4 (RMS 2.0 S16 = -84.29 dBFS);
  janela de 500 quadros: prelim = min(energias); ao fim: ruido = (ruido < prelim) ? 0.5*(ruido+prelim) : prelim
  ruido_dbfs = 10*log10(ruido/n) - 90.309

alvo:
  entrada = speech_dbfs + margem
  g = (entrada < -(HEADROOM+MAX_GAIN)) ? MAX_GAIN
    : (entrada < -HEADROOM)            ? -HEADROOM - entrada : 0
  g = min(g, max(MAX_NOISE - ruido_dbfs, 0))
  se !confiável && envLimitador_dbfs > -1: g = min(g, max(-1 - (envLimitador_dbfs - ganhoAtual), 0))

passo:
  se p < 0.95: contador = 12 senão se contador > 0: contador--
  maxUp = (contador == 0) ? 0.06 : 0;   se contador acabou de chegar a 0: maxUp = 12*0.06
  d = g - ganhoAtual;  se maxUp == 0: d = min(d, 0)
  d = clamp(d, -0.06, +maxUp);   ganhoAtual += d     // começa em INIT_GAIN = 15
  amostra *= 10^(ganhoAtual/20)  (rampa linear no quadro do ganho antigo p/ o novo)

saída: ganho fixo (0 dB) -> limitador:
  nível = max|x| por 0.5 ms, subida instantânea, queda 0.997126/subquadro (~-50 dB/s)
  ganhoLim = 1 até -0.75 dBFS; joelho até ~+0.1 dBFS; limiter ~5:1; satura em 0 dBFS (tabela 3.7);
  interpolar o ganho linearmente por amostra e fazer clip em ±1.0
```

Limites de segurança dos parâmetros (iguais ao `Validate`): `headroom >= 0`, `max_gain > 0`, `initial_gain >= 0`, `max_change > 0`, `max_noise <= 0`, `0 <= fixed < 50`.

Efeito prático com os defaults: o ganho tenta levar o **pico da fala a -5 dBFS** (RMS de fala em torno de -25 dBFS, já que a margem fica em 12..25 dB), sobe no máximo 6 dB/s e só depois de 120 ms de fala contínua com p >= 0.95, nunca amplifica o ruído acima de -50 dBFS e nunca passa de +50 dB.

## 6. Pendências e incertezas

- Valores efetivamente enviados pelo JS do Discord em outras contas/experimentos: não observáveis no `.node` (nesta máquina, sempre os defaults).
- Ordem exata dos campos de `InputVolumeController::Config` (DWARF não traz o tipo): faixa -30/-12/-50/100 marcada como BAIXA.
- Rampa do `GainApplier` e clip do limitador não foram lidos linha a linha (`0x85b9e0`, parte de `0x84a720`); seguem o WebRTC aberto.
- Não validei com execução (o Discord não foi iniciado nem o binário executado); a única "execução" usada são os logs que o Discord já gravou.
