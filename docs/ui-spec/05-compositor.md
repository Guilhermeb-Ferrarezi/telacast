# 05 - Compositor (campo de escrever mensagem)

Área: caixa de texto do canal e tudo que a cerca (botão "+", botões à direita, faixa de resposta, bandeja de anexos, autocompletar, "fulano está digitando…", contador, modo lento, estados desativados, rascunhos, colar/arrastar arquivos).
Fonte: cliente web do Discord extraído do cache (`discord-bundle/`). Legenda: **[F]** confirmado no código; **[I]** inferência/decisão de projeto; **[?]** não confirmado.

Convenção de citação: `web` = `web.0bcf9a9e83b2c3c7.js` (o maior; confirmado em `web.48946ead1e279bd2.js` e `web.72656029554c0370.js` para SlowmodeStore e validador); `cta` = `0dc9e8a50d7a8c6d.js` (chunk com o formulário `ChannelTextAreaForm`); `pt` = `12064a7d28fc7eb1.js` (textos pt-BR); `en` = `2552f3a00fc8192a.js`. Números depois de `@` são deslocamentos em caracteres dentro do arquivo.

**Aviso de honestidade sobre medidas.** O cache não contém o CSS dos componentes (os 4 `.css` só têm variáveis de tema; as classes de CSS-modules do compositor não estão em lugar nenhum). Portanto **toda medida em px de caixa, padding, raio e altura de linha abaixo é [I]** (derivada da maquete Grafite `docs/ui-preview/grafite.html`, do comportamento do código e do que se sabe do produto), salvo as constantes numéricas que existem no JS (tamanho de ícone, limites, tempos, altura máxima do popup, etc.), que estão marcadas [F]. Três módulos lazy também não estão no cache: o do indicador "digitando" (módulo 620014), o das regras de gatilho do autocompletar (módulo 778572) e o dos itens do menu "+" (chunks cortados em 64 KB). Ali, só strings e fluxo são [F].

---

## 1. Resumo e fora de escopo

O compositor é uma caixa de várias linhas que cresce com o conteúdo (até metade da altura da janela), com "+" à esquerda (menu de anexar/criar) e botões de expressão à direita (presente, GIF, figurinha, emoji). Enter envia, Shift+Enter (e Alt+Enter) quebra linha, Seta para cima com a caixa vazia edita a última mensagem sua, Esc fecha o autocompletar ou limpa os anexos pendentes. Acima da caixa empilham-se faixas (resposta). Dentro dela, uma bandeja de anexos pendentes. `@`, `#`, `:` e `/` abrem um popup de sugestões acima da caixa, navegável por teclado. Abaixo, "fulano está digitando…". Contador de caracteres e indicador de modo lento aparecem no canto inferior direito quando relevantes. Cada canal tem rascunho próprio, persistido entre execuções; a resposta pendente também é por canal.

**Fora de escopo deste arquivo** (outras áreas): renderização da mensagem enviada, lista de mensagens, edição inline da mensagem (só a entrada pela Seta para cima está aqui), o seletor de emoji/GIF/figurinha em si (só o botão que o abre e a âncora), menus de contexto de mensagem, gravação de mensagem de voz, enquetes, tópicos, apps/comandos de aplicativos (só os comandos embutidos de texto), cargos (o Telacast ainda não tem cargos), Nitro/upsells (o Telacast não tem; tudo que o Discord condiciona a Nitro vira "limite único do servidor").

---

## 2. Árvore de componentes

Visual Grafite: camadas L0-L4, acento âmbar, texto Inter 14,5 (`TextStyle::Body`), títulos em serifa só em overlays. Sem gradiente, sem brilho.

```
Painel inferior do canal  (Panel::bottom, altura automática, sem frame)
├─ Folga superior ........................ 8
├─ [opcional] Faixa de resposta ............ altura 34, largura = caixa        [reply bar]
├─ Caixa (Frame)  ......................... L3, traço 1 LINE2, raio 12
│  │   com faixa acima: raio superior 0 (a faixa assume os 12 de cima)
│  ├─ [opcional] Bandeja de anexos ......... dentro da caixa, no topo; filete LINE embaixo
│  ├─ Linha de entrada
│  │   ├─ Botão "+" ........................ 32x32 de área; disco de 22 px com cruz
│  │   ├─ Área de texto (rola) ............. TextEdit multilinha, sem moldura
│  │   └─ Botões à direita ................. [presente][GIF][figurinha][emoji] (+ apps, + enviar)
│  └─ Faixa de rodapé da caixa (só se há algo) ... altura +18: [modo lento] [contador]
├─ Faixa "digitando" ...................... altura 22, fora da caixa, abaixo dela
└─ Margem inferior ........................ (a faixa "digitando" ocupa os 22 px da maquete)
Camadas flutuantes (Area, Order::Foreground):
  - Popup de autocompletar (acima da caixa)
  - Menu do "+" (acima do botão)
  - Seletor de expressões (âncora: topo/direita da caixa; conteúdo é de outra área)
  - Sobreposição de arrastar-e-soltar (cobre a área do chat)
```

### 2.1 Medidas

| Item | Valor | Fonte |
|---|---|---|
| Margem horizontal do compositor | 28 px de cada lado | maquete `.compose{padding:0 28px 22px}` |
| Margem inferior | 22 px (a faixa "digitando" vive aqui) | maquete |
| Caixa: altura mínima | **50** | maquete `.box{height:50px}` [I]; o Discord usa ~44 |
| Caixa: altura máxima | **50 % da altura da janela** (depois disso a área de texto rola) | [F] o popup usa `min(0,5*window.innerHeight, alturaDoEditor)` ao calcular o espaço (`web`@3370400) |
| Caixa: raio / traço | 12 / 1 px LINE2; foco: traço AMBER (transição 160 ms ease-out) | maquete `.box` + `transition:border-color .16s` |
| Caixa: fundo | L3 (sem mudança no hover) | maquete |
| Fonte do texto | Inter 14,5 (Body), cor TEXT; dica (placeholder) MUTED | `theme.rs` |
| Altura de linha | **21** px (via `TextFormat::line_height`) | [I] (Discord: 16/22) |
| Altura da caixa com n linhas | `21*n + 29` (n=1 dá 50): padding vertical 15 em cima, 14 embaixo | [I] |
| Padding esquerdo (antes do "+") | 8 | [I] |
| Botão "+": área / ícone | 32x32 / disco de 22 px (L4; cruz de 2 px em TEXT2); hover: disco LINE2; menu aberto: disco AMBER, cruz AMBER_INK | [I]; ícone `+` em círculo é o desenho atual do Discord |
| Espaço "+" -> texto | 4 px (texto começa em x = 8+32+4 = 44 da borda da caixa) | [I] |
| Botões à direita: área / gap | 32x32 / 2 px; raio do hit 8; padding direito 6 | [I] |
| Ícones à direita | **20 px** (`refresh_sm`), traço fino, cor MUTED; hover TEXT; aberto AMBER | [F] tabela de tamanhos `{xxs:12,xs:16,sm:18,md:24,lg:32,refresh_sm:20}` (`web`@849418); botões GIF/figurinha/apps usam `size:"refresh_sm"` (`web`@3450000-3456400) |
| Ícone de enviar | `xs` = 16 px (avião de papel) | [F] `SendMessageIcon size:"xs"` (`web`@3455500) |
| Botão enviar | **oculto por padrão** (configuração "mostrar botão de enviar" começa desligada) | [F] `submitButtonEnabled:!1` (`web`@2600000-2700000) |
| Emoji do botão de emoji | sprite 18 px; no hover escala 1 -> **1,14** com mola (tension 800, friction 24 ≈ 120-150 ms) e passa de cinza para colorido; o emoji exibido é sorteado entre 77 a cada hover | [F] (`web`@4727726) |
| Largura que ativa modo estreito | container <= **450 px** troca GIF/figurinha/emoji por **um** botão "expressões" | [F] `width>450` (`web`@3490900); só vale se "condensar seletor quando estreito" (padrão ligado) |
| Faixa de resposta | altura 34; fundo L1; traço 1 LINE2 (sem borda inferior); raio superior 12; padding 16 | [I] |
| Barras flutuantes (comando ativo, aviso) | 40 px cada (entram no cálculo de espaço do popup) | [F] `barsHeight:40*floating.length` |
| Bandeja de anexos | padding 12; tiles 120x120 (imagem: cobrir, raio 8; arquivo: ícone + extensão), gap 8, legenda 12 px abaixo (nome truncado); rola na horizontal | [I] |
| Barra de ações do tile (hover) | 3 botões 28x28 num pílula L3 + traço LINE2, raio 8, canto superior direito, deslocada -10 px para cima | [I] (mesmo desenho do `.tools` da maquete) |
| Rodapé da caixa (contador/modo lento) | +18 px de padding inferior quando visível; texto 12 px, tabular | [F] classes alternam quando o contador está visível (`web`@3495700: `tU&&tx`); valor [I] |
| Faixa "digitando" | altura 22; texto 12,5 px TEXT2; x = esquerda da caixa + 16; 3 pontinhos (4 px) à esquerda | [I] (layout) / [F] strings |
| Popup de autocompletar: largura | = largura da caixa | [I] (no Discord o popup sem âncora de caret é absoluto na largura do wrapper; `web`@3376300 `F.pK`) |
| Popup: espaço até a caixa | 8 px acima do topo da caixa (ou acima da faixa de resposta, se houver) | [F] `spacing:8` |
| Popup: altura máxima | **490** (padrão e emoji); **245** (quando ancorado no caret); **200** (tipos `small`, ex. edição). Limite adicional: `window_h - 120 - min(0,5*window_h, max(editor_h, conteúdo_h))`. | [F] (`web`@3376200-3377000) |
| Popup: visual | L3, traço LINE2, raio 10, padding 8, sombra do tema | [I] |
| Linha de membro | altura 40; avatar 24 + status; nome (14,5 TEXT) + à direita usuário (12 MUTED) | [F] avatar `SIZE_24`, `body-md` + `body-xs` (`web`@3196000); alturas [I] |
| Linha de canal | altura 32; glifo `#` (serifa 17, MUTED) + nome + à direita nome da categoria (12 MUTED) | [F] estrutura (`web`@3196500); alturas [I] |
| Linha de emoji | altura 36; glifo/imagem 24 + `:nome:` + à direita nome do servidor (12 MUTED) | [F] estrutura; alturas [I] |
| Linha de comando | altura 52; ícone 32x32 + `/nome` (TEXT) + descrição embaixo (12 MUTED) + à direita nome do app (12 MUTED) | [F] ícone 32x32, descrição `body-xs` (`web`@3188700); alturas [I] |
| Linha-título (cabeçalho do grupo) | altura 28; 11,5 px caixa alta, MUTED | [F] `body-xs/semibold` (`web`@3194000); caixa alta é [I] (Grafite) |
| Linha de carregamento | barra esqueleto de largura aleatória 60-120 px | [F] `random(60,120)` (`web`@3192700) |
| Menu do "+" | L3, raio 10, item 36 px, ícone 20 à esquerda, largura mínima 220 | [I] |

