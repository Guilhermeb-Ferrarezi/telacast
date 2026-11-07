# 04 - Lista de mensagens do chat (cabeçalho, linhas, rolagem)

Legenda de evidência: **[F]** confirmado no bundle do Discord (arquivo e trecho citados), **[I]** inferência / conhecimento do produto, **[?]** não consegui confirmar.
Arquivos citados (todos em `.../scratchpad/discord-bundle/`): `web.0bcf9a9e83b2c3c7.js` = "WEB" (o maior; outras 4 versões `web.*` têm o mesmo conteúdo nas partes lidas), `1034d42990828b1b.js` = "CH" (cabeçalho do canal), `12064a7d28fc7eb1.js` = pt-BR, `2552f3a00fc8192a.js` = en-US (ambos truncados no fim do arquivo: ~22 mil chaves de ~23 mil). Os números após `@` são deslocamentos de caractere dentro do arquivo.

## AVISO SOBRE A QUALIDADE DA EVIDÊNCIA (ler antes)

O cache extraído **não contém os CSS dos componentes** (os 4 `.css` só têm variáveis de tema: `:root`, `.theme-dark` etc.) **nem os chunks lazy do componente da lista de mensagens** (o JSX de `MessageList`, `ChannelMessage`, `MessageActionBar`, `Reactions`, divisores). O módulo de classes `959760` é importado (`tu=n(959760)` @6560576) mas sua definição não está no cache; as classes aparecem ofuscadas (`tu.Sg`, `m.vO`). Consequências:

- **Medidas de padding/margem/altura de linha do Discord NÃO são [F].** Onde dou px do Discord, é [I] (memória do CSS conhecido do cliente web) e aparece marcado. As medidas finais do Telacast vêm da maquete `docs/ui-preview/grafite.html` (cabeçalho 64, padding lateral 28, avatar 38, gap 14, linha 14,5/22) e do `theme.rs`; essas são decisões nossas, não do Discord.
- **São [F]**: toda a lógica do store de mensagens (janelas, paginação, saltos), os limites de mídia e de embed (400x300 etc.), a estrutura de embed (campos, galeria, miniatura 80x80), o esqueleto de carregamento, os atalhos de mensagem, a barra "mensagens antigas / resposta", o preview de arquivo de texto, as configurações de exibição, e **todas as strings pt-BR** (i18n está completo mesmo onde o JSX falta).
- **Não confirmado em lugar nenhum do bundle**: intervalo de agrupamento (uso 7 min [I]), limiar de "carregar mais ao rolar", tamanho exato da barra de ações, tamanho de anexos de imagem (550x400 [I]).

---

## 1. Resumo e fora do escopo

A lista de mensagens é uma lista vertical de **linhas heterogêneas** (mensagem inicial de grupo, continuação, divisor de dia, divisor "NOVO", mensagem de sistema, carregador/esqueleto, "início do canal") ancorada no fundo, com histórico carregado ao rolar para cima e um botão para voltar ao presente quando o usuário está numa janela antiga. Cada mensagem pode ter cabeçalho de resposta, texto, anexos, embeds e reações, e mostra uma barra de ações flutuante no hover e um menu de contexto. O cabeçalho do canal tem título, tópico e uma barra de ícones (tópicos, notificações, fixadas, membros, busca).

No egui o ponto difícil é **altura variável + virtualização + ancoragem no fundo + preservar posição ao inserir histórico acima**. O plano propõe um modelo de linhas, cache de alturas por largura e `ScrollArea::show_viewport` com offset controlado por nós.

**Fora do escopo** (outras áreas): caixa de entrada/composer e barra "respondendo a" do composer, renderização de markdown (apenas o ponto de encaixe `render_content`), menções/autocomplete, indicador "digitando", lista de membros, busca (resultados), threads/fórum (só o ícone), voz, emoji picker, perfil/popout de usuário, modal de imagem (lightbox), confirmações de moderação.

---

## 2. Árvore de componentes e medidas

Convenção: "**D**" = valor do Discord, "**T**" = valor adotado no Telacast (Grafite). Onde só há T, a medida é decisão nossa derivada da maquete.

### 2.1 Cabeçalho do canal

```
ChannelHeader  (altura T 64, borda inferior 1px LINE, padding lateral 28)           [maquete .head]
 |- [esq] ícone de canal "#" (T 20px, MUTED) + título (serifa 24, TEXT)
 |        divisor vertical 1x20 (LINE2) + tópico (13px, MUTED, 1 linha, reticências)   [maquete .topic]
 |- [dir] Toolbar: [Tópicos][Notificações][Fixadas][Membros] [Campo de busca]
```
- D: altura 48 [I]. T: 64 (maquete; hoje o app usa 63.0 em `chat_view`: padronizar 64).
- Itens da barra [F, strings pt-BR]: "Tópicos", "Config. de notificação", "Mensagens fixadas", "Mostrar lista de membros"/"Ocultar lista de membros", campo "Buscar em {servidor}" (`LDpotA`) ou "Buscar" em DM; mais "Caixa de Entrada" e "Ajuda" (fora do pedido; só mencionar). Ordem à direita no Discord [I]: Tópicos, Notificações, Fixadas, Membros, Busca, Caixa de Entrada, Ajuda. T: os 5 pedidos.
- Botão de ícone: componente `nh.In` com props `tooltip, icon, iconSize:20, selected, badge` [F: CH @15200-15900, `iconSize:20`]. **Selecionado** (`selected`) = estado "painel aberto" (membros) e o **tooltip troca** entre "Ocultar lista de membros" e "Mostrar lista de membros" [F: CH `nN()` usa `Axvx8c` quando `section===MEMBERS`, senão `gxChDx`]. `badge` = `{type:"unread", position:"bottom"}` (ponto) ou `{type:"important", text:"N"}` (pílula com número) [F: CH `n_()`].
- T (Grafite): hit area 32x32, glifo 20px desenhado com `painter`, raio 8, gap 4. Repouso: glifo TEXT2; hover: fundo L4 + glifo TEXT; selecionado: fundo L3, glifo AMBER; desabilitado: glifo MUTED a 40% (usar para Tópicos/Fixadas enquanto não existirem). Badge: ponto de 8px AMBER no canto inferior direito do ícone; com contagem, pílula de altura 16, fundo AMBER, texto AMBER_INK 10px.
- Busca: D 144px que expande para 240 ao focar, altura 24 [I]. T: 200 -> 280 ao focar (animação 160ms ease-out), altura 32, raio 9, fundo L3, borda LINE2 (AMBER ao focar), placeholder MUTED 13px, lupa à direita (ícone vira "x" com texto). Atalho: existe o bind `FOCUS_SEARCH` "Buscar" [F: WEB @2357541]; combinação padrão Ctrl+F [I].
- Tópico: clique abre o texto completo (modal/popover) [I]; tópico até 1024 chars [DOC].
- Responsivo [I]: abaixo de ~700px de largura útil o tópico some primeiro, depois a busca vira ícone, depois Tópicos/Notificações entram num menu "...".
- Pílula de transmissão "Ao vivo" do Telacast continua no cabeçalho, entre o tópico e a barra (já existe no `chat_view`).

### 2.2 Região de mensagens

```
MessagesRegion (preenche entre cabeçalho e composer; fundo L2)
 |- NewMessagesBar (topo, sobreposta, altura T 28)            só com não lidas
 |- ScrollArea (virtualizada, âncora no fundo)
 |    |- Row::StartOfChannel  (ou Row::Loader/Skeleton no topo)
 |    |- Row::DayDivider | Row::NewDivider
 |    |- Row::Message { group_start, ... }     <- 90% das linhas
 |    |     |- ReplyHeader (opcional)
 |    |     |- Avatar(só group_start) | Gutter (hora no hover, se continuação)
 |    |     |- Header: nome(cor de cargo) + carimbo (+ tag BOT) [só group_start]
 |    |     |- Content: texto + "(editado)"  | ou EditBox inline
 |    |     |- Attachments / Embeds / Reactions
 |    |     |- HoverBar (overlay)
 |    |- Row::System
 |    |- Row::Failed("Erro ao carregar mensagens") (rodapé/topo)
 |- JumpBar (rodapé, sobreposta, altura T 36)                 só em janela antiga
```

### 2.3 Medidas verticais por tipo de linha (T)

Premissas: corpo 14,5px / linha 22 (maquete `.m p`), nome 14 semibold, carimbo 11,5. Avatar 38 (maquete) em x = 28; texto em x = 28 + 38 + 14 = **80**. Coluna de gutter da continuação: x 28..80 (52px), carimbo da hora no hover centrado, 11px MUTED.

