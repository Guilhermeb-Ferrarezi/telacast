# 08 - Voz, chamada e compartilhamento de tela (Go Live)

Área: painel "Voz conectada", visão do canal de voz (grade de participantes), tile de transmissão, assistir/parar de assistir, visão focada, tela cheia, picture-in-picture (PiP), controles sobrepostos com auto-ocultar, diálogo de transmitir (fonte e qualidade), espectadores, volume.

Legenda: **[F]** confirmado no cliente do Discord (arquivo + trecho/constante), **[I]** inferência (conhecimento do produto + o que o código sugere), **[?]** não consegui confirmar.
Fonte: `W` = `discord-bundle/web.0bcf9a9e83b2c3c7.js` (o maior). `@NNNN` = deslocamento em caracteres dentro de `W` (para achar de novo com `python3 -c "print(s[N:N+800])"`). `(x3)` = a mesma constante também está em `web.3f3154a164805d56.js` e `web.72656029554c0370.js`. Sem `(x3)`, só li em `W`. Textos pt-BR vêm dos arquivos de idioma do bundle (`12064a7d28fc7eb1.js`, `7d905c726a597bc4.js`, `ee76620bd990acfa.js`), indexados por chave de 6 caracteres (ex.: `q3O3J8`).

**Limite da evidência (importante).** O cache do bundle não traz os pedaços carregados sob demanda: o CSS dos componentes (só há o CSS de tokens de cor), o componente da grade de vídeo da chamada, o do tile de participante e o modal "Transmitir" (módulo 826789) estão ausentes. Por isso: **a lógica** (ordem, seleção, limites, tempos, qualidade, estados e textos) é [F]; **as medidas em px da grade, do tile e do modal** são [I] e estão marcadas assim. Onde o código dá número real (PiP, avatares, anel de fala, limiares), ele está citado.

---

## 1. Resumo e fora de escopo

O Telacast já transmite a tela por canal: captura X11, H.264 por NVENC (fallback openh264), relé WebSocket no servidor, uma transmissão por canal, faixa "X está ao vivo" com Assistir/Fechar, vídeo acima do chat, tela cheia por duplo clique. Não existe voz: `Channel` só tem `id` e `name`, não há áudio, nem estado de voz, nem número de espectadores, nem metadados da transmissão (resolução/fps), nem seletor de qualidade além de fps e Mbps soltos.

O padrão do Discord que vamos copiar é: **a transmissão é um participante da chamada** (tile especial na grade do canal de voz); quem não assiste vê um tile com botão de entrar; quem assiste ganha a **visão focada** (vídeo grande + faixa dos outros participantes), com **controles sobrepostos que somem em 1,8 s sem mexer o mouse**, espectadores (avatares + contagem), volume, tela cheia, PiP ao sair do canal, e um **seletor de qualidade em dois grupos (Resolução x Taxa de quadros)**. O painel "Voz conectada" fica acima do cartão do usuário e traz qualidade/ping, desligar, câmera e tela.

O plano evolui o que existe em passos pequenos: primeiro metadados + espectadores + controles sobrepostos na transmissão atual (sem voz), depois o diálogo de qualidade, depois canais de voz (presença, sem áudio) com grade e tile de transmissão, depois PiP, e por último o áudio (que destrava anel de fala real, volume e áudio da transmissão).

**Fora de escopo:** captura/codificação de áudio e transporte (cpal/Opus/jitter, doc `docs/discord/04-voz-video.md` §3), E2EE, palco (Stage), atividades incorporadas, soundboard, clipes, efeitos de câmera, Krisp/supressão de ruído, mensagens de voz, permissões finas (mover/silenciar membros: só citado), monetização (níveis de impulso: só como modelo de limites), mobile, janela de depuração RTC além do popover de ping.

---

## 2. Árvore de componentes e medidas

Convenções de medida: px lógicos do egui; "Grafite" = camadas L0..L4 de `theme.rs`. Tudo que o Discord pinta em verde/vermelho de marca vira: verde = `OK` (só "falando" e "conectado"), vermelho = `RED`, destaque/ativo/"ao vivo" = `AMBER`. Sem gradientes: onde o Discord usa degradê escuro sobre o vídeo, usamos faixa sólida `alpha(BLACK, 0.55)`.

### 2.1 Painel "Voz conectada" (dentro de `channel_panel`, acima do cartão do usuário)

Aparece só quando há conexão de voz. Coluna do painel tem 240 px, margem interna 10 => conteúdo 220 px.

```
VozPanel (Frame L2, raio 10, stroke LINE 1px, margem interna 8, gap vertical 6)   [I]
├─ linha 1 (altura 36)
│  ├─ SinalConexão (16x16)  barras 3px de largura, gap 2, alturas 5/9/13           [I]
│  │     cor por qualidade: FINE=OK, AVERAGE=AMBER, BAD=RED, UNKNOWN=MUTED          [F cores por nível]
│  ├─ coluna de texto (flex 1, gap 0)
│  │   ├─ status: "Voz conectada"  13 px, cor por estado (tabela 3.1)               [F texto]
│  │   │          "Vídeo conectado" quando há câmera/transmissão no canal            [F getStatus(state, hasVideo)]
│  │   └─ subtítulo: "{canal} / {servidor}"  11 px MUTED, 1 linha, truncado          [F body-xs/medium, lineClamp 1; "a / b" montado em bX]
│  └─ botão Desligar 32x32, raio 8, L3 (hover L4), glifo "telefone desligado" TEXT2 (hover RED)   [F existe, tooltip "Desconectar"; medidas I]
├─ linha 2 (altura 32, 2 botões iguais de (220-16-6)/2 = 99 px, gap 6)              [I]
│  ├─ Câmera   (glifo + rótulo "Vídeo")   aria-pressed = câmera ligada               [F rótulo "Vídeo" (FlNoSV); rótulo só aparece sem os outros botões]
│  └─ Tela     (glifo + rótulo "Tela")    aria-pressed = estou transmitindo           [F rótulo "Tela" (r0/+v7)]
└─ linha 3 (opcional, "mostrar participantes no painel")                              [F setting l=eO.vL.useSetting()]
   └─ fileira de avatares 24 px, slot de 28 (24 + gap 4), quebra de linha             [F: Math.floor((largura+4)/28)]
```

- No Discord os botões da linha 2 são, em ordem: câmera, transmitir (tela), atividades, soundboard (`b1` em @1705051). Telacast só tem os dois primeiros. Ao lado do desligar o Discord põe o botão de supressão de ruído (Krisp) quando suportado [F `b0`], fora de escopo.
- Botão ativo (`aria-pressed`): fundo `alpha(AMBER,0.16)`, stroke AMBER 1px, glifo AMBER. Desativado: opacidade 0,4 (Discord: câmera some/desativa sem permissão; tela desativa em canal onde não pode transmitir, tooltip "Você não pode transmitir para esse canal.") [F].
- Tooltip da tela: "Compartilhar a tela" / "Compartilhamento de tela indisponível" / "Você não pode transmitir para esse canal." [F `T` em bz].
- Clique no botão Tela: se não estou transmitindo e ninguém transmite => abre o diálogo Transmitir (2.9). Se estou transmitindo **ou** há outras transmissões no canal => abre o menu "Gerenciar transmissões" (2.10) [F: `m=null!=I||p.length>0 ? n(e) : t()`].
- Clique no sinal/status abre o **popover de conexão** (2.1.1).
- Lista de participantes (linha 3): mostra os outros (não eu), um avatar por pessoa; se `outros > cabem`, mostra `cabem-1` avatares + botão "+" 24x24 (expande) e, expandido, todos + botão "-" [F `bk`: `n(Math.max(1, e>i ? i-1 : i))`]. Cada avatar tem anel de fala (inset, máx. 3) e, se mudo/surdo, ícone pequeno por cima (2.4.3). Tooltip "{nome}, {estado}" [F `1+MVBP`]. Com 220 px: `floor((204+4)/28)` = 7 por linha.

#### 2.1.1 Popover de conexão (clique no sinal)
Largura ~280, margem 12, L3, raio 10 [I]. Conteúdo [F `U` @7899400..7903000]:
- Gráfico de pings (últimos 20 valores) 258x80 [F `dataPoints:e.pings,width:258,height:80`], linhas de grade LINE, curva TEXT2, último ponto na cor da qualidade.
- Nome curto do servidor de voz (`hostname.split(".")[0]`) [F `getShortHostname`].
- "Ping médio: N ms", "Último ping: N ms", "Perda de pacotes de saída: X%" [F três linhas; textos exatos ausentes do idioma, [I] redação].
- Rodapé 11 px MUTED: "Ping acima de 250 ms ou perda acima de 10% é considerado ruim." [F `badPing:250,badLossRate:10`].
- Selo de segurança "Conexão segura" com cadeado [F `ETIVvg`]. (E2EE não existe no Telacast: mostrar só "Conexão segura" quando TLS.)