---

## 3. Estados e interações

| Estado | Comportamento | Tempo/easing |
|---|---|---|
| Foco | traço da caixa vira AMBER; cursor de texto AMBER 2 px | 160 ms, ease-out (curva `cubic-bezier(.2,.7,.2,1)` da maquete) |
| Hover da caixa | nada muda; cursor `Text` na área do texto; clicar em qualquer ponto da caixa (fora de botões) foca o editor | [F] `onMouseDown` na caixa foca (`web`@3499000) |
| Hover dos botões | cor MUTED -> TEXT; sem fundo. Emoji: escala 1,14 + cinza -> cor | mola 800/24 [F]; demais 120 ms [I] |
| Botão com seletor aberto | ícone AMBER; `aria-expanded` | imediato |
| Desativado | ver 3.1 | - |
| Enviando | `submitting`: editor `disabled` até o retorno; um segundo envio é ignorado | [F] `if(o)return` (`web`@3483960) |
| Enviado | texto, rascunho e resposta pendente limpos; foco permanece; lista rola para o fim | imediato |
| Resposta pendente | faixa entra de cima para baixo | 150 ms ease-out [I] |
| Anexos pendentes | bandeja aparece no topo da caixa; caixa cresce | sem animação de altura [I] |
| Anexo enviando | barra de 3 px AMBER no pé do tile, de 0 a 100 % | linear |
| Erro de envio | texto continua no rascunho se a validação falhou; se o servidor falhou, a mensagem otimista vira "falhou" com "Tentar de novo" (área da lista) | - |
| Limite excedido | contador vermelho (RED) com número negativo; envio bloqueado com diálogo | - |
| Modo lento em contagem | selo com relógio e tempo restante; ao tentar enviar: caixa treme 2 px por 200 ms e o selo "pisca" | 200 ms [F] (`SHAKE_APP duration:200,intensity:2`); piscar: fundo AMBER 25 % -> 0 em 300 ms [I] |
| Popup de autocompletar | aparece sem animação de altura; fade 120 ms [I]; item sob o mouse realça L4 e **zera a seleção do teclado** | [F] `onResultHover` -> `setSelectedIndex(null)` |
| Arrastando arquivo sobre a janela | sobreposição cobre a área do chat (ver 4.9) | fade 120 ms |

### 3.1 Desativado (sem permissão etc.)

Quando `disabled`: editor vazio e não interativo, **sem botões**, cursor `NotAllowed`; fundo L1 em vez de L3, traço LINE (nunca AMBER). [F] `getPlaceholder`: se desativado e não é pré-visualização, mostra o texto de motivo (`cta`/`web`@3430500) e o valor é esvaziado. O motivo vem de `disabled = lurking || membro pendente || (!privado && !SEND_MESSAGES) || canal somente leitura` (`web`@3488500).

Textos (pt-BR, do bundle):
- Sem permissão: "Você não tem permissão para enviar mensagens neste canal." [F `IYKTTc`]
- Canal somente leitura: "O canal é somente leitura." [F `RRvRp/`]
- Usuário bloqueado (DM): "Você não pode enviar mensagens a um usuário que bloqueou." [F `9T6N5/`]
- Tópico arquivado: "Este tópico foi arquivado." [F `96UEzi`]
- Castigo (timeout) [I]: "Você está de castigo até {hora}." (no Discord é uma faixa própria, `communicationDisabledUntil`, `cta`@36000).

"Anexar arquivos" também depende de permissão: `canAttachFiles = (tipo permite anexos) && (DM || membro pendente || ATTACH_FILES || há comando ativo)` [F] (`web`@3488600).

---

## 4. Regras de comportamento

### 4.1 Teclado do campo

| Tecla | Efeito | Fonte |
|---|---|---|
| Enter | envia. **Exceções**: Shift ou Alt seguram -> quebra de linha; dentro de bloco de código aberto (número ímpar de "```" antes do caret) -> quebra de linha | [F] `handleKeyPress` e a versão Slate (`web`@3410400, @3422175) |
| Ctrl+Enter | envia mesmo quando "Enter envia" está desligado (tipos que usam `disableEnterToSubmit`) | [F] |
| Shift+Enter, Alt+Enter | nova linha | [F] |
| Seta para cima, com a caixa **vazia** e sem Shift/Alt/Ctrl/Meta | edita a última mensagem **sua** que seja editável no canal (se a última ação foi um comando de barra mais recente, recupera o comando) | [F] `handleKeyDown` em `cta` (procure `getLastEditableMessage`) |
| Esc (foco na própria caixa, sem modificador) | 1) se o popup está aberto: fecha só o popup (não apaga texto); 2) senão, se há anexos pendentes: limpa todos | [F] popup: `hideAutocomplete` (`web`@3410800); anexos: `clearAll` (`cta`) |
| Esc, resposta pendente | cancela a resposta | [I] (o código lido não mostra; no Discord o "x" da faixa é o caminho garantido) |
| Seta cima/baixo, Ctrl+P / Ctrl+N | movem a seleção do popup (se visível); caso contrário comportamento normal do texto | [F] (`web`@3410300, @3421336) |
| Tab | com popup: insere o item selecionado; **sem seleção**: seleciona o primeiro item (em listas de foco manual) e consome a tecla; sem popup: foco segue o tab normal (Tab não indenta, exceto dentro de bloco de código, onde insere tabulação) | [F] `onTabOrEnter`, `handleTabOrEnterDown` |
| Enter com popup | com item selecionado: insere/aplica; **sem seleção: não é consumido, a mensagem é enviada** | [F] `onTabOrEnter(true)` devolve false se não há seleção |
| Espaço | com popup: se o tipo define "índice de seleção por espaço" (ex.: nome de emoji exato), completa | [F] `getSpaceSelectIndex`; quais tipos [?] |
| Backspace/Delete | só contabiliza "combo" (modo de brincadeira) - ignorar | [F] |
| Ctrl/Cmd+V | ver 4.7 | - |
| Ctrl+Z / Ctrl+Y | desfazer/refazer do próprio campo (o egui já faz) | - |