| Tipo de linha | Altura (T) | Composição | Origem |
|---|---|---|---|
| Mensagem, início de grupo, 1 linha | **58** | margem sup. 14 + cabeçalho 20 + 1 linha 22 + inf. 2 | D usa 17 de margem entre grupos [I]; 58 é derivado |
| Mensagem, início de grupo, N linhas | 36 + 22*N | idem | derivado |
| Continuação, 1 linha | **24** | pad 1 + 22 + pad 1 | derivado |
| Continuação, N linhas | 2 + 22*N | | |
| Cabeçalho de resposta (acréscimo) | +22 | 18 de altura + 4 de espaço | [I] |
| Divisor de dia | **46** | 18 + rótulo 20 + 8 (maquete `.day`) | maquete |
| Divisor "NOVO" | **24** | 8 + linha 1px + rótulo 11px, com 8 abaixo | [I] |
| Mensagem de sistema | **34** | 8 + 22 + 4 | derivado |
| Início do canal (cabeçalho de boas-vindas) | 160 a 200 | ícone 56 + título serifa 28 + subtítulo 14 + 24 de margem | [I] |
| Esqueleto (1 mensagem fantasma) | 44 + 22*k [?] | ver 2.10 | WEB @6577000 (`A=44,h=22,f=26,I=26,p=6`, uso não confirmado) |
| Linha de falha ao carregar | 40 | texto + botão | [F] existência, [I] altura |
| Espaço final sob a última mensagem | 12 | maquete `padding:8px 0 12px` | maquete |

Estimativa de linhas de texto para linhas ainda não medidas: `ceil(chars / (largura_texto / 7.4))`, mínimo 1 (Inter 14,5px ~ 7,4px por caractere médio); depois da primeira renderização usa-se a altura real (ver 6.2).

### 2.4 Mensagem (anatomia)

- **Avatar**: círculo 38, em x = 28, alinhado ao topo do cabeçalho (y = top_linha + 16). D 40, `left:16`, `margin-top: calc(4px - .125rem)` [I]. Clique abre perfil (outra área); sem clique no Telacast hoje (`avatar()` já devolve `Response`).
- **Cabeçalho (só início de grupo)**: `nome` + 10px + `carimbo` (+ tag "BOT" depois do nome, T: pílula 16px de altura, fundo L4, texto 10px caixa alta). Baseline alinhada; altura 20.
  - **Nome com cor de cargo**: cor do cargo de maior posição com cor, senão TEXT; semibold; hover sublinha [I]; clique abre popout [fora]. Há configuração de estilo "cor do cargo em nome/ponto/nenhum" (`roleStyle: "username"` é o padrão) e dessaturação de cores de cargo [F: WEB @2651881 `roleStyle:"username"`, `desaturateUserColors`, `saturation`]. T: para contraste, misturar a cor do cargo com TEXT em 15% quando a luminância relativa < 0,35 sobre L2 (o Discord ajusta com "saturação"/"contraste" [F]; algoritmo exato [?]).
  - **Carimbo**: 11,5px MUTED, `tabular-nums` não existe no egui (usar fonte Inter que já é tabular para dígitos? Inter tem `tnum` opcional, não via egui; aceitar). Formato [I]: hoje -> `Hoje às 14:05`; ontem -> `Ontem às 14:05`; antes -> `07/11/2026 14:05`. O pt-BR do bundle confirma o padrão "Hoje às ..." [F: `FnFI3m` "Hoje às 9h"]. Tooltip no hover do carimbo (500ms): data completa `sábado, 7 de novembro de 2026 às 14:05` [I].
  - Nome de usuário de webhook/bot: tag BOT; fora do escopo agora.
- **Texto**: 14,5/22, TEXT2 (maquete). `max-width` da maquete é 70ch; **D não limita** a largura do texto (usa a largura toda da coluna menos 48 de padding direito [I]). T: não limitar a 70ch em janelas largas? Decisão: limitar a 880px de coluna de texto para leitura confortável é uma escolha nossa (pergunta em aberto, 8).
- **"(editado)"** [F: `C8sXIM` pt "editado"; tooltip `CDzOFd` "Editado {data}"; aria `0x1HBD` "mensagem editada"]: texto 11px MUTED logo após o último caractere, entre parênteses no Discord ("(editado)") [I]. Tooltip no hover com data de edição.
- **Hover da linha**: fundo `alpha(TEXT, 0.018)` (maquete `rgba(255,255,255,.018)`), sem transição ou 80ms. Em continuação o carimbo `HH:MM` aparece no gutter.
- **Linha em foco por salto/resposta** (flash): ao saltar para uma mensagem o store marca `jumpFlash` e a linha pisca [F: WEB @2366855 `jumpFlash=!0`, `jumpTargetId`, `jumpType: ANIMATED|INSTANT`]. T: fundo `alpha(AMBER, 0.14)` que decai a 0 em 1,6s (ease-out), duração [I].
- **Mensagem pendente / falhou**: estados `SENDING` e `SEND_FAILED` existem no store e **sempre ficam no fim da lista**, fora da ordenação por id [F: WEB @2362500 `h(e)=state===SENDING||SEND_FAILED`, `I(e)`; `_insertInOrder` faz `push` se for pendente]. T: pendente = texto a 55% de opacidade; falha = texto com RED e linha "Não enviada. Clique para tentar de novo / apagar" [I].

### 2.5 Agrupamento de mensagens (regras)

Nada disso está no cache (a função que decide o grupo está no chunk ausente). Regras [I], coerentes com o comportamento conhecido:
1. Mesmo autor da mensagem anterior; **e** diferença de tempo < **7 minutos** [I] (o app hoje usa 5 min: `5*60*1000` no `chat_view`; a constante `n2=6e5` do bundle [F @319639] é o *timeout de ociosidade* de 10 min (`A.sdF` em @9558674), não o de agrupamento; não confundir).
2. Mesmo dia local (`same_day`); divisor de dia ou "NOVO" entre as duas quebra o grupo.
3. A mensagem atual **não** é resposta (respostas sempre iniciam grupo) e a anterior não é de sistema. Mensagens de sistema quebram o grupo antes e depois.
4. Sem diferença de "tag/nome de bot-webhook" entre as duas (autor igual já cobre).
5. Uma mensagem com cabeçalho de resposta é sempre início de grupo; a seguinte do mesmo autor pode continuar o grupo dela.
6. Edição não muda agrupamento (usa o timestamp de criação).
7. O espaço entre grupos é configurável: "Espaço entre grupos de mensagens" [F: `Q6lKkg`; WEB @2656094 `messageGroupSpacing` com valor padrão por modo, `h.ES` acolhedor / `h.y5` compacto, e lista de valores válidos `h.qh`; valores numéricos [?]; UI mostra 0/4/8/16/24 [I]]. T: constante `GROUP_GAP = 14`; opção avançada futura.

### 2.6 Divisor de dia

- Estrutura: filete - rótulo - filete (D: linha de 1px com o rótulo centralizado sobre ela, 12px semibold [I]). T (já implementado em `theme::day_divider`): rótulo serifa itálica 14 MUTED, filetes LINE; altura 46. Texto: `hoje`/`ontem`/`dd/mm/aaaa` hoje; D usa data longa `7 de novembro de 2026` [I]. Sugestão: manter "hoje/ontem" em minúsculas (já é a voz do Grafite) e trocar a data numérica por `7 de novembro de 2026` (chrono `%-d de %B de %Y` exige tabela de meses pt-BR própria; chrono sem locale devolve inglês).
- Posição: o divisor aparece antes da primeira mensagem de cada dia local; também antes da primeira linha carregada se `!has_more_before` (início do histórico). Não há divisor "colado" no topo (sticky) [I].

### 2.7 Divisor e barra "novas mensagens"

- **Divisor "NOVO"** [F strings: `psHMa6` "Novo", `PHioKd` "NOVO"; usos do JSX [?]]: linha horizontal na posição `last_read_message_id` + pílula "NOVO" colada à direita. D: vermelho (`status-danger`) [I]. **T: AMBER** (Grafite só usa vermelho para erro/destrutivo; acento único é âmbar). Filete 1px AMBER a 70%, pílula altura 14, raio 7 (esq.), fundo AMBER, texto AMBER_INK 10px semibold caixa alta, alinhada à direita com 12px de margem.
- **Barra no topo** [F strings: `wvtbbG` "{count}+ mensagens novas desde {short}", `tHqbtg` "{count}+ mensagens novas desde as {short} do dia {long}" (quando passou de um dia), botão "Marcar como lida" `e6RscS`, dica "Avançar para a mensagem não lida mais antiga" `3HAurM`]. O "+" indica contagem truncada (há mais não lidas que as carregadas). Comportamento [I]: clicar na barra salta para a primeira não lida (`JUMP_TO_FIRST_UNREAD`); clicar em "Marcar como lida" confirma (ack) o canal e a barra some; a barra some também ao rolar até o divisor [I]. T: altura 28, fundo AMBER, texto AMBER_INK 12,5px semibold, "Marcar como lida" à direita com ícone de "visto"; sobreposta no topo da região (não empurra a lista), com animação de entrada de 140ms (deslizar 28px + fade).
- **Tecla Esc** [F: WEB @2332834 função `b`]: se há mensagens depois da janela atual (`hasMoreAfter`) **salta para o presente** (carrega 30, `L.EMb`); se o canal tem não lidas, **marca como lido**; sempre faz o scroll ao final. `Shift+Esc` marca todos os canais do servidor como lidos [F: WEB @2332740 `binds:["shift+esc"]`]. Dica de onboarding: "{hook} uma mensagem para marcá-la como não lida" (Alt+clique) [F: `aes5VI`].

### 2.8 Botão / barra "ir para o presente"

