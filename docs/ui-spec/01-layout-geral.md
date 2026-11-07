# 01 - Layout geral do app (esqueleto)

Fonte principal: cliente web extraído, `discord-bundle/web.0bcf9a9e83b2c3c7.js` (o maior). Cada constante marcada [F] abaixo foi **reconferida nos 5 `web.*.js`** (resizer 264/432, `setMinimumSize(800,500)`, molas das camadas e do tooltip, `isOpen:!0,width:375`, tokens de toast/modal, tempo de 10 s do splash: todos presentes e idênticos). O pacote de CSS do cache só traz variáveis de tema (`:root{--...}`), **sem as regras dos componentes**; por isso larguras puramente CSS (coluna de membros, altura da barra de aplicativo, z-index) aparecem como [I]/[?].
Strings pt-BR vêm de `12064a7d28fc7eb1.js` (catálogo truncado no fim do cache; 23 mil entradas legíveis). Chaves citadas entre parênteses, ex. (`ULvRFd`).

Legenda: **[F]** confirmado no código; **[I]** inferência; **[?]** não consegui confirmar.

---

## 1. Resumo e fora de escopo

O cliente do Discord de 2026 mudou a casca em relação ao que está em `docs/discord/10-ux-interface.md` (que fala em 72 + 240 + 240 fixos):

- A casca é uma **pilha vertical**: barra de aplicativo no topo (voltar/avançar, título da página, ícones à direita) -> **um único aviso** (banner) -> corpo. [F] (`PE`: `s_` titlebar, depois `ud` notice, depois `div.Qs`)
- O corpo é **barra lateral única redimensionável (264 a 432 px, padrão 375 px, persistida)** + conteúdo + **coluna direita que mostra uma só seção por vez** (busca > thread/post > amigos > perfil (DM) > resumos > membros). [F]
- Janela mínima **800 x 500**. [F]
- Camadas: portais nomeados ("App", "Chat", "Verification"), pilha de modais com estados de transição de **300 ms**, camadas de tela cheia com transição de **150 ms** (e a pilha `LayerStore` das configurações de canal/servidor com mola `friction 10 / tension 100`), tooltip com mola `tension 2400 / friction 52`, toasts de 300 ms. [F]
- Teclado: modo teclado (Tab/F6/setas ligam, clique desliga), F6 salta entre "seções", Ctrl+K, Alt+setas, cadeia de prioridade do Esc, "digitar em qualquer lugar foca a caixa de mensagem". [F]
- Estados globais: tela de carregamento "Conectando..." (reaparece em toda reconexão), "problemas" após 10 s, banners únicos por prioridade, tela de erro "Eita, que saia justa" com botão Recarregar. [F]

Fora de escopo (outras áreas): conteúdo da lista de servidores/pastas, lista de canais, cabeçalho do canal e mensagens, lista de membros por dentro, perfil/popouts por dentro, voz/painéis de voz, configurações por dentro, interior do Quick Switcher. Aqui entram **os contêineres, medidas externas, regras de visibilidade, camadas, teclado e estados globais**.

---

## 2. Árvore de componentes (de fora para dentro)

### 2.1 Pilha vertical da janela

```
Janela (mín. 800x500 [F]; inicial do Telacast 1280x760)
└─ Raiz (fundo L0)
   ├─ [Camada base] casca
   │   ├─ Barra de aplicativo  (some em tela cheia de elemento [F: !isFullscreenInContext])
   │   ├─ Aviso/banner (0 ou 1)                    [F: um único NoticeStore.getNotice()]
   │   └─ Corpo (flex horizontal)
   │       ├─ Barra lateral  = [trilho de servidores | lista de canais]  + painéis de usuário embaixo
   │       ├─ alça de redimensionar (separador vertical, tabIndex 0)
   │       └─ Conteúdo (rota atual) + Coluna direita (uma seção)
   ├─ Camadas empilhadas (configurações, etc.)  [F: LayerStore]
   ├─ Modais / Quick Switcher
   ├─ Popouts / menus de contexto / tooltips (portais)
   └─ Toasts
```

Barra de aplicativo (`AppTitleBar`) [F]:
- **Esquerda**: botões Voltar/Avançar (histórico; com abas de canal, vira barra de abas), no caso sem eles, o botão de caixa de entrada. Título (ícone 16-20 + texto `body-sm/medium`, 1 linha, reticências): nome do servidor, "Mensagens diretas", "Amigos", "Explorar"... conforme a rota. Quando há abas de canal (`ChannelTabBar`), o título é substituído pelas abas.
- **Direita** (nesta ordem): botão de amigos online (contador), caixa de entrada (com selo), ajuda, **botão de atualização** (ícone de download; tooltip "Baixando atualização" / "Atualização pronta!").
- Cada botão: componente `HeaderBarButton`, `role=button`, tooltip embaixo, estado `selected`, `data-jump-section` (para F6). Ícone `sm` (16-18 px).
- **Altura**: variável CSS `--custom-app-top-bar-height` existe e é lida pelo contêiner de toasts; o valor está no CSS que não veio no cache. [?] Proposta Telacast: **36 px**, ícones 20, alvo de clique 28 x 28, espaçamento 8.
- Fundo: mesma camada da barra lateral (L1 no Telacast); linha inferior de 1 px `LINE` só quando há aviso ou rolagem [I].

Telacast: no Linux a janela usa a decoração nativa do winit; a barra de aplicativo é **desenhada dentro** da janela. Voltar/Avançar entram só quando houver histórico (passo 9 da ordem de entrega).

### 2.2 Barra lateral (trilho + canais + painéis)

| Item | Discord (bundle) | Evidência | Telacast |
|---|---|---|---|
| Largura total da barra lateral | **264 a 432, padrão 375**, guardada em `resizable-sidebar-width-2`; store `{isOpen:true,width:375}` | [F] `minDimension:264,maxDimension:432`, `Pu(e)`, `su=...({isOpen:!0,width:375})` | `SIDEBAR_MIN/MAX/DEFAULT` = 264/432/375 |
| Largura recolhida | **76** (só o trilho) | [F] `Pu(76)` quando `isOpen=false`; mas o ramo está desligado no código (`su(e=>!e.isOpen)&&!1`) | recolher = só trilho 76; não habilitar no 1o corte |
| Trilho de servidores | largura = ícone + 2 x 16: **72** (ícone 40) ou **76** (ícone 44) | [I] derivado de `Modules.guildbar.AVATAR_SIZE`: 40 (compacto) / 44 (padrão e confortável); 76 = recolhido [F] | `rail_w`: 72/76 por densidade |
| Pasta (ícone) | 48 (52 em padrão/confortável) | [F] `guildbar.FOLDER_SIZE`, `FOLDER_ITEM_GUILD_ICON_SIZE=48px` | - |
| Painéis de usuário (conta, voz, atividade) | seção **abaixo do trilho e da lista**, largura total da barra lateral; altura medida e gravada em `--custom-app-panels-height` para a lista ganhar folga no fim | [F] `vg` é irmão de `v2` dentro de `div.pz`; `aria-label` = "Status do usuário e configurações" (`vTl6Lk`) | cartão de usuário de 52 + margem 8, largura total |
| Redimensionar | arrastar a alça; clique na alça **alterna** abrir/fechar; teclado: Setas ±10, Home = 264, End = 432; Enter alterna | [F] handler `h` (`ArrowRight:o+10`, `Home:264`, `End:432`, `Enter` -> `A()`) | idem |
| Arrasto além dos limites | durante o arrasto, classe de corpo ao passar de **<= 264** e **>= 432**; escala de "overdrag" `1 + min(|delta|/76, .25)` (até 1,25) na variável `--custom-overdrag` | [F] | [I] efeito opcional: pintar a alça mais clara nos limites; **sem** escala |
| Persistência | ao soltar | [F] `eG.w.set(rn.o,e)` + evento `CHANNEL_SIDEBAR_RESIZED` | `eframe::Storage` |

