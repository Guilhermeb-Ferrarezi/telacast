# 02 - Barra de servidores (coluna estreita à esquerda)

Legenda: **[F]** confirmado no cliente do Discord; **[I]** inferência; **[?]** não consegui confirmar.

Fontes. JS: `discord-bundle/web.0bcf9a9e83b2c3c7.js` (o maior; trechos em 5.97M–6.04M, módulos `649781` barra, `531053` alvos de arrastar, `862328` máscara do ícone, `67811` botão/sigla, `941971` pílula, `812993` selo, `771640` selos de mídia, `550591`/`866665`/`194981`/`280336` tooltip). Confirmado em `web.3f3154a164805d56.js` e `web.48946ead1e279bd2.js` (mesmos módulos). Strings: `12064a7d28fc7eb1.js` (pt-BR).
**O CSS dos módulos não está nos 93 arquivos extraídos** (só há o CSS de tokens `952007.*`). Eu o recuperei do cache do app (`~/snap/discord/320/.config/discord/Cache/Cache_Data`, entradas Simple Cache com corpo brotli; o corpo termina no marcador de EOF `d8410d97456ffaf4`). Os arquivos citados abaixo (`441674.*.css`, `504129.*.css`, `343178.*.css`, `703298.*.css`, `311106.f05e*.css`, `960252.*.css`, `230322.*.css`) vêm de lá. Decodificados em `<scratchpad>/barra/css/`.

## 1. Resumo e escopo

Coluna de **72 px** (padrão) com fundo mais escuro que o resto, rolável sem barra visível, contendo de cima para baixo: botão Início/DMs, DMs com não lidas, separador, servidores e pastas, botão "Adicionar um servidor", "Descubra" e "Baixar apps". Cada item é um **ícone de 40×40 em forma de "squircle"** (não há mais círculo→quadrado: ver 3.1) com uma **pílula branca encostada na borda esquerda** (altura 8 não lido / 20 hover / 40 selecionado), **selo vermelho de menções** no canto inferior direito e **selo de mídia** (voz/transmissão) no canto superior direito. Pastas agrupam até N servidores, mostram 4 mini-ícones fechadas e um glifo de pasta abertas. Reordenar/agrupar é por arrastar: metade de cima do item = inserir antes; metade de baixo = combinar (criar/entrar em pasta).

Fora de escopo: lista de canais, painel do usuário no rodapé da coluna, Ctrl+K, modais "Criar/Entrar", descoberta, página de configurações de notificação, dados de voz em tempo real (só o selo).

Correções ao `docs/discord/10-ux-interface.md`: o tooltip da barra aparece **sem atraso** (atraso padrão 0, não 500 ms) **[F]**; o ícone **não** morfa de círculo para quadrado neste build **[F]** (só DMs são círculos); a barra de rolagem existe como variante "fina que some", não "oculta por completo" **[F]**.

## 2. Árvore de componentes e medidas

Tokens relevantes **[F]** (`952007.c7b99...css`, `419070.573a...css`): `--guildbar-avatar-size` 40, `--guildbar-folder-size` 48, `--custom-guild-list-padding` = `--space-md`, `--custom-guild-list-width` = 40 + 2×padding. `--space-md` vale 16 (densidade padrão), 12 (compacta), 20 (confortável); `--space-xs` (vão entre itens) vale 8 / 6 / 10. Com o experimento `refresh-fast-follow-avatars` o ícone vira 44 (pasta 52) e o padding é `min(space-md, 16)` (larg. 76 no padrão). Adotar o padrão sem experimento.

| Constante Telacast | Valor | Origem |
|---|---|---|
| `RAIL_W` | 72 | 40 + 2×16 **[F]** |
| `ICON` | 40 | `AVATAR_SIZE` **[F]** |
| `FOLDER` | 48 | `FOLDER_SIZE` **[F]** |
| `PAD_X` | 16 | **[F]** |
| `GAP` | 8 | `space-xs` **[F]** |
| padding-bottom da rolagem | 24 (= gap + 16) | `.scroller_5dqHTG` **[F]** |
| padding-top da rolagem | 4 (Win/Mac) ou 12 (outros, no JS `ij`) | **[F]**, divergem; usar 8 **[I]** |

```
nav "Barra lateral dos servidores"                 largura 72, fundo background-base-lowest (o mais escuro)
 ├ tira de "salto para não lido" (topo)           aparece só se há servidor com menção/não lido fora da vista
 ├ árvore (role=tree) > rolagem fina, gap 8
 │   ├ [Início]      ícone 40, selo inferior = pedidos de amizade pendentes (+ ofertas)
 │   ├ [DMs não lidas] grupo "Mensagens diretas" (oculto se vazio), ícones 40 REDONDOS, gap 8
 │   ├ [convidados/lurking] (fora de escopo)  [pasta "Candidaturas Pendentes"] (fora de escopo)
 │   ├ separador: 32×1, raio 1, cor app-frame-border, centralizado
 │   ├ raízes da árvore (grupo "Servidores", gap 8): servidor | pasta
 │   ├ [aviso de servidores indisponíveis] (opcional)
 │   ├ [Adicionar um servidor]  sem pílula; selecionado enquanto o modal está aberto
 │   ├ [Descubra]               compasso; pílula; selecionado na rota de descoberta
 │   └ [Baixar apps]            só no navegador/desktop não embutido
 └ tira de "salto para não lido" (base)
```
Ordem do JSON-React: `i$` (início, DMs, convidados, pedidos) → separador `nR` → `iz` (árvore, indisponíveis `nP`, adicionar `t$`, descobrir `t8`, baixar `tM`) **[F]**.