Navegação do popup com laço: ↓ no último item volta ao primeiro (ou a "nada selecionado" em listas de foco manual); ↑ no primeiro vai para o último [F] (`web`@3368300). Mapa de modos de foco [F]: `AUTO` (primeiro item já selecionado), `AUTO_WHEN_FILTERED` (só se a consulta não está vazia), `MANUAL` (nada selecionado até Tab/seta). Qual tipo usa qual modo é [?]; proposta [I]: `@` e `#` = `AUTO_WHEN_FILTERED` (um "@" solto não sequestra o Enter), `:` = `AUTO`, `/` = `AUTO_WHEN_FILTERED`.

IME [?]: não enviar com Enter enquanto há pré-edição ativa (guardar o último `ImeEvent::Preedit` não vazio e ignorar o Enter do mesmo quadro).

### 4.2 Envio (ordem das validações) [F] (`web`@10321000, `cta`)

1. Aparar espaços nas pontas. Vazio **sem** anexos/figurinhas -> não faz nada, mantém foco.
2. `len > máximo` -> diálogo "Sua mensagem é muito longa." / "Encurte sua mensagem de {n} caracteres. Para o conforto de todos, estabelecemos o limite de {max} caracteres." [Ok]. Não limpa. (No Discord sem Nitro mostra o upsell; no Telacast só o diálogo.)
3. Canal de servidor **em modo lento com contagem > 0** -> não envia, não limpa, mantém foco, treme e pisca o selo.
4. Avisos de confirmação: `@everyone`/`@here` com mais de **30** destinatários e permissão para mencionar (mostra a contagem arredondada na ordem de grandeza); texto que parece token de autenticação -> pede confirmação. [F] (`web`@10320100). No Telacast: só `@everyone` se existir; token fica [?].
5. Fila de envio cheia (**5** mensagens pendentes) -> diálogo "EITA! PERAÍ. SEGURA A ONDA" / "Você está enviando mensagens muito depressa!" [Vou me acalmar]. [F] (`MessageQueue maxSize=5`, `web`@2390000; textos `7Q4eo2`, `gi6XHp`, `Z4U1g/`).
6. Resposta, anexos e figurinhas vão junto. Envio otimista com `nonce`; ao enviar: limpa texto, rascunho, resposta pendente (`DELETE_PENDING_REPLY`); se for envio com anexos, limpa a bandeja ao enfileirar e **restaura** (texto e anexos) se o upload falhar [F] (`cta` `handleSendMessage`).
7. Conversões de texto no envio (o servidor guarda o texto bruto com tokens) [F]: `@nome` -> `<@id>`, `#canal` -> `<#id>`, `:nome:` de emoji personalizado -> `<:nome:id>` / `<a:nome:id>`, emoji padrão -> o caractere; `@everyone`/`@here` ficam como texto (`web`@3095000). Em egui o buffer é texto puro: ver 6.3.
8. Após sucesso: reinicia o cronômetro do modo lento em `slowmode*1000 + 100` ms [F] (`web`@9774400, margem de 100 ms); se o servidor responder `SLOWMODE_RATE_LIMITED` com `retry_after`, usa esse valor [F] (`web`@683700).

### 4.3 Limite de caracteres e contador

- Limite do Discord: **2000** (padrão) e **4000** (Nitro) [F `nW=2e3`, `nY=4e3`, `web`@319652]. O banco do Telacast já tem `CHECK (char_length(content) BETWEEN 1 AND 4000)`. Proposta [I]: `MSG_MAX = 4000` (pergunta 1).
- Contador: restantes = `max - len`. **Visível quando restantes <= max/10** (padrão) [F] (`useCharacterCount`, `web`@3519600: `h>(d??o??u/10)`). Com 4000 aparece em ≤ 400. Aqui `max/10` é o limiar de "mostrar", e o número mostrado é sempre `restantes` (pode ficar negativo).
- Número: 12 px semibold tabular, TEXT2; **negativo -> RED**; dica ao pairar: `0` -> "Mensagem no tamanho máximo"; `>0` -> "{n} caracteres restantes"; `<0` -> "Mensagem muito longa" [F `tU6YQ7`, `qH8uFW`, `YSRIqa`]. Leitor de tela: "{n} caracteres restantes" em região `aria-live=polite`. Quando a contagem cruza o limite pela primeira vez no digitar, o cliente registra evento (ignorar).
- O campo **não bloqueia a digitação** além do limite; só marca e impede o envio [F] (`aria-invalid: len>max`).
- Colar texto maior que o limite vira anexo `message.txt` (ver 4.7). Menu "+" tem "Enviar texto como arquivo" quando o campo não está vazio: se o texto contém um bloco "```linguagem\n…\n```" usa a linguagem como extensão do arquivo e deixa o resto no campo [F] (`cta` módulo 316576).
- Rascunho salvo truncado em `max + 500` [F] (`CS1+500`, `web`@9481700).

### 4.4 Resposta ("respondendo a")

- Clicar em "Responder" na mensagem cria a resposta pendente do canal: `pinga = !Shift && !mensagemMinha`; `mostrarToggle = !DM && !mensagemMinha` [F] (`web`@4059999). Só **uma** resposta pendente por canal.
- A resposta pendente é **persistida** (sobrevive ao reiniciar) e é descartada se a mensagem-alvo for apagada [F] (`PendingReplyStore`, `web`@7783800).
- Faixa: "Respondendo para {nome colorido}" (o nome na cor do cargo no Discord; no Telacast, TEXT) [F `eleYb7`/`8E4GxS`]. Clicar no corpo da faixa rola até a mensagem original e a destaca [F] (`onClick: jump`). À direita: chave "@ LIGADO"/"@ DESLIGADO" (role=switch; dica "Clique para desabilitar a menção ao autor original." / "Clique para habilitar a menção ao autor original.") e botão fechar com rótulo "Cancelar resposta" [F `DH2o6R`, `utGGIY`, `p9jC2r`, `U7f3bK`, `jSnJGT`]. Ao enviar, vai `reply_to` e `ping_reply = chave`.
- Cadeia longa: após 2+ respostas encadeadas aparece "Quer facilitar quem quiser seguir esta sequência de N respostas? [Criar tópico]" [F `B3V0FM`/`rBIGBL`]; **fora de escopo** (não há tópicos).
- Foco: criar a resposta foca a caixa [F] (`FOCUS_CHANNEL_TEXT_AREA`).

### 4.5 Indicador "digitando" [F tempos, I layout]

Envio local (quando o usuário edita):
- Só conta se o texto resultante não é vazio, **o valor mudou de fato**, **não começa com "/"** e **não é o atalho de reação** `+:emoji:` (regex `^\+(?!\w+):?(?!:)(\w+)?:?$`) (`cta` `handleTextareaChange`). Texto vazio -> `stopTyping` (e para ao enviar).
- Reenvio no máximo a cada **8 s** (`0,8 x 10 s`). Primeiro aviso: **atraso de 1,5 s** se já houve aviso nos últimos 16 s, senão imediato. Não envia se já há **mais de 5** pessoas digitando no canal. (`web`@9817500: `TYPING_START_LOCAL`, `E=10 s`, `A=1,5 s`.)
- A resposta do servidor ao aviso pode trazer `message_send_cooldown_ms` (>0 inicia/atualiza o cronômetro do modo lento) [F].

Recepção:
- Cada `TYPING_START` marca o usuário como digitando por **10 s** (renova o prazo). Expira sozinho; some imediatamente quando chega uma **mensagem desse usuário** no canal (`MESSAGE_CREATE`) ou um `TYPING_STOP` [F] (`web`@9817300).
- Texto (pt-BR) [F `lJ9sZX`, `rB0CUa`, `StKThj`, `Q8lUnE`; nomes em negrito]:
  - 1: "**A** está digitando..."
  - 2: "**A** e **B** estão digitando..."
  - 3: "**A**, **B** e **C** estão digitando..."
  - 4 ou mais: "**Várias pessoas** estão digitando..."
- Quem entra na lista: todos menos você; ignora bloqueados [I]. Ordem: por chegada [I]. Três pontinhos animados à esquerda (ciclo 1,2 s, cada ponto defasado 0,2 s, alpha 0,35 -> 1) [I]; com "reduzir movimento", estáticos.
- Local do layout [?] (módulo ausente); proposta: faixa de 22 px sob a caixa.
- Em DMs o mesmo texto; na lista de DMs o ponto "digitando" é da área da barra lateral.