O que é a "lista de canais" dentro dos 375: largura = total - trilho (padrão ~299, mín. 188 com trilho 76). [I] (o bundle só dá o total). Constante `MdR = 240` aparece como fallback (`custom-guild-sidebar-width` ausente) e na fórmula do `homeSidebarWidth`: sobra do layout clássico 72 + 240. [I]

### 2.3 Coluna direita: uma seção por vez

`ChannelSectionStore.getSection(canalId, éDM)` [F], precedência decrescente:

1. `SEARCH`: busca ativa (resultados).
2. `SIDEBAR_CHAT`: existe sidebar "do canal" (thread, post de fórum, criar thread, denúncia de moderação, mensagem fixada em painel).
3. `FRIENDS`: lista "Ativo agora" (só na tela de amigos / com alternador na barra).
4. `PROFILE`: **DM** e `isProfileOpen` (padrão **true**).
5. `SUMMARIES`: resumos.
6. `MEMBERS`: `isMembersOpen && !éDM`.
7. `NONE`.

Estado persistido (`ChannelSectionStore2`): `isMembersOpen` (**inicial false** no código; ver pergunta 3), `isSummariesOpen=false`, `isProfileOpen=true`, `isFriendsOpen=true`, `sidebars` (por canal-base), `guildSidebars`. [F]

Alternância (`j(aberto, track)`): ao alternar membros/perfil/resumos, **fecha qualquer busca e a sidebar-de-canal do canal atual** e então: se algo foi fechado e a seção já estava aberta, ela fica aberta; senão inverte. [F] Resumo: clicar em "Membros" com uma thread aberta fecha a thread e mostra membros. `Ctrl+U` alterna membros no canal de servidor, perfil na DM, e abre/fecha o chat de texto em canal de voz. [F]

Larguras:

| Seção | Largura | Redimensionável | Evidência |
|---|---|---|---|
| Membros | **240** | não | [I] (CSS ausente; valor clássico) |
| Perfil (DM) | ~340 [?] | não | - |
| Thread | padrão **480** (`threadSidebarWidth ?? da6=480`) | sim, persistido por usuário | [F] padrão; limites [?] |
| Post de fórum | padrão `(innerWidth - 240)/2` (calculado 1x) | sim | [F] `postSidebarWidth` |
| Chat de chamada de voz | 480 | sim | [F] `callChatSidebarWidth` |
| Pedidos de mensagem | 650 | sim | [F] `messageRequestSidebarWidth = iy = 650` |
| Plateia de palco | 320 | sim | [F] |
| Home/"Ativo agora" | `max((innerWidth - 240)*.4, 480)` | sim | [F] `homeSidebarWidth` |
| Alça de redimensionar | **8 px** (`Modules.chat.RESIZE_HANDLE_WIDTH`) | - | [F] |
| Seletor de emoji | mín. **498**, vai até a largura total; `expressionPickerWidth` | sim | [F] |

### 2.4 Densidade

Configuração de interface com 4 modos: Responsivo, Compacto, Padrão, Confortável. **Responsivo** = confortável se `(min-width:1024px) and (min-height:820px)`, senão compacto. [F] (`157146`: `Un="(min-width: 1024px) and (min-height: 820px)"`)

| Token | compacto | padrão | confortável | Evidência |
|---|---|---|---|---|
| `channels.NAME_LINE_HEIGHT` | 20 | 24 | 28 | [F] |
| `channels.SPINE_OFFSET_LEFT` | 20 | 24 | 28 | [F] |
| `channels.SPINE_INVERTED_OFFSET_TOP` | 2 | 6 | 9 | [F] |
| `guildbar.AVATAR_SIZE` | 40 | 44 | 44 | [F] (com experimento "refresh-fast-follow-avatars"; sem ele é 40 em todos) |
| Escalas de fonte do chat | 12,14,15,**16**,18,20,24 | | | [F] `FONT_SIZES`, padrão 16 |
| Zoom da interface | 50,67,75,80,90,**100**,110,125,150,175,200 | | | [F] `ZOOM_SCALES` |
| Alturas de controle | `control.INPUT_HEIGHT_MD/ITEM_HEIGHT_MD`=40, `..._SM`=32; `form.INPUT_HEIGHT`=44; `select.OPTION_HEIGHT`=40, `select.MAX_WIDTH`=248 | | | [F] |
| Menu | borda 1, `SCROLLER_BLOCK_PADDING` 8 | | | [F] |
| Ícones | 32 / 24 / 18 / 16 / 12 (LG/MD/SM/XS/XXS) | | | [F] `icon.SIZE_*` |
| Avatar de chat | 40 | | | [F] `chat.AVATAR_SIZE` |
| Espaços | 4, 8, 12, 16, 24, 32, 40, 48, 56, 64, 72, 80, 96 | | | [F] `Spacing` |
| Breakpoints (tokens) | 480, 640, 768, 1024, 1280, 1536, 1800, 2500 | | | [F] existem; uso real no CSS [?] |

### 2.5 Ordem z (de baixo para cima)

Não há `z-index` nomeado no JS (só 14 ocorrências soltas). A ordem vem de **portais nomeados** e da **ordem de montagem** [F]: `createLayer("App" | "Chat" | "Verification")` gera `Layer` (portal), `LayerContainer` (nó onde os portais desembocam) e `LayerProvider`. Popouts, tooltips e menus são desenhados dentro do contêiner da camada onde nasceram; o que montou depois fica por cima. Tabela de trabalho:

| # | Camada | Quem | Observações | egui (`Order`) |
|---|---|---|---|---|
| 0 | Casca | barra de aplicativo, aviso, barra lateral, conteúdo, coluna direita | base; quando há camada em cima: opacity 0, scale .93 | `Background` (Panels) |
| 1 | Popouts do chat | reações, respostas, hover-toolbar | contêiner "Chat" | `Foreground` |
| 2 | Popouts do app | cartão de perfil, menu de status, seletor de emoji, menu do servidor | contêiner "App" | `Foreground` |
| 3 | Menus de contexto | clique direito, Shift+F10 | fechados por `CONTEXT_MENU_CLOSE` | `Foreground` |
| 4 | **Camadas de tela cheia** | Configurações (usuário/servidor/canal) e outras páginas cheias | `LayerStore` + `fullScreenLayers`; só a do topo é "SHOWN"; as de baixo ficam `HIDDEN` (scale .93, opacity 0) salvo `showAppUnderLayer` | `Middle` (cobre a casca) |
| 5 | Backdrop + Modais | confirmações, convite, criar servidor | pilha com `stackingBehavior`; backdrop `default | lightbox | blur` | `Foreground` (via `egui::Modal`) |
| 6 | Quick Switcher | Ctrl+K | `QuickSwitcherStore.isOpen`; **não abre se há camada de tela cheia** | `Foreground` |
| 7 | Toasts | avisos efêmeros | contêiner próprio por "surface" | `Foreground` |
| 8 | Tooltips | texto de ajuda | `dismissOnEscapeKey:false`; não interativos | `Tooltip` |

Ordem 5-7 entre si é [I]: toasts e quick switcher não têm z-index no JS; escolhi toast acima de modal para erros de envio ficarem visíveis.

---

## 3. Estados e interações

### 3.1 Alça da barra lateral
- Repouso: filete de 1 px `LINE`, alvo de clique de 8 px [F: `RESIZE_HANDLE_WIDTH=8`], cursor `ResizeHorizontal`.
- Hover/arrasto: filete `LINE2` -> `AMBER` em 0.12 s (Grafite). Ao atingir 264 ou 432 durante o arrasto: filete `AMBER_HOT` [I].
- Foco por teclado: anel âmbar 2 px; `role=separator`, `aria-valuenow/min/max`. [F]
- Clique simples alterna recolher (desligado no 1o corte).