### 2.1 Linha de item (ListItem)
`display:flex; justify-content:center; width: 72; position:relative` **[F]** (`960252.*.css`). A pílula é filha absoluta na esquerda; o ícone fica em x = 16..56. A área de hover/clique é o **ícone** (40×40), não a linha.

### 2.2 Pílula (indicador à esquerda) **[F]** (`441674.932ffb36706e90f3.css`)
- Contêiner 8 px de largura, altura = a da linha, `overflow:hidden`. Elemento interno 8×8, cor `text-strong` (branco), raios `0 4 4 0`, `margin-inline-start:-4` (só os 4 px da direita aparecem, logo é meia-pílula 4 px de largura).
- Estado base: `opacity 0`, `translateX(-4)`. Visível (`visible`) quando **não lido OU hover OU selecionado**: `opacity 1`, `translateX(0)`.
- Alturas: **8** (não lido), **20** (hover), **40** (selecionado). Selecionado vence hover (regra CSS posterior). Variante `small` (DM em grupo): 10 / 24, raio total.
- Transição `height, opacity, transform` **0,2 s ease-out**, só em modo "full-motion" (sem movimento reduzido).
- Mapeamento por item: servidor = `hovered` do ícone, `selected` = é o servidor ativo (ou o hover forçado), `unread` = tem não lido. Durante arrastar (`dragging`) a pílula some. Pasta: `selected` = algum filho é o servidor ativo; `unread` = algum filho não lido; **pílula da pasta desativada quando aberta** (os filhos têm as suas). Botão Adicionar: `showPill:false`.

### 2.3 Ícone do servidor
- Máscara "squircle" 40×40 (SVG). Forma: cantos contínuos, equivalente a raio circular ≈ 11 px; ajusta-se à superelipse |x|^4+|y|^4=1 (ponto da diagonal a 0,16 do lado) **[I, ajuste numérico sobre os caminhos SVG do módulo `862328`]**. DMs usam o caminho circular (`rounded`).
- Com imagem: URL pedida em 2× (80 px), `object-fit: cover`; GIF/avatar animado só toca com hover e janela focada **[F]** (`tB.Iv(guild, 2*40, hover && focused)`). Enquanto carrega: fundo `background-mod-subtle` (`.icon:before`).
- Sem imagem: **sigla**. Fundo `mod-subtle` (≈ branco 12%), texto `text-default`, peso medium, `line-height:1.2em`, sem quebra. **Hover/selecionado: fundo `background-brand` + texto branco, 150 ms ease-out** (`343178.*.css`: `.wrapper:hover .childWrapper`, `transition: background-color .15s, color .15s`; o próprio `wrapper` usa .1 s). **[F]**
- Tamanho da fonte da sigla por nº de letras: 1→18, 2→18, 3→16, 4→16, 5→14, 6→12, 7+→10 px (tabela `[18,18,16,16,14,12,10]`, módulo `67811`) **[F]**.
- Regra da sigla (módulo `23339`, função `oN`): remove `'s ` (vira espaço), pega a **primeira letra de cada palavra `\w+`**, junta sem espaços; **não força maiúscula nem limita a 2 letras** **[F]**. Defeito do original: `\w` é ASCII, então "Ação" vira "Ao" **[F]**; no Telacast usar palavras Unicode (`char::is_alphanumeric`).
- `:active` (botão do mouse pressionado): deslocar 1 px para baixo (`translateY(1px)`) **[F]** (`.listItemWrapper:active`, `.blobContainer:active`).
- Foco por teclado: anel de 4 px azul com o fundo preenchido **[F]**; no Telacast, anel âmbar 2 px.

### 2.4 Botões de ícone (Início, Adicionar, Descubra, Baixar) **[F]** (`311106.6a3e...css`)
Mesmo tamanho/máscara do ícone, fundo `mod-subtle`, cor `text-default`, glifo de 20 px (Início: logo de 24 px); **hover e selecionado: fundo brand + glifo branco, 150 ms**. Início: tooltip "Mensagens diretas", selo inferior = nº de pedidos de amizade pendentes (+ ofertas), clicar abre o último DM/lista de amigos. Adicionar: tooltip "Adicionar um servidor"; (variante de experimento "Entrar em um servidor"); botão direito abre menu criar/entrar **[F]** (módulo `4571`, lazy; itens não vistos). Descubra: "Descubra". Baixar: "Baixar apps".

### 2.5 Selos (badges)
**Selo inferior direito (contagem)** **[F]** (`812993`, `5257.*.css`):
- Altura/min-largura 16; largura **16** (<10), **22** (<100), **30** (≥100); raio total; texto branco `body-xs/semibold` (12 px), centralizado; alinhado ao canto inferior direito do ícone (right 0, bottom 0).
- Texto: `n` se <1000, senão `min(⌊n/1000⌋, 9)k+`.
- Fundo: vermelho de notificação; **cinza (`background-mod-strong`) se a menção é de baixa importância** (`isMentionLowImportance`: p.ex. só @everyone).
- Em volta, **anel recortado de 2 px** (a máscara do ícone abre um retângulo arredondado do tamanho do selo + 2 px por lado; o fundo da barra aparece).
- Selo de pausa (quando não há contagem e os convites do servidor estão pausados): círculo 16 `mod-strong` com glifo de 10 px.
- Contagem de pasta = **soma** das menções dos filhos, só mostrada com a pasta **fechada**.