### 4.6 Modo lento

- Valor por canal: `rate_limit_per_user` em segundos, **0 a 21600** (6 h); degraus oferecidos [F `n4` em `web`@319652]: 0, 5, 10, 15, 30, 1 min, 2 min, 5 min, 10 min, 15 min, 30 min, 1 h, 2 h, 6 h.
- Isentos: quem tem permissão "Ignorar modo lento" (`BYPASS_SLOWMODE`) e quem gerencia canal/mensagens [F nome da permissão; extensão I]. Cronômetro só existe para quem não é isento.
- Cliente [F] (`SlowmodeStore`, `web`@9774000): guarda `cooldownMs` por (canal, tipo); um timer de **1 s** recalcula `fim - agora`; ao enviar, `slowmode*1000 + 100`; ao mudar o valor do canal, trunca o restante para o novo máximo; ao enviar anexos, reinicia no início do upload e **zera** se o upload falhar/cancelar.
- UI: selo no rodapé da caixa. Com contagem > 0: relógio 12 px + tempo (`Ns` abaixo de 60 s, `m:ss` até 1 h, `h:mm:ss` acima) em AMBER; sem contagem mas canal com modo lento: relógio MUTED sem número. Dica ao pairar [F]: com contagem: "O Modo Lento está ativado. Você poderá mandar outra mensagem neste canal em {n} segundos." (`4rMiHy`); sem contagem: "O Modo Lento está ativado. Membros só podem mandar mensagens a cada {n} segundos." (`IWntYg`; versões com minutos/horas `sY3wlG`, `3hz51F`). A caixa **continua digitável** durante o modo lento (só o envio é bloqueado) [F: `shouldClear:false,shouldRefocus:true`].
- Ao tentar enviar em contagem: tremor 2 px/200 ms e `EMPHASIZE_SLOWMODE_COOLDOWN` (o selo pisca) [F].

### 4.7 Colar, arrastar e anexar

Colar (Ctrl/Cmd+V) [F] (`web`@3433000):
1. Se há **arquivos** na área de transferência (com caminho): viram anexos. Pasta -> erro "Tipo de arquivo inválido" / "Esse tipo de arquivo não é suportado." [F `azO1Pe`, `Koklr/`].
2. Senão, itens-arquivo sem caminho (imagem de captura de tela): vira anexo. Se é **um único `image/png`** e há também `text/html`, o nome vem do `<img src>` do HTML (`nome.png`), senão o nome do blob. 
3. Senão, texto: se for **maior que o limite** de mensagem (e o tipo permite "enviar mensagem longa como arquivo"), vira anexo `message.txt` (text/plain) e o campo não recebe o texto; senão cola como texto normal.
4. Só funciona se o canal permite anexos (`canPasteFiles`); em DM gerenciada (bot) não.
5. Antes de colar o arquivo, o rascunho é salvo; depois o foco volta ao campo.
- Colar com o editor **sem foco** também foca o campo (`GLOBAL_CLIPBOARD_PASTE`) [F].

Arrastar-e-soltar: ver 4.9. Botão "+" -> "Enviar um arquivo": seletor de arquivos nativo.

Regras de anexos (`promptToUpload`, `web`@10398866) [F]:
- Arquivo de 0 byte -> "Falha no envio" / "O arquivo não pode ser vazio".
- Soma/arquivo acima do limite -> "Seus arquivos são poderosos demais" / "O tamanho máximo de arquivos é {x}." (ou "O tamanho total da mensagem deve ter menos de {x}").
- Passar de **10 arquivos** por mensagem -> "Muitos uploads!" / "Você só pode subir 10 arquivos de cada vez!" [F `n5=10`, `wOr6hB`, `qqyp/e`].
- Limite padrão por arquivo: **10 MiB** e teto absoluto **1 GiB** [F as constantes `n6=0xa00000` e `n7=0x40000000` existem ao lado de `n5`; a leitura "padrão/teto" é [I]]. No Telacast: limite configurável pelo servidor (pergunta 2).
- Por padrão colar/arrastar **coloca os arquivos na bandeja** (não envia na hora); Enter envia texto + bandeja. Envio imediato só quando o chamador pede (`requireConfirm=false`).
- Bandeja: cada tile mostra miniatura (imagem/vídeo: jpeg, png, webp, gif, avif, mp4, mov) ou ícone por classe de arquivo; etiquetas pequenas "Alt" (tem texto alternativo) e "Spoiler" [F `QEW81z`, `F+x38C`]. Ações no hover: alternar spoiler ("Anexo com spoiler", olho / olho riscado), "Modificar anexo" (abre modal com nome do arquivo, descrição/alt de até 1024 caracteres, spoiler), "Remover anexo" (lixeira, vermelho) [F `cuurzA`, `Y8ujqr`, `vN7REz`]. Navegação por teclado horizontal; Seta para cima com a caixa vazia foca a bandeja quando há anexos (modo teclado) [F].
- A bandeja é **por canal e por tipo de rascunho** e fica só em memória (não persiste entre execuções) [F `UploadAttachmentStore` guarda em `Map`].

### 4.8 Autocompletar

Mecânica geral [F] (`web`@3370000-3379000; classe `Autocomplete`, `web`@3379500):
- A cada mudança de texto/caret/consulta/foco, recalcula **no próximo tick** (`setTimeout 0`), sem debounce perceptível. Também recalcula quando o armazém de dados do tipo muda (membros, canais).
- Visível se: caixa focada **e** nenhum seletor de expressões aberto **e** (há resultados ou está carregando ou o tipo mostra vazio).
- Mouse: passar sobre item realça e zera a seleção do teclado; clicar aplica; `mousedown` no popup não tira o foco do campo.
- Aplicar com **clique ou Enter** = tipo "SEND" (para GIF/figurinha/reação, envia direto); **Tab** = tipo "INSERT" [F `selectResult(..., e ? SEND : INSERT)`].
- Rolagem: o item selecionado é mantido visível.

Tipos [F nomes: `MENTIONS, CHANNELS, EMOJIS_AND_STICKERS, REACTIONS, STICKERS, GIFS, COMMANDS, COMMAND_OPTIONS, CHOICES, GAME, TIMESTAMPS`]. Gatilhos e regras exatos estão no módulo ausente: **todos os gatilhos abaixo são [I]**, calibrados pelos cabeçalhos e linhas [F].

| Gatilho | Condição [I] | Cabeçalho [F] | Linha |
|---|---|---|---|
| `@` | `@` no início de uma palavra (início do texto ou depois de espaço/quebra); consulta = o que vem após o `@` até o caret, sem espaço. Mostra já com consulta vazia | "Resultados de membros para @{q}" | avatar 24 + nome + usuário à direita |
| `@everyone` / `@here` | aparecem no topo se o usuário pode mencionar todos e a consulta é prefixo | - | descrições [F]: "Notifica todos neste canal." (`5atMLZ`); "Notifica todos os membros conectados neste canal." (`iX9SFD`) |
| `#` | idem, para canais de texto | "Resultados de canais de texto para #{q}" | glifo + nome do canal + categoria à direita |
| `:` | `:` no início de palavra **e** pelo menos **2** caracteres depois (evita "10:30" e URLs) | "Resultados de emojis para :{q}" | glifo/imagem 24 + `:nome:` + servidor à direita |
| `/` | `/` como **primeiro caractere** do campo vazio (o Discord só abre a lista de comandos nesse caso e dispara o evento "sentinela") | "Resultados de comandos para /{q}" | ícone 32 + `/nome` + descrição + app à direita |
| `+:` | texto no formato `+:emoji:` reage à última mensagem (atalho de reação); não conta como "digitando" | - | - |

Ordem/seleção dos resultados e limite por lista são [?]. Propostas [I]: 10 membros (ordenados: quem falou recentemente no canal, depois prefixo de nome de exibição, depois prefixo de usuário, depois contém; sem acento, sem diferença de caixa); 10 canais; 10 emojis; comandos todos. Se o resultado é zero, o popup some (não existe "Nenhum resultado") exceto em carregamento (esqueleto).