### 3.2 Camadas de tela cheia (configurações)
Há **dois mecanismos** no bundle [F]:
- **`LayerStore`** (empilha componentes: configurações de canal e de servidor, "playground"): cada camada é `MI` com `mode SHOWN|HIDDEN`. Entrada: nasce `opacity 0, scale 1.1` e anima para `1, 1` com **mola `friction 10, tension 100`** (Animated.spring do RN). Saída: `opacity 0, scale 1.1`. Camada de baixo ao ser coberta (ou a base): `opacity 0, scale .93`, mesma mola. A base também fica `aria-hidden`. O emissor de eventos fica pausado **500 ms** durante a animação (`Emitter.pause(500)`).
- **`fullScreenLayers`** (store novo, `openFullScreenLayer`; é a via das demais telas cheias): `CSSTransition` com `timeout = 150 ms` (`FULL_SCREEN_LAYER_ANIMATION_DURATION=150ms`), classes enter/exit; opções `disableAnimation`, `onEscape`, `showAppUnderLayer` (a base continua visível por baixo). Esc fecha a camada (`POP_LAYER`) a menos que `onEscape()` devolva true.
- Para egui: **fade de 150 ms** (opcional: escala 1,03 -> 1). A mola de 500 ms só vale a pena se sobrar tempo.
- Movimento reduzido: sem escala (`transform: undefined`), só opacity; popouts usam FADE. [F]
- Foco: ao abrir, foco vai para a camada e `Esc` fecha (a menos que o `onEscape` consuma). Ao fechar, **foco volta ao elemento que o tinha antes** (`useFocusLock` com `returnRef`). [F]

### 3.3 Modais
- Larguras: **pequeno 442, médio 602, grande 800**; padding horizontal 24, vertical 16. [F] `Modules.modal`
- Transição: estados `ENTERING -> ENTERED -> EXITING -> EXITED` (+ `HIDDEN`), cada um com `setTimeout` de **300 ms** (`xt=300`), instantâneo se `instant`. [F]
- Backdrop: variantes `DARK` (padrão), `LIGHTBOX`, `IMMERSIVE`, `BLUR`; clique no backdrop envia `MODAL_CLOSE`. [F] Cor no Telacast: preto 60% (`Color32::from_black_alpha(153)`), sem blur. [I]
- Pilha: `stackingBehavior`: `replace` (padrão: o de baixo fica invisível), `stack` (o de baixo continua visível), `replaceAll`. Um modal pode declarar `stackNextByDefault`. `dismissable:false` ignora Esc/backdrop. [F]
- Enter confirma o modal do topo **somente fora do modo teclado** (`MODAL_SUBMIT`). [F]

### 3.4 Popouts
- Posição: `top|bottom|left|right|center|window_center|overlap_*`; alinhamento `left|center|right|top|bottom`; `spacing` (px da âncora), `offset`; `autoInvert` troca de lado se não couber (e testa o par "overlap" se nenhum cabe); `nudgeAlignIntoViewport` empurra para dentro mantendo **12 px** de margem; acompanha a âncora (ResizeObserver + layout shift). [F] (`calculatePositionStyle` / `calculateState`)
- Animação: `TRANSLATE` (1), `SCALE` (2), `FADE` (3), `NONE`; classes por posição de origem. Movimento reduzido força FADE. Durações no CSS [?]; propor **120 ms** ease-out.
- Fecha: clique fora (`closeOnClickOutside`), Esc (`POPOUT_CLOSE`), saída do viewport (`system:viewport_exit`), scroll se `scrollBehavior:"close"`. `ignoreModalClicks` -> só fecha depois dos modais (`POPOUT_CLOSE_AFTER_MODALS`). [F]

### 3.5 Tooltips
- **Atraso padrão 0 ms** (`delay ?? 0`); o tooltip de texto truncado usa **500 ms** e `overflowOnly` (só aparece se o texto estourou). [F]
- Abre em hover; em foco **apenas no modo teclado**; fecha em mouse out, blur, clique (`hideOnClick`), menu de contexto, Esc. [F]
- Animação: mola `tension 2400, friction 52` (~80-100 ms), `scale .95 -> 1`, `opacity 0 -> 1`; movimento reduzido: só opacity. [F]
- Distância da âncora **11 px**, seta de 16 x 10; largura máxima 200 (rico 272, rico-grande 372). Atalho de teclado aparece à direita do texto. Posição padrão `top`. [F]

### 3.6 Menus de contexto
- Abrem na posição do ponteiro (`pageX/pageY`, retângulo de largura zero). Pelo teclado (Shift+F10 / tecla Menu): na seleção de texto, senão no centro do elemento. [F]
- Fecham com Esc (primeiro da cadeia depois de chamada/mídia), clique fora, ação. Navegação: Setas, Home/End, Enter; `isUsingKeyboardNavigation` liga o modo teclado global. [F]
- Largura/altura de item: item 32 (`control.ITEM_HEIGHT_SM`), padding do rolável 8. [F/I]

### 3.7 Toasts
- Um por vez por "surface"; novos entram numa **fila**; ao expirar, `VD()` promove o próximo. [F]
- Duração padrão **3000 ms** (usuário escolhe 3 a 20 s); animação **300 ms**; atraso de entrada de item da fila **400 ms**; largura máx. **280**; **2 linhas** (reticências). Posição `top` (padrão) ou `bottom`. [F]
- Deslocamento: topo = `--custom-app-top-bar-height`, base = **132 px** (acima da caixa de entrada). [F]
- Variantes: `default`, `success` (ícone positivo), `critical` (ícone crítico). Anunciado ao leitor de tela (`assertive` se crítico). Não rouba foco. [F]

### 3.8 Aviso (banner) do topo
- Um único por vez, o de **maior prioridade** cuja condição vale; só existe **conectado** (`if(!isConnected) return`). Pode ser dispensado (X "Ignorar" `WAI6xu`, volta depois conforme `dismissUntil`). [F]
- Variantes de cor: `DEFAULT, NEUTRAL, BRAND, WARNING, DANGER, INFO, STREAMER_MODE, CUSTOM, SPOTIFY, PLAYSTATION, PREMIUM_*`. [F]
- Estrutura: botão de fechar (`PM`, aria "Ignorar"), texto, âncora/botão de ação (`eC` link, `Z_` botão primário; `zr` botão menor). Altura/padding [?]; proposta **36 px**, texto 13,5/600.
- Tipos que importam ao Telacast (da lista de ~65): manutenção programada (DEFAULT, com link de status), atualização (botão na barra de aplicativo, não banner), erro de dispositivo de áudio (DANGER), voz restaurada, conta não verificada. Ordem real: quarentena, consentimento, ..., manutenção, e-mail não verificado... [F: array `c0`]
- Manutenção (pt-BR): "{app} estará em manutenção programada em {data longa} às {hora curta}." + link "Saiba mais". (`yb96S+`)

### 3.9 Carregamento / conexão / erro (resumo; detalhe na seção 4.7)
- **Splash "Conectando..."**: tela cheia com animação do mascote (Telacast: **sem** mascote; só o "t" serifado e uma linha de dica). Aparece enquanto `isTryingToConnect` (**incluindo reconexões**). Fade do contêiner **200 ms**, do conteúdo **150 ms** (`--connecting-container-fade-duration`, `--connecting-content-fade-duration`). Depois de **10 s** sem conectar mostra "Problemas de conexão? Nos avise!" + link de status. Se há incidente publicado, o título passa a ser o nome do incidente e o texto o corpo da última atualização. [F]
- **Tela de erro** (`ErrorBoundary`): título "Eita, que saia justa" (`3h+n+8`), texto "Parece que o {app} parou de funcionar do nada..." (`tx8CkI`) + "Identificamos o erro e vamos consertá-lo o quanto antes." (`CvQlAH`), botão primário "Recarregar" (`+hivLW`). Fundo sem imagem (`hideBackground`), ilustração central. Registra o erro (soft crash). [F]

