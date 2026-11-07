# 10 - UX, layout e configuracoes da interface (Discord -> Telacast)

Legenda: **[F]** = fato conhecido do Discord (documentacao oficial/uso do produto); **[I]** = inferencia/estimativa minha (incluindo viabilidade em egui).
Nota de metodo: este relatorio foi escrito a partir do conhecimento do produto e da documentacao publica; as paginas de suporte NAO foram re-baixadas nesta sessao (por restricao de RAM/escopo). Os detalhes finos (rotulos exatos, valores padrao) devem ser conferidos antes de implementar.

## 1) Visao geral

O Discord desktop tem layout fixo em 4 colunas **[F]**:

1. **Barra de servidores** (~72 px, esquerda): botao Home/DMs no topo, servidores como icones redondos (viram "quadrado arredondado" no hover/ativo), pastas, botao "+" (criar/entrar), "Explorar", baixar apps. Pilula branca a esquerda do icone indica estado: ponto = nao lido, barra curta = hover, barra alta = selecionado.
2. **Lista de canais** (~240 px): nome do servidor com menu dropdown, categorias recolhiveis, canais texto/voz/forum/palco, e no rodape o **painel de usuario** (avatar, nome, status, botoes mudo/ensurdecer/configuracoes). Quando em voz, aparece acima do painel o bloco "Voz conectada" (latencia, desconectar, camera, tela).
3. **Area de chat**: cabecalho do canal (nome, topico, botoes: threads, notificacoes, fixadas, membros, busca), lista de mensagens com agrupamento por autor/tempo, separadores de data, linha vermelha "NOVO", caixa de entrada com anexos, emoji, GIF, stickers.
4. **Lista de membros** (~240 px, alternavel): agrupada por cargo (hoist) e depois Online/Offline, com atividade ("Jogando X").

Em DMs/Home a coluna 2 vira lista de DMs + abas de amigos; a coluna 4 vira "Atividade ativa" **[F]**.
O principio geral: tudo acessivel por teclado (Ctrl+K), por clique direito e por hover-toolbar nas mensagens **[F]**.

## 2) Lista de funcionalidades de UX

### 2.1 Navegacao e busca
- **Quick Switcher (Ctrl+K / Cmd+K)** **[F]**: modal central com campo de busca; pesquisa fuzzy em canais, DMs, servidores e usuarios. Prefixos: `!` membros... na pratica `*` canais de texto, `!` canais de voz, `@` usuarios, `#` canais, `*` servidores (os prefixos exatos `#`, `!`, `@`, `*` sao fato conhecido; qual faz o que: conferir). Sem texto, mostra recentes. Enter abre; setas navegam.
- **Busca no servidor/DM (Ctrl+F)** **[F]**: barra no cabecalho; filtros `from:`, `mentions:`, `has:` (link, imagem, video, arquivo...), `before:`/`during:`/`after:`, `in:`, `pinned:`; autocompletar de filtros; resultados paginados com salto para a mensagem; destaque do termo.
- **Alt+Cima/Baixo** troca de canal; **Alt+Shift+Cima/Baixo** vai para o proximo canal nao lido; **Ctrl+Alt+Cima/Baixo** troca de servidor; **Ctrl+/** abre lista de atalhos; **Esc** marca como lido/fecha; **Ctrl+Shift+A** marca servidor como lido **[F]** (varios atalhos, valores de memoria; conferir na tela Teclas de atalho).
- **Historico Voltar/Avancar** (Ctrl+[ e Ctrl+]) **[I]** (existe no desktop; confirmar).
- **Pular para mensagem**/respondida: clicar no trecho de resposta rola ate a original e a destaca por ~1-2 s **[F]**.