**Selo superior direito (mídia)** **[F]** (`771640 C()`, `960252.*.css`): 16×16 no canto superior direito, glifo de 12 px, anel 2 px. Prioridade do glifo: evento ativo (calendário) > palco ao vivo > **compartilhamento de tela (transmissão)** > vídeo > voz > atividade. Fundo `icon-overlay-dark`; **verde (`status-positive`) quando o usuário atual está conectado ali**. Servidor indisponível: fundo branco + glifo vermelho. Pasta fechada mostra o selo de mídia agregado.

### 2.6 Pasta **[F]** (`504129.*.css`, JS `$`, `k`, `x`, `G`)
- Estrutura: grupo (`folderGroup`) > [fundo da pasta] + cabeçalho 48×48 + lista (`ul`, gap 8).
- **Fechada**: botão 48×48, raio 16 (`radius-lg`), fundo = cor da pasta a 40% de opacidade (ou `mod-subtle` sem cor), padding 4, grade flex-wrap gap 2 com os **4 primeiros** servidores em mini-ícones de **19×19** (= (48−8−2)/2). Raios dos mini-ícones: 1º `13 4 4 4`, 2º `4 13 4 4`, 3º `4 4 4 13`, 4º `4 4 13 4` (canto externo grande). Mini-ícone ausente/erro: ícone de servidor genérico/vermelho. Selo de mídia/menções no canto, como o ícone.
- **Aberta**: cabeçalho mostra o **glifo de pasta** (18 px, cor da pasta, padrão = brand) numa caixa 40×40 raio 12 (hover `mod-subtle`), centralizada no cabeçalho 48 (padding 4). A troca preview↔glifo é um **deslize vertical de 48 px em 150 ms ease-out** dentro de `overflow:hidden` (preview some para cima).
- **Fundo da pasta aberta**: coluna de largura 48 (x = 12..60), raio 16, borda 1 px `border-muted`, preenchimento = cor da pasta a 15% (sem cor: `mod-subtle`), fade 150 ms. Estende-se do topo do cabeçalho ao fim da lista.
- **Lista**: primeiro filho com `margin-top = gap − (48−40)/2 = 4`; altura animada 0→`n×(40+8) − 8 + 4 + 4 = 48n` em **150 ms** (`FOLDER_ITEM_ANIMATION_DURATION`); cada filho entra com `scale` 0→1 acompanhando a altura.
- Cor: inteiro RGB opcional; nome opcional (tooltip e leitor usam "Pasta do servidor" quando vazio). Só um nível: pasta não entra em pasta **[F]**.
- Persistência **[F]**: lista ordenada `guildFolders.folders[] = {id?, name?, color?, guildIds[]}` nas configurações do usuário (sincronizada); servidor solto = entrada sem `id` com 1 guildId; pasta vazia some. Abrir/fechar fica num store local (`isFolderExpanded`) **[I: não sincronizado]**.

### 2.7 Tooltip **[F]** (`866665`, `194981`, `280336`, `230322.03996...css`)
- Posição **à direita** do ícone, **12 px** de distância, centralizado na vertical, com **seta** de 5 px apontando para o ícone.
- **Atraso 0** (o hook usa `delay ?? 0`); some ao clicar, no menu de contexto, na tecla Esc, ao sair com o mouse. Nada de tooltip durante o arrastar.
- Caixa: máx. 190 px, padding 8×12, raio 8, borda 1 px, sombra alta, texto 14/16 peso medium, fundo `surface-high`.
- Animação: escala 0,95→1 e opacidade 0→1 com mola (tensão 2400, atrito 52, ~80 ms); sem movimento reduzido: só opacidade.
- Texto do servidor: nome. **Conteúdo rico** (módulo `ih`/`iE`, atrás do experimento `guild-tooltip-lazy-content`): ícone 20 + nome (selo de verificado/boost se houver); linhas de pessoas em voz (glifo de voz + até 6 avatares), no palco (até 3 + nº de ouvintes), transmitindo (até 6), em atividade; "Silenciado(a)" (+ até quando); "Convites desse servidor pausados". Experimento extra: "Nova atividade em #canal"/"Menção em #canal". Texto acessível do ícone: "{n} menção(ões), {nome}" ou "Mensagens não lidas, {nome}" + ", Compartilhamento de tela ativo / Chamada de voz ativa / ...".
- Pasta: nome (ou "Pasta do servidor"). Botão Início: "Mensagens diretas".

### 2.8 Tira "salto para não lido" **[F]** (módulo `h` 3.64M)
Nas pontas superior e inferior da coluna (largura 72, padding 8, `pointer-events:none` exceto a tira). Aparece (desliza ±180% em Y) quando existe servidor/pasta com **menção** ou **não lido** fora da área visível naquela direção; clique rola até o primeiro (menção tem prioridade) e o realça. Cálculo debounced 200 ms, percorre a lista achatada (pasta aberta expande os filhos) até achar um item visível **[F]**. Forma: cápsula (raio 20, padding 4×8, borda 1 px, sombra); vermelha com menção, neutra com não lido **[I]** (CSS genérico `.bar/.mentionsBar`).

## 3. Estados e interações