Componente `tR` (WEB @6565000-6566000) [F]:
- Props: `type` (0 = `OLD_MESSAGES`, 1 = `REPLY`), `onClick`, `className`. Não renderiza quando `disableInteractions` (modo overlay).
- Conteúdo: rótulo à esquerda + botão pequeno primário à direita (`size:"sm"`, `variant:"primary"`). Se **não há `onClick`**, mostra **reticências pulsantes** (carregando) em vez do botão.
- Textos: tipo 0 -> rótulo en "You're viewing older messages" [I: a chave `4EvBbw` não está nos dois arquivos de idioma truncados] / pt-BR sugerido "Você está vendo mensagens antigas"; botão `gpoQsB` "Avançar para mensagens recentes". Tipo 1 -> rótulo `1J6Xq7` "Você está visualizando uma resposta."; botão `k3RM8z` "Voltar à mensagem original".
- Segundo componente `tL` (WEB @6566200): linha clicável com texto `1zUvlw` "Erro ao carregar mensagens" e reticências quando `loading` (é o estado de falha de carga com tentar-de-novo); `focusProps` com folga de 4/4/12/4.
- Quando aparece [F]: `hasMoreAfter == true` (a janela não inclui o presente) [`hasPresent() = _after.length>0 && _after.wasAtEdge || !hasMoreAfter`, WEB @2371900]. `jumpToPresent(canal, 30)`: se o presente está em cache, só repõe a janela (`LOAD_MESSAGES_SUCCESS_CACHED`), senão busca 30 mensagens mais recentes [F: WEB @666100]. Depois disso `hasMoreAfter=false` e a barra some.
- T: faixa de altura 36 colada acima do composer (largura da lista), fundo L3, borda superior LINE2, texto 13 TEXT2 à esquerda (padding 16), botão pequeno AMBER à direita (altura 26, texto AMBER_INK 12 semibold). Entrada: 140ms deslizando de baixo. Em janela no presente com o usuário rolado para cima (sem `hasMoreAfter`), mostrar uma **pílula flutuante** "Novas mensagens" ao centro-inferior em vez da faixa [I]: contador de mensagens recebidas desde que saiu do fundo; clique rola ao fim.

### 2.9 Respostas

Cabeçalho de resposta (acima do cabeçalho da mensagem) [F strings, I layout]:
```
   ┌── (conector curvo)  [avatar 16]  @Nome  prévia do texto original (1 linha, reticências)
   [avatar 38]  Nome  Hoje às 14:05
              texto da resposta
```
- Conector: curva em L de 2px LINE2 saindo do centro do avatar da mensagem até a esquerda do mini-avatar; largura 28 (x 20..48), raio 6 [I]. Desenhar com `painter.line_segment` x2 + arco aproximado com `Shape::line` de 6 pontos.
- Mini-avatar 16px; nome com cor de cargo 13px semibold (menção `@` se a resposta menciona); prévia 13px MUTED em 1 linha; **clicar na prévia/nome salta para a mensagem original** (animado) e abre a barra "Você está visualizando uma resposta" com botão "Voltar à mensagem original" [F: `tR` tipo REPLY; store `jumpReturnTargetId` guarda para onde voltar, WEB @2366855].
- Original apagada: texto itálico `mE3KJN` "A mensagem original foi excluída" [F]. Original só com anexo: "Clique para ver anexo" `6hGo0c`; só sticker `kHdYCW`; mensagem de voz `XC3A52`; comando `E+6SSY`; "Clique para ver a mensagem" `Xxat6S` [F strings].
- Mensagens que respondem a você ganham destaque de "menção": barra lateral AMBER de 2px à esquerda e fundo `alpha(AMBER,0.06)` [I; D usa a cor de menção amarela/azul].
- "@ ligado/desligado" ao responder (composer) [F: `utGGIY`/`DH2o6R` "Clique para desabilitar/habilitar a menção ao autor original"] - área do composer.
- Atalho: tecla **R** com o foco/hover na mensagem [F: WEB @2355900 binds `r`].

### 2.10 Esqueleto de carregamento (primeiro carregamento e paginação)

[F: WEB @6576900-6578700, módulo `114212`]: o "ghost" de mensagem tem: (1) uma "barra de avatar" `d.my` com opacidade 0,08, (2) barra de nome com **largura aleatória 80-120 (rem = px/16)** e opacidade 0,1-0,2, (3) uma barra de corpo com **largura aleatória 120-320** e opacidade 0,06, (4) em modo compacto, barra de carimbo 40-50; só o primeiro de cada grupo desenha avatar+nome; `groupSpacing` entra como `marginTop` do primeiro ghost; anexo fantasma opcional com opacidade 0,03. Constantes exportadas `A=44, h=22, f=26, I=26, p=6` (significado exato [?], plausivelmente alturas de linha do ghost).
- T: sem shimmer/animação (Grafite proíbe fundos animados): barras L3 estáticas com as mesmas proporções. 3 grupos fantasmas (2 mensagens cada) no primeiro carregamento do canal; 1 grupo no topo durante a paginação para cima.

### 2.11 Reações

Estrutura [I] (JSX ausente; strings [F]):
- Linha de chips abaixo do conteúdo, `flex-wrap`, gap 4 (horizontal) e 4 (vertical), margem superior 4 (D) / 6 (T).
- **Chip**: altura T 24, padding 0 8, raio 8, emoji 16x16 + 6px + contagem 12,5px semibold. D 24 de altura, raio 8, borda 1px transparente [I].
  - Repouso: fundo L3, borda `LINE2`, contagem TEXT2.
  - Hover: fundo L4, borda `MUTED`; tooltip após 500ms com os nomes: "{a} e {b} reagiram {emoji}." / "{a}, {b} e {n} reagiram {emoji}." [F: `8rmtbd`, `/okjv0`]; títulos "Clique para reagir" `XVx5BN`, "Clique para reagir com {emoji}" `L1JQwE`, "Clique para remover" `wunKKA`, "Clique para remover {emoji}" `vjeruO` [F].
  - **Selecionada** (`me=true`): borda 1px AMBER, fundo `alpha(AMBER, 0.12)`, contagem AMBER (D: borda/fundo "brand" azulado [I]).
  - Clique: alterna a minha reação (otimista; reverter em erro). Contagem 0 remove o chip. Transição 100ms nas cores; ao incrementar, sem animação (Grafite).
- **Botão "+"** (`lfIHs4` "Adicionar reação"): chip 24x24 com "+" visível **só no hover da mensagem** (ou sempre que já houver reações [I]). Abre o seletor de emoji (outra área).
- Limites: 20 reações distintas por mensagem [DOC/I: o limite não aparece em nenhuma string do bundle]. Super-reações (`Kfcszg`): fora do escopo.
- Atalho: `+` com a mensagem em foco adiciona reação [F: bind "plus"]; digitar `+:nome:` no composer reage à última mensagem [F: dica `iLTeF6`].
- Configuração "mostrar reações" existe (`renderReactions`) [F: WEB @737600].

### 2.12 Anexos

**Limites de exibição** (o que é [F] está marcado):
- Imagem/GIF/vídeo de **embed**: máx **400x300** (vídeo vertical TikTok / YouTube Shorts: **400x450**) [F: WEB @10139000 função `w(embed,t,n)`; componente de imagem `maxWidth=400, maxHeight=300` por padrão @6344500].
- Imagem de **anexo** de mensagem: **550x400** [I] (valor não encontrado; o componente de mídia aceita qualquer `maxWidth/maxHeight`).
- Algoritmo de encaixe [F: módulo `515718` @10184216, funções `Uj`, `u`, `V`]: reduzir pela largura se `w > maxW` (fator `maxW/w`), arredondar, depois reduzir pela altura se `h > maxH`; nunca ampliar (`min(...,1)`); respeita `minWidth/minHeight`. Há também um cálculo para o lightbox: largura máx = `min(0.75 * innerWidth, 2000)`, altura máx = `min(0.65 * innerHeight, 2000)` [F: `c(e,t)`].
- Três layouts de mídia: `STATIC` (tamanho exato), `RESPONSIVE` (largura 100% até o máximo, `aspect-ratio: w/h`), `MOSAIC` (preenche a célula com `object-fit: cover`) [F: WEB @851900 e @12353470].
- Estados de carga: `LOADING` (placeholder com a cor/blurhash `placeholder`), `READY`, `ERROR` (ícone de imagem quebrada + texto "imagem indisponível" `sAfT1K` / alt `X4IxWL`) [F: WEB @852700-853200]. Carregamento é lazy por visibilidade. GIFs animam só no hover/foco (`hasMouseOver || hasFocus`) a menos que "reproduzir GIFs automaticamente" esteja ligado [F: WEB @6344700 `onMouseEnter`, `isAnimated`].
- Texto alternativo: botão pequeno "ALT" sobre a mídia que abre um popover com o texto [F: WEB @6560800-6562000 `tA`, `tf`].
- **Spoiler**: mídia/embeds escondidos por camada de ocultação com rótulo "SPOILER"; clicar revela [F: WEB @7916500-7917200, `obscureReason: SPOILER|EXPLICIT_CONTENT|...`, `onReveal`, `onToggleObscurity`]; para anexos a convenção é o prefixo do nome do arquivo `SPOILER_` [I].
- **Grade de imagens** de anexos (mosaico) [I]: até 10; 1 = própria; 2 = duas colunas iguais; 3 = grande à esquerda + 2 empilhadas à direita; 4 = 2x2; 5-10 = linhas de 2 e 3. Gap 4, raio 8, altura total máx 350 [I]. A grade equivalente de **embeds é [F]** (WEB @3631000 `renderImages`): contêiner com altura = `maxMediaHeight` (300), 2 colunas; coluna 1 = imagem 0 (+ imagem 2 quando há 4), coluna 2 = imagem 1 + (imagem 2 quando há 3) ou imagem 3.
- **Arquivo genérico**: cartão L3, borda LINE2, raio 8, largura máx 432 [I], ícone de arquivo, nome (link AMBER, reticências), tamanho `1,2 MB` MUTED, botão de download à direita.
- **Arquivo de texto com prévia** [F: WEB @6545800 `eT`]: mostra **6 linhas** recolhidas (variável CSS `--custom-plaintext-preview-collapsed-lines:6`) ou **100** expandidas; sufixo "... {n} linhas restantes" / "... {bytes} restantes"; barra inferior com nome+tamanho, seletor de linguagem, expandir/recolher, menu "..." (baixar, quebra de linha, renderizar markdown), modal de tela cheia.
- **Áudio / vídeo**: player inline (controles ao hover) [F: WEB @6374400]; vídeo mínimo 150x144 [F: `getMinSize`]. T v1: miniatura + triângulo de play; reprodução fica para depois.
- Configurações que controlam tudo isso [F: WEB @737600]: `inlineAttachmentMedia`, `inlineEmbedMedia`, `renderEmbeds`, `renderReactions`, `animateEmoji`, `animateStickers`, `gifAutoPlay`.