### 2.2 Nao lidos e mencoes
- **Marcadores** **[F]**: servidor com nao lido = pilula branca; mencao = badge vermelho com contagem sobre o icone; canal nao lido = nome em branco/negrito com ponto; canal silenciado = esmaecido. DMs com badge vermelho na barra.
- **Linha "NOVO"** vermelha no chat separando mensagens novas + barra no topo "X novas mensagens desde HH:MM" com botao "Marcar como lido" **[F]**.
- Botao flutuante "Voce esta vendo mensagens antigas / Ir para o presente" ao rolar para cima **[F]**.
- Titulo da janela/icone da bandeja mostra contagem de mencoes; badge no icone do app **[F]**.
- **Caixa de entrada** (icone no cabecalho): abas Para voce / Mencoes / Nao lidos **[F]**.
- Notificacao por canal/servidor/categoria: Todas as mensagens / Apenas @mencoes / Nada; suprimir @everyone e cargos; silenciar por 15 min, 1 h, 3 h, 8 h, 24 h, ate reativar **[F]**.

### 2.3 Menus de contexto (clique direito) **[F]**
- **Servidor**: marcar como lido, convidar, silenciar, configuracoes de notificacao, esconder canais silenciados, configuracoes do servidor, criar canal/categoria, sair.
- **Canal**: marcar como lido, copiar link, silenciar, notificacoes, editar, duplicar, excluir, copiar ID (modo desenvolvedor).
- **Mensagem** (e toolbar de hover com reagir/responder/editar/mais): responder, editar, fixar, copiar texto, copiar link, marcar nao lida, apagar, reportar, encaminhar, criar thread, falar em voz (TTS), copiar ID.
- **Usuario** (nome/avatar/lista de membros): perfil, mencionar, mensagem, ligar, adicionar amigo, nota, volume do usuario, silenciar, bloquear, cargos, expulsar/banir (com permissao), copiar ID.
- **Texto selecionado**: copiar, pesquisar, etc. (nivel Electron/Chromium) **[I]**.
- Submenus com seta; itens perigosos em vermelho; atalhos exibidos a direita **[F]**.

### 2.4 Cartoes de perfil
- **Popout de perfil** ao clicar no usuario **[F]**: banner/cor, avatar com bolinha de status, nome de exibicao + @username, pronomes, "Sobre mim", "Membro desde" (Discord e servidor), cargos como chips coloridos, nota privada (editavel), campo "Mensagem @usuario", botao de mais (opcoes). Modal de "Perfil completo" com abas (Atividade, Amigos em comum, Servidores em comum).
- **Painel de usuario** (rodape): clicar no avatar abre menu para trocar status (Online, Ausente, Nao perturbe, Invisivel), definir status personalizado (texto+emoji+expiracao), trocar de conta **[F]**.

### 2.5 Tooltips, modais, animacoes
- **Tooltips** escuros com seta, aparecem ~500 ms apos hover; usados nos icones de servidor (nome a direita), botoes de cabecalho, timestamps (data completa) e reacoes (quem reagiu) **[F]**.
- **Modais**: fundo escurecido, Esc ou clique fora fecha, animacao de escala/fade curta, foco preso dentro (focus trap) **[F]**. Confirmacoes de acoes destrutivas com botao vermelho.
- **Toasts/banners**: barra de "Conectando...", aviso de rede, banner de atualizacao **[F]**.
- **Animacoes**: icone do servidor morfa circulo->squircle, pilula de nao lido cresce, lista de mensagens com rolagem suave, emojis/GIFs/avatares animados so ao hover ou conforme config **[F]**.
- **Estados vazios**: "Sem amigos online" com ilustracao (Wumpus), "Nenhum resultado" na busca, "Este e o inicio do canal #x" no topo do historico, DMs vazias **[F]**.

### 2.6 Pastas e reordenacao
- Arrastar um servidor sobre outro cria **pasta**; pasta tem nome e cor, expande/colapsa, mostra mini-icones (ate 4) quando fechada, badge somado dos nao lidos **[F]**.
- **Arrastar para reordenar** servidores, pastas, DMs fixadas **[F]**; categorias e canais tambem reordenaveis por arrastar (com permissao) **[F]**.
- Clique direito em pasta: marcar como lida, silenciar tudo, configuracoes da pasta (nome/cor), fechar todas as pastas **[F]**.