| Estado | Ícone | Pílula | Extra |
|---|---|---|---|
| repouso | imagem; ou sigla em `mod-subtle` | invisível (ou 8 se não lido) | |
| hover | sigla → fundo brand + branco (150 ms); imagem: GIF toca | 20 (200 ms ease-out, x −4→0, opacidade) | tooltip imediato |
| pressionado | desloca 1 px p/ baixo | | |
| selecionado | sigla: brand; imagem inalterada | 40 | |
| hover + selecionado | | 40 | |
| não lido | | 8 (branca) | tooltip/leitor "Mensagens não lidas" |
| menções | | 8 (ou 20/40) | selo vermelho com contagem |
| silenciado | **sem alteração visual no ícone** **[F: nada encontrado]** | idem | tooltip "Silenciado(a)"; o não lido não conta canais silenciados **[I]** |
| indisponível | selo superior branco/vermelho | | tooltip do servidor |
| arrastando | origem vira marcador tracejado (borda 1 px tracejada `border-subtle`, raio 12, fundo `mod-normal`, 40×40) | some | itens ficam sem hover/tooltip |
| alvo de combinar | ícone vira prévia de pasta (escala 3→1 + translate 10→0 em 100 ms) após **70 ms** de permanência | | contorno verde 1 px, fundo verde 28% |
| desativado/erro | "guildsError": 40×40, borda 2 px crítica, raio 12 | | |
| carregando | marcador (círculo girando) no lugar do mini-ícone da pasta | | |

Movimento reduzido: todas as transições do CSS estão sob `.full-motion`; sem ele, valores aplicados direto.

### 3.1 Sobre o "círculo → quadrado arredondado"
Não existe neste cliente **[F]**: o módulo `862328` escolhe o caminho círculo só para `rounded` (DMs/grupos); servidores usam o caminho squircle sempre; o CSS do módulo (`239502.*.css`) não tem `transition` de raio; o CSS do botão (`343178.*.css`) só transiciona cor/escala. A "morfa" do doc 10 é do cliente antigo. O que dá a sensação de resposta hoje é: pílula 8→20→40, fundo brand na sigla e translateY(1 px) ao pressionar. **Decisão para o usuário**: seguir o build atual (squircle fixo, recomendado) ou implementar a morfa antiga (raio 50%→~30%). Ver seção 8.

## 4. Regras de comportamento

**Ordem e navegação**
- Lista achatada = raízes na ordem das configurações; a ordem de "próximo/anterior servidor" percorre a lista achatada (pastas incluídas, abertas ou não) e **dá a volta passando por Início**; abre o último canal visitado do servidor **[F]** (`928531`): próximo = **Ctrl+Alt+↓**, `Ctrl+Shift+]`, `Ctrl+Tab`; anterior = **Ctrl+Alt+↑**, `Ctrl+Shift+[`, `Ctrl+Shift+Tab`; alternar Início ↔ último servidor = **Ctrl+Alt+→**; Ctrl+K = trocador rápido (`mod+k`). Marcar servidor como lido tem atalho próprio (`MARK_SERVER_READ`) **[F existência; combinação ?]**.
- A árvore é `role=tree`; itens `treeitem` com `aria-setsize/posinset`; **→ abre / ← fecha** a pasta, ← num filho foca a pasta **[F]**.
- Ao trocar de servidor (também por rota/atalho), a rolagem **centraliza o item** (com 24 px de folga) com animação; a posição de rolagem é salva e restaurada ao iniciar **[F]**. Selecionado dentro de pasta fechada: rola até a pasta.
- Clique no servidor navega para o último canal; `mouse down` já pré-carrega o canal (otimização) **[F]**.

**Arrastar (react-dnd no original)** **[F]** (`531053`, `311106.f05e...css`)
- Tipos: servidor e pasta arrastáveis; DMs e botões não. Pasta **não** vai para dentro de pasta; só servidor "combina".
- Cada item tem uma faixa de alvo de `top −16` a `bottom +4` do ícone, dividida **ao meio**: **metade de cima = inserir antes** (barra de 4 px, raio 2, 4 px de margem lateral, posicionada ~6 px acima do ícone, cor "positiva"); **metade de baixo = combinar** (só itens de nível 1 aceitam; alvo aceita só servidores). Filhos de pasta só têm a metade de inserir. [I sobre a leitura "cima/baixo": as classes e flags (`below`, `combine`) são confirmadas; a ordem do DOM define cima/baixo.]
- Combinar: servidor+servidor cria pasta nova (sem nome/cor) na posição do alvo; servidor+pasta (aberta ou fechada) acrescenta no **fim** da pasta. Pasta aberta: fundo vira verde 28% e borda verde; fechada: cabeçalho com fundo verde e contorno 1 px.
- Fim da lista: alvo logo após o último item (junto ao botão Adicionar), texto "fim da lista" (acessível). Fim de pasta: faixa de 24 px que aparece durante o arrastar, "Mover para fora da pasta, abaixo de {pasta}" / "acima de" / "Adicionar à pasta {x}" / "Combinar e criar uma nova pasta".
- Ao soltar: persiste a lista inteira nas configurações (otimista). Pasta que fica vazia é removida. Ver `moveById(id, alvo, abaixo, combinar)`.