### 2.13 Embeds (prévia de link e embeds ricos)

Estrutura confirmada (classe `eE`, WEB @3626200-3641500). Ordem de blocos dentro do cartão: **botão de suprimir (x, canto)**, provedor, autor (ícone+nome), título (link), descrição, campos, mídia, miniatura (quando não há mídia grande), rodapé.
- Cartão `article`: **borda esquerda colorida** com `embed.color` (ignorada se for branco puro `0xffffff` ou oculto) [F: `getEmbedColor`]. D: 4px de borda, fundo secundário, raio 4, padding 8 12 16 16 [I]. T: borda 3px, fundo L3, raio 6, padding 10 14 14 14.
- `maxWidth` do cartão [F: `getMaxWidth`]: se tem imagem/vídeo: `largura_encaixada + 32` quando é vídeo ou a largura encaixada >= 300; **520** para embed rico com galeria de imagens; para GIFV, a largura encaixada. Senão sem limite (o cartão ocupa até a largura da coluna, e o D limita o texto a ~432 [I]).
- Miniatura: máx **80x80** [F: `maxThumbnailWidth/Height:80`], canto superior direito.
- **Campos**: grade de **12 colunas**; campos `inline` são agrupados em linhas de até **3**; cada campo ocupa `12/n` colunas; campo não-inline ocupa a linha inteira [F: `renderFields`]. Nome do campo semibold, valor normal.
- Rodapé: ícone opcional + texto, e `• carimbo` quando há data [F: `renderFooter`].
- Vídeo: miniatura com botão de play que carrega o player; GIFV autoplay mudo ao hover/visível.
- Embed de áudio (Spotify) é um `iframe` com largura máx 400 / mín 300 e altura por tipo: faixa **80**, episódio/programa **232**, álbum/artista/playlist **352** [F: WEB @8215838]. T: link simples (sem iframe).
- Limites de conteúdo (DOC, ver `docs/discord/03` 2.9): 10 embeds, título 256, descrição 4096, 25 campos, soma 6000.
- Telacast v1: **sem embeds**. Prévia de link (unfurl) só com servidor próprio (SSRF!) - P3.

### 2.14 Barra de ações no hover

Presente no hover de qualquer linha de mensagem (e quando a linha tem foco do teclado / menu aberto). Conteúdo [F strings pt-BR; ordem e composição I]:
```
 ┌───────────────────────────────────────────────┐
 │ [Reagir] [Responder | Editar] [Encaminhar] [⋯]│   altura T 28, raio 8, fundo L3, borda LINE2
 └───────────────────────────────────────────────┘
```
- Itens: "Adicionar reação" (`lfIHs4`), "Responder" (`5IEsGx`; em mensagem própria vem "Editar mensagem" `fsBWmS` no lugar), "Encaminhar" (`I3ltXO`), "Mais" (`UKOtz+`, abre o mesmo menu do botão direito). D mostra também 2-3 emoji rápidos recentes e o botão de criar tópico [I]. **Shift** mantido no hover revela extras (Fixar, Copiar texto, Apagar...) [I].
- Posição: canto superior direito da linha, **sobreposta à borda superior**: `top = -14` (maquete) / D `-16`; `right = 28` (T) / D 14 [I]. Item: botão 28x28 (maquete: altura 28, padding 0 10), divisor 1px LINE2 entre botões (maquete), glifo 16px.
- Visibilidade: aparece com `opacity 0->1` e `translateY 3->0` em **120ms** (maquete); sai em 120ms. Fica visível enquanto o ponteiro estiver sobre a linha **ou** sobre a própria barra (a barra sai 14px para fora da linha; ver 6.4).
- Teclado (com foco na mensagem) [F: WEB @2355500 lista de binds de MESSAGE]: `E` editar, `Backspace` apagar, `P` fixar, `+` adicionar reação, `R` responder, `F` encaminhar, `S` falar mensagem, `Ctrl+C` copiar texto, `Alt+Enter` marcar como não lida, `Esc` sair/focar o composer.

### 2.15 Menu de contexto da mensagem (botão direito / "Mais")

Itens e strings pt-BR [F por i18n], **ordem [I]** (mensagem de terceiros / própria):
1. Adicionar reação (submenu: 4 recentes + "Mais reações") `lfIHs4`
2. Responder `5IEsGx`
3. Encaminhar `I3ltXO` (P3)
4. Copiar texto `JrGD7E` (só se há texto)
5. Editar mensagem `fsBWmS` (só autor)
6. Fixar mensagem `MUldyN` / Desafixar mensagem `n4zKhA` (quem pode gerenciar mensagens)
7. Criar tópico `rBIGBL` (P3)
8. Apps (submenu) `PHjkRE` (fora)
9. Marcar como não lida `RpE9k7`
10. Copiar link da mensagem `Xrt5Po`
11. Falar mensagem `yGLjXF` (TTS; omitir)
12. --- Excluir mensagem `xwMqD7` (autor ou moderador; **vermelho**)
13. --- Denunciar mensagem `GwbdGe` (de terceiros; vermelho)
14. Copiar ID da mensagem `zBoHlf` (só modo desenvolvedor)
- Menus de link/imagem dentro da mensagem têm itens próprios: "Copiar link", "Abrir link", "Copiar imagem", "Salvar imagem" [F existência de `z4sP5J`/`iVzBOh`/`WqhZss` "Copiar link"].
- T: largura 232, item altura 32, padding 0 10, texto 13,5; hover L4; item destrutivo RED; separadores 1px LINE; atalho à direita MUTED 12px quando existe; cantos 10 (já é `menu_corner_radius`); sombra do tema. Abre no ponteiro, fecha com Esc/clique fora. Não deve abrir sobre a barra de ações.
- Rótulos de seleção: ao abrir o menu, a linha fica com fundo hover fixo.

### 2.16 Edição inline

- Entrar: `E`, "Editar mensagem" no menu/barra, ou seta para cima com o composer vazio (editar a última própria) [I].
- A área do texto vira um editor de várias linhas (L3, borda 1px AMBER, raio 8, padding 8 12) com o texto original; foco automático, cursor no fim. Abaixo, dica: "esc para **cancelar** • enter para **salvar**" onde as palavras em negrito são links clicáveis [F: `wDsPXs` "esc para {link} • enter para {link}"; `HVqAgL` "Esc para {link}"; `a7oCBj` "Enter para salvar"]. Fonte 12, MUTED.
- `Enter` salva; `Shift+Enter` quebra linha; `Esc` cancela; salvar vazio abre a confirmação de apagar [I]. Salvar sem mudança = cancelar silenciosamente.
- Enquanto edita, a linha fica realçada (fundo L3 suave) e a barra de ações some. Só uma edição por vez; começar outra cancela a primeira (descarta sem perguntar) [I].
- Resultado: o servidor devolve a mensagem com `edited_at`; "(editado)" aparece. Se o envio falhar, mantém o editor aberto com erro em RED abaixo.
- Erro por mensagem muito longa [F: `YSRIqa` "Mensagem muito longa", `l8rYLt` "Sua mensagem é muito longa."]. Limite do Telacast: 4000 (CHECK da migração).

### 2.17 Mensagens de sistema

