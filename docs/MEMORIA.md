# Memória do app desktop: onde está e o que dá para cortar

Medido em 2026-11-07 no app aberto (release, wgpu, driver NVIDIA 580, ~25 min de uso, uma conversa com imagens).

| Medida | Valor |
|---|---|
| RSS | 296 MB (pico 416 MB) |
| **PSS** (o que ele de fato custa, descontando páginas compartilhadas) | **226 MB** |
| memória anônima (heap + mapeamentos) | 115 MB |
| arquivos mapeados (binário + bibliotecas) | 111 MB de PSS |
| VRAM | 203 MiB |
| Threads | 13 |

Para onde vai o PSS: `[anon]` 78 MB, `[heap]` 21 MB, o próprio binário 21 MB (42 MB em disco, com símbolos), `/dev/nvidiactl`
20 MB, `libcuda` 17 MB, `libLLVM` 17 MB, `libnvidia-rtcore` 11 MB, `libnvidia-gpucomp` 11 MB, `libnvidia-glcore` 7 MB.

## O que isso diz

1. **O app em si (heap Rust) é pouco: ~21 MB.** O resto é driver gráfico e bibliotecas.
2. **`libLLVM` (17 MB) e as threads `[vk…]`** vêm do carregador do Vulkan, que enumera *todos* os drivers instalados
   (`/usr/share/vulkan/icd.d/` tem lavapipe, asahi, intel, radeon…). O app só precisa do ICD da NVIDIA.
3. **`libcuda` + `libnvcuvid` + `libnvidia-encode` estavam carregadas** e havia uma thread `cuda…`: o encoder NVENC já tinha sido
   usado nesse processo. As bibliotecas nunca descarregam (dlopen) e o contexto CUDA custa de ~100 a 200 MiB de VRAM
   enquanto existe. Não dá para saber, só por esse processo, quanto disso é a transmissão; precisa medir um app recém-aberto.
4. `Images` (`app/src/files.rs`) é um `HashMap` **sem limite**: cada imagem vista fica como textura para sempre. Com
   miniaturas de ≤1024 px cada uma custa ~3 MB de VRAM; 100 imagens são 300 MB. Também sem limite: `Texts` e o histórico por canal.
5. Pico de 416 MB contra 296 MB atuais: o pico vem de decodificar imagens grandes (uma foto de 12 MP vira 48 MB de RGBA, mais o
   arquivo). As miniaturas do servidor (commit 8472c5e) já cortam isso para ≤3 MB por imagem.

## O que a pesquisa na internet achou (e o que vale)