**Menu de contexto (botão direito)**
- **Servidor** (módulo lazy `237600`, **não está no cache; ordem inferida** do menu de cabeçalho do servidor no mesmo build, ids `settings, insights, create-channel, create-category | notifications, privacy | per-server-profile, hide-muted-channels | report-raid, leave | copy-id`) **[I]**. Itens a implementar, nesta ordem, com os textos do pt-BR **[F para as strings]**:
  1. Marcar servidor como lido
  2. Convidar pessoas
  3. Silenciar servidor ▸ (15 min, 1 h, 3 h, 8 h, 24 h, até reativar)
  4. Config. de notificação
  5. Config. de privacidade
  6. Editar perfil por servidor
  7. Ocultar canais silenciados (caixa de marcação)
  8. Config. do servidor ▸ (só com permissão)
  9. Sair do servidor (vermelho; escondido para o dono)
  10. Copiar ID do servidor
  Separadores entre grupos 1-2 / 3-5 / 6-7 / 8-9 / 10 **[I]**.
- **Pasta** (módulo `595139`, não cacheado) **[F: strings "Marcar pasta como lida", "Configurações de pasta", "Fechar todas as pastas"; "Silenciar tudo" vem do doc 10, texto pt-BR ?]**: Marcar pasta como lida; Silenciar (pasta); Config. de pasta (modal com "Nome da pasta" e "Cor da pasta"; mensagem "Configurações de pasta salvas!"); Fechar todas as pastas **[ordem I]**. Limites do nome/paleta de cores **[?]**.
- Botão Adicionar: menu próprio (criar / entrar) **[?]**. Botão direito num DM não lido: menu de usuário/DM.

**Rolagem**: variante **fina com "fade"** (8 px, thumb arredondado 4 px `neutral-38`, trilho transparente) que só aparece com hover/rolagem **[F]** (`193433.*.css`, `.thin`+`.fade`; o JS usa `Ch` = fina). Na prática, com a lista cabendo, nunca aparece. Roda do mouse rola. **Telacast: ocultar sempre** (`AlwaysHidden`), manter roda e arrastar. Itens ficam `pointer-events:none` enquanto rola (`scrolling`) **[F]**.

**DMs não lidas na barra** **[F]** (`tO`): só DMs com não lido (+ a DM da chamada atual primeiro), ícone **redondo** com avatar, selo de menções, pílula de não lido; entrada/saída animada (altura, opacidade, escala 0,5→1, mola atrito 28 tensão 600, ~150 ms). Clique abre a DM.

**Limites**: cap de servidores no cliente (`qlD`), upsell ao atingir **[F existência; valor ?]**. Para o Telacast, ver pergunta 5.

## 5. Dados necessários (o que falta)

Hoje: `ServerInfo{id,name,owner,invite,channels,members}`, `Snapshot.live`, `read_state(user,channel,last_read_id,mention_count)` já existe no banco mas **não** é exposto; `Session.unread` é um `HashSet<channel>` só em memória.

Prioridade:
1. **P0 Não lidos e menções por servidor.** Servidor manda no `Snapshot` (e em evento) `ReadState{channel_id, last_message_id, last_read_id, mention_count, muted}`. Cliente deriva `server_unread` (algum canal não silenciado com `last_message_id > last_read_id`) e `server_mentions` (soma). Novos: `C2S::Ack{channel_id, message_id}` ao abrir/rolar o canal até o fim; `C2S::AckServer{server_id}` (marcar servidor como lido, atalho e menu); `S2C::ReadState(...)` incremental a cada mensagem nova com menção detectada no servidor (coluna `messages.mentions`/`mention_everyone`, ou parse de `@usuario` no servidor). Índice: `channels(server_id)`, `read_state(user_id)`; coluna `messages.id` já ordena.
2. **P0 Pastas.** Tabela `user_guild_folders(user_id, pos INT, folder_id BIGINT NULL, name TEXT NULL, color INT NULL, server_ids BIGINT[])` (ou uma coluna `jsonb` em `user_settings`). `Snapshot.folders: Vec<Folder{id:Option<i64>, name:Option<String>, color:Option<u32>, server_ids:Vec<i64>}>` e `C2S::SetFolders{folders}` (substitui tudo, como o original). Validação: cada servidor em no máximo uma entrada; descartar ids em que o usuário não é membro; nome ≤ 32 [?]; ≤ 200 pastas [?]; servidores novos (`ServerJoined`) entram no fim da lista.
3. **P1 Ícone do servidor.** `servers.icon_hash TEXT NULL`; `C2S::SetServerIcon` (ou rota HTTP de upload) + download `GET /icons/{server_id}/{hash}?size=80` (WebP/PNG 80 e 160); `ServerInfo.icon: Option<String>`; cache de texturas no cliente (`HashMap<hash, TextureHandle>`, carregamento em thread, miniatura de 80 px). Sem isso a sigla é o único ícone.
4. **P1 Silenciar/configurações por membro.** `member_settings(user_id, server_id, muted_until TIMESTAMPTZ NULL, suppress_everyone BOOL, hide_muted_channels BOOL)`; `C2S::MuteServer{server_id, until}`; `C2S::LeaveServer{server_id}`; `ServerInfo.muted_until`. Canais silenciados: `channel_settings(user_id, channel_id, muted_until)`.
5. **P2 DMs não lidas e pedidos pendentes** já existem em `friends` (status `Incoming`); `Dm` ganha `unread`/`mentions` via `read_state` do canal tipo 10.
6. **P2 Selo de mídia.** O cliente já recebe `live: Vec<Live{channel_id,user_id}>`; mapear canal→servidor no cliente. Para "conectado aqui" (selo âmbar) usar o estado local de assistir/publicar (`Act::Watch/Publish`). Voz com avatares no tooltip rico exige `voice_states` por servidor (fora do escopo agora).
7. **P3 Abrir/fechar pasta** persistido local (eframe `persistence`, `HashSet<i64>` por usuário). Posição de rolagem idem.