Linha sem avatar: ícone de 16px na coluna do avatar (centrado em x ≈ 47), texto 14/22 TEXT2, nome do usuário em semibold com cor de cargo, carimbo 11,5 MUTED à direita do texto. Altura 34.
- **Entrada no servidor** [F: 13 variantes pt-BR em `12064a7d...` (`Jm6e0x`, `MGRnRT`, `EXOEGh`, `5uCTFN`, `rl45Qo`, `Bh9zpQ`, `RdEy1J`, `qcdp00`, `F7w2Ru`, `gSyOgK`, `uYgqv7`, `b/1SBX`, `LhebZF`)]:
  1. "{u} entrou no grupo." 2. "{u} chegou." 3. "Bem-vindo (ou vinda), {u}. Tem pizza aí?" 4. "Um {u} selvagem apareceu." 5. "{u} acabou de aterrissar." 6. "{u} acabou de cair de paraquedas no servidor." 7. "{u} acabou de aparecer!" 8. "Bem-vindo(a) {u}. Diga oi!" 9. "{u} pulou para o servidor." 10. "Todos são bem-vindos {u}!" 11. "Que bom que você chegou, {u}." 12. "Bom te ver, {u}." 13. "Eba, você conseguiu, {u}!"
  Escolha da variante: determinística pelo id/timestamp da mensagem (Discord usa o `timestamp % 13` [I]); no Telacast, guardar só `kind=Join` e escolher no cliente por `id % 13` (consistente entre clientes). Ícone: seta para a direita (verde no Discord [I]; T: OK `#74b98f`, cor de presença positiva já existente). Ação: botão "Diga oi" com a figurinha de aceno [I]; T: omitir.
- Outros tipos úteis para o Telacast [F strings]: "{u} fixou uma mensagem neste canal." (`/M60j0`, com sufixo "Ver todas as mensagens fixadas" em `6TrHq2`), "{u} iniciou uma chamada." (`DbgSA0`), "{u} adicionou {v} ao grupo." (`7/Xl0S`), "{u} removeu {v}..." (`QtZ0RD`), "{u} saiu do grupo." (`Qn5+Lf`), "{u} mudou o nome do canal: {n}" (`XCPMEG`), "{u} mudou o nome do grupo: {n}" (`rk0be9`), "{u} mudou o ícone do grupo." (`wypJZ0`), "{u} começou um tópico, "{t}"" (`B8H+Cl`). Ficam fora da v1, exceto Join e Pin.
- Mensagens de sistema **não** têm barra de ações completa (só "Apagar" para moderador) e não são editáveis; reação só em algumas [I].

### 2.18 Estado vazio e início do canal

- Início do canal (quando `!has_more_before`) [F strings]: título "Bem-vindo(a) a #{canal}!" (`Szye2K`) e subtítulo "Este é o começo do canal #{canal}." (`jRNiwM`). DM: "Foi aqui que a sua inesquecível conversa com {usuário} começou." (`Q56TRC`) ou "Este é o começo do seu histórico de mensagens diretas com {usuário}." (`Qvg+6+`); grupo: "Bem-vindo (ou vinda) ao começo do grupo {nome}." (`MFwcqO`). Tópico: "Ainda não há mensagens neste tópico." (`HYtNyE`).
- T: ícone grande "#" em círculo L3 de 56, título em serifa 28 TEXT, subtítulo 14 MUTED, margem inferior 24. Substitui o texto atual "Ainda não há mensagens. Diga olá!" (manter essa frase como subtítulo do canal vazio, é a voz do app).
- Pins vazios [F]: "Este canal não tem mensagens fixadas... por enquanto." (`fmyaWJ`).

---

## 3. Estados e interações

| Estado | Comportamento | Tempo / easing |
|---|---|---|
| Hover da linha | fundo `alpha(TEXT,0.018)`; barra de ações; carimbo no gutter (continuação) | 0 a 80ms; barra 120ms ease-out, `translateY 3->0` |
| Foco por teclado | contorno 1px AMBER interno; barra visível; atalhos E/R/P/+/F/Backspace ativos | imediato |
| Menu de contexto aberto | linha mantém fundo de hover; barra fica visível | - |
| Selecionada (reply alvo / editando) | fundo `alpha(AMBER,0.06)`, barra lateral AMBER 2px | 100ms |
| Mencionada / resposta a mim | barra lateral AMBER 2px + fundo `alpha(AMBER,0.06)` | - |
| Flash de salto | fundo `alpha(AMBER,0.14)` -> 0 | 1,6s ease-out [I] |
| Entrada de mensagem nova (no fundo) | fade-in de opacidade (já existe, 220ms `ease_out`) e **não** animar histórico carregado ([F]: o store tem `suppressRowAnimationSequenceId` para suprimir animação de linhas em carga inicial/foreground) | 220ms |
| Enviando | texto a 55% | - |
| Falhou | texto + linha RED "Não enviada" | - |
| Carregando histórico (topo) | esqueleto (2.10) | sem animação |
| Falha ao carregar | linha "Erro ao carregar mensagens" clicável (reticências ao tentar) [F `tL`] | - |
| Vazio | início do canal (2.18) | - |
| Silenciado / sem permissão de ver | cabeçalho com ícone de sino cortado; lista continua (sem mudar a lista) | - |
| Janela antiga (`hasMoreAfter`) | faixa "mensagens antigas" no rodapé | 140ms |
| Não lidas | barra superior + divisor "NOVO" | 140ms |
| Reduzir movimento | zerar todos os tempos acima (`reduced_motion` -> `animate_*_with_time(.., 0.0)`) [F: existe `prefersReducedMotion`, WEB @2656500] | - |

Tooltips: 500ms de atraso (padrão do bundle, ver `docs/discord/10`), no carimbo (data completa), "(editado)" (data da edição), reações (quem reagiu), ícones do cabeçalho.

---

## 4. Regras de comportamento

### 4.1 Ordenação, identidade e janela de dados [F]
- Mensagens ordenadas pelo **id (snowflake)** comparado numericamente (`c.default.compare`); pendentes sempre no fim [F: WEB @2362500-2371900].
- O store por canal (`ChannelMessages`) tem: `_array` (janela atual), `_before` e `_after` (caches de até **150** mensagens cada, `iMx = 150`), `hasMoreBefore`, `hasMoreAfter`, `loadingMore`, `ready`, `cached`, `hasFetched`, `error`, `jumpTargetId`, `jumpType (ANIMATED|INSTANT)`, `jumpFlash`, `jumpReturnTargetId`, `focusTargetId`, `revealedMessageId`.
- `hasPresent()` verdadeiro quando não há mensagens depois da janela ou o cache "depois" chegou à borda. A UI mostra a faixa de "janela antiga" quando `hasMoreAfter`.
- Mensagem recebida em tempo real: `insertInOrder`; **se o canal já tinha histórico e `hasMoreBefore`, e a mensagem recebida é mais antiga que a mais antiga da janela, é ignorada** (não entra solta no meio) [F: `insertInOrder`]. Se a janela não está no presente (`hasMoreAfter`), a nova mensagem não entra na janela; só atualiza contadores/não lidas [I].
- Truncamento: ao carregar para cima, a janela pode descartar linhas do fundo (e vice-versa) para não crescer sem limite (`truncateTop/truncateBottom`; as linhas descartadas vão para o cache `_before/_after`) [F]. Isso torna o jump ao presente barato.
- Delta de reconexão: `mergeDelta(novas, modificadas, apagadas)` reconcilia sem recarregar o canal [F: `BACKGROUND_SYNC_CHANNEL_MESSAGES`].
- `LOAD_MESSAGES_SUCCESS_CACHED` reaproveita cache antes de ir à rede [F].

### 4.2 Paginação / histórico ao rolar para cima
- Requisição [F: WEB @667358]: `GET /channels/{id}/messages?before=&after=&around=&limit=`. Resposta com `limit` itens => `hasMoreBefore=true` (se `before` ou primeira carga), `hasMoreAfter` análogo. Com `around=<id>` (salto) divide `limit/2` para cada lado.
- Tamanhos: salto ao presente **30** (`L.EMb=30`); prefetch **30** (`L.d7Q`); carga inicial/stale/thread via configuração remota (`MessageManager.initialFetch`, `.staleFetch`, `.threadStart`, `.threadUnread`) [F existência; valores [?]]; API padrão 50 [DOC]. **T: 50 por página** (já é o que o servidor devolve).
- Gatilho de carregar mais para cima: não está no cache [?]. T: quando `offset_y < 400` (menos de ~7 linhas de folga) e `has_more_before && !loading`, enviar `History{before: primeiro_id}`.
- Enquanto carrega: esqueleto no topo; se falhar: linha "Erro ao carregar mensagens" (clicável).
- Ao chegar o lote: prepend e **preservar a posição visual** (a mesma mensagem fica no mesmo pixel) - ver 6.2.

### 4.3 Ancoragem no fundo e rolagem
- Ao abrir um canal sem posição salva: rolar para o fim sem animação (`INSTANT`); com salto a mensagem: `ANIMATED` (rolagem suave) [F: enum `vx`]. O scroller expõe `scrollToTop/Bottom({animate, callback})`, `isScrolledToBottom`, `getDistanceFromTop/Bottom`, `scrollIntoViewNode({node, padding})` [F: WEB @11048000]; `scrollToBottom` usa destino `MAX_SAFE_INTEGER` (clamp).
- "Está no fundo" = distância ao fundo <= 2px [I; o scroller do bundle usa igualdade `scrollTop >= scrollHeight - offsetHeight`]. T: tolerância 8px para evitar flapping por arredondamento.
- Ao chegar mensagem nova: se estava no fundo, **fica no fundo** (fade-in); se não, não mexe na posição e incrementa o contador de pílula "Novas mensagens".
- Ao enviar uma mensagem própria: sempre vai ao fundo (e ao presente, se estava em janela antiga) [I].
- Redimensionamento: a janela ao fundo continua ao fundo; fora do fundo mantém a âncora (6.2).
- Imagens que carregam depois e mudam a altura (se o item está acima da viewport): compensar o offset (a âncora faz isso sozinha); reservar o espaço com `width/height` conhecidos do anexo para evitar salto [I; o ghost de anexo existe por isso, [F] 2.10].
- Page Up/Down: `SCROLL_UP/SCROLL_DOWN` [F: binds de chat]; Ctrl+Home/End [I].
- Salto para mensagem: `jump(id)`: se na janela, rolar até ela com `padding` e piscar; senão, `History{around: id}` e trocar a janela (e mostrar a faixa "mensagens antigas").