### 2.2 Cartão do usuário com mudo/surdo (dependência da voz)

O cartão atual tem avatar, nome, "Online" e "Sair". Para voz, acrescentar (Discord: microfone, fone, engrenagem):
- Botão microfone 32x32 e botão fone 32x32, raio 8, L3/L4; desligado = glifo com **barra diagonal RED** 2px (desenhada com `painter.line_segment`) e fundo `alpha(RED,0.14)`.
- "Sair" sai do cartão (vai para configurações; outro agente).
- Atalhos [I]: Ctrl+Shift+M (mudo), Ctrl+Shift+D (surdo), só com a janela focada. Ensurdecer também muta o mic (semântica do Discord [I]); desensurdecer devolve o estado anterior do mic [I].
- Rótulos [F]: "Desativar microfone"/"Ativar microfone" (`Fx/4wY`/`QbFzMx`), "Desativar áudio"/"Ativar áudio" (`wjcRFX`; "áudio" = fone).

### 2.3 Canal de voz na lista de canais (gancho; a lista é de outra área)

```
Seção "CANAIS DE VOZ" (caps 11 px MUTED)
└─ Linha do canal de voz (list_row, altura 32)  glifo alto-falante + nome   clique = entrar (e abrir a visão de voz)   [I]
   └─ participantes (um por linha, recuo 20 px)                                                                      [I]
      avatar 24, nome 14 px, à direita: ícones de estado 16 px e etiqueta "AO VIVO" (AMBER, 10 px caps) se está transmitindo
      anel de fala INSET 3 px [F `maxInnerSpreadRadius:3` em bx/lista lateral]
```
Prioridade do ícone de estado (um só): surdo-pelo-servidor > surdo > mudo-pelo-servidor > mudo-local ("mutado por você") > mudo [F `bG`/seletor em bx @1688700]. Variantes "pelo servidor" em RED; as demais em MUTED [F `colorize:true` só em serverDeaf/serverMute].

### 2.4 Visão do canal de voz (área central quando o canal aberto é de voz)

```
VozView (CentralPanel, fundo L1)                                                  [I fundo]
├─ Cabeçalho (altura 63, igual ao chat)  glifo alto-falante 18 + serif(nome, 24)
│   direita: [Grade|Foco] [Tela cheia] [Chat]                                      [F botões FOCUS/GRID/FULL_SCREEN/CHAT no enum OP @3322400]
├─ Palco (flex 1, margem interna 16, gap entre tiles 8)                            [I]
│   ├─ modo GRADE:  tiles de todos os participantes, proporção 16:9, linha incompleta centralizada
│   └─ modo FOCO:   tile selecionado grande + faixa de miniaturas embaixo (2.5)
└─ Barra de controles (altura 72, centralizada)                                    [I]
    [Câmera] [Tela] [ ... ] [Desligar]   botões 44x44 raio 12 gap 8; Desligar 56x44, fundo RED
```
- Layout da área: o Discord tem 4 modos de layout por canal: `normal` (com chat), `minimum`, `no-chat`, `full-screen` [F enum @511927]. Canal de voz de servidor abre em **no-chat** por padrão; DM em `normal` [F `getLayout` @3286000: `i?NO_CHAT:NORMAL`, `isGuildVocal()`]. Telacast: voz abre sem chat; botão Chat abre o chat do canal ao lado (painel de 360 px) [I medida].
- Modo da chamada: `VIDEO` se o canal é de voz de servidor **ou** há stream/câmera/atividade; `VOICE` (lista simples) só em DM sem vídeo [F `getMode`, `et()`]. Telacast: sempre a grade em canal de voz.

#### 2.4.1 Cálculo da grade (n tiles) [I algoritmo; F proporção de vídeo 16:9 do bundle: `S=16/9` @6120700]
Entrada: área útil `W x H` (palco menos margem), `n`, gap `g=8`, proporção `a=16/9`.
Para `cols` em `1..=n`: `rows=ceil(n/cols)`; `w = min((W-(cols-1)g)/cols, ((H-(rows-1)g)/rows)*a)`; escolher o `cols` que **maximiza w**. Tile = `w x w/a`. A grade inteira é centralizada na vertical; **a última linha incompleta é centralizada na horizontal**. Largura mínima de tile 160 [I; PiP-câmera mínimo é 160, F]: se `w<160`, trocar para `cols=floor((W+g)/(160+g))` e rolagem vertical. Com `n=1` o tile é limitado a 1280 de largura [I]. Exemplos esperados (1280x720 útil): n=1 -> 1x1; n=2 -> 2x1; n=3 e 4 -> 2x2 (n=3 com a última linha centralizada); n=5 e 6 -> 3x2; n=7..9 -> 3x3.

#### 2.4.2 Tile de participante sem vídeo
- Fundo: tom determinístico do usuário (`user_tint(id)` já existe em `theme.rs`), cantos 8 [I].
- Avatar centralizado, diâmetro `clamp(altura_tile*0.44, 40, 128)` [I; no PiP o avatar é 80 px com tile de 320x180, razão 0,44, F `SIZE_80` em @8885400].
- Pílula do nome: canto inferior esquerdo a 8 px das bordas, altura 22, padding-x 8, raio 6, fundo `alpha(BLACK,0.62)`, texto 12,5 px TEXT; ícones de estado (mudo/surdo) 16 px **dentro da pílula, antes do nome** [I; no PiP o nome usa `body-md/normal` cor "overlay-light", F @8885300].
- Anel de fala (2.4.4). Duplo clique: focar (entrar no modo foco naquele tile) [I; o tile de vídeo recebe `onDoubleClick`, F `712577`].
- Clique direito: menu do usuário com "Volume do usuário" (slider 0 a 200), "Silenciar {nome}" / "Silenciado por você" (só local), perfil [F strings `m7TNdF`, `TRp5wR`, `Q8Uzof`; dica `on980U`: "Clique com o botão direito nos participantes de uma chamada para alterar o volume de cada um deles."].
- Com câmera ligada o vídeo preenche o tile (cobrir/cortar nas bordas) e a pílula continua; sem sinal de vídeo: erro "Erro ao iniciar a câmera" [F `z+mxvo`] com botão "tentar de novo".

#### 2.4.3 Ícone de estado sobre avatar pequeno (painel, lista, faixa)
Avatar com imagem de fundo e, por cima, um glifo `xs` (~12 px) centralizado quando há estado; avatar escurece (`alpha(BLACK,0.45)` sobre a imagem) [I]. Mesma prioridade de 2.3.

#### 2.4.4 Anel de "falando"  [F @3324396, `W`(x3)]
Função de espessura a partir do volume em dB: `r(db, max) = 0.5 + (max - 0.5) / (1 + exp(-0.03 * (db + 45)))`.
Pintura (CSS `box-shadow`): `0 0 0 {n}px verde, inset 0 0 0 {r}px verde, inset 0 0 0 {r+1}px fundo` (anel externo `n`, anel interno `r` e uma linha de 1 px na cor do fundo logo dentro, que separa o anel da imagem). Padrões:
- Avatar de lista/painel: só anel interno (`INSET_ONLY`), `max = 3` (lista/painel) ou `4` (avatar genérico) [F].
- Quando não há medida de volume (`-inf`) ou o usuário reduz animação: anel fixo `r=2`, linha de fundo `3` [F `n=0,r=2,a=3`].
- Cor: "falando" = verde de status; soundboard usa a cor da marca; modo contraste forçado usa `Highlight` [F].
- Transição: `box-shadow 50ms ease-out`, só com o app focado [F].
- Existe um experimento "indicador proporcional ao volume" (`2025-12-proportional-vad-indicator`) que liga/desliga a variação por dB [F @3290800]. Ou seja, o anel que "pulsa" é opcional; o fixo é o básico.

Valores calculados (avatar, max 3): -90 dB -> 1,0 px; -45 -> 1,75; -20 -> 2,2; 0 -> 2,5 (3 px nunca é atingido).
No tile grande (16:9) usar o mesmo traço escalado: espessura `r*1.5` clampada em [2, 4] e raio = raio do tile [I].