## 6. Plano de implementação em egui

**Arquivos**: criar `app/src/rail.rs` (o `main.rs` já tem 1224 linhas) com `RailState`, `rail_ui`, desenho do ícone, pílula, selo, tooltip, arrastar; `theme.rs` ganha só constantes e funções de forma (`squircle_pts`, `paint_squircle`, `mix`). `Session` ganha `rail: RailState{expanded: HashSet<i64>, drag: Option<Dragging>, scroll_to: Option<NodeId>}`. Substitui `fn rail` e `rail_button` atuais.

**Painel**: `egui::Panel::left("rail").frame(Frame::new().fill(L0)).exact_size(72.0)` **sem** margem interna; dentro, `ScrollArea::vertical().scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden).auto_shrink([false,false])` (a margem lateral de 16 px é feita posicionando o ícone em `rect.left()+16`, para a pílula poder ficar em x=0). `ui.spacing_mut().item_spacing.y = GAP`.

**Armadilhas do egui**
- Sem máscara de recorte arbitrária: imagem em squircle = **malha em leque** com UV (trecho 1). A borda da malha não tem suavização: sobrepor um contorno de 1 px na cor da barra (`L0`) por cima.
- Sem letter-spacing nem negrito de fonte variável: a sigla usa `FontId::proportional(size)` na tabela 18/18/16/16/14/12/10 (peso "medium" indisponível; aceitar regular).
- Sem emoji colorido: glifos (casa/logo, "+", compasso, pasta, tela, alto-falante, pausa) desenhados com `painter.line_segment`, `circle_stroke`, `convex_polygon`. Início: o "t" serifado do Telacast em 24 px, em vez do logo do Discord.
- Anel recortado em volta do selo: desenhar um círculo/cápsula da cor `L0` de raio +2 antes do selo (o fundo é sólido, então equivale ao recorte). Na pasta aberta o fundo muda; usar a cor do fundo da pasta nessa altura.
- Tooltip nativo (`on_hover_text`) tem atraso de 0,5 s e posição no ponteiro: não serve. Desenhar `Area` própria (trecho 4) ou zerar `style.interaction.tooltip_delay` só neste escopo e ancorar manualmente.
- `animate_bool_with_time` é linear: para o ease-out do CSS usar `animate_bool_with_time_and_easing(id, v, 0.2, egui::emath::easing::cubic_out)`.
- `CornerRadius` usa `u8` por canto (`nw/ne/sw/se`); `Margin::symmetric(i8,i8)`.
- Arrastar: `Sense::click_and_drag()` + `Response::dnd_set_drag_payload`; alvos com `ctx.input(|i| i.pointer.hover_pos())` e `DragAndDrop::payload::<T>(ctx)`; soltar com `i.pointer.any_released()`. O ícone arrastado é pintado numa `Area` em `Order::Tooltip` seguindo o ponteiro, com o original trocado pelo marcador tracejado (`Shape::dashed_line` fechando o caminho).
- A repintura: pedir `ctx.request_repaint()` enquanto alguma animação estiver entre 0 e 1 (o `animate_*` já faz).

### Trecho 1 - squircle e imagem recortada

```rust
use eframe::egui::{self, epaint::Vertex, pos2, Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, TextureId};

/// Superelipse |x/a|^n + |y/b|^n = 1. n = 4 reproduz o ícone da barra.
pub fn squircle_pts(r: Rect, n: f32, seg: usize) -> Vec<Pos2> {
    let (c, a, b) = (r.center(), r.width() / 2.0, r.height() / 2.0);
    (0..seg)
        .map(|i| {
            let t = i as f32 / seg as f32 * std::f32::consts::TAU;
            let (s, k) = t.sin_cos();
            let f = |v: f32| v.signum() * v.abs().powf(2.0 / n);
            pos2(c.x + a * f(k), c.y + b * f(s))
        })
        .collect()
}

pub fn paint_squircle(p: &Painter, r: Rect, fill: Color32) {
    p.add(Shape::convex_polygon(squircle_pts(r, 4.0, 48), fill, Stroke::NONE));
}

/// Imagem dentro da forma, com UV proporcional ao retângulo.
pub fn paint_squircle_image(p: &Painter, r: Rect, tex: TextureId, ring: Color32) {
    let pts = squircle_pts(r, 4.0, 64);
    let uv = |q: Pos2| pos2((q.x - r.min.x) / r.width(), (q.y - r.min.y) / r.height());
    let mut m = Mesh::with_texture(tex);
    m.vertices.push(Vertex { pos: r.center(), uv: uv(r.center()), color: Color32::WHITE });
    for q in &pts {
        m.vertices.push(Vertex { pos: *q, uv: uv(*q), color: Color32::WHITE });
    }
    let n = pts.len() as u32;
    for i in 0..n {
        m.indices.extend([0, 1 + i, 1 + (i + 1) % n]);
    }
    p.add(Shape::mesh(m));
    // esconde o serrilhado da borda sem suavização
    p.add(Shape::closed_line(pts, Stroke::new(1.0, ring)));
}
```