### 3.10 Foco
- **Modo teclado** (`keyboardModeEnabled`): liga ao pressionar Tab, F6, Setas, Home ou End (fora de campo de texto, exceto Tab; sem Ctrl/Alt/Meta; e se a tecla não for atalho já vinculado). Desliga em `mousedown`. Na 1a vez mostra um modal explicativo. [F]
- Em modo teclado o anel de foco aparece e Enter não confirma modais por conta própria (o Enter ativa o foco). [F]
- Anel de foco do Telacast: 2 px `AMBER`, deslocamento 2 px, raio do widget (igual a `:focus-visible` da maquete). [I]

---

## 4. Regras de comportamento

### 4.1 Redimensionamento e pontos de quebra
O que o bundle garante: janela mín. **800 x 500** [F]; densidade responsiva em **1024 x 820** [F]; barra lateral 264-432 [F]. O JS **não** contém lógica de largura para esconder a coluna direita (isso é CSS, não veio). O seguinte é minha regra [I], derivada dessas medidas (com a barra lateral padrão 375, a 800 de largura sobram 425 para o centro, então a coluna direita **tem** de sumir ali):

Seja `W` a largura lógica da janela (já descontado o zoom: com zoom 200% uma janela 1280 vira 640).

1. `sidebar_w = clamp(guardada, 264, 432)`.
2. Largura desejada da direita `R` por seção (2.3). `centro = W - sidebar_w - R`.
3. Se `centro >= CENTER_MIN (420)`: mostra tudo.
4. Senão, **cai nesta ordem**:
   a. Thread/post/busca: encolhe até **340**; ainda não cabe -> vira **sobreposição** sobre o centro (largura = `W - sidebar_w`), com botão "Voltar".
   b. Membros/perfil/amigos: **somem** (a preferência `members_open` do usuário não muda; volta sozinho quando couber, sem animação).
   c. Se `W - sidebar_w < CENTER_MIN`: a barra lateral encolhe (só visualmente, sem gravar) até 264.
   d. Se ainda `< CENTER_MIN` (W < 684): **modo estreito**.
5. **Modo estreito** (`W < 684`, só com zoom alto): a lista de canais vira gaveta sobre o conteúdo; fica só o trilho (76); botão "Canais" no cabeçalho do canal abre a gaveta; selecionar um canal fecha a gaveta. A barra de aplicativo mantém título e ícones.
6. Alturas: `H < 600` -> densidade compacta (já pelo token 820), barra de aplicativo mantém 36.
7. Toda mudança de visibilidade é **instantânea** (sem animação) [I], para não animar durante o arrasto da janela.

### 4.2 Atalhos globais (padrão; `mod` = Ctrl no Linux/Windows)

| Ação | Teclas | Notas | Evidência |
|---|---|---|---|
| Quick Switcher | `mod+k` (e `mod+t` quando não há abas) | **não abre** com camada de tela cheia; reabrir fecha | [F] `r.A.hasLayers()||WU()` |
| Canal anterior / seguinte | `alt+up` / `alt+down` | | [F] |
| Próximo canal não lido | `alt+shift+up/down` | | [F] |
| Canal com menção anterior/seguinte | `mod+shift+alt+up/down` | | [F] |
| Servidor anterior/seguinte | `mod+alt+up/down`, `mod+shift+[` e `mod+shift+]`, `ctrl+shift+tab`/`ctrl+tab` | com abas ligadas, Ctrl+Tab troca de aba | [F] |
| Alternar último servidor <-> DMs | `mod+alt+right` | | [F] |
| Rolar a lista até o canal atual | `mod+alt+left` | | [F] |
| Ir ao servidor N | `mod+1` = Mensagens diretas, `mod+2...9` = servidores na ordem do trilho | `0` vira 10 | [F] |
| Histórico voltar/avançar | `alt+left` / `alt+right` (Mac: `mod+[` / `mod+]`) | | [F] |
| Lista de atalhos | `mod+/` ou `mod+shift+/` (alterna) | | [F] |
| Ajuda | `F1` ou `mod+shift+h` | | [F] |
| Membros / perfil (DM) / chat de voz | `mod+u` | | [F] |
| Fixadas | `mod+p` | | [F] |
| Caixa de entrada | `mod+i` | bloqueado com camada/modal | [F] |
| Buscar | `mod+f` ou `mod+shift+f` | foca a busca | [F] |
| Emoji / GIF / Figurinha | `mod+e` / `mod+g` / `mod+s` | bloqueados com camada | [F] |
| Silenciar / ensurdecer | `mod+shift+m` / `mod+shift+d` | | [F] |
| Criar DM de grupo | `mod+shift+t` | | [F] |
| Criar/entrar servidor | `mod+shift+n` | | [F] |
| Enviar arquivo | `mod+shift+u` | | [F] |
| Criar thread | `mod+shift+alt+n` | | [F] |
| Copiar link do canal | `mod+shift+l` | | [F] |
| Recolher/expandir todas as categorias | `mod+shift+a` | **corrige** `docs/discord/10`: não marca como lido | [F] |
| Marcar servidor como lido | `shift+esc` | | [F] |
| Pular p/ 1a mensagem não lida | `shift+pageup` | | [F] |
| Rolar o chat | `pageup` / `pagedown` | | [F] |
| Menu de contexto | `shift+f10` | | [F] |
| Zoom | `mod+plus`, `mod+minus`, `mod+0` | passos de `ZOOM_SCALES` | [F] |
| Navegar sem mouse | `Tab`, `F6` (ver 4.4) | | [F] |

Todos os binds têm `comboKeysBindGlobal:true` (valem com foco em qualquer lugar, inclusive campo de texto) e a maioria checa `getLayers().length>0 || hasAnyModalOpen()` antes de agir. [F]

### 4.3 Esc: cadeia de prioridade (primeiro que se aplica, consome e para) [F: `92960`]
1. Elemento em tela cheia (vídeo): sai da tela cheia (o navegador cuida).
2. Chamada recebida: recusa.
3. Menu de contexto aberto: fecha.
4. Visualizador de mídia: fecha.
5. Popout aberto: fecha.
6. Seletor de GIF/figurinha: fecha.
7. **Modal do topo**: fecha (se `dismissable !== false`).
8. Modo foco de conversas: sai.
9. Resultados de busca: fecha.
10. Popouts "depois dos modais": fecha.
11. No canal atual (e no canal da sidebar, se houver): cancela **edição de mensagem**; senão cancela **resposta pendente**; senão limpa **rascunho agendado**; senão limpa **prévia de figurinha**.
12. Cancela colocação de nota/overlay de sala (recurso fora de escopo).
13. Se a sidebar é "criar thread": fecha.
14. Cauda (`b(canal)`, executada para o canal e para o canal da sidebar): se há mensagens mais novas não carregadas, **salta para o presente**; se há não lida, **marca o canal como lido**. Depois:
    - se `b` fez algo (ou não há sidebar/canal): se nada foi feito, o chat de texto-em-voz está aberto e no fim, fecha-o; senão **rola até o presente** (`SCROLLTO_PRESENT`);
    - se `b` não fez nada **e** há sidebar de canal (thread/post): **fecha a sidebar**.
    Ou seja: o Esc "limpa" o canal (lido + fim) antes de fechar a thread; só no 2o Esc a thread fecha.

A camada de tela cheia tem Esc próprio (`POP_LAYER`, a menos que o `onEscape` do conteúdo devolva true) e **desliga** os binds globais enquanto aberta (`iV.A.disable()` + só `esc`). [F]

### 4.4 Teclado entre regiões
- **F6 / Shift+F6**: só no modo teclado. Pega o elemento focado, sobe até o ancestral com `data-jump-section`, e move o foco para o **próximo/anterior elemento marcado com o mesmo valor de seção** (padrão `"global"`), com volta ao início. Sem foco -> primeiro. [F] (`312138`). As "entradas" marcadas no cliente: primeiro botão da barra de aplicativo (voltar/caixa de entrada/ajuda), avatar do painel de conta, etc.
  - Mapa Telacast (ciclo): `barra de aplicativo` -> `trilho` -> `lista de canais` -> `conteúdo (mensagens)` -> `caixa de mensagem` -> `coluna direita` -> `painel de conta`.