### 4.4 Virtualização
- O Discord só monta as linhas visíveis (+ margem) e mede alturas [I; o JSX do scroller não está no cache]. T: obrigatório no egui porque o modo imediato recalcula layout de **todas** as linhas por frame; com 2 mil mensagens o chat atual (`for m in msgs`) estoura o orçamento de frame.
- Ver plano em 6.2 (cache de alturas + soma de prefixos + overscan).

### 4.5 Não lidas
- Servidor guarda `last_read_message_id` e contagem de menções por (usuário, canal) [F: `hasUnread`, `getMentionCount`, `getUnreadCount`, `MARK_CHANNEL_AS_READ_KEYBIND`, `ack`].
- Ao abrir o canal: posição de leitura = divisor "NOVO" visível; se há muitas não lidas, a barra superior com contagem (cap com "+"). Ao rolar até o fim com a janela focada por >= 1s, ack automático [I]. `Esc` (6.2) marca como lido. "Marcar como não lida" (Alt+clique / menu) define `last_read` = id anterior à mensagem [F: `RpE9k7`; `aes5VI`].
- Aba/janela sem foco: não dar ack; mostrar o divisor ao voltar [I].

### 4.6 Seleção e cópia
- Texto selecionável com o mouse dentro da mensagem; Ctrl+C copia a seleção ou, sem seleção e com mensagem em foco, o texto da mensagem [F: bind `mod+c`]. No egui só é possível selecionar dentro de **um** `Label` (`.selectable(true)`); seleção contínua através de várias mensagens não é suportada (limitação a aceitar).

### 4.7 Configurações que afetam a lista [F: WEB @2646800-2656500, @737600]
Modo de exibição ("Acolhedor"/"Compacto", `messageDisplayCompact`), tamanho da fonte do chat (mapa legado 82/92/100/110/125/150% -> 12/15/16/18/20/24 px; padrão 16), espaço entre grupos, mostrar embeds, mostrar anexos inline, mostrar reações, animar emoji/figurinhas, GIF automático, "sempre sublinhar links", estilo de cor de cargo, dessaturar cores de usuário, reduzir movimento. T v1: só tamanho de fonte (via `TextStyle` escalado) e reduzir movimento.

---

## 5. Dados necessários (protocolo e banco)

Estado hoje: `proto::Message{id, channel_id, author_id, author, content, ts}`; `C2S::Send{channel_id,content}`, `C2S::History{channel_id,before}`; `S2C::Message`, `S2C::History{channel_id,messages}`; tabela `messages(id, channel_id, author_id, content, edited_at, created_at)` com índice `(channel_id, id DESC)` (já existe `edited_at`, nunca usado).

**Prioridade 0 (lista básica correta)**
1. `S2C::History` ganha `has_more_before: bool` (e o servidor devolve em ordem crescente, `LIMIT page+0`): hoje o cliente não sabe quando parou. Tamanho de página 50.
2. `C2S::History { channel_id, before: Option<i64>, after: Option<i64>, around: Option<i64>, limit: u16 }` (ao menos `around` para saltos e `limit` para a pagina de 30). Servidor: `WHERE id < $before ORDER BY id DESC LIMIT $n`, e `around` = união de `id <= x` e `id > x`.
3. `Message` ganha `edited_ts: Option<i64>` (para "(editado)").
4. `S2C::MessageUpdate(Message)` e `S2C::MessageDelete{channel_id, id}`; `C2S::Edit{message_id, content}` e `C2S::Delete{message_id}` com checagem de autoria/permissão. Rejeitar edição de mensagem de outro; limite 4000.
5. `User`/membro ganha cor de nome: `role_color: Option<u32>` (RGB) por (servidor, usuário). Hoje não há cargos (outra área); até lá, cor padrão.

**Prioridade 1 (conversa)**
6. `Message.reply_to: Option<ReplyRef{id, author_id, author, snippet: String(<=140), kind, deleted: bool}>`; `C2S::Send` ganha `reply_to: Option<i64>`. Coluna `messages.reply_to_id BIGINT NULL REFERENCES messages(id) ON DELETE SET NULL` (SET NULL + marcador `deleted` no cliente quando o id some).
7. Leitura: tabela `channel_reads(user_id, channel_id, last_read_id, mention_count, PRIMARY KEY(user_id, channel_id))`; `C2S::Ack{channel_id, message_id}` / `C2S::MarkUnread{channel_id, message_id}`; `S2C::ReadState{channel_id, last_read_id, mention_count}` no snapshot e em tempo real (multi-dispositivo). Para a barra "N novas desde HH:MM" o cliente precisa do timestamp da mensagem `last_read_id + 1`: derivável do snowflake (o `id` já embute o tempo) ou enviar `first_unread_ts`.
8. `Message.kind: u8` (0 normal, 1 resposta, 2 entrada, 3 fixou, ...). Servidor insere `kind=2` quando `JoinServer` (sem conteúdo; `author` = quem entrou).
9. Fixar: `messages.pinned_at TIMESTAMPTZ NULL` + `C2S::Pin{message_id,pin}` + `S2C::Pins{channel_id, messages}` e mensagem de sistema `kind=3`.

**Prioridade 2 (rico)**
10. Reações: tabela `message_reactions(message_id, emoji TEXT, user_id, created_at, PRIMARY KEY(message_id, emoji, user_id))`; `Message.reactions: Vec<{emoji, count, me}>` (agregado na leitura); `C2S::React{message_id, emoji, add}`; `S2C::ReactionDelta{channel_id, message_id, emoji, user_id, add}` (o cliente atualiza `count` e `me`). Para o tooltip "quem reagiu": `C2S::ReactionUsers{message_id, emoji}` sob demanda.
11. Anexos: tabela `attachments(id, message_id, name, mime, size, width, height, spoiler, storage_key)`; upload HTTP separado do WebSocket (`POST /upload` multipart com token, devolve id) e `C2S::Send` referencia `attachment_ids`; `GET /files/{id}/{name}` com cache. `Message.attachments: Vec<{id,name,mime,size,width,height,spoiler,url}>`. Dimensões obrigatórias (gera o `aspect-ratio` e evita pulo de layout). Limite de tamanho por arquivo [pergunta 5].
12. Embeds/unfurl: tabela `embeds` ou JSON em `messages.embeds JSONB`; worker no servidor que busca OpenGraph com proteção SSRF (bloquear IPs privados, limite de tamanho/tempo). P3.

**Índices**: `messages(channel_id, id DESC)` já cobre histórico; `message_reactions(message_id)`; `attachments(message_id)`; `channel_reads(user_id)`; parcial `messages(channel_id, id DESC) WHERE pinned_at IS NOT NULL` para pins.

---

## 6. Plano de implementação em egui

### 6.1 Onde entra
- `theme.rs`: constantes (`MSG_SIDE=28.0, AVATAR=38.0, TEXT_X=80.0, ROW_BODY=22.0, HEADER_H=64.0, GROUP_GAP=14.0`), helpers de desenho `icon_btn`, `hover_bar`, `reaction_chip`, `jump_bar`, `new_bar`, `skeleton_group`, `reply_connector`, `fit_media`, `fmt_stamp` (hoje/ontem/data) e `fmt_day_long`.
- Novo módulo `app/src/chat.rs` (tirar `chat_view` de `main.rs`, que já tem 1224 linhas): `ChatState` por canal (offset, cache de alturas, âncora, edição, hover), `build_rows`, `show_messages`, `show_header`.
- `main.rs`: `Session` guarda `chat: HashMap<i64, ChatState>` e `msgs` passa a ser `ChannelWindow { items: Vec<Message>, has_more_before, has_more_after, loading_before, read: ReadState }`.

### 6.2 Trecho 1: modelo de linhas + agrupamento

```rust
#[derive(Clone, Copy, PartialEq)]
enum RowKey { Skeleton, Start, Day(i64), New, Msg(i64), Sys(i64), Failed }

struct Row {
    key: RowKey,
    msg: Option<usize>, // índice em items
    group_start: bool,
}

fn build_rows(w: &ChannelWindow, last_read: Option<i64>) -> Vec<Row> {
    let mut rows = Vec::with_capacity(w.items.len() + 16);
    if w.has_more_before { rows.push(Row { key: RowKey::Skeleton, msg: None, group_start: false }); }
    else { rows.push(Row { key: RowKey::Start, msg: None, group_start: false }); }
    let mut prev: Option<&p::Message> = None;
    let mut new_done = false;
    for (i, m) in w.items.iter().enumerate() {
        let day_break = prev.map_or(true, |p| !same_day(p.ts, m.ts));
        if day_break { rows.push(Row { key: RowKey::Day(day_key(m.ts)), msg: None, group_start: false }); }
        // divisor "NOVO": antes da primeira mensagem com id > last_read, que não seja minha
        let first_unread = !new_done && last_read.map_or(false, |lr| m.id > lr) && m.author_id != w.me;
        if first_unread { rows.push(Row { key: RowKey::New, msg: None, group_start: false }); new_done = true; }
        let system = m.kind != 0 && m.kind != 1;
        let continues = !day_break && !first_unread && !system && m.reply_to.is_none()
            && prev.map_or(false, |p| p.kind == 0 || p.kind == 1)
            && prev.map_or(false, |p| p.author_id == m.author_id && m.ts - p.ts < GROUP_MS); // 7*60*1000
        rows.push(Row { key: if system { RowKey::Sys(m.id) } else { RowKey::Msg(m.id) }, msg: Some(i), group_start: !continues });
        prev = Some(m);
    }
    rows
}
```
Armadilha: `RowKey::Day` não pode usar o id da mensagem (o dia some ao prepend); usar `yyyymmdd` local. Reconstruir `rows` só quando `items` ou `last_read` mudam (guardar `version: u64` e comparar), não a cada frame.