### Trecho 2 - pílula animada e selo

```rust
/// `left` = x da borda esquerda da barra; `cy` = centro vertical do ícone.
pub fn rail_pill(ui: &egui::Ui, id: egui::Id, left: f32, cy: f32, hov: bool, sel: bool, unread: bool) {
    let ez = egui::emath::easing::cubic_out;
    let a = |v: bool, k: &str| ui.ctx().animate_bool_with_time_and_easing(id.with(k), v, 0.2, ez);
    let (h, s, u) = (a(hov, "h"), a(sel, "s"), a(unread, "u"));
    let vis = h.max(s).max(u);
    if vis <= 0.001 { return; }
    // 8 (não lido) -> 20 (hover) -> 40 (selecionado); selecionado vence hover
    let hh = mix(mix(8.0, 20.0, h), 40.0, s);
    let dx = -4.0 * (1.0 - vis);                     // translateX(-4) -> 0
    let r = Rect::from_min_size(pos2(left + dx, cy - hh / 2.0), egui::vec2(4.0, hh));
    let clip = Rect::from_min_size(pos2(left, cy - 20.0), egui::vec2(8.0, 40.0));
    ui.painter().with_clip_rect(clip).rect_filled(
        r,
        egui::CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 },
        alpha(TEXT, vis),
    );
}
fn mix(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

pub fn count_text(n: u32) -> String {
    if n < 1000 { n.to_string() } else { format!("{}k+", (n / 1000).min(9)) }
}
pub fn badge_w(n: u32) -> f32 { if n < 10 { 16.0 } else if n < 100 { 22.0 } else { 30.0 } }

/// Selo inferior direito com anel de 2 px da cor de fundo.
pub fn count_badge(p: &Painter, icon: Rect, n: u32, fill: Color32, bg: Color32) {
    let w = badge_w(n);
    let r = Rect::from_min_max(pos2(icon.right() - w, icon.bottom() - 16.0), icon.right_bottom());
    p.rect_filled(r.expand(2.0), 10.0, bg);
    p.rect_filled(r, 8.0, fill);
    p.text(r.center(), egui::Align2::CENTER_CENTER, count_text(n), egui::FontId::proportional(11.5), Color32::WHITE);
}
```
O selo usa branco sobre `RED` (fundo `#d9806f`): contraste baixo (~2,6:1). Usar `AMBER_INK` não combina com o vermelho; ver pergunta 4.

### Trecho 3 - alvos de arrastar e prévia

```rust
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Zone { Before, Combine }

/// Mesma geometria do original: faixa de (top-16) a (bottom+4) dividida ao meio.
pub fn drop_zone(icon: Rect, y: f32, can_combine: bool) -> Zone {
    let mid = (icon.top() - 16.0 + icon.bottom() + 4.0) / 2.0;
    if y < mid || !can_combine { Zone::Before } else { Zone::Combine }
}

// no laço de itens, depois de allocate/pintar o ícone:
if let Some(drag) = egui::DragAndDrop::payload::<NodeId>(ui.ctx()) {
    let hit = Rect::from_x_y_ranges(icon.x_range().expand(16.0), (icon.top() - 16.0)..=(icon.bottom() + 4.0));
    if let Some(pos) = ui.ctx().pointer_hover_pos().filter(|p| hit.contains(*p)) {
        let can_combine = node.parent.is_none() && !drag.is_folder();      // pasta nunca combina
        let z = drop_zone(icon, pos.y, can_combine && *drag != node.id);
        match z {
            Zone::Before => ui.painter().rect_filled(
                Rect::from_min_size(pos2(left + 20.0, icon.top() - 6.0), egui::vec2(RAIL_W - 40.0, 4.0)),
                2.0, AMBER),
            Zone::Combine => paint_combine_preview(ui, icon),   // contorno AMBER 1 px + fundo alpha 0.28
        }
        if ui.input(|i| i.pointer.any_released()) {
            acts.push(Act::MoveNode { id: *drag, target: node.id, zone: z });
        }
    }
}
```
A "permanência de 70 ms" antes de mostrar a prévia: guardar `Instant` de entrada no alvo e só pintar após `>= 70 ms` (usar `ctx.input(|i| i.time)`).

### Trecho 4 - tooltip à direita, sem atraso, com seta

```rust
pub fn rail_tooltip(ui: &egui::Ui, resp: &egui::Response, text: &str) {
    let show = resp.hovered() && !resp.is_pointer_button_down_on() && !drag_active(ui);
    let t = ui.ctx().animate_bool_with_time(resp.id.with("tip"), show, 0.08);
    if t <= 0.0 { return; }
    let anchor = pos2(resp.rect.right() + 12.0, resp.rect.center().y);
    let area = egui::Area::new(resp.id.with("tip_area"))
        .order(egui::Order::Tooltip).fixed_pos(anchor).pivot(egui::Align2::LEFT_CENTER).interactable(false);
    let out = area.show(ui.ctx(), |ui| {
        ui.set_opacity(t);
        egui::Frame::new().fill(L3).stroke(Stroke::new(1.0, LINE2)).corner_radius(8)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_max_width(190.0);
                ui.label(egui::RichText::new(text).size(14.0).color(TEXT));
            });
    });
    // seta de 5 px apontando para o ícone
    let r = out.response.rect;
    let c = r.left_center();
    let tri = vec![pos2(c.x - 5.0, c.y), pos2(c.x + 0.5, c.y - 5.0), pos2(c.x + 0.5, c.y + 5.0)];
    ui.ctx().layer_painter(out.response.layer_id).add(Shape::convex_polygon(tri, L3, Stroke::NONE));
}
```
Para a escala 0,95→1 não há equivalente simples; ficar só com a opacidade (~80 ms).