- **Tab / Shift+Tab**: ordem do DOM; o menu e os modais prendem o foco (`useFocusLock`), e ao fechar devolvem o foco ao que o tinha. [F]
- **Digitar em qualquer lugar** (`any-character`): se o foco não está em campo editável, não há modal/camada, não há Ctrl/Meta, e (em modo teclado) o foco está no `body` ou a tecla é alfanumérica: **foca a caixa de mensagem e entrega a tecla**. `Tab` com a caixa disponível (`[data-can-focus=true]`) foca a caixa. `Enter` sem Shift/Alt foca e marca "enter pressionado". Teclas de mídia/F1-F20/Alt/Shift soltos são ignoradas; com PTT ativo, a tecla do push-to-talk não rouba o foco. [F] (`82038`)
- **Setas/Home/End/PageUp/PageDown** nas listas: navegação de lista (árvore do trilho: `role=tree`).
- Quick Switcher (`mod+k`): campo único; prefixos **`@` usuários, `#` canais de texto, `!` canais de voz, `*` servidores, `$` jogos; `@@` usuários globais**. Sem texto: recentes. Setas movem (`QUICKSWITCHER_RESULT_FOCUS`), Enter abre e **fecha todos os modais**, Esc fecha. Máx. de consulta `maxQueryLength` [?]. [F]

### 4.5 Zoom
`mod+plus/minus/0` percorrem `ZOOM_SCALES` e chamam `setZoomFactor`. [F] No egui: `ctx.set_zoom_factor(z/100)`; **desligar** o zoom embutido (`options.zoom_with_keyboard=false`) para usar a mesma escada.

### 4.6 Barra de aplicativo: títulos por rota [F: `a4`]
Servidor selecionado: ícone pequeno do servidor + nome; DM/grupo: "Mensagens diretas" (`YUU0RF`); Amigos: "Amigos" (`TdEu5X`); notificações: "Notificações" (`HcoRu0`); descobrir, loja etc. fora de escopo.

### 4.7 Conexão
Discord [F]: componente `MZ` renderiza o splash `Mq` sempre que `GatewayConnectionStore.isTryingToConnect()` (socket aberto/abrindo, não "fechado"). `componentDidUpdate`: quando `connected` muda, zera `problems` e define `hide = connected`; ao esconder, desmonta após **200 ms**. `problems` liga 10 000 ms depois de montar. Ao reconectar o gateway com voz ativa e reinício recente (<5 min) volta à chamada. O RTC (voz) usa backoff 1-5 s. O backoff do gateway principal está em módulo que não consegui isolar [?].

Telacast (decisão de design, ver pergunta 2): **splash só na 1a conexão** (carregamento inicial, resume do token); **reconexão** = interface mantida, aviso DANGER no topo "Sem conexão. Reconectando em {n}s..." + botão "Tentar agora" (sem fechar o app) e caixa de mensagem desabilitada com texto "Sem conexão". Backoff 1, 2, 4, 8, 10 s (teto) com jitter 0-25%. Ao reconectar com sucesso: aviso some com fade 200 ms e um toast "Reconectado" (3 s). Se o token expirou (`Error` de auth), volta ao login com mensagem.

### 4.8 Atualização
Botão de download na barra de aplicativo (verde `GREEN_360` no Discord; no Telacast **âmbar**), 3 estados: oculto, "Baixando atualização" (desabilitado, tooltip), "Atualização pronta!" (clicável). Em voz ativa pede confirmação: "Sair temporariamente do canal de voz?" -> "Atualizar mesmo assim!" / "Cancelar". [F]

---

## 5. Dados necessários (servidor/protocolo)

Hoje `proto` tem C2S: Register/Login/Resume/Logout/CreateServer/JoinServer/CreateChannel/Send/History/Friend*/OpenDm e S2C: Auth/Error/Snapshot/Message/History/DmOpened/ServerJoined. Falta, em ordem de prioridade:

1. **Estado de conexão no cliente** (sem mudar o servidor): `gateway.rs` hoje abre uma conexão e **morre** em `Closed`; o `App` derruba a sessão (`logout`). Precisa: loop de reconexão com backoff dentro de `gateway::open`, novos eventos `Event::Reconnecting { attempt, retry_in }` e `Event::Resumed`. Reenviar `Resume{token}` após reconectar.
2. **`C2S::Ping` / `S2C::Pong`** (ou ping WebSocket do tungstenite a cada 20 s) para detectar conexão morta; hoje só descobre quando a escrita falha.
3. **`S2C::Notice { id, kind, severity: info|warn|danger, text, link: Option<String>, dismissible, until: Option<i64> }`** + `C2S::DismissNotice { id }` (manutenção programada, versão mínima do cliente, aviso do operador).
4. **`S2C::Hello { server_version, min_client_version, motd: Option<String> }`** logo após Auth: dispara aviso "Atualização necessária" e alimenta o botão de atualização.
5. **Estado de leitura no servidor** (para Alt+Shift+setas e `Esc`/`Shift+Esc`): tabela `read_state(user_id, channel_id, last_read_msg_id, mention_count)` e `C2S::Ack { channel_id, message_id }`, `C2S::AckServer { server_id }`; `Snapshot` passa a trazer `unread: [{channel_id, mentions}]`. Hoje `unread` é `HashSet` local e se perde ao fechar.
6. **Preferências de interface do usuário**: `C2S::SetPrefs { json }` / `S2C::Prefs` (largura da barra lateral, `members_open`, densidade, zoom, duração de toast). Pode começar 100% local (eframe `Storage`); só vira protocolo se quiser sincronizar entre máquinas.
7. **Busca de usuários** para o Quick Switcher global (`@@`): `C2S::SearchUsers { q }` / `S2C::UserResults`. Sem isso o Quick Switcher só acha o que já está no `Snapshot`.
8. **Colunas/índices**: `read_state` PK `(user_id, channel_id)`, índice em `(channel_id, id)` de mensagens já existente; `notices` (id, kind, text, starts_at, ends_at); `user_prefs(user_id, json)`.

---

## 6. Plano de implementação em egui 0.37

### 6.1 Onde entra
- `theme.rs`: constantes de layout (`WIN_MIN`, `SIDEBAR_*`, `CENTER_MIN`, `MEMBERS_W`, `THREAD_W`), `Density`/`Metrics`, `focus_ring`, `banner()`, `toast_card()`, `Z`.
- Novo `app/src/shell.rs`: `ShellState` (sidebar_w, members_open, profile_open, search_open, right_override, narrow_drawer, kbd_mode, region), `resolve_layout(win, &ShellState) -> Layout`, `session_ui` chamando painéis na ordem certa.
- Novo `app/src/layers.rs`: `Overlays` (pilha de modais, camadas cheias, quick switcher, popout, menu), `handle_escape`.
- Novo `app/src/keys.rs`: `global_keys` e F6.
- Novo `app/src/conn.rs`: `Link` (estado de conexão), splash, banner de conexão.
- `main.rs`: `with_min_inner_size([800.0, 500.0])` (hoje 900 x 520); trocar o `Modal` enum e o `modals()` atual por `Overlays`; `toast()` por fila.

Regra do egui: **painéis de fora para dentro** (top antes de left/right, todos antes do `CentralPanel`).

### 6.2 Trecho 1: métricas e resolução do layout