Comandos embutidos de texto [F] (`web`@3036900): `shrug` (acrescenta "¯\\_(ツ)_/¯"), `tableflip` (acrescenta "(╯°□°)╯︵ ┻━┻"), `unflip` (acrescenta "┬─┬ノ( º _ ºノ)"), `me` (envolve em itálico `_texto_`), `spoiler` (envolve em `||texto||`), mais `tts`, `nick`, `thread`, `kick`, `ban`, `timeout` (dependem de permissão). Cada um tem uma opção "Sua mensagem" (`message`). Descrições pt-BR [F]: "Inclui ¯\\_(ツ)_/¯ à sua mensagem.", "Adiciona (╯°□°)╯︵ ┻━┻ à sua mensagem.", "Adiciona ┬─┬ ノ( ゜-゜ノ) à sua mensagem.", "Exibir o texto com ênfase.", "Marca sua mensagem como um spoiler."; grupo "Integrado". No v1: `shrug`, `tableflip`, `unflip`, `me`, `spoiler` (transformações locais de texto).

Inserção [I]: substitui `@q` por `@Nome ` (com espaço) e posiciona o caret depois; a lista de (nome -> id) das menções inseridas fica no estado do compositor para virar `<@id>` no envio (6.3).

### 4.9 Arrastar-e-soltar

Ao entrar com arquivos sobre a janela (`hovered_files` não vazio): sobreposição cobre a área do chat com L0 a 70 %, retângulo tracejado AMBER com raio 12 a 24 px das bordas, no centro "Enviar para #{canal}" (serifa 22) e abaixo "Você pode adicionar comentários antes de enviar." [F `9uKafS`, `lpgkzq`; `usQh4J` acrescenta "Pressione Shift para enviar diretamente"]. Soltar -> arquivos entram na bandeja (4.7). Shift ao soltar = enviar já [F texto; comportamento I].

### 4.10 Rascunhos

[F] (`DraftStore`, `web`@9481647, módulo 31717):
- Persistido localmente, chaveado por **usuário -> canal -> tipo de rascunho** (0 mensagem de canal, 1 configurações de tópico, 2 primeira mensagem de tópico, 3/5 comando, 4 enquete, 8 mensagem agendada…) com `{timestamp, draft, command}`. O Telacast usa só o tipo 0 por enquanto.
- Atualiza o armazém a cada alteração; grava no disco com *throttle* de **500 ms**, e também ao perder foco, trocar de canal e fechar (`saveCurrentTextThrottled`, `handleBeforeUnload`).
- Rascunho vazio (ou só espaços) **remove** a entrada. Truncado em `max + 500`.
- Podado: ao conectar e ao sair de servidor, remove rascunhos de canais que não existem mais (só quando nenhum servidor está indisponível); ao apagar canal, remove o dele.
- Ao voltar a um canal, o texto do rascunho reaparece e o foco vai para o campo (se pode enviar, e não é celular) [F `textAreaFocused: SEND_MESSAGES && !mobile`].
- Rascunho **não** inclui anexos (memória) nem resposta (a resposta tem armazém persistido próprio).
- Quando o texto do canal é igual ao `template` do canal, não é guardado [F].

### 4.11 Outros

- Focar o campo ao trocar de canal; digitar uma tecla imprimível sem nenhum campo focado foca o campo e insere o caractere [I].
- Botão de emoji: abre o seletor; "GIF" envia na hora o GIF escolhido como mensagem (com Shift, insere o link no campo em vez de enviar) [F] (`web`@3487000); figurinha: envia na hora se o campo e a bandeja estão vazios; senão fica como prévia anexada à mensagem [F] (`nA`).
- Seletor de expressões abre acima da caixa, alinhado à direita [F] (`position:"top", align:"right"`).
- Ordem dos botões da direita, da esquerda para a direita [F] (`web`@3457300): presente (só em DM com quem pode receber, e nunca com comando ativo), GIF, figurinha, emoji, apps (lançador `/`), enviar (se habilitado). Em modo estreito: GIF/figurinha/emoji viram um botão "expressões".
- Tooltips pt-BR [F]: GIF "Abrir o selecionador de GIFs"; figurinha "Abrir selecionador de figurinhas"; emoji "Adicionar emoji"; apps "Apps"; enviar "Enviar mensagem"; menu do "+" "Ações de canal"; presente "Enviar um presente".

---

## 5. Dados necessários (protocolo, servidor, banco)

Estado atual: `C2S::Send { channel_id, content }`; `S2C::Message` com `id, channel_id, author_id, author, content, ts`; `ServerInfo.members/channels` já vão no `Snapshot`; `messages.content` tem `CHECK 1..4000`.

**P0 - necessário para o núcleo**
1. `C2S::Send { channel_id, content, nonce: i64, reply_to: Option<i64>, ping_reply: bool, attachments: Vec<i64> }` e `Message.nonce: Option<i64>` de volta (para trocar a mensagem otimista pela real e sinalizar erro).
2. `S2C::Error` ganha código e dica: `Error { code: ErrCode, msg, retry_after_ms: Option<u32> }` com `ErrCode::{TooLong, Empty, SlowmodeRateLimited, NoPermission, FileTooLarge, TooManyFiles, RateLimited}`. O cliente nunca depende só do texto.
3. Digitando: `C2S::Typing { channel_id }` e `S2C::Typing { channel_id, user_id }` (volátil, sem banco). O servidor: confere que o usuário está no canal, limita a **1 por 5 s por usuário e canal** (descarta o resto), reenvia aos demais membros que veem o canal. Não persiste. `S2C::Typing` é renovado a cada 8 s pelo cliente; o cliente expira em 10 s.
4. Regra no servidor: `content` com `char_length > MSG_MAX` -> `TooLong`; **trim** no servidor também.
5. Banco: `ALTER TABLE messages ADD COLUMN reply_to BIGINT REFERENCES messages(id) ON DELETE SET NULL;` e relaxar o `CHECK` para permitir mensagem só com anexo: `CHECK (char_length(content) <= 4000)` + validação na aplicação "conteúdo não vazio **ou** ao menos um anexo".
6. `Message` ganha `reply: Option<ReplyRef { message_id, author_id, author, snippet(<=100 chars), deleted: bool }>` (resolvido no servidor; `snippet` evita busca no cliente) e `edited_at: Option<i64>`.

**P1 - modo lento e permissões**
7. `Channel` (no `ServerInfo`) ganha `slowmode_secs: u32` (0..21600) e, no `Snapshot`, o conjunto de permissões efetivas do usuário por canal: `can_send`, `can_attach`, `bypass_slowmode`, `can_mention_everyone` (bits em `perms: u32` por canal). Coluna `channels.slowmode_secs INT NOT NULL DEFAULT 0 CHECK (slowmode_secs BETWEEN 0 AND 21600)`.
8. Servidor aplica o modo lento: tabela em memória `(channel_id, user_id) -> last_send_ms` (não precisa de banco; reinicia vazia); se `now - last < secs*1000` e o usuário não é isento -> `Error{SlowmodeRateLimited, retry_after_ms}`. Também envia `S2C::Slowmode { channel_id, remaining_ms }` ao abrir o canal e em resposta ao primeiro `Typing` (o Discord devolve o restante na resposta do "digitando") para o selo aparecer sem tentativa de envio.
9. `C2S::SetSlowmode { channel_id, secs }` (gerência) - área de administração.

**P2 - anexos**
10. Upload: `POST /upload` (multipart, autenticado pelo token da sessão) -> `{ id, filename, size, content_type, width, height }`; guardado em disco no servidor com `attachments(id, channel_id, uploader_id, filename, content_type, size, width, height, spoiler, description, created_at)` e `message_attachments(message_id, attachment_id, position)` (ordem preservada). O servidor impõe: máx **10** por mensagem, tamanho por arquivo configurável (padrão 10 MiB), recusa vazio, grava `spoiler` e `description` (<= 1024). `Message.attachments: Vec<Attachment{ id, filename, size, content_type, width, height, spoiler, description, url }>`. Download via `GET /files/{id}/{filename}` com `Content-Disposition` correto e `X-Content-Type-Options: nosniff` (nunca servir HTML inline).
11. Progresso: o cliente mede os bytes enviados no próprio POST; sem evento do servidor.
12. Cota por servidor/usuário e limpeza de anexos órfãos (enviados mas nunca vinculados a mensagem em 1 h).

**P3 - edição e menções**
13. `C2S::Edit { message_id, content }` e `S2C::MessageEdited { message_id, channel_id, content, edited_at }` (Seta para cima precisa disso); `C2S::Delete` é de outra área.
14. Menções: tabela `message_mentions(message_id, user_id)` com índice `(user_id, message_id DESC)` preenchida pelo servidor ao extrair `<@id>` do texto; é o que alimenta contadores de menção (área de não lidos). O servidor **valida** tokens (id existe e está no servidor) e **ignora** o resto.
15. Emoji personalizados e cargos: ausentes hoje. Quando existirem: `Emoji { id, server_id, name, animated }` no `ServerInfo`, `Role`. Para `@cargo`.

