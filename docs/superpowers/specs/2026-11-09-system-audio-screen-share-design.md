# Áudio do sistema no compartilhamento de tela

## Objetivo

Permitir que quem assiste a uma transmissão ouça também o áudio reproduzido pelo dispositivo de quem transmite. O áudio do sistema é uma faixa separada do microfone e do áudio de voz já existente no Telacast.

## Escopo

- Windows: capturar o áudio do endpoint de saída por WASAPI loopback.
- Linux: capturar o monitor do sink de saída selecionado usando PipeWire.
- Android: adicionar a captura de áudio ao compartilhamento de tela do app Android quando esse fluxo for implementado.
- iOS fica fora do escopo.
- O microfone continua no caminho atual de voz. A nova faixa não deve misturar, duplicar nem alterar o áudio do microfone.

## Experiência

- A tela de transmissão terá a opção **Compartilhar áudio do sistema**, desligada por padrão.
- O app informa quando a captura não está disponível ou foi negada e mantém a transmissão de vídeo funcionando.
- O áudio capturado será reproduzido por quem está assistindo à transmissão, mesmo que essa pessoa não esteja usando o microfone.
- O espectador poderá silenciar essa faixa sem sair da transmissão.

## Arquitetura

### Captura

Cada plataforma implementa uma fonte de captura de áudio que entrega PCM em um formato comum ao pipeline de Opus. A captura deve usar o endpoint de saída selecionado, ou permitir escolher um endpoint caso o sistema tenha mais de um.

- **Windows:** WASAPI em loopback compartilhado no endpoint de renderização. A documentação do Windows descreve a captura do fluxo reproduzido pelo endpoint e as limitações de conteúdo protegido: [Loopback Recording](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording).
- **Linux:** capturar os ports de monitor do sink PipeWire; a implementação deve enumerar sinks e apresentar a saída selecionada. A documentação do PipeWire descreve streams de captura que recebem os ports de monitor: [Protocol Simple](https://docs.pipewire.org/page_module_protocol_simple.html).
- **Android:** usar `MediaProjection` e `AudioPlaybackCapture` com `AudioRecord`. O Android requer consentimento do usuário e `RECORD_AUDIO`; a captura só inclui apps que permitem playback capture e respeita os limites de perfil/usuário: [Capture video and audio playback](https://developer.android.com/media/platform/av-capture).

### Codificação e transporte

- Codificar a faixa do sistema em Opus, reutilizando o codec presente no projeto.
- Transportar áudio e H.264 na sessão da transmissão de tela, com identificação explícita do tipo de pacote e timestamps monotônicos para sincronização.
- O servidor valida os pacotes e retransmite as faixas sem decodificar nem misturar áudio.
- O cliente espectador decodifica Opus e H.264 em caminhos independentes e apresenta o áudio sincronizado com os timestamps dos quadros.
- O relay existente de voz continua separado e não transporta essa faixa; isso evita exigir que o espectador entre no canal de voz e evita misturar a transmissão do sistema com o microfone.

### Aplicativo Android

O repositório contém um plano de implementação Android, mas ainda não contém o aplicativo Android. A captura de áudio será integrada junto ao compartilhamento de tela Android, com o fluxo de consentimento do sistema e serviço de captura em primeiro plano já previstos no plano mobile.

## Alternativas consideradas

1. **Usar o relay de voz existente:** reaproveita o caminho atual, mas só chega a pessoas conectadas à voz e mistura conceitos de áudio de sistema e microfone. Não recomendado.
2. **Faixa Opus sincronizada no stream de tela:** permite que qualquer espectador receba som com o vídeo e oferece sincronização explícita. Recomendado.
3. **WebSocket separado para áudio:** mantém faixas independentes, mas adiciona conexão, ciclo de vida e sincronização entre sockets sem benefício suficiente para esta versão.

## Fases

1. Definir o enquadramento binário versionado de áudio/vídeo, timestamps, validação e descarte seguro no servidor; implementar recepção e reprodução no cliente desktop.
2. Implementar captura WASAPI e PipeWire, seletor de saída, opção desligada por padrão e controles de mute/volume para espectadores.
3. Integrar a captura Android ao app e ao fluxo `MediaProjection` quando o app Android e a transmissão de tela mobile forem implementados.

## Critérios de aceite

- Com a opção desligada, nenhuma captura de áudio do sistema é aberta nem enviada.
- Com a opção ligada e permitida, o espectador ouve o áudio do sistema e não recebe uma segunda cópia do microfone.
- A sincronização áudio/vídeo permanece estável durante a transmissão; atraso e perda de pacotes não acumulam fila indefinidamente.
- Silenciar áudio no espectador não encerra nem reinicia a transmissão de vídeo.
- Ausência de permissões, endpoint indisponível, conteúdo protegido ou app Android que bloqueie captura gera estado claro e não derruba vídeo ou voz.
- Windows e Linux são entregues primeiro; Android é implementado em conjunto com a base de transmissão de tela mobile. iOS não será incluído.

## Riscos e decisões pendentes para o plano de implementação

- Escolher e validar bibliotecas/FFI de captura para WASAPI e PipeWire sem afetar os callbacks em tempo real.
- Definir limites de tamanho, sequência/timestamp, sincronização inicial e recuperação após perdas no enquadramento de pacotes.
- Confirmar o comportamento da captura PipeWire em sessões Wayland e em saídas que não exponham monitor.
- Respeitar as políticas de captura Android: áudio protegido ou apps que desabilitam playback capture não poderão ser transmitidos.