```rust
// shell.rs
use eframe::egui::{self, vec2, Vec2};

pub const WIN_MIN: [f32; 2] = [800.0, 500.0];   // [F]
pub const SIDEBAR_MIN: f32 = 264.0;             // [F]
pub const SIDEBAR_MAX: f32 = 432.0;             // [F]
pub const SIDEBAR_DEFAULT: f32 = 375.0;         // [F]
pub const CENTER_MIN: f32 = 420.0;              // [I]
pub const MEMBERS_W: f32 = 240.0;               // [I]
pub const THREAD_W: f32 = 480.0;                // [F]
pub const THREAD_W_MIN: f32 = 340.0;            // [I]
pub const APPBAR_H: f32 = 36.0;                 // [?]

#[derive(Clone, Copy, PartialEq)]
pub enum Density { Responsive, Compact, Default, Cozy }

#[derive(Clone, Copy)]
pub struct Metrics { pub rail_icon: f32, pub rail_w: f32, pub name_lh: f32 }

impl Metrics {
    pub fn new(d: Density, win: Vec2) -> Self {
        let d = match d {
            Density::Responsive if win.x >= 1024.0 && win.y >= 820.0 => Density::Cozy,
            Density::Responsive => Density::Compact,
            x => x,
        };
        let (icon, lh) = match d {
            Density::Compact => (40.0, 20.0),
            Density::Default => (44.0, 24.0),
            _ => (44.0, 28.0),
        };
        Self { rail_icon: icon, rail_w: icon + 32.0, name_lh: lh }
    }
}

#[derive(Clone, PartialEq)]
pub enum Right { None, Members, Profile, Thread(i64), Search }

pub struct Layout {
    pub sidebar_w: f32,
    pub right: Right,
    pub right_w: f32,
    pub right_overlay: bool, // thread/busca que não coube: cobre o centro
    pub narrow: bool,
}

pub fn resolve(win_w: f32, st: &ShellState, is_dm: bool) -> Layout {
    let mut sidebar_w = st.sidebar_w.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
    let wanted = if st.search_open { Right::Search }
        else if let Some(t) = st.thread { Right::Thread(t) }
        else if is_dm && st.profile_open { Right::Profile }
        else if !is_dm && st.members_open { Right::Members }
        else { Right::None };
    let mut right_w = match wanted {
        Right::None => 0.0,
        Right::Members => MEMBERS_W,
        Right::Profile => 340.0,
        Right::Thread(_) | Right::Search => st.right_w.max(THREAD_W_MIN), // padrão THREAD_W
    };
    let mut right = wanted.clone();
    let mut overlay = false;
    if win_w - sidebar_w - right_w < CENTER_MIN {
        match wanted {
            Right::Thread(_) | Right::Search => {
                let fit = win_w - sidebar_w - CENTER_MIN;
                if fit >= THREAD_W_MIN { right_w = fit } else { overlay = true; right_w = win_w - sidebar_w }
            }
            Right::None => {}
            _ => { right = Right::None; right_w = 0.0 }
        }
    }
    if win_w - sidebar_w - if overlay { 0.0 } else { right_w } < CENTER_MIN {
        sidebar_w = (win_w - CENTER_MIN).max(SIDEBAR_MIN); // só visual, não grava
    }
    let narrow = win_w - SIDEBAR_MIN < CENTER_MIN;
    Layout { sidebar_w, right, right_w, right_overlay: overlay, narrow }
}
```

Em `session_ui` a ordem fica: `Panel::top("appbar")` -> `Panel::top("notice")` (só se houver aviso) -> `Panel::left("sidebar")` -> `Panel::right("right")` (se `right != None && !overlay`) -> `CentralPanel`. A largura vem de `ctx.content_rect().width()` (no 0.37 `screen_rect` virou `content_rect`).

### 6.3 Trecho 2: barra lateral única com trilho, canais e cartão de usuário em largura total

```rust
fn sidebar(&mut self, ui: &mut egui::Ui, m: Metrics, lay: &Layout, s: &Session, acts: &mut Vec<Act>) {
    use egui::{pos2, Rect, UiBuilder};
    let pr = egui::Panel::left("sidebar")
        .resizable(!lay.narrow)
        .size_range(SIDEBAR_MIN..=SIDEBAR_MAX)
        .default_size(SIDEBAR_DEFAULT)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(SIDE))
        .show(ui, |ui| {
            let full = ui.available_rect_before_wrap();
            let card_h = 52.0 + 16.0;
            let body = Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - card_h));
            let rail = Rect::from_min_max(body.min, pos2(body.min.x + m.rail_w, body.max.y));
            let chan = Rect::from_min_max(pos2(rail.max.x, body.min.y), body.max);
            ui.painter().rect_filled(rail, 0.0, RAIL);
            ui.scope_builder(UiBuilder::new().max_rect(rail), |ui| self.rail_ui(ui, m, s, acts));
            ui.scope_builder(UiBuilder::new().max_rect(chan), |ui| self.channels_ui(ui, m, s, acts));
            let card = Rect::from_min_max(pos2(full.min.x + 8.0, body.max.y + 8.0), pos2(full.max.x - 8.0, full.max.y - 8.0));
            ui.scope_builder(UiBuilder::new().max_rect(card), |ui| self.account_card(ui, s, acts));
            ui.allocate_rect(full, egui::Sense::hover()); // reserva o espaço
        });
    // largura para gravar: pr.response.rect.width()
    self.shell.sidebar_w = pr.response.rect.width();
    edge(ui, pr.response.rect, true);
}
```

Armadilhas: (a) o `Panel` redimensionável do egui guarda a largura na memória do contexto; com `eframe` `persistence` ela já sobrevive entre execuções, mas confira se `persist_egui_memory` está ligado (padrão sim). (b) `size_range` vale para o painel inteiro, então os 264 incluem o trilho. (c) `Panel` não anima o arrasto, e não tem o "snap" ao clicar na alça: se quiser o clique-para-recolher, trate `pr.response` + `Sense::click` sobre uma faixa de 8 px na borda direita (`Rect::from_x_y_ranges`). (d) `show_collapsible` existe no 0.37 e anima, mas o Discord não anima a casca: use só no modo estreito.

### 6.4 Trecho 3: pilha de camadas e a cadeia do Esc

```rust
// layers.rs
pub struct ModalEntry { pub key: u64, pub kind: ModalKind, pub dismissable: bool, pub stack: bool, pub since: Instant }
pub struct Overlays {
    pub ctx_menu: Option<CtxMenu>,
    pub media: Option<Media>,
    pub popout: Option<Popout>,
    pub picker: Option<Picker>,
    pub modals: Vec<ModalEntry>,   // topo = último
    pub quick: Option<Quick>,
    pub full: Vec<FullLayer>,      // configurações; topo = último
}
impl Overlays {
    pub fn any_layer(&self) -> bool { !self.full.is_empty() }
    pub fn blocks_hotkeys(&self) -> bool { self.any_layer() || !self.modals.is_empty() }
}

pub fn handle_escape(ctx: &egui::Context, s: &mut Session, ov: &mut Overlays, ui: &mut UiState) {
    if !ctx.input(|i| i.key_pressed(egui::Key::Escape)) { return }
    let eat = || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if ui.video_fullscreen { eat(); ui.video_fullscreen = false; return }
    if ov.ctx_menu.take().is_some() { eat(); return }
    if ov.media.take().is_some()    { eat(); return }
    if ov.popout.take().is_some()   { eat(); return }
    if ov.picker.take().is_some()   { eat(); return }
    if let Some(m) = ov.modals.last() {
        eat();
        if m.dismissable { ov.modals.pop(); }
        return;
    }
    if ov.quick.take().is_some() { eat(); return }
    if let Some(f) = ov.full.last_mut() { eat(); if !f.on_escape() { ov.full.pop(); } return }
    if s.search_open { eat(); s.search_open = false; return }
    let ch = s.current_channel();
    if let Some(ch) = ch {
        if s.editing.remove(&ch).is_some()     { eat(); return }
        if s.reply_to.remove(&ch).is_some()    { eat(); return }
    }
    // cauda: limpar o canal primeiro; só depois fechar a thread
    if let Some(ch) = ch {
        eat();
        let did = s.jump_to_present(ch) | s.mark_read(ch);   // `|` sem curto-circuito: executa os dois
        if did { s.scroll_to_present(ch); return }
        if s.shell.thread.take().is_some() { return }
        s.scroll_to_present(ch);
    }
}
```