**Dados já suficientes para o autocompletar do v1:** membros (`ServerInfo.members`), canais (`ServerInfo.channels`), emojis Unicode (tabela embutida no cliente com nome/aliases pt-BR).

---

## 6. Plano de implementação em egui 0.37

### 6.1 Módulos e funções

Novo `app/src/compose.rs` (para não inchar `main.rs`, que já tem 1224 linhas). `main.rs` só chama `compose::show(...)` em `center()` no lugar do trecho "// entrada" (linhas ~876-904) e remove o `input_h = 82.0` fixo (o painel passa a ter altura automática).

```rust
pub struct ComposeState {            // um por canal: HashMap<i64, ComposeState> em Session
    pub text: String,                // rascunho
    pub mentions: Vec<(String, Tok)>,// ("@Ana", Tok::User(7)) inseridos pelo autocompletar
    pub reply: Option<PendingReply>, // { msg_id, author, author_id, ping, show_toggle }
    pub files: Vec<Pending>,         // anexos pendentes (memória)
    pub ac: Option<Ac>,              // popup aberto
    pub typing: TypingOut,
    pub shake_until: Option<f64>,    // segundos do ctx.input(|i| i.time)
    pub flash_slow_until: Option<f64>,
}
pub struct ComposeEnv<'a> {          // somente leitura, montado em main.rs
    pub channel: i64, pub label: &'a str, pub is_dm: bool,
    pub members: &'a [p::User], pub channels: &'a [p::Channel],
    pub disabled: Option<DisabledWhy>, pub can_attach: bool,
    pub slowmode_secs: u32, pub slow_remaining_ms: u64, pub bypass_slow: bool,
    pub typing_names: Vec<String>,   // já filtrado: sem você, sem bloqueados
    pub max_len: usize,
}
pub fn show(ui: &mut egui::Ui, ctx: &egui::Context, st: &mut ComposeState, env: &ComposeEnv, acts: &mut Vec<Act>);
```

Funções (puras onde possível, testáveis sem janela):
- `find_trigger(text, caret) -> Option<Trigger>`; `apply_completion(&mut String, &Trigger, caret, &str) -> usize` (6.3).
- `rank_members/channels/emoji(query, ...) -> Vec<Item>`.
- `typing_runs(names) -> Vec<(String, bool)>`; `TypingOut::poll(...)`; `TypingIn` (mapa `(canal, usuário) -> Instant`, poda a 10 s).
- `slow_label(ms) -> String` (`4s`, `1:05`, `1:02:03`).
- `encode_outgoing(text, &mentions, &members, &channels) -> String` (vira tokens).
- `counter_state(len, max) -> Option<(i64, Color32)>`.
- Desenho: `plus_button`, `icon_button`, `draw_icon(painter, rect, Icon, color)` com `Icon::{Plus, Emoji, Gif, Sticker, Gift, Apps, Clock, Send, Close, Eye, EyeOff, Pencil, Trash}` feitos com `painter.circle_stroke / line_segment / PathShape / rect_stroke` (sem fonte de ícones; texto "GIF" em 9 px para o ícone de GIF).
- `reply_bar`, `attachment_tray`, `ac_popup`, `typing_strip`, `drop_overlay`, `slow_badge`, `counter`.
- Persistência: `drafts_to_json(&HashMap<i64,ComposeState>) -> String` / `from_json`, chamadas em `App::save` (já existe, usa `storage.set_string`) e em `App::new`; chave `"drafts:{user_id}"`. Reduzir `fn auto_save_interval(&self) -> Duration` para 5 s (padrão do eframe é 30 s).
- `theme.rs`: acrescentar `SHAKE`, constantes `COMPOSE_MIN_H=50.0`, `COMPOSE_LINE_H=21.0`, e helpers `kbd_hint`. Nada de gradiente.

### 6.2 Esboço A: caixa, teclas e envio (a parte mais traiçoeira)

```rust
/// Enter "puro" (sem Shift/Alt). NÃO use consume_key(Modifiers::NONE, Enter): ele usa
/// matches_logically, que ignora Shift/Alt extras e engoliria o Shift+Enter.
fn take_plain_enter(ctx: &egui::Context) -> bool {
    ctx.input_mut(|i| {
        let pos = i.events.iter().position(|e| matches!(e,
            egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. }
            if !modifiers.shift && !modifiers.alt));
        if let Some(p) = pos { i.events.remove(p); true } else { false }
    })
}

fn in_open_code_block(text: &str, caret_char: usize) -> bool {
    let upto: String = text.chars().take(caret_char).collect();
    upto.matches("```").count() % 2 == 1
}

pub fn show(ui: &mut egui::Ui, ctx: &egui::Context, st: &mut ComposeState, env: &ComposeEnv, acts: &mut Vec<Act>) {
    let id = egui::Id::new(("compose", env.channel));
    let focused = ctx.memory(|m| m.has_focus(id));
    let caret = egui::TextEdit::load_state(ctx, id)
        .and_then(|s| s.cursor.char_range()).map(|r| r.primary.index).unwrap_or(st.text.chars().count());

    // 1) teclas que precisam sumir ANTES do TextEdit enxergá-las
    let mut ac_keys = AcKeys::default();
    if focused && st.ac.is_some() { ac_keys = take_ac_keys(ctx); }   // ↑ ↓ Ctrl+P/N Tab Esc; Enter só se há seleção
    let enter_ok = focused && env.disabled.is_none()
        && !(st.ac.as_ref().is_some_and(|a| a.sel.is_some()))        // com item selecionado o Enter é do popup
        && !in_open_code_block(&st.text, caret);
    let send_now = enter_ok && take_plain_enter(ctx);                 // Shift/Alt+Enter seguem para o TextEdit (nova linha)

    // 2) moldura
    let max_h = (ctx.content_rect().height() * 0.5).max(120.0);       // 50 % da janela [F]
    let shake = shake_offset(ctx, st);                                // ±2 px por 200 ms
    let hint = env.disabled.map(|d| d.text()).unwrap_or(env.label);
    let stroke = egui::Stroke::new(1.0, lerp(LINE2, AMBER, ctx.animate_bool_with_time(id.with("f"), focused, 0.16)));
    let top_r = if st.reply.is_some() { 0 } else { 12 };
    egui::Frame::new().fill(if env.disabled.is_some() { L1 } else { L3 }).stroke(stroke)
        .corner_radius(egui::CornerRadius { nw: top_r, ne: top_r, sw: 12, se: 12 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width() - 2.0 * 28.0 + shake);
            if !st.files.is_empty() { attachment_tray(ui, st, env); }
            ui.horizontal_top(|ui| {
                if env.disabled.is_none() { plus_button(ui, ctx, st, env, acts); }
                let right_w = right_buttons_width(ui.available_width(), env);
                let text_w = (ui.available_width() - right_w).max(80.0);

                let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                    let mut job = egui::text::LayoutJob::default();
                    job.wrap.max_width = wrap;
                    let fmt = |c| egui::TextFormat { font_id: egui::FontId::proportional(14.5), color: c,
                        line_height: Some(21.0), ..Default::default() };
                    paint_mentions(&mut job, buf.as_str(), &st.mentions, fmt(TEXT), fmt(AMBER)); // @Ana em âmbar
                    ui.ctx().fonts_mut(|f| f.layout_job(job))
                };
                let out = egui::ScrollArea::vertical().id_salt(id.with("sc")).max_height(max_h - 29.0)
                    .auto_shrink([false, true]).show(ui, |ui| {
                        egui::TextEdit::multiline(&mut st.text)
                            .id(id).hint_text(egui::RichText::new(hint).color(MUTED))
                            .frame(egui::Frame::NONE).margin(egui::Margin::symmetric(0, 14))
                            .desired_width(text_w).desired_rows(1)
                            .lock_focus(false)                 // Tab não insere tabulação
                            .interactive(env.disabled.is_none())
                            .layouter(&mut layouter).show(ui)
                    }).inner;
                if out.response.changed() { st.on_edit(ctx, env, out.cursor_range); }   // typing, ac, rascunho
                right_buttons(ui, ctx, st, env, acts);
            });
            footer_band(ui, st, env, ctx);                      // modo lento + contador
        });

    // 3) envio
    if send_now || st.send_clicked { try_send(ctx, st, env, acts); }
    if let Some(a) = st.ac.as_ref() { ac_popup(ctx, st, env, ac_keys, frame_rect); }
    typing_strip(ui, ctx, env);
}
```

`try_send` segue a ordem de 4.2 (vazio -> tamanho -> modo lento -> confirmações -> fila). Se válido: `acts.push(Act::Send(C2S::Send { channel_id, content: encode_outgoing(...), nonce, reply_to, ping_reply, attachments }))`, depois `st.text.clear()`, `st.reply = None`, `st.files.clear()`, `TextEditState::load` + `clear_undoer()` + `store`, `ctx.memory_mut(|m| m.request_focus(id))`.

### 6.3 Esboço B: gatilhos e inserção (puro, com testes unitários)

Atenção: o índice do caret do egui (`CCursor.index`) é em **caracteres**, não bytes.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind { Mention, Channel, Emoji, Command }
pub struct Trigger { pub kind: Kind, pub start: usize /* índice do sigilo, em chars */, pub query: String }

pub fn find_trigger(text: &str, caret: usize) -> Option<Trigger> {
    let cs: Vec<char> = text.chars().collect();
    let caret = caret.min(cs.len());
    let mut i = caret;
    while i > 0 && !cs[i - 1].is_whitespace() { i -= 1; }          // início da "palavra" do caret
    let mut it = cs[i..caret].iter();
    let sigil = *it.next()?;
    let query: String = it.collect();
    let kind = match sigil {
        '@' => Kind::Mention,
        '#' => Kind::Channel,
        ':' if query.chars().count() >= 2 && !query.contains(':') => Kind::Emoji,
        '/' if i == 0 => Kind::Command,
        _ => return None,
    };
    Some(Trigger { kind, start: i, query })
}

fn byte_of(s: &str, ci: usize) -> usize { s.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(s.len()) }

/// Troca [start, caret) por `insert` e devolve o novo caret (em chars).
pub fn apply_completion(text: &mut String, t: &Trigger, caret: usize, insert: &str) -> usize {
    let (a, b) = (byte_of(text, t.start), byte_of(text, caret));
    text.replace_range(a..b, insert);
    t.start + insert.chars().count()
}

/// No envio: "@Ana" -> "<@7>", só se ainda bate exatamente com uma menção registrada e vem seguida de fim/espaço/pontuação.
pub fn encode_outgoing(text: &str, ms: &[(String, Tok)]) -> String { /* percorre ms ordenado por len desc */ }
```