- Relato de um app egui que foi de 135 MB para 30 MB trocando para renderização por CPU: boa parte do RSS é o driver gráfico
  mapeando os próprios buffers no processo ([dev.to](https://dev.to/trystan_sarrade/how-i-took-my-rust-gui-from-135-mb-to-30-mb-by-ditching-the-gpu-2i7d)).
  Comentários ali dizem que `glow` usa bem menos que `wgpu`, **sem medição**.
- glibc: arenas por thread fragmentam e seguram RSS; `MALLOC_ARENA_MAX=2` e `malloc_trim` ajudam em alguns casos, em outros
  piora, e jemalloc pode até aumentar o RSS. Tem que medir no nosso caso
  ([discussão na glibc](https://inbox.sourceware.org/libc-help/7a0294c9-1ff0-95fb-0231-4366ba014c4a@gmx.net/T/),
  [relato com Lance](https://gh.r-mj.com/lance-format/lance/issues/7242)).
- `strip`, LTO e `codegen-units=1` encolhem o arquivo, não o RSS de heap
  ([min-sized-rust](https://github.com/parasew/min-sized-rust)). Ganho esperado aqui: só os ~21 MB do mapeamento do binário, e menos.
- Não achei benchmark de `glow` contra `wgpu` com NVIDIA no Linux, nem guia oficial de memória do eframe.

## Plano, do mais barato ao mais caro (ganho esperado = estimativa, medir antes de aceitar)

| # | Ação | Esforço | Ganho esperado |
|---|---|---|---|
| 1 | Limitar o Vulkan ao ICD da NVIDIA (`VK_LOADER_DRIVERS_SELECT=nvidia_icd.json` ou `wgpu` só com o adaptador discreto) | 5 linhas + teste | ~17–25 MB (libLLVM, threads vk) |
| 2 | LRU nas texturas de `Images` (teto de ~64 MB) e em `Texts`; descartar textura ao sair do canal | médio | evita crescimento sem limite (centenas de MB em sessão longa) |
| 3 | Descarregar o contexto CUDA/NVENC ao parar a transmissão e só carregar ao transmitir | médio (cudarc) | **descartado**: um app recém-aberto, sem CUDA, já usa ~200 MiB de VRAM (é o wgpu/Vulkan); o CUDA só entra ao transmitir |
| 4 | `MALLOC_ARENA_MAX=2` no `main` (via `mallopt`) e `malloc_trim` ao trocar de canal | pequeno | 10–30 MB de `[anon]`, a medir |
| 5 | Histórico por canal: manter só as últimas N mensagens de canais não abertos | médio | depende do uso |
| 6 | `strip = true`, `lto = "thin"`, `codegen-units = 1`, `panic = "abort"` no release | trivial, build mais lento | ≤10 MB de arquivo mapeado |
| 7 | Trocar `wgpu` por `glow` (feature de eframe) | médio; muda o comportamento no Windows | desconhecido (anedótico); medir antes |

Como medir (sem tocar na tela do usuário): abrir o app novo, deixar 2 min parado, e rodar
`grep -E '^(Rss|Pss|Pss_Anon|Pss_File)' /proc/<pid>/smaps_rollup` e `nvidia-smi --query-compute-apps=pid,used_memory --format=csv`;
repetir depois de abrir um canal com 20 imagens e depois de transmitir 30 s. Em Xvfb o wgpu cai no lavapipe (CPU), então
o número **não representa** o da tela real: a medição precisa ser na sessão do usuário, com o OK dele para reabrir o app.

## Resultado (feito em 2026-11-07, `app/src/memoria.rs` e `app/src/files.rs`)

A/B no mesmo binário, na sessão real (`:0`), 75 s depois de abrir, conta do usuário logada. `TELACAST_SEM_AJUSTES=1` desliga os ajustes.

| | RSS | PSS | PSS anônimo | PSS de arquivos | bibliotecas gráficas extras |
|---|---|---|---|---|---|
| sem ajustes | 192 MB | 123 MB | 36 MB | 88 MB | libgallium, libLLVM, lavapipe |
| **com ajustes** | **122 MB** | **89 MB** | 24 MB | 66 MB | nenhuma |

Menos 34 MB de PSS (−27%) e 70 MB de RSS logo na abertura. VRAM não mudou (200 MiB).

O que entrou:
- **Vulkan só com drivers das GPUs que existem** (`VK_LOADER_DRIVERS_DISABLE` montado a partir de `/sys/class/drm` e dos manifestos
  instalados; não mexe se a variável já existe ou se nenhum driver serve). Tirou lavapipe, nouveau, radeon, asahi e a libLLVM.
- **wgpu só com Vulkan/DX12/Metal** (sem GL, que carregava EGL/Mesa só para listar adaptadores). `WGPU_BACKEND` do usuário vence.
  Consequência: Linux sem Vulkan deixa de abrir; use `WGPU_BACKEND=gl`.
- **`mallopt(M_ARENA_MAX, 2)`** no começo do `main` (efeito só aparece em sessão longa).
- **Cache de imagens com teto de 96 MiB**: as texturas menos recentes e fora da tela (não pedidas nos últimos 120 quadros) saem;
  quem está visível nunca sai. Se a imagem for pedida de novo, baixa a miniatura outra vez. 4 testes.

Achado que muda o plano: o mesmo app, depois de ~25 min de uso, tinha 115 MB de memória anônima contra 36 MB recém-aberto. **O crescimento
em sessão longa (~80 MB) é o problema maior**, e vem de cache sem limite e de fragmentação, não do driver. O teto de imagens e o
`M_ARENA_MAX` atacam isso, mas só uma sessão longa mostra o efeito: reabrir o app depois de algumas horas e comparar com os 115 MB.
Ainda sem limite: histórico de mensagens por canal e `Texts`.

## Notificações do sistema (2026-11-07)

`app/src/notif.rs` + `notif_os.rs` (ver `docs/NOTIFICACOES.md`). A/B com o mesmo binário-base, 75 s depois de abrir, Xvfb/lavapipe (só o delta vale):
PSS 80,1 MB sem a funcionalidade, 80,4 MB com ela, 80,8 MB depois da primeira notificação (thread `notif` + executor do zbus, +2 threads).
Binário Linux +1,04 MB (o zbus já estava no binário pelo `accesskit_unix`). O `Limiter` guarda um `HashMap` por canal e uma fila de no máximo 10
instantes: limitado. Nota: as primeiras medições do dia deram ~120 MB de PSS nos dois binários (não identifiquei a causa; logo depois voltou a ~80 MB nos dois); o par intercalado acima é o confiável.

## Voz: DSP novo e cancelamento de eco opcional (2026-11-07)

`app/src/voice_dsp.rs` (passa-alta, pré-AGC, AGC2, esticador de tempo) não traz dependência. A única dependência nova é `sonora` 0.3
(AEC3 do WebRTC em Rust puro, sem C/C++) para o cancelamento de eco, **desligado por padrão**; o estado dele só existe enquanto a
opção está ligada. A/B no mesmo Xvfb, logado, numa chamada com um remoto simulado, 60 s depois de abrir (PSS):

| | PSS | Pss_Anon | RSS | threads |
|---|---|---|---|---|
| binário de antes (2 rodadas) | 103,0 e 103,0 MB | 31,4 e 31,6 MB | 145 MB | 23 |
| binário novo, eco desligado (2 rodadas) | 102,3 MB (e um valor atípico de 141 MB) | 31,4 MB | 145 MB | 23 |
| binário novo, eco ligado | 103,8 MB | 31,9 MB | 146 MB | 23 |

Sobre o valor atípico: o app às vezes sobe para ~140 MB de PSS (46 MB anônimos) em vez de ~103 MB; apareceu também no binário de antes
(140,7 MB na primeira medição), então não é desta mudança (é o mesmo efeito das medições "~120 contra ~80 MB" de cima, que continua sem causa
identificada). O custo do eco ligado é **~1 MB de PSS e ~0,5% de um núcleo**; o dos buffers por usuário remoto (jitter até 48 pacotes,
esticador de ~7 KB) é desprezível. Binário: +0,85 MB no Linux (+0,71 MB só do `sonora`) e +0,83 MB no Windows.