Chame `handle_escape` **antes** de `modals()` (senão `egui::Modal::should_close` consome o Esc por conta própria). Para modais, use `egui::Modal::new(id).show(ctx, ...)` (traz backdrop clicável, foco preso e Esc só no topo) e ignore `should_close()` quando `dismissable == false`. Para as camadas de tela cheia use `egui::Area::new(id).order(Order::Middle).fixed_pos(content_rect.min)` com um `Frame` opaco L2 do tamanho de `content_rect()`, animando opacidade com `ctx.animate_bool_with_time(id, aberto, 0.15)` + `ui.set_opacity(t)`; escala só se tiver tempo (`ctx.set_transform_layer(layer_id, TSTransform::from_translation_scaling(..))` existe, mas confira a assinatura no 0.37).

### 6.5 Trecho 4: teclado global, modo teclado e F6

```rust
// keys.rs: chamar no INÍCIO de ui(), antes de qualquer painel (o TextEdit do egui trata Ctrl+K como "apagar até o fim da linha")
pub fn global_keys(ctx: &egui::Context, s: &mut Session, ov: &mut Overlays, acts: &mut Vec<Act>) {
    use egui::{Key, Modifiers as M};
    let cmd = M::COMMAND;                                   // Ctrl no Linux/Windows
    let alt_shift = M { alt: true, shift: true, ..M::NONE };
    let cmd_alt = M { command: true, alt: true, ..M::NONE };
    let layer = ov.any_layer();
    let blocked = ov.blocks_hotkeys();
    ctx.input_mut(|i| {
        if i.consume_key(cmd, Key::K) && !layer { acts.push(Act::Quick) }
        if blocked { return }
        if i.consume_key(M::ALT, Key::ArrowUp)   { acts.push(Act::PrevChannel) }
        if i.consume_key(M::ALT, Key::ArrowDown) { acts.push(Act::NextChannel) }
        if i.consume_key(alt_shift, Key::ArrowUp)   { acts.push(Act::PrevUnread) }
        if i.consume_key(alt_shift, Key::ArrowDown) { acts.push(Act::NextUnread) }
        if i.consume_key(cmd_alt, Key::ArrowUp)   { acts.push(Act::PrevServer) }
        if i.consume_key(cmd_alt, Key::ArrowDown) { acts.push(Act::NextServer) }
        if i.consume_key(cmd_alt, Key::ArrowRight) { acts.push(Act::ToggleLastServerDms) }
        if i.consume_key(cmd, Key::U) { acts.push(Act::ToggleRight) }
        if i.consume_key(cmd, Key::Slash) { acts.push(Act::Shortcuts) }
        for (n, k) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9].into_iter().enumerate() {
            if i.consume_key(cmd, k) { acts.push(Act::GoServerIndex(n)) }
        }
    });
}

// modo teclado + F6
pub fn update_kbd_mode(ctx: &egui::Context, st: &mut ShellState) {
    let (tab_or_nav, pressed) = ctx.input(|i| (
        i.events.iter().any(|e| matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. }
            if matches!(key, egui::Key::Tab | egui::Key::F6 | egui::Key::ArrowUp | egui::Key::ArrowDown
                | egui::Key::ArrowLeft | egui::Key::ArrowRight | egui::Key::Home | egui::Key::End)
            && !modifiers.ctrl && !modifiers.alt && !modifiers.command)),
        i.pointer.any_pressed()));
    let in_text = ctx.wants_keyboard_input();
    if pressed { st.kbd = false } else if tab_or_nav && (!in_text || ctx.input(|i| i.key_pressed(egui::Key::Tab))) { st.kbd = true }
}

pub fn region_of_focus(ctx: &egui::Context, regions: &[(Region, egui::Rect)]) -> Option<Region> {
    let id = ctx.memory(|m| m.focused())?;
    let r = ctx.read_response(id)?.rect;          // retângulo do widget focado no quadro anterior
    regions.iter().find(|(_, rc)| rc.contains(r.center())).map(|(k, _)| *k)
}

pub fn f6(ctx: &egui::Context, st: &ShellState, entries: &[(Region, egui::Id)], regions: &[(Region, egui::Rect)]) {
    if !st.kbd { return }
    let (go, back) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, egui::Key::F6),
                                         i.consume_key(egui::Modifiers::SHIFT, egui::Key::F6)));
    if !(go || back) { return }
    let cur = region_of_focus(ctx, regions).and_then(|r| entries.iter().position(|(k, _)| *k == r));
    let n = entries.len();
    let next = match (cur, back) { (None, _) => 0, (Some(i), false) => (i + 1) % n, (Some(i), true) => (i + n - 1) % n };
    ctx.memory_mut(|m| m.request_focus(entries[next].1));
}
```

Armadilhas gerais:
- **Sem letter-spacing / sem negrito de fonte variável**: usar `RichText::strong()` só onde a fonte já tem peso; para títulos do aviso e do toast use Inter 600 via família dedicada, ou caixa alta + tamanho 11 como no `caps()`.
- `Sense::click()` já torna o widget focável por Tab. Widgets custom pintados precisam desenhar o anel: `if resp.has_focus() && kbd { painter.rect_stroke(resp.rect.expand(2.0), r, Stroke::new(2.0, AMBER), StrokeKind::Outside) }`.
- **Tooltips**: `ctx.global_style_mut(|s| { s.interaction.tooltip_delay = 0.0; s.interaction.show_tooltips_only_when_still = false; s.spacing.tooltip_width = 200.0; })` (padrão do egui: 0,5 s e 600 px; confira os nomes de campo no 0.37). Tooltip de texto truncado: chame `on_hover_text` **somente quando o texto estourou** (compare a largura do `Galley` com o retângulo), o equivalente ao `overflowOnly` com 500 ms do Discord.
- **Emoji colorido** não existe no egui; ícones da barra de aplicativo desenhados com `painter` (traços de 1,5 px).
- `Context::request_repaint_after` obrigatório durante splash, backoff e toasts (hoje `toast` já faz).
- Movimento reduzido: ler preferência do sistema não é trivial em Linux; expor chave `reduce_motion` nas configurações e trocar animações por 0 s (`animate_bool_with_time(.., 0.0)`).
- `ui.ctx().content_rect()` muda com zoom; compare em pixels lógicos.

### 6.6 Banner, toast e splash (resumo)

```rust
pub enum BannerKind { Neutral, Info, Warning, Danger }
pub fn banner(ui: &mut egui::Ui, kind: BannerKind, text: &str, action: Option<&str>, dismissible: bool) -> BannerResp {
    let (fill, ink) = match kind {
        BannerKind::Neutral | BannerKind::Info => (L3, TEXT2),
        BannerKind::Warning => (AMBER, AMBER_INK),
        BannerKind::Danger  => (lerp(L3, RED, 0.16), TEXT),
    };
    // altura 36, padding lateral 16, texto 13.5 centralizado verticalmente,
    // ação = small_button à direita do texto, X de 16 px na ponta direita (tooltip "Ignorar")
    ...
}
```
Toast: fila `VecDeque<Toast>` em `Session`; um visível por vez; `shown_at`; duração `max(toast.duration, 3 s)`; entrada/saída 300 ms (opacidade + deslocamento vertical 8 px, `ease_out`); atraso 400 ms entre itens; posição superior central abaixo da barra de aplicativo (deslocamento = `APPBAR_H + 12`) ou inferior a 132 px da base; largura máx. 280; 2 linhas com reticências (`Label::truncate` + `Galley` com `wrap_mode`); ícone 16 (`success` = `OK`, `critical` = `RED`).

Splash: `egui::Area` com `fixed_pos(content_rect.min)`, fundo `L0`, "t" serifado 64 px no centro, abaixo a linha "Conectando..." (`ULvRFd`) 14 px `MUTED`, e abaixo uma dica rotativa (`Você sabia que ...`) a cada 6 s [I]; após 10 s mostra "Problemas de conexão?" + botões "Tentar agora" e "Trocar de servidor". Fade do contêiner 200 ms, do conteúdo 150 ms.