### Esqueleto de `rail_ui` (ordem de desenho)
1. Início (`icon_button(Glyph::Logo)`, selo = pedidos de amizade).
2. DMs não lidas (`s.snap.dms.filter(unread)`), ícones circulares com avatar e selo.
3. Separador 32×1 (`LINE2`), `item_spacing` 8 acima e abaixo.
4. Para cada raiz de `s.folders`: servidor solto → `server_icon`; pasta → `folder_group` (cabeçalho, fundo, lista com altura animada; recorte com `ui.new_child` + `set_clip_rect`).
5. "Adicionar" (sem pílula), "Descubra" (omitir ou desativado), nada de "Baixar apps".
6. Fim: alvo de soltar "fim da lista".
7. Pós-laço: tooltip, menus de contexto (`resp.context_menu(|ui| ...)`, helper `menu_item(ui, label, danger) -> bool`, largura mínima 200, raio 10), rolagem até o selecionado (`ui.scroll_to_rect(icon, Some(Align::Center))` quando `rail.scroll_to` está setado), tira de salto para não lido (secundário).

Cores Grafite sugeridas: fundo `L0`; ícone sem imagem repouso `L3` + texto `TEXT2`; hover `L4` + `TEXT`; selecionado `AMBER` + `AMBER_INK` (150 ms); botão Adicionar/Descubra repouso `L3` + glifo `MUTED`, hover `AMBER`; pílula `TEXT`; selo de menção `RED`; selo de mídia `L3` com glifo `TEXT2`, **âmbar quando eu estiver assistindo/transmitindo ali**; alvo de combinar/inserir `AMBER` (verde fica só para presença); fundo de pasta aberta `alpha(cor,0.15)` com borda `LINE2`; sem cor de pasta: glifo `TEXT2`. Nada de gradiente (a máscara em degradê de 16 px no pé da coluna do original **não** será copiada).

## 7. Ordem de entrega (cada passo verificável)

1. **Medidas e forma**: `RAIL_W 72`, ícones 40 com squircle, sigla pela regra do Discord (tabela de tamanhos), separador, botões Início e "+" no novo estilo, rolagem sem barra. *Usuário vê:* coluna nova, ícones squircle com siglas corretas.
2. **Pílula e feedback**: pílula 8/20/40 animada, hover/selecionado 150 ms, 1 px ao pressionar. *Vê:* a barrinha branca crescendo ao passar o mouse e no servidor ativo.
3. **Tooltip imediato** à direita com seta. *Vê:* nome do servidor ao encostar, sem espera.
4. **Não lido local**: pílula de 8 px no servidor com canal em `Session.unread` (usa o que já existe; ainda sem contagem).
5. **P0 servidor: `read_state` + `Ack`** e selo de menções com contagem e anel; tira "salto para não lido". *Vê:* número vermelho no canto; some ao abrir o canal.
6. **Menu de contexto do servidor** (marcar como lido, convidar [já há convite], copiar ID, sair; demais itens desabilitados com texto "em breve" só se o usuário aceitar).
7. **Ícone com imagem** (P1: `icon_hash`, upload, cache de texturas, malha recortada).
8. **Pastas (somente leitura)**: `Snapshot.folders`, cabeçalho 48 com 4 mini-ícones, abrir/fechar com animação de 150 ms e fundo; abrir/fechar salvo localmente.
9. **Arrastar**: reordenar (linha âmbar), combinar (prévia, pasta nova), mover para fora, `SetFolders` persistente.
10. **Menu e configurações da pasta** (nome, cor, marcar como lida, fechar todas).
11. **Selo de mídia / transmissão ao vivo** (canal→servidor a partir de `live`), DMs não lidas na barra, atalhos Ctrl+Alt+↑/↓/→, rolar até o selecionado.
12. Tooltip rico (silenciado, pessoas em voz) e salto para não lido refinado.

## 8. Perguntas em aberto (mudam o que será construído)

1. **Forma e morfa**: seguir o cliente atual (squircle fixo; recomendado) ou recriar a morfa antiga círculo→quadrado arredondado ao passar o mouse (raio 50% → ~30%, 150 ms)?
2. **Hover da sigla igual ao selecionado** (fica tudo âmbar ao passar o mouse, como no Discord) ou hover discreto (`L4`) e só o selecionado em âmbar?
3. **Pílula** branca (`TEXT`, como no Discord) para todos os estados, ou âmbar no selecionado?
4. **Cor do selo de menções**: usar `RED` (hoje reservado a "erro/destrutivo") ou âmbar com tinta escura? Vermelho tem a semântica universal e baixo contraste com texto branco (uso `AMBER_INK`/preto no texto do selo?).
5. **Escopo de dados**: pastas e silenciar valem a pena já (exige tabelas e `SetFolders`), ou começar só com não lido/menções e sigla?
6. **Descubra e Baixar apps**: omitir (instância privada) ou manter "Descubra" desativado como placeholder?
7. **Pastas**: paleta de cores fixa (quantas? sugestão: 6 neutras quentes do Grafite) ou seletor livre; tamanho máximo do nome?