Depois de `apply_completion`, grave o novo caret: `let mut s = TextEditState::load(ctx, id).unwrap_or_default(); s.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(novo)))); s.store(ctx, id);` e `request_focus(id)`.

### 6.4 Esboço C: digitando (envio e exibição)

```rust
const TYPING_TTL: Duration = Duration::from_secs(10);
const RESEND: Duration = Duration::from_secs(8);       // 0,8 x 10 s
const FIRST_DELAY: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct TypingOut { last: Option<Instant>, due: Option<Instant> }
impl TypingOut {
    /// Chame a cada quadro. Devolve true quando deve mandar C2S::Typing agora.
    pub fn poll(&mut self, now: Instant, wants: bool, others_typing: usize) -> bool {
        if !wants { self.due = None; return false; }
        if let Some(d) = self.due { if now >= d { self.due = None; self.last = Some(now); return others_typing <= 5; } return false; }
        if self.last.is_some_and(|t| now - t < RESEND) { return false; }
        let delay = if self.last.is_some_and(|t| now - t < 2 * RESEND) { FIRST_DELAY } else { Duration::ZERO };
        if delay.is_zero() { self.last = Some(now); return others_typing <= 5; }
        self.due = Some(now + delay); false
    }
}
// `wants` = o texto mudou neste quadro, não está vazio, não começa com "/" e não é o atalho "+:emoji:".
// Texto vazio ou envio: wants=false e zera `last`.
pub fn typing_runs(names: &[String]) -> Vec<(String, bool)> {      // (trecho, em negrito/TEXT)
    match names {
        [] => vec![],
        [a] => vec![(a.clone(), true), (" está digitando...".into(), false)],
        [a, b] => vec![(a.clone(), true), (" e ".into(), false), (b.clone(), true), (" estão digitando...".into(), false)],
        [a, b, c] => vec![(a.clone(), true), (", ".into(), false), (b.clone(), true), (" e ".into(), false), (c.clone(), true), (" estão digitando...".into(), false)],
        _ => vec![("Várias pessoas".into(), true), (" estão digitando...".into(), false)],
    }
}
```
Recepção: `HashMap<(i64, i64), Instant>`; a cada quadro `retain(|_, t| t.elapsed() < TYPING_TTL)`; remova a entrada quando chegar `S2C::Message` do autor. Enquanto houver alguém digitando: `ctx.request_repaint_after(Duration::from_millis(100))` para os pontinhos e para expirar no tempo certo.

### 6.5 Esboço D: popup do autocompletar

```rust
fn ac_popup(ctx: &egui::Context, st: &mut ComposeState, env: &ComposeEnv, keys: AcKeys, box_rect: egui::Rect) {
    let Some(ac) = st.ac.as_mut() else { return };
    let win_h = ctx.content_rect().height();
    let editor_h = box_rect.height();
    let max_h = 490f32.min(win_h - 120.0 - (0.5 * win_h).min(editor_h));          // [F] fórmula do Discord
    let n = ac.items.len();
    if keys.down { ac.sel = match ac.sel { None => Some(0), Some(i) if i + 1 < n => Some(i + 1), _ => if ac.manual { None } else { Some(0) } }; ac.scroll_to_sel = true; }
    if keys.up   { ac.sel = match ac.sel { None | Some(0) => Some(n - 1), Some(i) => Some(i - 1) }; ac.scroll_to_sel = true; }
    if keys.tab && ac.sel.is_none() && ac.manual { ac.sel = Some(0); }

    let mut picked = None;
    egui::Area::new(egui::Id::new("ac_popup")).order(egui::Order::Foreground)
        .fixed_pos(box_rect.left_top() - egui::vec2(0.0, 8.0))
        .pivot(egui::Align2::LEFT_BOTTOM).constrain(true).show(ctx, |ui| {
            egui::Frame::new().fill(L3).stroke(egui::Stroke::new(1.0, LINE2))
                .corner_radius(egui::CornerRadius::same(10)).inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(box_rect.width() - 16.0);
                    ui.set_max_height(max_h);
                    egui::ScrollArea::vertical().max_height(max_h - 16.0).show(ui, |ui| {
                        ui.label(caps(&ac.heading));                                   // "RESULTADOS DE MEMBROS PARA @ana"
                        for (i, it) in ac.items.iter().enumerate() {
                            let (rect, r) = ui.allocate_exact_size(egui::vec2(ui.available_width(), it.row_h()), egui::Sense::click());
                            let sel = ac.sel == Some(i);
                            if r.hovered() && r.ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO) { ac.sel = None; }   // mouse zera o teclado [F]
                            if sel || r.hovered() { ui.painter().rect_filled(rect, 8.0, L4); }
                            it.paint(ui, rect);                                         // avatar/ícone + título + secundário à direita
                            if sel && ac.scroll_to_sel { r.scroll_to_me(None); }
                            if r.clicked() { picked = Some(i); }
                        }
                    });
                });
        });
    ac.scroll_to_sel = false;
    if keys.enter_or_tab_accept { picked = picked.or(ac.sel); }
    if let Some(i) = picked { st.accept_completion(ctx, env, i); }    // devolve o foco ao TextEdit no mesmo quadro
}
```

### 6.6 Outros esboços curtos