#### 2.4.5 Tile de transmissão ("Go Live")
Mesmo tamanho dos outros tiles. Por estado (todos [F] salvo medidas):
```
Tile de transmissão
├─ fundo: quadro de vídeo (contain, barras L0) ou prévia ou avatar do transmissor
├─ etiqueta "AO VIVO" (top-left, 8 px de margem): AMBER sólido, texto AMBER_INK 10,5 px caps, altura 20, raio 5, ponto 6 px    [I; igual ao .tag do grafite.html]
├─ chip de espectadores (top-right): olho 12 px + N, fundo alpha(BLACK,.62), altura 22, raio 6                                  [F ícone olho + contagem em compact `M`]
├─ pílula do nome (bottom-left): "{nome}" (com glifo de monitor)
└─ corpo por estado (centro)
```
Corpo por estado (ver 3.2 para textos):
1. **Não assistindo** (`streamId` nulo): prévia (imagem) ou avatar + botão primário **"Assistir"** [F: Discord "Entrar na transmissão" (`sTERoL`); o Telacast já usa "Assistir"] e, se houver razão de bloqueio, o botão fica desativado com o motivo [F `711589`: "O canal está cheio", "Nenhuma permissão", "Idade restrita"].
2. **Conectando**: "Aguardando vídeo…" (já existe).
3. **Assistindo**: quadro de vídeo.
4. **Reconectando**, **Pausada**, **Terminou**, **Falhou**, **Oculta** (3.2).
Tamanho da tela de mensagem por largura do tile: `<400` pequena, `<1000` média, senão grande [F `c(e)` @8877207]; tile de "terminou": `<550` pequena [F]; tile compacto `<195` esconde o título e usa botão `sm` e, `<175`, texto curto no botão [F `m=n<195`, `n<175`].

### 2.5 Visão focada (assistindo uma transmissão ou olhando uma pessoa)

```
Foco
├─ Palco principal (flex 1): tile selecionado, vídeo "contain" centralizado, fundo L0
│     controles sobrepostos (2.6) por cima
└─ Faixa de miniaturas (altura 112 + padding 8, gap 8; tile 200x112)           [I]
      todos os outros participantes na ordem de 4.1, rolagem horizontal; botão recolher (chevron 24x24)
      recolhida: some (state `participantsOpen`)                                 [F `getParticipantsOpen`, padrão true]
```
- Entrada no foco: clicar em "Assistir"; duplo clique num tile; ou automático (4.2).
- Voltar à grade: botão "Grade" no cabeçalho, ou Esc se estiver em tela cheia primeiro, depois grade [I].
- Filtro "esconder quem está sem vídeo" (`voiceParticipantsHidden`): a faixa/grade mostra só quem tem câmera/transmissão; botão "Mostrar participantes sem vídeo" [F store `j[e]`; texto `hoZYAA` e aviso `8eBJ73` "Os participantes sem vídeo estão ocultados. Deseja exibi-los?"].

### 2.6 Controles sobrepostos do vídeo (focado, tela cheia e PiP)

Estrutura do Discord (componente `k.A` @8961687) [F]:
```
Contêiner do vídeo (onMouseMove/onMouseDown -> ativo; onMouseLeave -> ocioso; duplo clique -> "abrir no teatro"/ir ao canal)
├─ Vídeo
├─ Mensagem de estado (reconectando/pausada/terminou...)  (centro)
└─ Camada de controles (tema escuro; some quando ocioso)
   ├─ topo:  [ícone "abrir no teatro" 20x20 + título do canal]   [seletor de outra transmissão (dropdown)]
   └─ base:  esquerda: nome do participante (1 linha)
             direita:  [espectadores] [engrenagem (só a minha)] [abrir em nova janela] [parar / desligar]
```
Medidas Telacast [I]: barra superior 44 px, barra inferior 52 px, fundo sólido `alpha(BLACK,0.55)` (sem degradê), padding 12, botões 32x32 raio 8 (fundo transparente, hover `alpha(WHITE,0.10)`), gap 4, texto 13 px TEXT.
- Botões da base na visão focada (ordem da direita para a esquerda): **Parar de assistir** (ou **Parar de transmitir** se a transmissão é minha) com hover RED; **Tela cheia / Sair da tela cheia** [F `423562`, strings `OIDkcp`/`Z7MyNB`]; **Abrir em nova janela** ("Transmissão em Nova Janela" `Cm4aDQ`); **Engrenagem** (qualidade, só se é minha) [F `canSettings: i && isSelf`]; **Espectadores**; **Volume** (glifo + slider ao passar o mouse, 0 a 200) [F rótulo "Volume da transmissão" `pEAl4b`/`t4JBnI`].
- Quem não é dono: tem Popout e Parar de assistir; dono: Configurações e Parar de transmitir; no PiP de quem só está na chamada sem transmissão: botão Desconectar no lugar de "parar" [F `viewProperties`: `canPopout=i&&!r, canSettings=i&&r, canStopStream=i, canSeeViewers=i, canDisconnect=!i`].
- **Auto-ocultar:** ver 3.3 (1800 ms).

### 2.7 Espectadores

- Na base do vídeo: até **3 avatares de 24 px** sobrepostos (sobreposição de 6 px [I]) + chip "+N" se houver mais [F `take(A=3)`, `SIZE_24`]. Versão compacta (PiP/tile pequeno): olho + número, tooltip com os nomes [F `M`].
- Passar o mouse abre um popover acima com título **"Espectadores - N"** e lista (avatar 24 + nome, 12 px, linhas de 28) [F `BR7Tno`]. Fecha 150 ms depois de sair [F `new c.J_(150,...)`]. Clique direito num nome abre o menu do usuário e segura o popover aberto [F].
- Aviso de entrada/saída: "Espectador entrou"/"Espectador saiu" (som/toque ao transmissor) [F chaves `KccUI1`/`dsjkiN` existem no idioma].
- Limite de espectadores: ao exceder, "Desculpe, esta transmissão está com o máximo de espectadores." [F `VVZDBL`]. Valor: 50 por canal [doc `04-voz-video.md`, fonte de suporte].

### 2.8 Picture-in-picture (PiP)

Aparece quando estou em chamada e abri outro canal/tela do app, com uma transmissão ou câmera para mostrar [F `ConjureAppPictureInPictures`, `PictureInPictureStore`].
- Largura padrão **320**, mínima **200**, máxima **960** (vídeo); PiP de câmera: padrão **160**, mínima 160, máxima 480 [F `p={0:320,1:160}, m={0:960,1:480}, T={0:200,1:160}` @3274238 (x3)]. Altura = largura*9/16. Largura persistida por tipo [F `pipWidths`].
- Posição: um dos **4 cantos** (padrão inferior direito) [F `CUs.TOP_LEFT..BOTTOM_RIGHT`, `_=BOTTOM_RIGHT`]. Arrastar com inércia: ao soltar, projeta a posição por `pos + 200*velocidade` e **gruda no canto** mais próximo, com animação [F `calculateDecayingPosition`]. Redimensionar pelas bordas, limitado ao min/max e à janela [F `ensureWidth`].
- Quem aparece: a transmissão selecionada; senão a última transmissão ativa; senão quem falou por último **entre os que têm vídeo** (o de menor tempo de fala contínuo, ou seja, quem começou a falar há menos tempo), recalculado com *throttle* de **300 ms** [F `VideoSpeakerStore` @3293500].
- Pessoa sem vídeo: avatar 80 px centralizado.
- Controles: os de 2.6, só os necessários, mais título do canal no topo-esquerdo (clique = voltar ao canal) e, se há mais de uma transmissão para assistir, um botão de **trocar transmissão** (menu "Mude para a transmissão de {usuário}", `sqmFRg`) [F `6155`]. Auto-ocultar em **1800 ms** (3.3).
- Raio 8, sombra de janela, borda `LINE2` [I].

### 2.9 Diálogo "Transmitir" (fonte + qualidade)

Hoje (`Modal::GoLive`): combo de tela, chips de fps 30..144, chips de Mbps 4/8/15/30, "Ao vivo". Alvo (estrutura do modal do Discord, textos [F], medidas [I]):