### 2.7 Configuracoes de usuario (Esc ou engrenagem) **[F]**
Layout: modal de tela cheia, barra lateral a esquerda com secoes, conteudo a direita, botao "X" + dica "ESC" a direita.
- **Minha conta**: avatar, nome, e-mail, telefone, senha, 2FA, remover/desativar conta.
- **Perfis**: nome de exibicao, pronomes, avatar, banner, cor, "Sobre mim", previa ao vivo do cartao.
- **Conteudo e Social / Privacidade / Dispositivos autorizados / Conexoes / Amigos** (privacidade de DMs, quem pode adicionar).
- **Aparencia**: tema (Escuro / Claro / Sincronizar com o sistema; temas extras com Nitro), cor do tema, **modo de exibicao de mensagens** (Cozy / Compacto), **escala de zoom da interface** (Ctrl +/-, 50-200%), **tamanho de fonte do chat** (12-24 px), **espaco entre grupos de mensagens**, cores de cargo/gradiente, icones de cargo.
- **Acessibilidade**: modo de contraste alto/saturacao (slider de saturacao de cores de cargo), "Sincronizar com computador" para movimento reduzido, **reduzir movimento** (desativa animacoes de hover/emoji/GIF), reproduzir GIFs/emoji animados automaticamente (liga/desliga), sticker animado, **mostrar sublinhado em links**, mostrar cor de cargo em nomes/pontos, TTS (leitura em voz alta, velocidade), exibir pre-visualizacao de links, "modo de leitor de tela" implicito (rotulos ARIA).
- **Voz e video**: dispositivo e volume de entrada/saida, teste de microfone ("Vamos conferir"), modo de entrada (Atividade de voz com limiar automatico/manual vs Push-to-talk), supressao de ruido (Krisp), cancelamento de eco, controle automatico de ganho, camera (selecao + previa), qualidade de video, atalho de silenciar.
- **Notificacoes**: ativar notificacoes de desktop, som de mensagem/chamada, TTS, notificacoes por e-mail e push, silenciar tudo em "Nao perturbe".
- **Teclas de atalho**: tabela de acoes -> teclas, gravar atalho, atalhos globais (funcionam com jogo em foco), push-to-talk.
- **Idioma**: lista de ~30 idiomas, formato de hora 12/24 h (automatico).
- **Atividade / Jogos / Streamer / Texto e imagens** (previa de links, emoji ao digitar, mostrar anexos, converter emoticons) / **Avançado** (aceleracao de hardware, modo desenvolvedor (copiar ID), logs, "Mostrar atividade em tempo real"), **Iniciar com o sistema**, **minimizar para bandeja**, **abrir ao iniciar minimizado**.
- Rodape da barra lateral: Sair, versao do app, redes sociais; "Novidades" (changelog).