---

## 7. Ordem de entrega (passos pequenos, cada um verificável)

1. **Métricas e janela mínima.** `with_min_inner_size([800,500])`, `Metrics`, `Density::Responsive`, constantes. Ao final: dá para reduzir a janela a 800 x 500; o trilho passa de 68 para 72/76 conforme o tamanho (1024 x 820 vira o ponto de troca).
2. **Barra lateral única redimensionável (264-432, padrão 375) com alça de 8 px** e persistência. Ao final: arrasta a alça, trava nos limites, fecha e reabre o app e a largura volta.
3. **Cartão de usuário em largura total** sob trilho + canais. Ao final: o cartão cruza as duas colunas.
4. **`resolve()` + coluna direita única** (membros hoje; pronto para thread/busca). Ao final: abrir busca/thread esconde os membros; alternar membros fecha a thread; `Ctrl+U` funciona; reduzir a janela esconde a direita sem mexer na preferência.
5. **Camadas**: `Overlays`, `egui::Modal` para os 4 modais atuais com largura 442/602/800 e padding 24/16, Esc só no topo, `handle_escape` completo (editar/responder ainda só ganchos). Ao final: Esc fecha modal, depois sobe a cadeia.
6. **Teclado global**: `global_keys` (Ctrl+K abre um Quick Switcher mínimo só com canais/servidores/DMs do `Snapshot`, Alt+setas, Ctrl+1..9, Ctrl+/ lista de atalhos), zoom por escada, desligar zoom embutido. Ao final: navegação inteira sem mouse.
7. **Modo teclado + F6 + anel de foco** nos widgets de `theme.rs` (`rail_button`, `list_row`, botões). Ao final: Tab mostra o anel âmbar, clique o esconde, F6 salta entre regiões.
8. **Toast em fila** (300 ms, 280 px, 2 linhas, 3 s, posição) e **tooltip 0 ms** global. Ao final: erros empilham em sequência.
9. **Barra de aplicativo** (título por rota, botão de ajuda/atalhos, botão de atualização oculto) e **banner** (um por vez, 4 variantes). Ao final: `S2C::Error` fatal vira banner; "Atualização necessária" aparece com `min_client_version` falso.
10. **Conexão**: loop de reconexão com backoff, `Link`, aviso "Reconectando em {n}s", splash inicial com "Problemas?" aos 10 s, ping/pong. Ao final: derrubar o servidor mostra o aviso, ligar de novo reconecta sem perder a tela; kill -STOP mostra o timeout.
11. **Tela de erro**: `std::panic::set_hook` + flag e tela "Eita, que saia justa" com Recarregar (reinicia a sessão). Ao final: um `panic!` de teste mostra a tela.
12. **Modo estreito** (gaveta de canais) e sobreposição de thread/busca. Ao final: zoom 200% numa janela de 1280 continua usável.
13. Opcional: voltar/avançar de histórico, abas de canal, recolher barra lateral para 76.

---

## 8. Perguntas em aberto

1. **Barra lateral**: seguir o cliente atual (uma barra de 375 redimensionável 264-432, incluindo o trilho) ou o desenho clássico (trilho 72 fixo + canais 240)? Recomendo o atual; muda toda a conta de larguras e a maquete (`grafite.html` usa 68/252/236).
2. **Queda de conexão**: aviso no topo mantendo a interface (recomendado) ou repetir o comportamento do Discord, que volta à tela cheia "Conectando..." a cada reconexão?
3. **Lista de membros**: abrir por padrão em servidor? O valor inicial no código do cliente é `false` (`isMembersOpen`), mas não consegui confirmar se algum experimento ou migração o liga na prática [?]. Recomendo `true` na primeira execução.
4. **Barra de aplicativo**: desenhar a nossa de 36 px com título/ajuda/atualização, ou usar só a decoração nativa do winit e não ter barra? (A barra é o lugar natural do botão de atualização, caixa de entrada e voltar/avançar.)
5. **Esc marca canal como lido** (comportamento do cliente: sobe a cadeia e, no fim, ack do canal). Manter ou exigir um atalho explícito?
6. **Camadas de configuração**: tela cheia com animação de escala (mola) ou só fade de 150 ms? O fade é bem mais barato em egui.

---

## Apêndice A: strings pt-BR do cliente (para reuso com a marca trocada)

| Uso | Texto | Chave |
|---|---|---|
| Splash | "Conectando..." / "Conectando…" | `7LkwqE`, `ULvRFd` |
| Splash, rótulo da dica | "Você sabia que" | `v0R1Lh` |
| Dica de teclado | "Sem mouse? Use o modo teclado. {Tab} para começar." | `PHuQsQ` |
| Dica de F6 | "Vá para diferentes seções do app com {F6}. {Tab} para começar." | `FVVp79` |
| Problemas | "Problemas de conexão? Nos avise!" | `AG2zPM` |
| Link de status | "Estado do servidor" | `AgXXyy` |
| Sem conexão | "Sem conexão." | `zPerw8` |
| Reconexão | "Reconectando" / "Reconectar" | `PygWIS`, `aTuFYT` |
| Tentar de novo | "Tentar novamente" | `w3ND8L` |
| Fechar aviso | "Ignorar" | `WAI6xu` |
| Fechar | "Fechar" | `cpT0Cq` |
| Carregando | "Carregando..." / "Carregando" | `lfzt24`, `ZTNur7` |
| Atualização | "Baixando atualização" / "Atualização pronta!" | `zp9d/4`, `u18OXF` |
| Confirmar atualizar | "Sair temporariamente do canal de voz?" / "Atualizar mesmo assim!" / "Cancelar" | `tiu1ly`, `QDX/qu`, `ETE/oC` |
| Erro | "Eita, que saia justa" | `3h+n+8` |
| Erro, texto | "Parece que o {app} parou de funcionar do nada...." / "Identificamos o erro e vamos consertá-lo o quanto antes." | `tx8CkI`, `CvQlAH` |
| Erro, botão | "Recarregar" | `+hivLW` |
| Membros | "Lista de membros" / "Mostrar lista de membros" / "Ocultar lista de membros" | `x87QCk`, `gxChDx`, `Axvx8c` |
| Barra de app | "Mensagens diretas", "Amigos", "Caixa de Entrada", "Notificações", "Ajuda", "Voltar" | `YUU0RF`, `TdEu5X`, `GSmTKJ`, `HcoRu0`, `cqEoj/`, `B9vzIP` |
| Manutenção | "{app} estará em manutenção programada em {data} às {hora}." | `yb96S+` |
| Atalhos (grupos) | Navegação, Bate-papo (CHAT), Voz e vídeo, Diversos, Mensagem, Arrastar e soltar | lista em `92960`/`467691` |
| Atalhos (itens) | "Navegar entre servidores", "Navegar entre canais", "Voltar ou avançar no histórico de páginas", "Navegar entre canais não lidos", "Navegar entre canais com menções não lidas", "Alterne entre o último servidor e mensagens diretas", "Ativar/desativar QuickSwitcher", "Criar ou entrar em um servidor", "Marcar servidor como lido", "Marcar canal como lido", "Focalizar área de texto", "Abrir/fechar mural de fixações" | - |

## Apêndice B: correções às notas anteriores (`docs/discord/10-ux-interface.md`)

- Colunas 72/240/240 fixas: no cliente atual a barra lateral é única, 264-432, padrão 375.
- `Ctrl+Shift+A` **recolhe/expande categorias**; "marcar servidor como lido" é `Shift+Esc`.
- `Esc` não só "fecha": é uma cadeia de 14 passos (4.3), terminando em "ir ao fim + marcar como lido".
- Zoom: 50-200% em 11 degraus fixos, não passo livre.
- Quick Switcher: prefixos confirmados `@ # ! * $` (+ `@@`).
- Janela mínima 800 x 500.