```
Janela "Transmitir"  (egui::Window, âncora centro, largura 540, L3, raio 12, margem 20)
├─ serif("Transmitir", 22)   subtítulo 12 px MUTED: "Compartilhar uma tela, janela ou aplicativo"          [F XyYoFc]
├─ abas (altura 32): "Telas" | "Janelas"                                                                  [F "Telas" /iX8uz; "Aplicativos" tHoi7u, aqui "Janelas"]
│   grade de miniaturas: 3 por linha, miniatura 16:9 (156x88), gap 12, legenda 12 px 1 linha
│   selecionada: stroke AMBER 2px + legenda TEXT; hover: stroke LINE2
│   legenda de tela: "Tela {n}" ("Tela 1") ou o nome do monitor; janela: título                           [F y/R7n4 "Tela {index}"]
├─ rótulo caps "CANAL DE TRANSMISSÃO": "{canal de voz} / {servidor}" (somente leitura)                    [F KPuWsm]
├─ rótulo caps "QUALIDADE DA TRANSMISSÃO"
│   ├─ "Resolução": 480p | 720p | 1080p | 1440p | Fonte        (5 botões segmentados, altura 32)
│   ├─ "Taxa de quadros": 15 | 30 | 60 [| 90 | 120 | 144 em "Avançado"]   ("{n} FPS")
│   └─ linha de resumo em mono 11,5 px MUTED: "1920x1080 · 60 fps · H.264 NVENC · ~9 Mbps"
├─ checkbox "Compartilhar áudio" (desativado, tooltip "em breve", até existir áudio)                      [F ZJEHt7]
├─ (recolhido) "Avançado": slider de bitrate (Mbps), codificador (NVENC/CPU), ocultar prévia              [I; "Ocultar prévia da transmissão..." F JE73jB]
└─ rodapé: [Cancelar]   [Transmitir] (primário AMBER)                                                      [F UE/rP/ "Transmitir"]
```
Regras [F onde indicado]:
- **Resoluções** `[480, 720, 1080, 1440, Fonte(0)]` e **taxas** `[15, 30, 60]` com rótulos "{n}p" e "{n} FPS"; "Fonte" é `resolution=0` [F `Jk`/`ce` em @5470800, `XjXqzh`="Fonte"].
- **Predefinições** [F]: `PRESET_VIDEO` (prioriza fluidez: tenta 1440p60, 1080p60, 720p60, 720p30, o primeiro permitido), `PRESET_DOCUMENTS` (prioriza nitidez: Fonte a 15 fps, depois 5 fps), `PRESET_CUSTOM` (o que o usuário escolher), `PRESET_AUTO`. No Telacast: chips "Fluidez" e "Nitidez" acima dos grupos; mexer num grupo vira "Personalizado".
- **Padrões** [F `ApplicationStreamingSettingsStore` @9404220]: predefinição Vídeo, 720p, 30 fps, áudio da transmissão ligado. Telacast: padrão `min(Fonte,1080p)` a 60 fps se NVENC, 720p30 se só CPU [I; decisão].
- Em documentos, taxa efetiva é limitada a 30 [F `l===PRESET_DOCUMENTS ? FPS_30 : c`].
- Opções acima da resolução real da fonte ficam desativadas (não faz sentido ampliar) [I].
- Limites por servidor (modelo "nível de impulso"): nível 0 = 720p30; 1 = 720p60; 2 e 3 = 1080p60 (e 1440p/Fonte com assinatura) [F tabela `tl` @7234341: `screenShareQualityResolution/Framerate`]. Telacast: coluna `servers.stream_max_height/fps` (NULL = sem limite); opções acima ficam desativadas com tooltip "O servidor limita a transmissão a 1080p60" [I].
- Nota: o seletor do Discord não mostra bitrate; o Telacast hoje expõe Mbps. Manter só no "Avançado" e derivar o padrão da resolução/taxa (5.4).
- Fonte "Telas" usa os monitores do RandR (já existe `monitor_names`). "Janelas": ver 6.5 (X11 vs Wayland).

### 2.10 Menus

**Gerenciar transmissões** (popover acima do botão Tela quando já transmito ou há outras no canal) [F módulo 848362, `navId:"manage-streams"` @8879411; itens de qualidade/áudio/parar @3317500..3320700]:
- Para cada transmissão alheia: "Parar de assistir — {usuário}" (`7rkg+/`).
- Se transmito: [Qualidade da transmissão (submenu) > grupos "Taxa de quadros" (15/30/60) e "Resolução" (480/720/1080/1440/Fonte), radio], [Compartilhar áudio da transmissão (checkbox)], ["Trocar janela" / "Alterar a Transmissão"], **["Parar de transmitir" (cor de perigo)]**. Ordem quando o seletor do SO é usado: parar, trocar, qualidade, áudio.
- Se não transmito: "Compartilhar a tela".
- Ao mudar a qualidade o Discord reaplica a fonte (`setGoLiveSource`); no Telacast = reiniciar o encoder com novo IDR e novo "hello" (5.2).

**Menu do tile de transmissão (botão direito)** [I com strings F]: Assistir/Parar de assistir, Tela cheia, "Transmissão em Nova Janela", Volume da transmissão (slider), "Convidar para assistir a transmissão" (P3, `wS+5Wb`).

---

## 3. Estados e interações

### 3.1 Estados da conexão de voz (texto, categoria, cor)  [F módulo 917592, `getStatus` @10266593]

| Estado interno | Texto pt-BR | Categoria | Cor do texto |
|---|---|---|---|
| CONNECTING | Conectando | conectando | AMBER |
| AUTHENTICATING | Autenticando | conectando | AMBER |
| AWAITING_ENDPOINT | Aguardando o servidor de voz | conectando | AMBER |
| RTC_CONNECTING | Conectando RTC | conectando | AMBER |
| ICE_CHECKING | Verificando rota | conectando | AMBER |
| DTLS_CONNECTING | Conectando DTLS (Telacast: "Protegendo a conexão") | conectando | AMBER |
| RTC_CONNECTED | Voz conectada / Vídeo conectado | conectado | OK |
| NO_ROUTE | Sem rota | erro | RED |
| RTC_DISCONNECTED | RTC desconectado (Telacast: "Conexão perdida, reconectando...") | erro | RED |
| DISCONNECTED | Desconectado | erro | RED |

Obs.: o idioma pt-BR do Discord traduz "RTC" por "CTR" (erro de tradução): não copiar. Telacast só precisa de: Conectando, Autenticando, Voz conectada, Reconectando, Sem rota, Desconectado.
Passar o mouse sobre o status troca o texto por "Detalhes de Voz"/"Detalhes de Vídeo" quando há canal conectado [F `N`/`IlHdW8`,`WsOisp`].

### 3.2 Estados do tile de transmissão e textos  [F `638480`, `566331`, `768088`, `256195`, `729365`]