- **Colar imagem.** O `egui-winit` só entrega `Event::Paste(String)`; imagem exige `arboard` direto (já está no `Cargo.lock` via eframe; declarar `arboard = { version = "3.7", default-features = false, features = ["image-data"] }` e, para codificar PNG, `image = { version = "0.25", default-features = false, features = ["png"] }`). No quadro em que `i.modifiers.command && i.key_pressed(Key::V)` com o campo focado: em uma thread, `Clipboard::new()?.get_image()` -> `RgbaImage::from_raw(w, h, bytes)` -> `write_to(&mut Cursor::new(&mut png), ImageFormat::Png)` -> manda por canal para a UI, que cria `Pending { name: "image.png", bytes }`. Se `get_image()` falha, o texto colado normal já foi tratado pelo egui. Em Wayland puro a captura de imagem exige a feature `wayland-data-control` de `arboard` [?]. Arquivos copiados no gerenciador de arquivos: `Clipboard::get().file_list()` (arboard >= 3.6) [?].
- **Arrastar.** `ctx.input(|i| i.raw.hovered_files.len())` para a sobreposição; `i.raw.dropped_files` para ler (`path` no desktop). Validar tamanho/quantidade como em 4.7.
- **Rascunho.** `fn drafts_json(&self) -> String` percorre o mapa e inclui só `text` não vazio (truncado a `max+500`); `App::save` chama `storage.set_string(&format!("drafts:{uid}"), json)`. Ao receber o `Snapshot`, remover entradas de canais inexistentes.
- **Tremor.** `shake_offset = (t * 60.0).sin() * 2.0 * (1.0 - t_norm)` por 200 ms, aplicado só ao `x` do Frame; `request_repaint()` enquanto ativo.
- **Emoji no botão.** Dois ícones (cinza e colorido) com `lerp` de alpha por `animate_bool_with_time(id, hover, 0.12)` e escala 1 -> 1,14 sobre o rect (sem emoji colorido nativo no egui: usar glifo monocromático de uma fonte de emoji ou textura; ver armadilhas).

### 6.7 Armadilhas do egui

1. **`consume_key(Modifiers::NONE, Key::Enter)` também consome Shift+Enter** (usa `matches_logically`, que ignora Shift/Alt a mais). Use `take_plain_enter` (6.2) ou compare `modifiers` à mão.
2. A tecla tem de ser consumida **antes** de `TextEdit::show`, senão o multilinha já inseriu `\n`. Shift+Enter e Alt+Enter passam e viram `\n` sozinhos (o `return_key` padrão usa `matches_logically`).
3. Setas, Tab e Esc do popup também precisam ser consumidas antes, ou o `TextEdit` move o caret/perde o foco. `lock_focus(false)` para o Tab não inserir tabulação.
4. O caret do egui é em **caracteres**; `String::replace_range` é em **bytes**. Converta (6.3). Teste com acentos e emoji de 2 unidades de código.
5. Troca programática de texto: depois de alterar `st.text`, atualize `TextEditState` (caret) e chame `clear_undoer()` quando limpar no envio, senão Ctrl+Z "ressuscita" a mensagem enviada.
6. O `Id` do `TextEdit` por canal (`("compose", ch)`) perde o foco ao trocar de canal. Peça `request_focus` explicitamente ao trocar (e só se `disabled.is_none()`).
7. Altura: `desired_rows(1)` + `ScrollArea::max_height` dá o crescimento e a rolagem; o `TextEdit` já rola até o caret. Sem `Panel::bottom` de altura automática, a lista de mensagens não sabe quanto sobra: troque o `input_h = 82.0` por `egui::Panel::bottom("compose").frame(Frame::NONE).show_inside(ui, ...)` desenhado **antes** do `ScrollArea` das mensagens.
8. `TextFormat::line_height` existe (e `background`/`expand_bg` servem para realçar menções), mas **não existe negrito** da fonte variável Inter: o "negrito" dos nomes em "digitando" é cor (TEXT contra TEXT2). Sem `letter-spacing` confiável: títulos em caixa alta do popup só usam tamanho 11,5 + cor.
9. **Sem emoji colorido** em texto: o glifo do emoji sai monocromático (fonte de emoji monocromática) ou precisa de imagem (`egui_extras::install_image_loaders` + PNG/atlas). O sprite do botão de emoji e as linhas `:nome:` do popup precisam disso; começar com monocromático.
10. `request_repaint_after` só enquanto algo anima (pontinhos, contagem do modo lento, tremor); a máquina tem pouca RAM e CPU ociosa importa.
11. A contagem do modo lento deve usar o relógio do servidor para o *restante* recebido (`remaining_ms`) e `Instant` local para decrescer; nunca `SystemTime` do cliente.
12. Popup como `Area` em `Order::Foreground` não rouba foco do `TextEdit` se você não fizer `request_focus` nele; mas clicar numa linha tira o foco: repita `request_focus(id)` no quadro do clique.
13. IME: ver 4.1 [?].

---

## 7. Ordem de entrega (passos pequenos e verificáveis)

1. **Caixa multilinha** em `compose.rs`: Enter envia, Shift+Enter e Alt+Enter quebram, bloco de código aberto aceita Enter, altura de 50 até 50 % da janela, foco âmbar com transição de 160 ms, texto **por canal** (hoje `s.input` é único e vaza entre canais). *Usuário vê:* a caixa crescer e o texto de cada canal ficar no lugar ao alternar.
2. **Rascunhos persistidos** (`App::save` + `auto_save_interval` 5 s). *Vê:* fechar e abrir o app e o rascunho do canal voltar.
3. **Contador e limite**: contador em ≤ 10 % restante, vermelho abaixo de zero, diálogo "Sua mensagem é muito longa", servidor recusa `TooLong`. *Vê:* colar 4100 caracteres e não conseguir enviar.
4. **"Digitando"**: `C2S::Typing` / `S2C::Typing`, throttle de 8 s / atraso 1,5 s, expiração 10 s, texto de 1/2/3/várias pessoas. *Vê (2 clientes):* "Ana está digitando..." que some ao enviar.
5. **Faixa de resposta**: botão "Responder" na mensagem, `reply_to`, chave @ LIGADO/DESLIGADO, X, persistência por canal, citação na mensagem. *Vê:* responder e ver a referência.
6. **Autocompletar `@` e `#`** (dados locais): popup acima da caixa com ↑↓ Tab Enter Esc, hover zera teclado, tokens `<@id>` / `<#id>` no envio, realce âmbar no editor. *Vê:* digitar `@an` e escolher.
7. **`:emoji:` e `/comandos`** (shrug, tableflip, unflip, me, spoiler). *Vê:* `/shrug oi` virar "oi ¯\_(ツ)_/¯".
8. **Botões**: "+" com menu ("Enviar um arquivo" desativado até o passo 10), botões à direita (GIF, figurinha, emoji desativados com dica "em breve"; ocultáveis), modo estreito ≤ 450 px, estados desativados por permissão (`can_send`, somente leitura, DM bloqueada). *Vê:* canal só-leitura com a mensagem de motivo e sem botões.
9. **Modo lento**: coluna `slowmode_secs`, `Error::SlowmodeRateLimited`, selo com relógio, tremor + piscar. *Vê:* canal com 10 s: enviar de novo antes do fim treme e o selo pisca.
10. **Anexos**: seletor de arquivo, colar imagem (arboard), arrastar-e-soltar com sobreposição, bandeja com miniaturas e ações, limites de 10 arquivos e tamanho, upload HTTP com barra de progresso, mensagem com anexos na lista. *Vê:* colar uma captura e enviar.
11. **Seta para cima edita a última** (depende de `C2S::Edit` e da edição inline da lista). *Vê:* ↑ com caixa vazia abre a edição da sua última mensagem.
12. **Acabamento**: mola do botão de emoji, confirmação de `@everyone`, "Enviar texto como arquivo", colar texto > limite vira `message.txt`, ajustes de IME.

---

## 8. Perguntas em aberto

1. **Limite de caracteres:** 2000 (como o Discord) ou 4000 (já é o limite do banco)? Muda quando o contador aparece (≤ 200 ou ≤ 400 restantes).
2. **Anexos:** vamos guardar arquivos no disco do servidor (Oracle A1)? Qual limite por arquivo (10 MiB como o padrão do Discord, ou outro)? Começamos só com imagens ou aceitamos qualquer tipo (com `nosniff` e sem servir HTML)?
3. **GIF, figurinhas, presente:** ficam fora do v1 (botões ocultos), com só o botão de emoji? Quem faz o seletor de emoji (área separada) e o emoji colorido (fonte monocromática ou imagens)?
4. **Botão "Enviar":** o app hoje tem um botão "Enviar" visível; o Discord o esconde por padrão (só Enter). Manter visível ou seguir o Discord?
5. **Menções:** `@everyone`/`@here` existem sem cargos? Quem pode usar? `@cargo` fica para quando houver cargos?
6. **Texto colado acima do limite:** virar anexo `message.txt` (como o Discord) ou apenas recusar o envio com o diálogo?
7. **Tempo real do aviso de castigo e do modo lento:** o servidor (Postgres + processo único) pode guardar o "último envio por usuário e canal" só em memória (zera ao reiniciar)?