### 2.8 Temas
- **Escuro** (padrao, fundo #313338 aprox., sidebar #2b2d31, barra de servidores #1e1f22), **Claro**, **Ash/Onyx** (escuro/AMOLED em versoes recentes), **Sincronizar com o sistema** **[F para existencia; hexadecimais = I, de memoria]**. Onyx = fundo quase preto para telas OLED **[I]**.
- Tokens semanticos de cor (background-primary/secondary/tertiary, text-normal/muted, interactive-hover/active, brand #5865f2, danger vermelho, positive verde) **[F]**.

### 2.9 Onboarding e primeiro uso
- Cadastro -> "Entre em um servidor / Crie o seu" (modelos: Para mim e meus amigos, Comunidade de jogos etc.), tour de Wumpus, pop-ups "Dicas" na primeira vez (ex.: "Clique aqui para...") **[F]**.
- **Onboarding de servidor** (novos membros): tela de boas-vindas, perguntas para escolher canais/cargos, tela de regras (verificacao) **[F]**; sao funcionalidades de servidor, nao de cliente.
- Estado vazio da Home: "Nao ha ninguem por perto... " com botao "Adicionar amigo" **[F]**.

### 2.10 Acessibilidade
- Navegacao por teclado total (Tab/Shift+Tab, Ctrl+K, atalhos), indicador de foco visivel (anel azul) **[F]**.
- Rotulos ARIA e papeis (`role="list"`, `aria-label`) para leitores de tela (NVDA/VoiceOver); regiao viva anunciando novas mensagens **[F]** (suporte parcial, criticado pela comunidade **[I]**).
- Contraste alto + slider de saturacao; reduzir movimento; tamanho de fonte/zoom; legenda/TTS **[F]**.
- Dica de atalho Ctrl+/ e tela "Teclas de atalho" **[F]**.

## 3) Telacast: ja tem vs falta (com viabilidade em egui)

Ja existe (segundo o briefing): tema escuro estilo Discord, barra de servidores, lista de canais, chat, membros online/offline, cartao do usuario com "Sair", abas de amigos, modais de criar servidor/canal/convite, tela cheia no video.

| Item | Estado | Viabilidade em egui **[I]** |
|---|---|---|
| Quick Switcher Ctrl+K | falta | Facil: `egui::Area`/`Window` centralizado com `TextEdit` focado; filtro fuzzy local (crate `fuzzy-matcher`/`nucleo`), lista navegavel com setas via `ctx.input`. Atalho global dentro da janela e trivial. |
| Marcadores de nao lido/mencao (pilula, badge, canal em negrito) | falta (verificar) | Facil: desenhar com `Painter` (circulos/retangulos). Exige servidor contar `last_read` por canal e mencoes por usuario (protocolo). |
| Linha "NOVO" + "marcar como lido" + salto para presente | falta | Medio: depende de `last_read_message_id` no servidor e de `ScrollArea` com `scroll_to_rect`. |
| Menus de contexto (clique direito) | provavel falta | Facil: `Response::context_menu` ja existe no egui; submenus via `menu_button`. Itens vermelhos com `RichText`. |
| Toolbar de hover na mensagem (reagir/responder/editar/apagar) | falta | Medio: sobrepor botoes quando `response.hovered()`; exige acoes no protocolo (editar/apagar/responder). |
| Popout de perfil (clique em usuario/avatar) | so cartao do proprio usuario | Medio: `Area` ancorada na posicao do clique, fechar ao clicar fora (`Area` + `ctx.input pointer`). Banner/cargos dependem de dados de servidor. |
| Tooltips | parcial? | Facil: `Response::on_hover_text`/`on_hover_ui` nativos. Atraso configuravel em `Style::interaction.tooltip_delay`. |
| Modais e Esc para fechar, focus trap | tem modais | Facil: padronizar um helper `modal()` com fundo escurecido (`Area` full-screen + `Frame`), fechar com Esc. Focus trap real e dificil (egui nao tem ordem de foco rica), mas Tab dentro de um `Window` modal e aceitavel. |
| Busca em mensagens (filtros from:/has:/in:) | falta | Medio/Grande: UI simples (campo + lista) e facil; o motor (FTS no SQLite do servidor, ja usa `telacast.db`) e o trabalho real. |
| Configuracoes de usuario (modal tela cheia com barra lateral) | so "Sair" | Medio: layout `SidePanel` + `CentralPanel` dentro de uma `Area`; secoes como funcoes independentes. Persistir em arquivo (`serde` + dir de config). |
| Temas escuro/claro/AMOLED | so escuro | Facil: `ctx.set_visuals()` + struct de tokens de cor (copiar nomes dos tokens do Discord). Claro/AMOLED = 2 paletas. Sincronizar com sistema via `dark-light` crate **[I]**. |
| Zoom da interface | falta | Facil: `ctx.set_zoom_factor()` (egui >= 0.23) + Ctrl +/- ja e embutido em eframe (`zoom_with_keyboard`). |
| Tamanho de fonte do chat | falta | Facil: alterar `TextStyle::Body` size no `Style`. |
| Modo compacto vs cozy | falta | Medio: dois renderizadores de linha de mensagem (avatar a esquerda, sem avatar) ja que o layout e codigo proprio. |
| Pastas de servidores | falta | Medio: renderizar grupo recolhivel; exige persistir a estrutura por usuario (no servidor ou local). |
| Arrastar para reordenar | falta | Medio: egui tem `dnd_drag_source`/`dnd_drop_zone` desde 0.24 (ou `egui_dnd` crate **[I]**). Persistir ordem. |
| Status do usuario (online/ausente/nao perturbar/invisivel) | falta (verificar) | Facil na UI (menu do avatar); precisa presenca no servidor. |
| Silenciar canal/servidor, prefs de notificacao | falta | Medio: UI facil; notificacao de desktop via crate `notify-rust` e som via `rodio` (ja ha audio de voz? verificar). |
| Animacoes (hover, pilula, squircle) | falta | Facil/Medio: `ctx.animate_value_with_time` e `animate_bool`; interpolar raio do icone. Reduz movimento = zerar o tempo. |
| Estados vazios com ilustracao | falta | Facil: texto + icone/imagem estatica (SVG com `egui_extras`). |
| Onboarding primeiro uso | falta | Medio: tela inicial "criar/entrar em servidor" + dicas unicas (flag em config). |
| Atalhos de teclado (Alt+setas, Esc, Ctrl+/) | falta | Facil: leitura de `ctx.input(|i| i.key_pressed(...))`; tela "Teclas de atalho" so lista. Remapeamento customizavel = Medio. Atalhos globais (fora de foco) = Dificil (depende de X11/Wayland, crate `global-hotkey`, portal no Wayland). |
| Acessibilidade (leitor de tela) | falta | **Limitado**: egui integra AccessKit (feature `accesskit` no eframe) **[F]** e funciona em Windows/macOS/Linux de forma basica; qualidade ainda irregular **[I]**. Contraste alto = paleta extra; reduzir movimento = flag; fonte/zoom = itens acima. |
| Emojis, markdown rico, GIF, mencoes coloridas | falta | **Dificil**: `egui` nao tem rich text inline nem emoji colorido nativo; precisa de layouter customizado (`LayoutJob` cobre negrito/italico/cor/codigo) e imagens para emoji (atlas de texturas, `egui_extras`). Markdown basico via `LayoutJob` = Medio; blocos de codigo/citacoes = Medio. |
| Lista de membros por cargo | so online/offline | Medio: depende do modelo de cargos (outra area). Agrupamento na UI e facil. |
| Idioma (i18n) | falta | Medio: strings centralizadas + `fluent`/tabela propria; fontes com fallback CJK/arabe (egui precisa de fontes extra; RTL nao e suportado bem **[I]**). |
| Layout responsivo/colapso de colunas | verificar | Facil: esconder membros/canais por largura (`ctx.screen_rect()`), botoes de alternar. |
| Cheiro de Discord: pilula, squircle, cores de cargo | falta | Facil via `Painter`. |

Observacoes gerais de egui **[I]**: modo imediato redesenha tudo; listas longas de mensagens exigem `ScrollArea::show_rows`/virtualizacao manual (altura variavel dificulta; considerar cache de alturas). Persistir preferencias via `eframe::Storage` ou JSON proprio. Para tema, concentrar cores em uma struct `Tokens` para facilitar claro/escuro/AMOLED/alto contraste.

## 4) Prioridade

| Pri | Item | Esforco | Motivo |
|---|---|---|---|
| P0 | Marcadores de nao lido e mencao (servidor/canal/DM) + `last_read` no servidor | M | Sem isso o chat nao e usavel com varios canais; base da linha "NOVO" |
| P0 | Menus de contexto (mensagem, canal, servidor, usuario) | M | Concentra editar/apagar/responder/copiar/convidar; barato em egui |
| P0 | Tela de Configuracoes (esqueleto) com Minha conta, Aparencia, Notificacoes, Voz, Sair | M | Ponto central para todas as demais preferencias |
| P0 | Modal padronizado (Esc/clique fora/confirmacao destrutiva) | S | Consistencia, reaproveitado em tudo |
| P1 | Quick Switcher Ctrl+K | S | Alto valor percebido, quase todo local |
| P1 | Popout de perfil de outros usuarios + status (online/ausente/nao perturbar) | M | Identidade e socializacao |
| P1 | Toolbar de hover nas mensagens + responder/editar/apagar/fixar | M | Paridade basica de chat (depende do protocolo) |
| P1 | Temas claro/AMOLED + zoom + tamanho de fonte | S | Quase gratis com `Tokens`, forte para acessibilidade |
| P1 | Atalhos basicos (Alt+setas, Esc, Ctrl+/ , Ctrl+K) | S | Produtividade |
| P1 | Silenciar e prefs de notificacao + notificacao de desktop e som | M | Evita ruido, necessario em uso real |
| P2 | Busca em mensagens com filtros | L | Precisa FTS no servidor; UI simples |
| P2 | Modo compacto/cozy | M | Preferencia comum, nao bloqueia nada |
| P2 | Pastas e arrastar para reordenar servidores/canais | M | Organizacao para quem tem muitos servidores |
| P2 | Markdown basico + emoji unicode/custom no chat | L | Impacto grande, mas custo alto em egui (outra area deve cobrir) |
| P2 | Animacoes (pilula, squircle) + "reduzir movimento" | S | Acabamento; reduz movimento depende de ter animacoes |
| P2 | Onboarding e estados vazios | S/M | Primeiro uso mais amigavel |
| P2 | Acessibilidade: AccessKit, alto contraste, indicador de foco, saturacao de cores | M | Inclusao; AccessKit e opt-in no eframe |
| P3 | Idioma (i18n) | M | So depois de estabilizar o texto da UI |
| P3 | Atalhos remapeaveis e globais (push-to-talk global) | L | Wayland/X11 complicam |
| P3 | Lista de membros por cargo, cores/icones de cargo | M | Depende do modelo de cargos |
| P3 | Historico voltar/avancar, "Inicio do canal", linha "NOVO" refinada | S | Polimento |
| P3 | Config "Avancado" (modo dev copiar ID, aceleracao, iniciar com o sistema/bandeja) | S/M | Nicho; bandeja exige crate + suporte do compositor |

Ordem sugerida **[I]**: Tokens de tema -> modal helper -> nao lidos no servidor -> menus de contexto -> Configuracoes -> Ctrl+K -> perfil/status -> hover-toolbar -> o resto.

## 5) Fontes

- Central de ajuda Discord (support.discord.com, artigos publicos, conhecimento previo; nao re-baixadas): "Quick Switcher", "Atalhos de teclado", "Configuracoes de notificacao de servidor", "Pastas de servidores", "Como usar o modo de acessibilidade/Configuracoes de acessibilidade", "Perfil de usuario 101", "Aparencia de mensagens (Cozy/Compacto)", "Search / Pesquisa no Discord", "Status de usuario", "Voz e video: configuracoes".
- Blog oficial discord.com/blog (temas, onboarding de servidor, novo design de perfis) - citado de memoria.
- Clique em `Configuracoes de usuario` no proprio app: nomes de secoes baseados no cliente atual; sem acesso a conta nesta sessao (proibido). Textos de interface em `/snap/discord/current/usr/share/discord` existem (binario Electron), mas nao foram varridos por restricao de escopo/RAM; recomenda-se um `grep -a` filtrado em strings pt-BR do app se for preciso conferir rotulos exatos.
- egui/eframe docs: `context_menu`, `set_zoom_factor`, `animate_bool`, `accesskit`, `dnd_drag_source` (conhecimento previo, conferir na versao do `vendor/` do repo).
- Observacao de qualidade: valores como hexadecimais, larguras em px, escalas de zoom e prefixos do Quick Switcher sao de memoria e marcados como a confirmar.