| Estado | Mensagem principal | Secundária | Ação |
|---|---|---|---|
| Não assistindo | (prévia/avatar) | motivo se bloqueado | **Assistir** (primário) |
| Conectando | Aguardando vídeo... | | |
| Assistindo | vídeo | | |
| Reconectando | Você foi desconectado(a)... :( | Segura firme, vamos te jogar de volta! / Erro: {código} | |
| Pausada (transmissor minimizou o app) | Transmissão pausada | {usuário} minimizou o app... aguenta aí! (para o dono: "Mude para seu app para continuar.") | |
| Terminou | Esta transmissão terminou. | | **Fechar transmissão** |
| Falhou ao iniciar | A transmissão não iniciou :( | Erro: {código} + link de ajuda; erro de sistema: "A transmissão terminou devido a um erro no sistema ({erro})" | **Fechar transmissão** |
| Minha, oculta | Esta transmissão foi ocultada. | | **Mostrar** (+ ícone "assistir várias" se há outras) |
| Minha, prévia pausada | Sua transmissão ainda está ligada! | Nós pausamos esta prévia para preservar seus recursos. | |

- "Minha, prévia pausada" é a boa notícia para o Telacast: **não decodificar a própria transmissão** (economiza CPU/RAM; a máquina tem pouca RAM). Mostrar esse cartão com as métricas do encoder (fps, Mbps).
- Estados de câmera: "Erro ao iniciar a câmera" + botão (`z+mxvo`); "Câmera indisponível" / "Ops, não detectamos nenhuma câmera." (`8jSzSe`, `bhqSWl`); "Sua câmera está transmitindo a uma taxa de quadros baixa. Isso pode ser causado por carga de sistema alta." (`1NyoVt`).

### 3.3 Auto-ocultar dos controles (ociosidade)  [F `461782` @8895883]

Componente `Idle(timeout)`: um temporizador reiniciável; `idle=true` quando estoura.
- `onMouseMove` / `onMouseDown` no contêiner => `onActive` (idle=false, reinicia o temporizador se nada segura).
- `onMouseLeave` => `onForceIdle` (idle=true na hora, **a menos que algo esteja segurando**).
- "Segurar" (`preventIdle(chave)`/`allowIdle(chave)`): conjunto de chaves; enquanto não vazio, o temporizador está cancelado e idle=false. Chaves usadas: `interact-hover` (mouse sobre os botões), `interact-focus` (foco de teclado dentro), `popup` (algum popover/menu aberto) [F `447404`, `246356`].
- Tempo: **1800 ms** no vídeo/PiP [F `timeout:1800` (x3)]. Os outros dois usos (2000 ms) são de atividades incorporadas. Para a visão focada/tela cheia usar também 1800 [I].
- Quando ocioso: controles saem (fade 150 ms, `ui.set_opacity`), cursor some só em tela cheia (`CursorIcon::None`) [I]; um popover aberto chama `closePopout` quando fica ocioso [F `C`: `s&&r()`].
- Teclado: qualquer tecla reativa [I].

### 3.4 Interações por elemento

| Elemento | Hover | Clique | Duplo clique | Direito | Teclado |
|---|---|---|---|---|---|
| Tile de usuário | borda LINE2 | selecionar (opcional) | focar | menu do usuário (volume, silenciar local) | Enter = focar |
| Tile de transmissão | mostra botão Assistir e chip | Assistir | tela cheia (se assistindo) | menu do tile | F = tela cheia, Esc = sair |
| Botão Câmera/Tela | fundo L4 | alterna / abre diálogo ou menu | | | |
| Sinal de conexão | tooltip "N ms" | abre popover | | | |
| Espectadores | popover (150 ms de atraso para fechar) | | | menu do usuário | |
| Vídeo focado | mostra controles | mostra controles | tela cheia | menu do tile | M = mudo local da transmissão |
| PiP | mostra controles, cursor "mover" | | volta ao canal | | |

Tempos [I salvo indicação]: hover de botões 120 ms (já em `theme.rs` `anim`); entrada de tile 220 ms de fade (como `view_since`); anel de fala 50 ms ease-out [F]; fade de controles 150 ms; PiP gruda no canto em ~220 ms ease-out.

### 3.5 Qualidade de conexão  [F `get quality` @2506062 (x3)]
`UNKNOWN` se não conectado ou sem ping; `BAD` se ping > **500 ms** ou perda de saída > **10%**; `AVERAGE` se ping > **250 ms** ou perda > **5%**; senão `FINE`. Ping médio = média dos últimos **20** [F `getAveragePing`]. Tooltip do sinal: "{ping} ms" arredondado [F `toFixed(0)`].

### 3.6 Prévia da transmissão (miniatura antes de assistir)  [F `384200` @9402800 (x3 em 12e4)]
Buscada ao exibir o tile se há permissão; vale **120 s** (`expires = agora + 12e4`); no máximo **5** tentativas seguidas; falha: espera `10 s * tentativas` ou o `retry_after` do 429; é descartada quando o transmissor para. Ocultar prévia: opção do transmissor (`JE73jB`).

---

## 4. Regras de comportamento

### 4.1 Ordenação dos participantes  [F `w()` @3275778]
Chave de ordenação (string): atividade `\x01{ordem}`; transmissões `\x02{nome}\x03` se o dono também tem câmera, senão `\x03{nome}\x03`; usuário `\x03{nome}` se tem câmera, `\x04{nome}` se está transmitindo (sem tile próprio), senão `\x05{nome}`. O nome é `nick.toLowerCase().padEnd(32,"!") + id`. Ou seja: **atividades, transmissões, quem tem câmera, quem está ao vivo, o resto; cada grupo por nome (minúsculo), desempate por id.** A grade **não** reordena por quem fala (só o anel muda).
Índices de consulta mantidos: SPEAKING, VIDEO, STREAM, FILTERED (têm vídeo e não estão em popout), NOT_POPPED_OUT [F `participantByIndex`].
Cada pessoa gera até 2 participantes: o usuário e, se transmite, a transmissão (id = chave `guild:canal:dono`; em DM `call:canal:dono`) [F `_getParticipantsForUser`, `Iy`/`_z` @721300].

### 4.2 Seleção automática (foco x grade)  [F `ChannelRTCStore`, seleção em @3282961 e classe em @3285186]
Estado por canal: `[explícito, auto]`. `explícito` pode ser `NONE` (grade), `AUTO` ou um id.
- `getSelectedParticipantId`: explícito `NONE` -> grade (nulo); explícito id -> esse id; explícito `AUTO` -> o valor `auto` (se válido).
- Cálculo de `auto`: (1) se assisto a uma transmissão ativa -> ela; (2) se só há 1 participante -> eu; (3) se exatamente 1 tem câmera -> ele; (4) senão o primeiro usuário que não sou eu e não está tocando ("ringing"); (5) senão eu.
- Padrão do explícito: **DM = AUTO, canal = NONE** (grade) [F `Z()`]. Se o explícito aponta para uma transmissão que acabou, volta a `NONE`. (Em palco o Discord ainda seleciona sozinho a minha transmissão; ignorar, palco está fora de escopo.)
- Assistir uma transmissão (`watchStream`): **foca** nela, exceto se for "múltipla" ou `noFocus`; vira "múltipla" automaticamente quando já há **2 ou mais** transmissões alheias no canal, ou por `forceMultiple` (botão "Comece a assistir a várias transmissões") [F `b()` @721700].
- Ao assistir uma transmissão **sem** modo múltiplo, as outras transmissões que eu assistia (menos a minha) são fechadas [F `STREAM_WATCH` @5338900].
- Se saio do canal de voz, o estado do canal é descartado (e o chat fecha, salvo preferência) [F `VOICE_CHANNEL_SELECT`].
- Estatística de uso: tempo em grade x foco e nº de trocas [F `getSelectedParticipantStats`] (não replicar).

### 4.3 Limites e política
- Pessoas com vídeo/transmissão por canal: 50 [doc 04, suporte do Discord].
- Volume por usuário: 0..200, padrão **100** (`M=100`, máx `w=200`) [F @11249936]. Volume da transmissão: mesmo intervalo, padrão do Discord **18** (`U=18`; só em 2 dos 3 builds) [F]. **Decisão Telacast: padrão 100**, pois o áudio da transmissão vem do monitor do sistema sem normalização (ver pergunta 5).
- "Reduzir o volume da transmissão automaticamente quando as pessoas estiverem falando" (ducking) existe como opção [F `zlA23F`/`IwZbbP`]. P3.
- Bitrate de áudio por canal: 64 kbps padrão; 128/256/384 kbps nos níveis 1/2/3 [F `tl`: `bitrate: L.DqD, 128e3, 256e3, 384e3`; doc 04].
- Quando a janela/app compartilhado é minimizado, o transmissor entra em estado "pausado" (`STREAM_SET_PAUSED`) e os espectadores veem a mensagem do estado "Pausada" [F @721300]. Telacast: detectar janela minimizada só quando houver fonte "Janela".

### 4.4 Tela cheia
- Dois mecanismos no Discord: (a) **layout `full-screen` da chamada** (esconde barras laterais mantendo a janela) [F enum]; (b) **tela cheia do navegador** no nó do vídeo (`requestFullscreen`; o botão alterna "Tela cheia"/"Sair da tela cheia" conforme `isFullscreen(nó)`) [F `423562`].
- Telacast já faz (b)-like: `ViewportCommand::Fullscreen`, esconde painéis, Esc volta. Acrescentar: controles com auto-ocultar, botão de sair, tecla F, e que **a faixa de miniaturas e o chat não aparecem** em tela cheia.

### 4.5 Fala (VAD) e estado
- "Falando" = tem energia de voz, não está mudo (`checkIsMuted: true`) [F `updateParticipantSpeaking`]; também pisca quando toca soundboard.
- Falar mudo: aviso "Você está mudo" / dica no botão, com tooltip temporário (cooldown `rX`) [F `speakingWhileMutedTooltipTimeout`, `shouldShowSpeakingWhileMutedTooltip` @6969600]. P2.
- O estado de fala vem do **cliente de cada um** (evento `Speaking`) e é retransmitido pelo servidor; o anel local do próprio usuário usa o VAD local (zero latência).

### 4.6 Rolagem, arrastar, menus
- Grade: sem rolagem enquanto `w >= 160`; abaixo, rolagem vertical.
- Faixa de miniaturas: rolagem horizontal com roda do mouse (deslocamento vertical convertido).
- PiP: arrastar pelo corpo; redimensionar pelas bordas/cantos.
- Menus de contexto: sempre `response.context_menu` do egui; o popover de espectadores/qualidade usa `egui::Popup`.

---

## 5. Dados necessários (protocolo/servidor), em ordem de prioridade

Estado atual: `Live { channel_id, user_id }` dentro de `Snapshot`; transmissão em `/publish/{canal}` e `/watch/{canal}`; quadro = `[key:u8][annexb]`; `Room { tx, key, channel, user }`; mudança de transmissão manda **snapshot completo** a todos os participantes (`push_many`). A coluna `channels.kind` já existe (`SMALLINT`, DM = 10); usar **2 = voz** (mesmo número do Discord) [I].

### 5.1 P0 - metadados e espectadores da transmissão (sem voz)
1. `proto`: `StreamMeta { w: u32, h: u32, fps: u32, kbps: u32, codec: String /*"h264"*/, encoder: String /*"nvenc"|"cpu"*/, audio: bool, started_ms: i64 }`.
2. WS `/publish`: **primeira mensagem Text** com `{"t":"hello", ...StreamMeta}`; nova `hello` quando a qualidade muda. Servidor guarda em `Room.meta`, repassa ao **começo** de cada `/watch` (antes do quadro-chave guardado) e a todos os espectadores quando muda.
3. `Room.viewers: HashMap<i64 /*uid*/, u32 /*conexões*/>`; `run_viewer` recebe o `uid` de `auth_stream`, registra/remove e notifica. Limite `max_viewers` (padrão 50): acima, `Text("limite de espectadores")` e fecha.
4. `S2C::LiveUpdate { channel_id, owner_id, meta: Option<StreamMeta>, viewers: Vec<i64> /*capado em 50*/ }` (**delta**, não snapshot), enviado aos participantes do canal e ao dono. `Live` do `Snapshot` ganha `meta` e `viewers_count`.
5. Servidor -> publicador (na mesma WS): `Text({"t":"viewers","n":3})` para a UI do dono (opcional se (4) já chega pelo gateway).

### 5.2 P0 - qualidade em tempo real
6. Trocar resolução/fps durante a transmissão: o publicador reinicia o encoder, manda IDR + novo `hello`; espectador recria o decodificador ao mudar de dimensão. Nada novo no servidor além de repassar `hello`.
7. `servers`: `stream_max_height INT NULL`, `stream_max_fps INT NULL`, `stream_max_viewers INT NOT NULL DEFAULT 50`. `ServerInfo` expõe os dois primeiros. O servidor **valida** o `hello` (rejeita se exceder).

### 5.3 P1 - canais de voz e presença de voz
8. `proto::Channel { id, name, kind: ChannelKind }` com `enum ChannelKind { Text, Voice }` (serde por inteiro 0/2); `C2S::CreateChannel { server_id, name, kind }`.
9. Estado de voz **em memória** (não vai ao Postgres): `HashMap<channel_id, HashMap<user_id, VoiceState>>`.
   `VoiceState { user_id: i64, channel_id: i64, self_mute: bool, self_deaf: bool, self_video: bool, server_mute: bool, server_deaf: bool, joined_ms: i64 }`.
10. `C2S`: `VoiceJoin { channel_id }`, `VoiceLeave`, `VoiceSet { self_mute: Option<bool>, self_deaf: Option<bool>, self_video: Option<bool> }`, `VoiceSpeaking { speaking: bool }` (o cliente envia quando o VAD muda; servidor reenvia aos do canal).
11. `S2C`: `VoiceStates { server_id, states: Vec<VoiceState> }` (carga inicial por servidor, junto do snapshot), `VoiceStateUpdate(VoiceState)` e `VoiceLeft { user_id, channel_id }` (deltas), `Speaking { user_id, channel_id, speaking: bool }`.
12. Regras no servidor: um usuário em **um** canal de voz por vez (entrar em outro move); queda do gateway remove o estado (com tolerância de 10 s para reconexão [I]); só quem está no canal de voz pode publicar/assistir transmissões desse canal (como no Discord) [I]; `can_access(channel, uid)` já valida membro do servidor.
13. Ping de voz: quadro `Ping{nonce}` / `Pong{nonce}` na conexão de áudio ou no gateway; o cliente guarda os últimos 20 RTTs e a perda.

### 5.4 P1 - bitrate por qualidade (tabela de partida, [I])
Mbps para H.264 NVENC CBR de tela (conteúdo de jogo/vídeo): 480p30 = 1,5; 720p30 = 3; 720p60 = 4,5; 1080p30 = 5; 1080p60 = 8; 1440p60 = 14; Fonte@60 (4K) = 22; 15 fps = 60% do valor de 30; 5 fps (nitidez) = 2. Referências do Discord [F]: teto de desktop ~3,5 Mbps (`desktopBitrate max 35e5`, @11250405) e 1080p30 limitado a 3,5 ou 5 Mbps em experimentos (`2026-05-frontier-tuning` `maxBitrate 35e5/5e6`). Como o Telacast roda em servidor próprio, a tabela acima é mais generosa; o usuário pode sobrescrever em "Avançado".

### 5.5 P2 - prévia e áudio
14. Prévia: o publicador envia a cada 60 s (e ao começar) um JPEG de 320x180 (~15 kB) como mensagem `Binary` com tag distinta (`0x02`) ou `Text({"t":"preview"})` + corpo; servidor guarda o último e serve `GET /preview/{canal}?token=` (com cache de 120 s no cliente [F]). Sem prévia: avatar do transmissor.
15. Áudio da transmissão: segundo fluxo Opus na mesma WS (tag `0x03`, timestamp comum), `meta.audio=true`. Depende do módulo de áudio (doc 04).
16. Câmera: `self_video` no `VoiceState` + fluxo H.264 próprio `/camera/{canal}` (reusa o publicador). P2.

### 5.6 P3
17. `VoiceModerate { user_id, mute, deaf }` (mudar/silenciar membro), convite para assistir, notificação aos amigos ao começar a transmitir (`Cef4t7`), "ocultar minha prévia".

---

## 6. Plano de implementação em egui 0.37

### 6.1 Onde entra
- `theme.rs`: `ring_px`, `paint_speaking_ring`, `icon(painter, Icon, rect, color)` (glifos desenhados), `Idle`, `fit_grid`, `pill`, `chip`. 
- Novo `app/src/voice.rs`: `VoiceState` do cliente, painel, grade, tiles, foco, PiP (desenho). `main.rs` só chama.
- `main.rs`: `Modal::GoLive` ganha `GoLiveForm`; `View::Chan` passa a conhecer `ChannelKind`; `Session` ganha `voice: VoiceClient` e `watch_ui: WatchUi`; `Act` ganha `VoiceJoin(i64)`, `VoiceLeave`, `SetMute`, `SetDeaf`, `FocusTile(Option<String>)`, `SetVolume(StreamKey, f32)`.
- `viewer.rs`: expor `meta`, fps/Mbps recebidos, `volume`. `publisher.rs`: aceitar `StreamQuality`, mandar `hello`, reiniciar encoder.
- Sem `ColorImage` por quadro no PiP/miniaturas: reusar a mesma textura (um `TextureHandle` por transmissão).

### 6.2 Armadilhas do egui
- **Sem emoji colorido e sem ícones na fonte:** desenhar todos os glifos com `Painter` (linhas, círculos, `Shape::convex_polygon`, `Shape::line`). Lista: microfone, microfone cortado, fone, fone cortado, câmera, tela/monitor, telefone desligado, olho, engrenagem, janela/nova janela, expandir/recolher, volume (3 níveis), seta, "+", "-", barras de sinal (3).
- **Sem negrito de fonte variável / sem letter-spacing:** diferenciar com tamanho e cor (já é a regra do tema); caps pequenos como `caps()`.
- **Sem gradiente:** faixas sólidas translúcidas (`alpha(BLACK,0.55)`).
- `ui.set_opacity(t)` afeta tudo que vem depois no mesmo `Ui` (restaurar com `ui.scope`).
- Entrada ociosa: o egui só repinta com evento; ao esperar o timeout, chamar `ctx.request_repaint_after(restante)`. Durante vídeo ao vivo já repinta (500 ms hoje, `request_repaint_after`), subir para o fps do vídeo só quando houver quadro novo (`ctx.request_repaint()` no thread do viewer, já feito).
- Esconder o cursor: `ctx.set_cursor_icon(egui::CursorIcon::None)` todo quadro em que estiver ocioso e em tela cheia.
- Textura por quadro 1080p60 = ~500 MB/s de upload (RGBA 8 MB); aceitável para P0, mas medir. Caminho de melhora: shader NV12 via `egui_wgpu::Callback` ou decodificação NVDEC; não é requisito de UI.
- PiP do SO: `ctx.show_viewport_deferred(id, ViewportBuilder::default().with_always_on_top().with_inner_size([320.0,180.0]).with_decorations(false), |ctx, _| ...)`; o closure é `Send + Sync + 'static`: passar `Arc<Mutex<...>>` (o `frame` já é). "Sempre no topo" não funciona em Wayland [?].
- `egui::Popup` / `context_menu` para menus; slider de volume: `egui::Slider::new(&mut v, 0.0..=200.0).show_value(false)` estilizado.
- IDs: tiles precisam de `Id::new(("tile", tile_key))` estável (reordenar não pode perder animação do anel).

### 6.3 Trecho 1 - grade de tiles
```rust
/// Melhor número de colunas para `n` tiles 16:9 numa área `avail`. Devolve (colunas, tamanho do tile).
pub fn fit_grid(n: usize, avail: egui::Vec2, gap: f32, aspect: f32) -> (usize, egui::Vec2) {
    let mut best = (1usize, egui::Vec2::ZERO);
    for cols in 1..=n.max(1) {
        let rows = n.div_ceil(cols);
        let w_fit = (avail.x - gap * (cols as f32 - 1.0)) / cols as f32;
        let h_fit = (avail.y - gap * (rows as f32 - 1.0)) / rows as f32;
        let w = w_fit.min(h_fit * aspect);
        if w > best.1.x {
            best = (cols, egui::vec2(w, w / aspect));
        }
    }
    best
}

/// Retângulos dos tiles; a última linha incompleta é centralizada.
pub fn tile_rects(n: usize, area: egui::Rect, cols: usize, tile: egui::Vec2, gap: f32) -> Vec<egui::Rect> {
    let rows = n.div_ceil(cols.max(1));
    let total_h = rows as f32 * tile.y + gap * (rows as f32 - 1.0);
    let top = area.center().y - total_h / 2.0;
    (0..n)
        .map(|i| {
            let (r, c) = (i / cols, i % cols);
            let in_row = if r == rows - 1 { n - r * cols } else { cols };
            let row_w = in_row as f32 * tile.x + gap * (in_row as f32 - 1.0);
            let left = area.center().x - row_w / 2.0;
            egui::Rect::from_min_size(egui::pos2(left + c as f32 * (tile.x + gap), top + r as f32 * (tile.y + gap)), tile)
        })
        .collect()
}
```
Uso: `let area = ui.available_rect_before_wrap().shrink(16.0); let (cols, tile) = fit_grid(n, area.size(), 8.0, 16.0/9.0);` se `tile.x < 160.0`, usar `cols = ((area.width()+8.0)/(160.0+8.0)) as usize`, `tile = (160, 90)` e envolver em `ScrollArea::vertical()`. Para cada rect: `ui.interact(rect, id, Sense::click())` e pintar com `ui.painter_at(rect)`.

### 6.4 Trecho 2 - anel de "falando" e pílula
```rust
/// Espessura (px) do anel a partir do volume em dB (curva do cliente do Discord). `max` = 3 em avatares.
pub fn ring_px(db: f32, max: f32) -> f32 {
    0.5 + (max - 0.5) / (1.0 + (-0.03 * (db + 45.0)).exp())
}

/// Anel interno (INSET) numa região. `t` = 0..1 vindo de animate_bool_with_time(…, 0.05).
pub fn speaking_ring(painter: &egui::Painter, rect: egui::Rect, radius: u8, t: f32, db: Option<f32>, surface: Color32) {
    if t <= 0.01 { return; }
    let w = db.map_or(2.0, |d| ring_px(d, 3.0)) * t;           // sem medida => 2 px fixos
    let r = egui::CornerRadius::same(radius);
    // anel
    painter.rect_stroke(rect, r, egui::Stroke::new(w, OK), egui::StrokeKind::Inside);
    // linha de 1 px na cor do fundo logo dentro do anel (separa do conteúdo)
    painter.rect_stroke(rect.shrink(w), r, egui::Stroke::new(1.0, surface), egui::StrokeKind::Inside);
}

/// Pílula de texto sobre vídeo/tile: devolve o retângulo ocupado.
pub fn pill(painter: &egui::Painter, pos: egui::Pos2, text: &str, icon_w: f32) -> egui::Rect {
    let g = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(12.5), TEXT);
    let size = egui::vec2(8.0 + icon_w + g.size().x + 8.0, 22.0);
    let rect = egui::Rect::from_min_size(pos, size);
    painter.rect_filled(rect, egui::CornerRadius::same(6), alpha(egui::Color32::BLACK, 0.62));
    painter.galley(egui::pos2(rect.min.x + 8.0 + icon_w, rect.center().y - g.size().y / 2.0), g, TEXT);
    rect
}
```
No avatar redondo o anel é um `circle_stroke` com `radius - w/2`. Para animar: `let t = ctx.animate_bool_with_time(egui::Id::new(("fala", user)), speaking, 0.05);`.

### 6.5 Trecho 3 - ocioso (auto-ocultar) e camada de controles
```rust
/// Auto-ocultar: devolve `idle`. `hold` = algo segurando (mouse sobre os botões, popover aberto, foco).
#[derive(Default)]
pub struct Idle { last_active: f64 }

impl Idle {
    pub fn tick(&mut self, ctx: &egui::Context, area: egui::Rect, timeout: f64, hold: bool) -> bool {
        let now = ctx.input(|i| i.time);
        let (moved, pos) = ctx.input(|i| {
            let key = i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }));
            (i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_pressed() || key, i.pointer.hover_pos())
        });
        let inside = pos.is_some_and(|p| area.contains(p));
        if (moved && inside) || hold { self.last_active = now; }
        if !inside && !hold { self.last_active = now - timeout; }        // saiu: ocioso já (onForceIdle)
        let idle = now - self.last_active >= timeout;
        if !idle { ctx.request_repaint_after(std::time::Duration::from_secs_f64(timeout - (now - self.last_active) + 0.02)); }
        idle
    }
}

fn video_overlay(ui: &mut egui::Ui, rect: egui::Rect, idle: bool, f: impl FnOnce(&mut egui::Ui, egui::Rect, egui::Rect)) {
    let a = ui.ctx().animate_bool_with_time(ui.id().with("ctl"), !idle, 0.15);
    if a <= 0.01 { return; }
    let top = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 44.0));
    let bot = egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - 52.0), rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.set_opacity(a);
        for r in [top, bot] { ui.painter().rect_filled(r, 0, alpha(egui::Color32::BLACK, 0.55)); }
        f(ui, top, bot);
    });
}
```
`hold` = `ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| top.contains(p) || bot.contains(p))` (mouse sobre as barras) `|| ui.memory(|m| m.any_popup_open())`. Em tela cheia ocioso: `ctx.set_cursor_icon(CursorIcon::None)`.

### 6.6 Trecho 4 - qualidade -> parâmetros do encoder, mensagem `hello`
```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Res { P480, P720, P1080, P1440, Source }

pub fn out_size(src: (u32, u32), r: Res) -> (u32, u32) {
    let target_h = match r { Res::P480 => 480, Res::P720 => 720, Res::P1080 => 1080, Res::P1440 => 1440, Res::Source => src.1 };
    let h = target_h.min(src.1);                       // nunca ampliar
    let w = (src.0 as f32 * h as f32 / src.1 as f32).round() as u32;
    (w & !1, h & !1)                                    // H.264 pede dimensões pares
}

pub fn default_kbps(h: u32, fps: u32) -> u32 {          // tabela 5.4, interpolada por altura e fps
    let base = match h { 0..=480 => 1500, 481..=720 => 3000, 721..=1080 => 5000, 1081..=1440 => 9000, _ => 14000 };
    let k = if fps >= 60 { 1.6 } else if fps >= 30 { 1.0 } else { 0.6 };
    (base as f32 * k) as u32
}
// hello (primeira mensagem Text de /publish): serde_json::to_string(&StreamMeta { w, h, fps, kbps, codec: "h264".into(), .. })
```
Escala: hoje a captura envia o monitor inteiro ao encoder. Para `Res < Fonte`: (a) **no servidor X** com a extensão RENDER (`render_set_picture_transform` + `render_composite` para um pixmap menor, depois `get_image` do pixmap pequeno): transfere menos bytes, é a opção a testar; (b) fallback CPU com `fast_image_resize`. Ambas [I], medir antes de escolher.

### 6.7 Fonte "Janelas"
X11: listar com `_NET_CLIENT_LIST` + `_NET_WM_NAME`; capturar o conteúdo com `composite_redirect_window` + `name_window_pixmap` + `get_image` (sem compositor, janelas encobertas saem erradas) [?]. Wayland/XWayland: janelas nativas Wayland aparecem pretas; o caminho correto é o portal do PipeWire (`ashpd`), que mostra o seletor do próprio sistema (o Discord faz isso: "Escolha o que compartilhar usando uma interface do sistema" `Yp6h5W`, `sbbPhW`) [F strings]. Ver pergunta 3.

### 6.8 Painel "Voz conectada" (esqueleto)
```rust
fn voice_panel(&self, ui: &mut egui::Ui, s: &Session, acts: &mut Vec<Act>) {
    let Some(v) = &s.voice.current else { return };
    egui::Frame::new().fill(L2).stroke(egui::Stroke::new(1.0, LINE)).corner_radius(10).inner_margin(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            signal_bars(ui, v.quality, v.last_ping);                       // 16x16, tooltip "N ms", clique abre popover
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(v.status_text()).size(13.0).color(v.status_color()));
                ui.label(RichText::new(format!("{} / {}", v.channel_name, v.server_name)).size(11.0).color(MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::HangUp, false, "Desconectar").clicked() { acts.push(Act::VoiceLeave); }
            });
        });
        ui.add_space(6.0);
        ui.columns(2, |c| {
            if toggle_button(&mut c[0], Icon::Camera, "Vídeo", v.self_video).clicked() { /* Act::VoiceSet */ }
            if toggle_button(&mut c[1], Icon::Screen, "Tela", s.is_live_here()).clicked() { /* abre diálogo ou menu (2.1) */ }
        });
    });
}
```
Em `channel_panel`, `list_h` hoje subtrai 52 (cartão); passar a subtrair `52 + painel_de_voz.height + 6`.

---

## 7. Ordem de entrega (cada passo compila e dá algo visível)

1. **Metadados + espectadores na transmissão atual.** `StreamMeta`/`hello`, `Room.viewers`, `LiveUpdate`. Vê-se: no cartão "X está ao vivo" aparece "1920x1080 · 60 fps · 7,9 Mbps" (mono) e "N assistindo"; o dono vê o contador subir quando alguém assiste.
2. **Controles sobrepostos no vídeo.** `Idle` (1800 ms), barras topo/base, botões: espectadores (avatares + popover "Espectadores - N"), tela cheia, parar de assistir, volume (slider, ainda sem som); tecla F/Esc; cursor some em tela cheia. Vê-se: o vídeo ganha o aspecto do Discord e os controles somem sozinhos.
3. **Diálogo Transmitir v2.** Abas Telas/Janelas (só Telas), miniaturas dos monitores, grupos Resolução/Taxa de quadros, predefinições Fluidez/Nitidez, resumo "~X Mbps", Avançado com Mbps. Escala de saída no publicador. Vê-se: escolher 720p30 transmite menor de fato (conferir o `hello` e o `--bench`).
4. **Qualidade durante a transmissão.** Engrenagem do dono abre o submenu; trocar reinicia o encoder; espectadores trocam de resolução sem fechar.
5. **Canais de voz sem áudio (presença).** `ChannelKind::Voice`, criar canal de voz, `VoiceJoin/Leave`, participantes sob o canal na lista, painel "Voz conectada" (estados, subtítulo, desligar), cartão com mic/fone alternando `self_mute/self_deaf` e ícones nos outros clientes. Vê-se: dois clientes entram no mesmo canal e se enxergam; mutar num mostra o ícone no outro.
6. **Visão do canal de voz: grade.** `fit_grid`, tiles com avatar/pílula/ícones, centralização da última linha, rolagem abaixo de 160 px. Vê-se: 1, 2, 3, 5, 7 participantes com a disposição de 2.4.1.
7. **Tile de transmissão e foco.** Transmissão vira participante (chave `canal:dono`), estados de 3.2, "Assistir" no tile, foco com faixa, seleção automática 4.2, "assistir várias". Vê-se: transmitir dentro do canal de voz mostra o tile com etiqueta AO VIVO; Assistir leva ao foco com os outros na faixa.
8. **PiP dentro do app.** `Area` flutuante com canto, arrastar com inércia, redimensionar 200..960, controles 1800 ms, troca de transmissão. Vê-se: abrir outro canal enquanto assiste mantém o vídeo no canto.
9. **Qualidade de conexão.** Ping do canal de voz (Ping/Pong), sinal com 3 níveis, popover com gráfico e limiares 250/500 ms. Vê-se: sinal muda de cor ao simular atraso.
10. **Áudio (outro doc) -> anel de fala real.** VAD local, `VoiceSpeaking`, `speaking_ring` com dB. Volume por usuário (0..200, menu de contexto) e da transmissão. Vê-se: o anel acende ao falar e pulsa com o volume.
11. **Áudio da transmissão** (monitor PipeWire -> Opus), checkbox "Compartilhar áudio" ativo, estado Pausada.
12. **Câmera** (`self_video`, V4L2), janelas como fonte (X11 composite ou portal), pop-out em janela do SO, prévia (`/preview`), convite para assistir.

---

## 8. Perguntas em aberto (mudam o que será construído)

1. **A transmissão passa a exigir estar num canal de voz (padrão do Discord) ou continua podendo ser feita de um canal de texto/DM?** Muda onde está o botão "Ao vivo" e o modelo de dados (`Live` por canal de texto hoje). Sugestão: manter texto como está nos passos 1 a 4 e migrar no passo 7.
2. **Cor do anel de "falando": verde (`OK`, como no Discord) ou âmbar?** `OK` hoje é "só presença online" no Grafite; âmbar já é "ao vivo". Sugestão: verde para falar/conectado, âmbar só para transmissão e seleção.
3. **Sessão X11 ou Wayland no seu PC (e nos amigos)?** O capturador usa X11; "janela como fonte" e captura Wayland pedem o portal do PipeWire. Define se o passo 12 é composite do X11 ou `ashpd`.
4. **Limites de qualidade por servidor (como níveis de impulso) ou liberar o que o NVENC aguenta?** Define se `stream_max_height/fps` entram agora (5.2) e se o diálogo mostra opções desativadas.
5. **Volume padrão da transmissão: 100 (proposto) ou 18 (valor do Discord)?** E ducking (baixar a transmissão quando alguém fala) entra ou fica para depois?
6. **PiP: só dentro do app (passo 8) ou também janela do SO sempre no topo?** A segunda é extra (viewport secundária) e não funciona em Wayland.

---

## Apêndice A - textos pt-BR prontos (do bundle), para reuso
Voz conectada · Vídeo conectado · Desconectar · Conectando · Autenticando · Aguardando o servidor de voz · Verificando rota · Sem rota · Desconectado · Detalhes de Voz · Compartilhar a tela · Compartilhamento de tela indisponível · Você não pode transmitir para esse canal. · Vídeo · Tela · Transmitir · Ao Vivo · Assistindo · Entrar na transmissão · Parar de assistir · Parar de assistir — {username} · Parar de transmitir · Fechar transmissão · Esta transmissão terminou. · Esta transmissão foi ocultada. · Transmissão pausada · {username} minimizou o app... aguenta aí! · Você foi desconectado(a)... :( · Segura firme, vamos te jogar de volta! · A transmissão não iniciou :( · Erro: {errorCode} · Sua transmissão ainda está ligada! · Nós pausamos esta prévia para preservar seus recursos. · Espectadores - {numViewers} · Tela cheia · Sair da tela cheia · Transmissão em Nova Janela · Mude para a transmissão de {username} · Volume da transmissão · Volume do usuário · Silenciado por você · Qualidade da transmissão · Resolução · Taxa de quadros · Fonte · {n}p · {n} FPS · Compartilhar áudio · Compartilhar áudio da transmissão · Trocar janela · Alterar a Transmissão · Telas · Tela {index} · Compartilhar uma tela, janela ou aplicativo · Canal de transmissão · O canal está cheio · Nenhuma permissão · Idade restrita · Desculpe, esta transmissão está com o máximo de espectadores. · Mostrar participantes sem vídeo · Os participantes sem vídeo estão ocultados. Deseja exibi-los? · Erro ao iniciar a câmera · Ops, não detectamos nenhuma câmera. · Abrir no Teatro · Ativar microfone / Desativar microfone · Ativar áudio / Desativar áudio.