### 6.3 Trecho 2: lista virtualizada com cache de alturas e âncora (o mais difícil)

```rust
struct ChatState {
    heights: HashMap<RowKey, f32>, // altura medida
    width_bucket: u32,             // largura útil / 8; mudou => heights.clear()
    anchor: Option<(RowKey, f32)>, // linha visível mais acima e seu deslocamento (px) dentro da viewport
    stuck_bottom: bool,            // true = acompanhar o fundo
    offset: f32,                   // último offset aplicado por nós
    unseen_new: u32,
    loading_before: bool,
}

fn est_height(r: &Row, w: &ChannelWindow, text_w: f32) -> f32 {
    match r.key {
        RowKey::Day(_) => 46.0, RowKey::New => 24.0, RowKey::Start => 180.0, RowKey::Skeleton => 3.0 * 80.0,
        RowKey::Failed => 40.0, RowKey::Sys(_) => 34.0,
        RowKey::Msg(_) => {
            let m = &w.items[r.msg.unwrap()];
            let lines = ((m.content.chars().count() as f32 * 7.4) / text_w).ceil().max(1.0);
            (if r.group_start { 36.0 } else { 2.0 }) + 22.0 * lines
        }
    }
}

fn show_messages(&mut self, ui: &mut egui::Ui, st: &mut ChatState, w: &ChannelWindow, rows: &[Row]) {
    let avail = ui.available_size();
    let text_w = (avail.x - TEXT_X - 28.0).max(120.0);
    let bucket = (text_w / 8.0) as u32;
    if st.width_bucket != bucket { st.heights.clear(); st.width_bucket = bucket; }

    // soma de prefixos com alturas medidas ou estimadas
    let mut tops = Vec::with_capacity(rows.len() + 1);
    let mut y = 0.0;
    for r in rows { tops.push(y); y += st.heights.get(&r.key).copied().unwrap_or_else(|| est_height(r, w, text_w)); }
    tops.push(y);
    let total = y + 12.0;

    // offset desejado
    let max_off = (total - avail.y).max(0.0);
    let mut want = st.offset;
    if st.stuck_bottom { want = max_off; }
    else if let Some((key, dy)) = st.anchor {            // mantém a mesma linha no mesmo pixel
        if let Some(i) = rows.iter().position(|r| r.key == key) { want = (tops[i] - dy).clamp(0.0, max_off); }
    }

    let out = egui::ScrollArea::vertical()
        .id_salt(("msgs", w.channel_id))
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .vertical_scroll_offset(want)
        .show_viewport(ui, |ui, vp| {
            ui.set_height(total);                          // altura virtual
            let first = tops.partition_point(|&t| t <= vp.min.y - 600.0).saturating_sub(1);
            let mut i = first;
            while i < rows.len() && tops[i] < vp.max.y + 600.0 {
                let top = ui.min_rect().top() + tops[i];
                let rect = egui::Rect::from_min_size(egui::pos2(ui.min_rect().left(), top),
                                                     egui::vec2(avail.x, (tops[i + 1] - tops[i]).max(1.0)));
                let used = ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    self.draw_row(ui, &rows[i], w, text_w)
                }).response.rect.height();
                if (used - (tops[i + 1] - tops[i])).abs() > 0.5 {
                    st.heights.insert(rows[i].key, used);
                    ui.ctx().request_repaint();           // corrige no próximo frame
                }
                i += 1;
            }
        });

    // atualiza estado a partir do que o usuário fez
    let off = out.state.offset.y;
    st.offset = off;
    st.stuck_bottom = off >= max_off - 8.0;
    if !st.stuck_bottom {
        let i = tops.partition_point(|&t| t <= off).saturating_sub(1);
        st.anchor = Some((rows[i].key, tops[i] - off));    // dy <= 0
    }
    if off < 400.0 && w.has_more_before && !st.loading_before { st.loading_before = true; /* push C2S::History */ }
}
```
Notas:
- `show_viewport` entrega o retângulo visível em coordenadas do conteúdo; `ui.set_height(total)` dá a barra de rolagem correta. O offset é aplicado com `vertical_scroll_offset(want)` **no frame seguinte ao cálculo**; por isso `want` é sempre calculado antes de chamar o `ScrollArea`.
- `stick_to_bottom(true)` do egui só funciona quando o conteúdo cresce no fim e **quebra com prepend** (puxa o usuário para baixo ou deixa tudo pulando); por isso controlamos o offset à mão e **não** usamos `stick_to_bottom`.
- Quando o lote do histórico chega, `rows` ganha linhas no começo; como `anchor` é uma `RowKey` estável (id da mensagem), a próxima passada recalcula `want = tops[i] - dy` e a mensagem fica no mesmo pixel. Ao medir uma linha acima da viewport e a altura mudar, a âncora corrige no frame seguinte (pode haver 1 frame de salto; mitigar com `request_repaint` imediato e ocultando via `set_opacity(0)` no primeiro frame após o prepend).
- Largura mudou => `heights.clear()` (o texto reflui). Para redimensionar a janela sem custo, `bucket` agrupa em passos de 8px.
- Armadilha: linhas muito altas (embeds/anexos) têm altura conhecida só pelo `width/height` do anexo: calcular `est_height` com `fit_media()` para evitar saltos.

### 6.4 Trecho 3: barra de ações com "tolerância" de hover e menu de contexto

```rust
fn draw_message(&mut self, ui: &mut egui::Ui, row: &Row, m: &p::Message, st: &mut ChatState) {
    let id = egui::Id::new(("msg", m.id));
    let full = ui.max_rect();                       // retângulo da linha (largura toda)
    let hit = ui.interact(full, id, egui::Sense::click()); // registrar ANTES dos filhos (os filhos ficam por cima)
    let near = ui.rect_contains_pointer(full.expand2(egui::vec2(0.0, 0.0)).with_min_y(full.min.y - 16.0));
    let bar_open = st.menu_for == Some(m.id) || st.editing == Some(m.id);
    let hovered = hit.hovered() || near || bar_open;
    if hovered { ui.painter().rect_filled(full, 0.0, alpha(TEXT, 0.018)); }
    // ... avatar, cabeçalho, texto, reações (ver 6.5) ...
    // barra: Area em Foreground, fora do clip da ScrollArea
    let t = ui.ctx().animate_bool_with_time(id.with("bar"), hovered && !st.editing_any(), 0.12);
    if t > 0.0 {
        let pos = egui::pos2(full.right() - 28.0 - BAR_W, full.top() - 14.0 + (1.0 - t) * 3.0);
        egui::Area::new(id.with("barArea")).order(egui::Order::Foreground).fixed_pos(pos).interactable(true).show(ui.ctx(), |ui| {
            ui.set_opacity(t);
            egui::Frame::new().fill(L3).stroke(egui::Stroke::new(1.0, LINE2)).corner_radius(8).show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if bar_btn(ui, Glyph::Smile, "Adicionar reação").clicked() { /* abrir seletor */ }
                if m.author_id == st.me { if bar_btn(ui, Glyph::Pencil, "Editar mensagem").clicked() { st.editing = Some(m.id); } }
                else if bar_btn(ui, Glyph::Reply, "Responder").clicked() { st.replying = Some(m.id); }
                if bar_btn(ui, Glyph::Dots, "Mais").clicked() { st.menu_for = Some(m.id); /* abrir popup */ }
            });
        });
    }
    hit.context_menu(|ui| { message_menu(ui, m, st); });   // botão direito
}
```
Armadilhas: (1) a barra sobressai 14px acima da linha, então a linha acima "perde" o hover antes da barra aparecer: o `near` (retângulo expandido para cima em 16px) e o `animate_bool` com fade evitam piscar. (2) Quando duas linhas disputam (`near` verdadeiro em ambas) vale a que contém o ponteiro: salvar `st.hover_row = Some(id)` e só mostrar a barra do `hover_row`. (3) `Area` é desenhada fora do `ScrollArea`, então não é recortada; mas some quando a linha sai da viewport (não é desenhada). (4) `Response::context_menu` precisa de `Sense::click()` na resposta; registrar `ui.interact` antes dos filhos para que filhos (avatar, links) continuem recebendo cliques.

### 6.5 Trecho 4: chip de reação e encaixe de mídia

```rust
fn reaction_chip(ui: &mut egui::Ui, emoji: &str, count: u32, mine: bool) -> egui::Response {
    let font = egui::FontId::proportional(12.5);
    let g = ui.painter().layout_no_wrap(count.to_string(), font.clone(), if mine { AMBER } else { TEXT2 });
    let size = egui::vec2(8.0 + 16.0 + 6.0 + g.size().x + 8.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.10);
    let fill = if mine { alpha(AMBER, 0.12) } else { lerp(L3, L4, t) };
    let stroke = if mine { AMBER } else { lerp(LINE2, MUTED, t) };
    ui.painter().rect(rect, 8.0, fill, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    // emoji Unicode: texto com a fonte de emoji monocromática; custom: textura 16x16
    ui.painter().text(rect.left_center() + egui::vec2(8.0 + 8.0, 0.0), egui::Align2::CENTER_CENTER, emoji,
                      egui::FontId::proportional(15.0), TEXT);
    ui.painter().galley(egui::pos2(rect.left() + 8.0 + 16.0 + 6.0, rect.center().y - g.size().y / 2.0), g, TEXT2);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// encaixe sem ampliar: primeiro pela largura, depois pela altura (mesma regra do cliente)
fn fit_media(w: f32, h: f32, max_w: f32, max_h: f32) -> egui::Vec2 {
    let k1 = if w > max_w { max_w / w } else { 1.0 };
    let (w1, h1) = ((w * k1).round(), (h * k1).round());
    let k2 = if h1 > max_h { max_h / h1 } else { 1.0 };
    egui::vec2((w1 * k2).round().max(1.0), (h1 * k2).round().max(1.0))
}
```
Armadilhas gerais do egui:
- **Sem emoji colorido**: `FontId` com fonte de emoji do egui é monocromática; para emoji coloridos é preciso um atlas de PNGs (ex.: Twemoji, licença CC-BY 4.0, exige atribuição) carregado como `TextureHandle` e desenhado com `painter.image`. Reações e emoji no texto dependem disso (pergunta 2).
- **Sem negrito de fonte variável**: o nome do autor "semibold" hoje usa `.strong()`, que só escurece/engrossa se houver outra família; carregar `Inter-SemiBold.ttf` como família `Name("semibold")` ou aceitar `.strong()`.
- **Sem letter-spacing** (rótulo "NOVO" caixa alta 10px: ajustar o tamanho, não o espaçamento) e **sem `font-variant-numeric`**.
- **Texto com cor de cargo**: usar `LayoutJob` quando nome e carimbo estão na mesma linha com cores diferentes; ou dois `painter.galley` consecutivos.
- **Seleção de texto** só por `Label::selectable`; `Label` com `wrap()` precisa de largura fixa: usar `ui.set_max_width(text_w)` antes de adicionar o label, senão ele ocupa a linha inteira e a medição de altura fica errada.
- **Cliques em links** no texto: `RichText` não tem links inline; usar `LayoutJob` + `ui.interact` por seção ou `egui::Hyperlink` em fluxo `horizontal_wrapped` (perde alinhamento). Fica para a área de markdown.
- **Scroll suave** (animar a rolagem até uma mensagem): `ScrollArea::vertical_scroll_offset` aplica salto; para suavizar, interpolar `want` com `ctx.animate_value_with_time(id, alvo, 0.25)` e `request_repaint` até convergir.
- **Frame time**: com virtualização, cada linha desenha ~10 formas; manter `layout_no_wrap` fora do loop quente (cache de galleys do nome/carimbo por mensagem em `HashMap<i64, Arc<Galley>>`, invalidado por `width_bucket`).

### 6.6 Skeleton, barras e faixa

```rust
fn skeleton_group(ui: &mut egui::Ui, seed: u32) {
    // proporções do cliente: nome 80-120px, corpo 120-320px; sem shimmer
    let name_w = 80.0 + (seed * 37 % 40) as f32;
    let body_w = 120.0 + (seed * 91 % 200) as f32;
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 58.0), egui::Sense::hover());
    let p = ui.painter();
    p.circle_filled(egui::pos2(r.left() + MSG_SIDE + AVATAR / 2.0, r.top() + 16.0 + AVATAR / 2.0), AVATAR / 2.0, alpha(TEXT, 0.06));
    p.rect_filled(egui::Rect::from_min_size(egui::pos2(r.left() + TEXT_X, r.top() + 18.0), egui::vec2(name_w, 12.0)), 6.0, alpha(TEXT, 0.10));
    p.rect_filled(egui::Rect::from_min_size(egui::pos2(r.left() + TEXT_X, r.top() + 38.0), egui::vec2(body_w, 12.0)), 6.0, alpha(TEXT, 0.06));
}
```
Barra "novas mensagens" e faixa "ir para o presente": `Area` ancorada com `Align2::LEFT_TOP`/`LEFT_BOTTOM` sobre o retângulo da `ScrollArea` (guardar o `inner_rect` do `ScrollAreaOutput`), animando posição com `animate_bool_with_time(id, visível, 0.14)`.

---

## 7. Ordem de entrega (cada passo verificável)

1. **Constantes e cabeçalho** (`theme.rs`, `chat_view`): altura 64, ícones desenhados (Tópicos/Notificações/Fixadas desabilitados com tooltip "Em breve", Membros alterna a lista, busca só visual). *Ao final o usuário vê a barra de ícones, o tooltip trocando "Mostrar/Ocultar lista de membros" e o campo de busca expandindo.*
2. **Modelo de linhas e medidas Grafite** (`build_rows` + desenho de grupo/continuação com avatar 38 e texto em x=80, hora no gutter ao hover, divisor de dia, formato "Hoje às"). Sem virtualização ainda. *Ver o chat idêntico à maquete, com carimbo no hover das continuações.*
3. **Virtualização + cache de alturas + âncora no fundo** (6.3). Gerar 5000 mensagens de teste no mock. *Rolar suave com 5000 linhas, fixo no fundo ao receber, sem pulo ao redimensionar.*
4. **Histórico ao rolar para cima**: `has_more_before` + `History{before,limit}` + esqueleto + preservar posição. *Rolar até o topo carrega o próximo lote sem a tela pular; ao esgotar aparece "Este é o começo do canal #x".*
5. **Pílula "Novas mensagens" e fundo** (sem servidor): contador de mensagens recebidas fora do fundo, clique rola ao fim, Esc rola ao fim. *Estar rolado para cima e receber mensagens não puxa a tela.*
6. **Barra de ações + menu de contexto** com ações locais: copiar texto, copiar link (`telacast://canal/mensagem`), copiar ID. *Passar o mouse mostra a barra com fade de 120ms; botão direito abre o menu.*
7. **Editar/excluir** (servidor: `Edit`, `Delete`, `MessageUpdate`, `MessageDelete`) + edição inline + "(editado)". *Editar com E ou pelo menu, Esc cancela, Enter salva; outros clientes veem a mudança.*
8. **Respostas** (servidor: `reply_to`, `kind`) + cabeçalho com conector e salto para o original + faixa "Você está visualizando uma resposta". *Responder aparece com mini-prévia; clicar salta e pisca a linha.*
9. **Não lidas** (servidor: `channel_reads`, `Ack`): divisor "NOVO" âmbar, barra superior com "Marcar como lida", Esc marca como lido. *Voltar a um canal mostra "N mensagens novas desde 14:05".*
10. **Janela antiga** (servidor: `around`): salto a mensagem fora da janela + faixa "Avançar para mensagens recentes". *Pular para uma resposta antiga mostra a faixa e voltar ao presente funciona.*
11. **Mensagens de sistema** (Join e Fixou). *Entrar num servidor gera a linha "X acabou de aparecer!" estável entre clientes.*
12. **Reações** (servidor + emoji): chip, selecionado, "+" no hover, tooltip de quem reagiu. 
13. **Anexos**: upload, imagem (400/550 máx.), arquivo, mosaico, spoiler. 
14. **Embeds/unfurl** (P3).

---

## 8. Perguntas em aberto

1. **Agrupamento**: manter 5 min (hoje) ou ir para 7 min (valor do Discord, [I])? Afeta só `GROUP_MS`.
2. **Emoji**: aceita incluir um atlas de Twemoji (CC-BY 4.0, ~3 MB, atribuição na tela "Sobre")? Sem isso, reações e emoji no texto saem monocromáticos (fonte do egui).
3. **Marcador de não lida**: âmbar (coerente com o Grafite, minha proposta) ou vermelho como no Discord?
4. **Densidade**: só "acolhedor" (avatar+cabeçalho) ou também "compacto" (sem avatar, hora à esquerda)? O bundle tem os dois modos; compacto custa um ramo a mais em `draw_message`.
5. **Anexos**: onde guardar (disco da instância Oracle A1 ou armazenamento de objetos), limite por arquivo (Discord ~10 MB gratuito [DOC]) e política de retenção/limpeza. Muda o desenho do servidor, não o cliente.
6. **Largura do texto**: limitar a coluna a ~880px (leitura confortável, estilo maquete) ou usar a largura toda como o Discord?
7. **Quem pode fixar/apagar**: sem cargos ainda, a regra é "autor apaga a própria; dono do servidor apaga qualquer; fixar = dono"? (a área de cargos pode redefinir).
8. **Busca/threads/fixadas** no cabeçalho: ficam desabilitadas com "Em breve" até as outras áreas entregarem?
