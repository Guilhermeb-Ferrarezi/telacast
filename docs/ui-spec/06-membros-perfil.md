# 06 — Lista de membros, presença, perfil (popout e modal), menu de contexto do usuário

Fontes analisadas (somente leitura): `discord-bundle/web.0bcf9a9e83b2c3c7.js` (o maior; módulos citados por id webpack, ex. `#110574`) e o chunk `59aff6840c54dd99.js` (lista de membros de tópico, que reaproveita o mesmo layout). Strings pt-BR vêm de `12064a7d28fc7eb1.js`/`7d905c726a597bc4.js` (chaves tipo `WbGtnH`). Legenda: **[F]** confirmado no bundle, **[I]** inferência, **[?]** não consegui confirmar.

## 1. Resumo e escopo

O painel de membros do Discord é uma lista virtualizada de **linhas de altura fixa de 42 px**, tanto para cabeçalho de grupo quanto para membro **[F]**. O servidor manda só faixas de 100 linhas ao redor da rolagem, junto com a contagem de cada grupo, e empurra operações incrementais (SYNC/UPDATE/INSERT/DELETE/INVALIDATE) **[F]**. Cada membro cai em um grupo: cargo "hoist" mais alto (se online), senão "Disponível" (online sem hoist), senão "Offline"; dentro do grupo a ordem é por apelido/nome em minúsculas **[F]**. A linha mostra avatar 32 px com indicador de presença desenhado por máscara (formas distintas por estado, não só cor), nome na cor do cargo, selos (APP, coroa) e uma segunda linha com atividade ou status personalizado **[F]**. Clicar abre o cartão de perfil (popout) à esquerda do painel; o botão direito abre o menu de contexto; Shift+clique insere a menção no campo de texto **[F]**.

**Limite honesto da evidência.** (a) Os valores de CSS dos componentes (padding, raio, fonte em px, cores de hover) **não estão no cache**: só existem os nomes das classes (`K.Dc` etc.). Medidas vindas de JS (alturas de linha, tamanhos de avatar, banners, geometria dos indicadores) são **[F]**; o resto é **[I]** e está marcado. (b) O código do cartão de perfil completo, do modal e do menu de contexto do usuário está em chunks "lazy" que **não foram capturados**; só consegui os trechos que vazam para o `web.*.js` (dimensões do popout/modal, tipografia do bloco de nome, bio, status personalizado, enumerações de seções e de ações, menu de status). A composição desses três é **[I]** baseada nessas enumerações e em strings pt-BR.

**Fora do escopo** (existem no Discord, não vamos copiar agora): nameplates e decoração de avatar, tags de clã, efeitos de perfil, tema/cores de perfil Nitro, insígnias, boost (ícone de impulsionador), widgets (jogos favoritos, clipes), contas conectadas, "Ouvir junto", quests, modo Foco, multi-conta, status "via celular/VR", confete ao digitar, gradiente de cargo animado, fade nas bordas do scroller (é um gradiente; o Grafite proíbe).

## 2. Árvore de componentes e medidas

### 2.1 Painel de membros (`MemberPanel`)

```
Painel direito (L1)  largura 240 [I; hoje 220]
└─ Lista rolável (rolagem vertical, gutter estável [F "scrollbarGutter: stable"], paddingTop 0 [F])
   ├─ Cabeçalho de grupo ......... 42 px [F sectionHeight=42]
   ├─ Linha de membro ............ 42 px [F rowHeight=42]
   ├─ Linha-esqueleto ............ 42 px (membro ainda não carregado) [F]
   └─ Separador do grupo "unknown"  só um filete, sem texto [F]
```

Como cabeçalho e linha têm a mesma altura (42), **a lista inteira é uniforme** e a altura total é `42 * (grupos + membros)`; isso casa com `ScrollArea::show_rows` do egui.

**Cabeçalho de grupo [F texto, I resto]**
- Texto: `"{TÍTULO} — {contagem}"` com travessão cheio (`t," — ",i`) **[F]**. Título: nome do cargo, "Disponível" (`WbGtnH`), "Offline" (`Vv0abJ`), "Desconhecido" (`UQMV/E`). Caixa alta por CSS **[I]**; no Grafite use `caps()` (11 px, MUTED).
- Ícone do cargo de 16 px antes do título, se o cargo tem ícone/emoji **[F `size:16`]**; v2.
- Alinhamento: texto colado embaixo da linha de 42 (os ~24 px de cima são o respiro entre grupos) **[I]**, recuo esquerdo 16 **[I]**.
- Cabeçalho do grupo `unknown` (lista ainda não chegou): só o filete **[F]**.
- Contagens vêm do servidor; a lista de grupos é a "verdade" da altura total antes de qualquer membro carregar **[F `setGroups`: índice do grupo = soma das contagens anteriores + 1 por cabeçalho; `rows.length = total`]**.

**Linha de membro [F estrutura, I px]**
```
┌ margem 8 ───────────────────────────────────────────────── margem 8 ┐
│ [pad 8] [avatar 32] [gap 12] [coluna de texto ............] [selos] │ 42 px
│                              ├ linha 1: NOME (cor do cargo)  ┤       │
│                              └ linha 2: subtexto 12 px       ┘       │
└─────────────────────────────────────────────────────────────────────┘
```
- Slots do layout compartilhado `#193663`: `avatar`, coluna de texto com `name` + `decorators` na primeira linha e `subText` na segunda, `children` (área final, não usada em membros) **[F]**.
- Avatar: `SIZE_32` **[F]**, sempre com folga de 2 px para decoração de avatar (`MEMBER_LIST_ITEM_AVATAR_DECORATION_PADDING = 2px`) **[F]**; em Telacast sem decoração: ignorar.
- Indicador de presença no canto inferior direito: diâmetro 10, anel de recorte 3, deslocamento 0 **[F tabela `#778712`: 32 → status 10, stroke 3, offset 0]**. **Offline não tem indicador**: o código zera o status quando `status===OFFLINE` **[F `T=h?void 0:T`]**.
- Nome: uma linha com reticências, cor do cargo (ver 2.4). Tipografia em px **[?]**; Telacast: Inter 14.5 (Body), sem negrito (limitação do egui), diferenciar por cor.
- Subtexto: variante `body-xs/medium` **[F]**, ou seja ~12 px, uma linha, reticências, tooltip após **150 ms** se truncado **[F `delay:150`]**.
- Selos após o nome, nesta ordem **[F `#110574` componente `Z`]**: etiqueta de app (`APP`, com marca de verificado se app verificado), coroa de dono (`CrownIcon size "md"`, tooltip "Dono(a) do servidor" `pclUFJ`), ícone de impulsionador (tooltip "Impulsionando o servidor desde {data}").
- Etiqueta de app: texto `APP` (pt-BR `9RNkeF`), `aria-label` "App" (`qwJHjo`), "App Verificado" com check (`g76OcH`) **[F `#709066`]**. Tamanho **[?]**: Telacast ~15 px de altura, fonte 10, raio 3 **[I]**.
- Foco por teclado: anel com offset de 4 px nos quatro lados **[F `focusProps.offset`]**.

**Esqueleto [F `#295160`]:** círculo 32 + barra de texto com largura máxima aleatória de 40% a 80% (`floor(40*rand)+40`), com pulso (classe `lN`, desligada com `doNotAnimate`). No Telacast: formas estáticas em L3 (sem pulso, regra do projeto).

### 2.2 Cartão de perfil em popout (`UserPopout`)

Medidas confirmadas em `#207634` (especificação por layout) e `#518477` (`h=300`):

| Layout | avatar | offset X/Y do avatar | banner (L×A) | padding de tema |
|---|---|---|---|---|
| POPOUT (clicar em membro/avatar) | 80 | 10 / 4 | 300 × 105 | 2 (só perfil Nitro, senão 0) |
| MODAL | 120 | 16 / 5 | 600 × 210 | 0 |
| MODAL_V2 | 120 | 24 / 0 | 400 × 140 | 0 |
| SIDEBAR (painel lateral de DM) | 80 | 10 / 8 | 340 × 120 | 0 |

Todos **[F]**. Ainda existem `ACCOUNT_POPOUT` (o do próprio usuário, com menu de status), `ACTION_SHEET`, `YOU_SCREEN`, `EMBED` **[F enum]**. Largura do popout = 300 (banner = largura do cartão) **[F por dedução: `h=300` e banner 300]**.

Estrutura (de cima para baixo) **[I, com [F] onde indicado]**:
1. **Banner** 300×105: imagem; sem imagem, cor sólida = cor de destaque do perfil ou cor dominante do avatar **[F `#601231`: `primaryColor ?? dominante(avatar)`]**. Telacast: cor plana derivada de `user_tint(id)` (sem gradiente).
2. **Avatar 80** à esquerda (inset 10), com o centro perto da borda inferior do banner (sobe `offsetY` = 4); anel de recorte sobre o banner de raio `(80/2 + 6)` (stroke de 6 para tamanho 80 **[F tabela]**); indicador de presença de diâmetro 16 **[F]**. A posição exata do eixo vertical **[?]** (os campos são `insetStart/insetBottom` do recorte, sem o CSS).
3. **Barra de ícones no canto superior direito do banner** (insígnias, ações) **[I]**; Telacast: botão "⋯" (`UKOtz+` "Mais" **[F]**).
4. **Bolha de status personalizado** ao lado do avatar **[F `#394816`]**: balão com emoji + texto; fechado mostra uma linha truncada; ao passar o mouse expande (animado) e mostra chevron (tooltips "Expandir/Recolher status personalizado" `xPkLPy`/`fFaN1b`). No perfil de outra pessoa, a barra de ferramentas do balão oferece reagir ("Envie a reação" `nhaI4b`) e responder ("Responder" `RmDYKK`); no seu próprio, editar ("Editar" `bt75uw`) e limpar ("Limpar" `VkKicb`). Sem status: o dono vê "Adicionar status" (`evw0oz`).
5. **Bloco de nome** **[F `#465829`]**: nome de exibição em `heading-lg/semibold` (popout) ou `heading-xl/semibold` (modal) + etiqueta APP ao lado; abaixo, nome de usuário (`@username`), pronomes ("Pronomes: {x}", `body-sm/medium`, separados por um traço) e tag de clã (fora de escopo). Nome clicável no modal.
6. **Seções** (cada uma com título `body-xs/semibold` em `text-default`, ícone opcional **[F `#900179`]**):
   - **Sobre mim** (`61W33d`): no popout o título é escondido (`hideHeading`) **[F `#442228`]**; texto `body-sm/normal`. Altura máxima do trecho ≈ **57,75 px** (cerca de 3 linhas); se estourar, aparece o botão **"Ver Biografia Completa"** (`YDiPq8`) que fecha o popout e abre o modal **[F]**. No modal a bio vai até 6 linhas (`lineClamp = 6`) **[F `#308244`]**.
   - **Atividade atual** (`J6STd9`): cartões (transmissão, atividade, voz) e link **"Ver todas as atividades"** (`pD1L1u`) que abre o modal na aba Atividade; a seção some se o usuário está offline **[F `#646986`]**.
   - **Membro desde** (`AOcwWB`): data de criação da conta e data de entrada no servidor **[I]**.
   - **Cargos** (`2SZsWX`): chips; expandir/recolher (`EXPAND_ROLES`/`COLLAPSE_ROLES`) e chip "+ Adicionar cargo" (`ljnBlo`) com permissão **[F strings e ações]**.
   - **Nota** (`PbMNh2`): campo privado, placeholder "Clique para adicionar uma nota" (`VBhOe2`), evento de gateway `USER_NOTE_UPDATE` **[F]**.
   - Enumerações das seções do popout: `ROLES, CONNECTIONS, APPS, NOTE, RECENT_ACTIVITY` **[F `#518477` `m`]**.
7. **Ações**: botão principal "Mensagem" (`BAB0yK`), amizade (Adicionar amigo / Aceitar / Ignorar / Cancelar pedido / Remover amigo: todas existem como ações **[F `S` enum]**), "⋯" abre o menu de contexto **[I]**.
8. Erro parcial: faixa de aviso "Não foi possível carregar partes do perfil" (`L9wE7H`) quando a busca do perfil falha mas o usuário básico existe **[F `#559506`]**.

**Posicionamento [F para a lista de tópico, I para a de servidor]:** `position: "left"`, `spacing: 16` (o cartão abre à esquerda da lista, com 16 px de folga); alinhado ao topo da linha; fecha ao clicar fora (`onRequestClose`) e com Esc; um clique na linha já selecionada fecha (toggle) e o `mouseDown` em linha selecionada não propaga para não fechar e reabrir **[F]**. Sem animação de entrada **[F para o popout de quests; I para este]**.

### 2.3 Perfil completo em modal (`UserProfileModal`)

- Abre por `openUserProfileModal({userId, guildId, tabSection, ...})`, que dispara `GET /users/{id}/profile?type=modal&with_mutual_guilds=true&with_mutual_friends_count=true&guild_id=…` **[F `#402860`, `#731078`]**; mostra o usuário já em cache e completa depois.
- Banner 600×210, avatar 120 em (16,5), indicador diâmetro 24 / anel 8 / offset 8 **[F]**. Variante V2: banner 400×140, avatar (24,0).
- Abas **[F enum `p`]**: `USER_INFO` (padrão), `ACTIVITY` ("Atividade"), `MUTUALS` → `MUTUAL_GUILDS` ("Servidores em comum" `sySsXR`) e `MUTUAL_FRIENDS` ("Amigos em comum" `afBKs5`); para bots `BOT_INFO`. `WIDGETS`/`WISHLIST`/`FEATURED` fora de escopo. A aba pode ser pedida na abertura (`tabSection`) e há `scrollTarget` **[F]**.
- Lista de ações de perfil que o cliente rastreia (serve de inventário do menu/modal): VER perfil principal/do servidor, mensagem direta, pedido/aceitar/ignorar/cancelar amizade, remover amigo, **definir apelido de amigo**, bloquear/desbloquear, ignorar/parar de ignorar, denunciar, copiar nome de usuário, copiar ID, **convidar para servidor**, adicionar nota, volume do usuário, ligar/vídeo, silenciar/silenciar no servidor/ensurdecer/desconectar/mover de canal, **adicionar/remover cargo**, **aplicar/remover castigo, expulsar, banir**, "visão de moderação" **[F enum `S`]**.

### 2.4 Cor e estilo do nome

- Cor do nome = cor do **cargo de maior posição com `color > 0`**; grupo = cargo `hoist` de maior posição; ícone = cargo de maior posição com ícone/emoji **[F `#…` função `Y(guildRoles, memberRoleIds)`]**. Comparação: posição, depois id (`m.zA`).
- Existe a configuração de acessibilidade **`roleStyle`**: `"username"` (padrão, nome colorido) ou `"dot"` (bolinha colorida antes do nome, nome com a cor normal) **[F `#463930`]**. Implementar as duas; padrão = nome colorido.
- Cargos com gradiente (`colorStrings.primaryColor/secondaryColor`) animam no hover **[F]**: fora de escopo (cor primária só).
- Nome exibido: `nick` do servidor, senão nome global, senão `username` **[F `#427262 u()`]**. Em DM/lista de amigos: apelido de amigo, depois nome global, depois username **[F `#562153`]**. A ordenação usa esse nome em minúsculas **[F]**.

### 2.5 Menu de contexto do usuário

Código em chunk não capturado (`#668569` genérico e `#202643` para lista de tópico). Montado a partir das ações e strings **[F strings, I ordem]**:

| Grupo | Itens (pt-BR do bundle) | Condição |
|---|---|---|
| 1. Navegação | **Perfil** (`s+YSgo`) · **Mencionar** (`P8tvKG`) · **Mensagem direta** (`EJRzg7`) · **Chamar** (`JJogjm`) · **Adicionar nota** (`Dglxrf`) | Mensagem/Chamar/Nota: não em si mesmo |
| 2. Relação | **Adicionar amigo** (`w5uwoI`) ou **Desfazer amizade** (`cvSt1J`, vermelho) ou **Cancelar pedido de amizade** (`bTfA//`) · **Convidar para o servidor** ▸ (`Sd8Ixw`) · **Ignorar** (`SlM9wZ`) · **Bloquear** (`l4Emac`, vermelho) / **Desbloquear** (`Hro40y`) | não em si mesmo |
| 3. Servidor | **Alterar apelido** (`dilOF6`) · **Gerenciar cargos** ▸ (`KXcEC9`) · **Mover para** ▸ (`FAplms`, só em voz) | apelido: próprio com CHANGE_NICKNAME, outros com MANAGE_NICKNAMES; cargos: MANAGE_ROLES |
| 4. Moderação (vermelho) | **Castigo** ▸ (`Nd4BcM`) · **Expulsar usuário** (`03N0UL`) · **Banir usuário** (`HWuskv`) | KICK_MEMBERS / BAN_MEMBERS / MODERATE_MEMBERS e hierarquia de cargos |
| 5. Utilitário | **Copiar ID do usuário** (`/AXYnE`) | no Discord só com modo desenvolvedor; Telacast: sempre |
| Voz (outro doc) | **Volume do usuário** (`m7TNdF`, slider), **Silenciar**, **Ensurdecer**, **Desconectar** | em contexto de voz |

Itens destrutivos (`color: "danger"` no componente de item **[F `{id:"remove-friend", color:"danger"}`]**): remover amigo, bloquear, expulsar, banir, castigo. Separadores entre grupos **[F]** (`ev.bX`). Medidas do menu **[?]**; Telacast: largura 200, item 30 px, raio 10 (já é `menu_corner_radius`), hover L4, perigo em RED.

### 2.6 Menu de status e status personalizado (painel do usuário)

Menu do popout da conta **[F `~#6937200`]**:
- **Disponível** (sem submenu) · separador · **Ausente** (`qWbtVU`) · **Não perturbar** (`jaNpQH`; subtexto "Você não receberá notificação na área de trabalho" `tq/fMK`) · **Invisível** (`bg24HO`; subtexto "Você vai aparecer Off-line" `zPc6Mc`).
- Ausente/Não perturbar/Invisível têm submenu de duração: **Por 15 minutos, Por 1 hora, Por 8 horas, Por 24 horas, Por 3 dias, Para sempre** **[F]**. Escolher uma duração agenda o retorno automático para Disponível (`manageExpiringStatus`); depois de escolhido o subtexto vira "Até 14:30" (hoje) ou "Até {data} {hora}" **[F `ZxxHIO`/`9OFjSe`]**.
- Cada item mostra o indicador do estado (diâmetro 10) à esquerda **[F `leadingAccessory: status`]**.
- Estando invisível, o painel do usuário mostra "Status: Invisível" com um ícone de aviso e tooltip "Atenção! Seu status e atividades permanecerão ocultos se você estiver invisível" (`L99HQm`) **[F `#n7`]**.
- Status personalizado: entra como atividade tipo `CUSTOM_STATUS(4)` com `state` (texto), `emoji`, `timestamps.end` (expiração) **[F `#685396`]**. Um temporizador no cliente limpa o status quando `expiresAtMs` passa **[F `manageExpiringCustomStatus`]**. Opções de "Limpar depois de": Não limpar, Hoje, 4 horas, 1 hora (strings `bRn8cq`, `GQmLrZ`, `5CNt/M`, `GA/d4I` **[F]**; "30 minutos" **[I]**). Texto máx. 128 caracteres **[I]**.

## 3. Estados e interações

### 3.1 Presença: formas, cores, tempos

Geometria dos indicadores (de `#935154`, em unidades de `s` = diâmetro, 10 px no avatar 32) **[F]**. O indicador ocupa o quadrado `s×s` do canto inferior direito do avatar; atrás dele o avatar é recortado por um círculo de raio `s/2 + stroke`.

| Estado (`clD`) | Forma | Cor Discord **[F]** | Cor Telacast (proposta) |
|---|---|---|---|
| Disponível | círculo cheio | `#45a366` | `OK` `#74b98f` |
| Ausente | círculo com recorte circular deslocado (lua): centro `(-0,25s, -0,25s)` do centro do indicador, raio `0,375s` | `#ffc04e` | ocre `#c9a45c` (decisão, ver §8) |
| Não perturbar | círculo com barra horizontal vazada `0,75s × 0,25s`, cantos `0,125s` | `#da3e44` | `RED` `#d9806f` |
| Invisível / Offline | anel: círculo com furo central de raio `0,25s` | `#84858d` | `MUTED` `#8a8882` |
| Transmitindo | círculo com triângulo vazado (`±0,2165s × ±0,25s`, centro em `0,5625s`) | `#9147ff` (Twitch) | `AMBER` (ao vivo) |
| Digitando | pílula `2,5s × s`, com pontos animados | n/d | fora de escopo |
| Online no celular | retângulo `s × 1,5s` com tela vazada e ponto | `#45a366` | fora de escopo |

- Quem enxerga "Invisível": só o próprio usuário. Para os outros é `offline` (a máscara de INVISIBLE é a mesma de OFFLINE) **[F `#935154 g()` + docs/05]**. Servidor deve mascarar.
- Nome do estado no tooltip do indicador: `humanizeStatus` ("Disponível", "Offline", "Ausente", "Não perturbar", "Invisível", "Transmitindo") com atraso de 150 ms **[F `#427262 h()`, `statusTooltip`]**; `aria-label` "{nome}: {estado}" (`/6mw10`).
- Transição de forma ao trocar de estado: mola com `tension 1200`, `friction 70` **[F `S`]** → amortecimento crítico, ~130–150 ms; Telacast: interpolar o estado com `animate_value_with_time(..., 0.14)`; se for trabalhoso, troca seca.
- Transmitindo tem prioridade sobre qualquer status quando há atividade de transmissão **[F `(0,m.A)(activities) ? STREAMING : status`]**. No Telacast: se `user_id` está em `snap.live`.

### 3.2 Estados da linha de membro

| Estado | Visual |
|---|---|
| Normal | fundo transparente |
| Hover | fundo `lerp(L1, L2, t)`, t em 120 ms (igual ao `list_row` atual) **[I]**; avatar animado e gradiente do cargo só no hover **[F]** (fora de escopo) |
| Selecionado (popout aberto) | fundo L3 **[F `selected`/classe `PJ`]** |
| Offline | linha atenuada (classe `WK` quando `status==OFFLINE && !selected`): avatar alpha ~0,5 e nome MUTED; hover e selecionado voltam ao normal **[F existência da classe, I valores]** |
| Foco de teclado | anel âmbar 2 px com raio 8 **[F offset 4]** |
| Bloqueado/ignorado | não mostra subtexto de atividade **[F `isBlockedOrIgnored → null`]** |
| Usuário sem acesso ao canal | tooltip "Não é possível ver este tópico" (`/QcoTz`) **[F, só tópicos]** |
| Carregando | linha-esqueleto |
| Lista vazia | no tópico: título "Membros", ícone de grupo e "Não há ninguém neste tópico." (`9/n5vz`) **[F]**; Telacast: texto simples |

### 3.3 Subtexto (segunda linha) — regras exatas **[F `#394871`]**
1. Offline ou invisível: **sem subtexto** (`#397244` devolve falso) ; atividades `HANG_STATUS` são ignoradas.
2. Ordem visual: `[atividade] • [status personalizado]` (a bolinha "•" só aparece se existem os dois).
3. Com status personalizado **com texto**, a atividade vira só o ícone (texto escondido) e o status mostra `emoji + texto`.
4. Status só com emoji: mostra o emoji, a atividade mantém o texto.
5. Mais de uma atividade (stream + jogos + voz): mostra a primeira e um "+N" (N = total − 1; cor "positiva" quando há texto de status); tooltip lista todas. Ícones: controle de jogo (jogando/competindo), fones (ouvindo), TV (assistindo/transmitindo), de app oculto para bots.
6. O texto da atividade é só o **nome** ("Minecraft"); o prefixo "Jogando" fica no tooltip (`HBJiRR`). Ouvindo: título/artistas. Voz sem outra atividade: "Em voz" (`msxteM`), palco "No palco" (`QygGCN`), DM/grupo "Em uma chamada" (`9FaEzi`). Compartilhando tela: "Compartilhando tela" (`eXan7B`).
7. Texto truncado ganha tooltip após 150 ms.

### 3.4 Presença automática **[F `#885576`]**
- Ausente após **10 min** (`6e5` ms) sem atividade; verificação a cada 30 s na web (10 s no desktop, via tempo ocioso do SO).
- Contam como atividade: `mouseup`, `wheel`, `keypress` (debounce de 500 ms), trocar de canal, foco da janela, falar em voz. **Mover o mouse não conta** **[F]**.
- Suspender o computador ou travar a tela força ausente na hora; ao voltar, volta a "disponível".
- Só vale se o usuário não escolheu manualmente outro estado **[I]**.

### 3.5 Tempos conhecidos
Tooltip 150 ms **[F]**; mola de status ~140 ms **[F]**; esvaziar assinaturas de membros após **60 s** sem uso **[F `A=+MINUTE`]**; assinatura de lista com debounce de 0 ms (`J_(0)`) **[F]**; hover da linha 120 ms **[I]**.

## 4. Regras de comportamento

**Agrupamento e ordem [F `#963307`, `#834058`]**
1. Grupos: cargos `hoist` por posição (maior primeiro), depois "Disponível", depois "Offline". Um membro offline/invisível vai para "Offline" **mesmo se tiver cargo hoist**. Um membro online sem hoist vai para "Disponível".
2. Dentro do grupo: ordem por apelido/nome em minúsculas, desempate por id. Nome sem valor vem antes.
3. Mudança de status/cargo/apelido move o membro entre grupos (`updateUserId`: remove, insere na posição ordenada); mais de 50 mudanças de uma vez reconstroem a lista inteira.
4. Membros que não enxergam o canal não entram (a lista é por canal) **[F `VIEW_CHANNEL` no id da lista]**.

**Qual lista**: o id da lista é `"everyone"` se o canal é visível a todos, senão um hash (SHA-1) dos overwrites `allow:`/`deny:` que concedem/negam VIEW_CHANNEL, ordenados; canais com a mesma visibilidade compartilham a lista **[F `b()` em `#963307`]**.

**Interação**
- Clique: abre/fecha o popout (toggle). Shift+clique: insere `@nome` no campo de texto (texto bruto `<@id>`), foca o campo e dispara "digitando" **[F]**.
- Botão direito: menu de contexto do usuário.
- Teclado: navegador de lista com setas (foco move linha a linha), Home/End rolam ao início/fim, a primeira linha rola com folga de 42 px para o cabeçalho aparecer **[F]**. Enter abre o popout **[I]**.
- Mostrar/ocultar painel: botão no cabeçalho do canal, "Mostrar lista de membros"/"Ocultar lista de membros" (`gxChDx`/`Axvx8c`) **[F]**; estado persistido por usuário **[I]**; esconder sozinho abaixo de ~900 px de largura da janela **[I]**.
- DM em grupo: mesma lista, com um único cabeçalho "Participantes — N" (`e95u3C`) **[F string]**.

**Muitos membros — virtualização e janelas [F]**
- Render: lista em "chunks" de 256 px; só desenha as linhas dos chunks visíveis.
- Gateway: o cliente pede faixas `[[0,99],[100,199],…]` de **100 linhas** (`JM=100`), cobrindo a janela visível mais **meia altura de viewport** acima e abaixo (`ceil(0,5*altura/42)` linhas de folga); faixa inicial `[[0,99]]`; no máximo **5 canais** assinados por servidor (LRU) **[F `#36124`]**.
- Mensagem de assinatura por servidor: `{typing, threads, activities, members:[ids], member_updates, channels:{canal:[[a,b],…]}, thread_member_lists}` enviada com `flush` imediato e mesclada **[F `class g`]**. Assinar presença de usuários específicos (cartão, autocompletar menção): `members:[ids]`, aviso se passar de 50 ids; desassinar com atraso de 60 s **[F]**.
- Eventos de volta: `SYNC{range, items}`, `UPDATE{index,item}`, `INSERT{index,item}`, `DELETE{index}`, `INVALIDATE{range}` mais `groups[{id,count}]`, `member_count`, `online_count` **[F]**. Linhas fora das faixas carregadas são `undefined` e aparecem como esqueleto.
- Autocompletar `@` assina `[[0,99]]` do canal se ainda não há lista **[F `#486319`]**.

**Perfil**
- Buscar sempre ao abrir (mostrando o cache na hora). O cache é invalidado por eventos (atualização do usuário/membro, entrada/saída, mudança de relação), não por TTL **[F `#8513213` store]**.
- Ações do popout/modal usam as mesmas funções do menu de contexto.
- Nota: privada, salva ao sair do campo **[I]**, evento `USER_NOTE_UPDATE` sincroniza entre sessões **[F]**.
- Apelido de servidor: vazio = remover. Apelido de amigo: privado do dono **[F `getNickname` em relacionamentos]**.

## 5. Dados necessários (protocolo e banco)

Estado atual: `User{id,name,online}`, `ServerInfo.members: Vec<User>` inteiro no `Snapshot`, nada de cargos, status, perfil, nota.

**P0 (necessário para a lista decente)**
1. `proto::Status { Online, Idle, Dnd, Invisible }` + `User.status` (o servidor nunca manda `Invisible` a terceiros: manda `Offline`). Manter `online: bool` por compatibilidade ou substituir por `status`.
2. `User { username, display_name, is_bot }` (hoje só `name`).
3. `C2S::SetStatus { status, until: Option<i64> }`, `C2S::SetIdle { idle: bool }` (o servidor só aplica se a preferência é Online); `S2C::PresenceUpdate { user_id, status, custom: Option<CustomStatus>, activity: Option<Activity> }` (fan-out para membros de servidores em comum e amigos).
4. `Member { user_id, nick: Option<String>, roles: Vec<i64>, joined_at }` e `Role { id, name, color: u32, hoist: bool, position: i32, icon: Option<String> }`; `S2C::RoleUpdate`, `S2C::MemberUpdate { server_id, member }`, `MemberAdd/Remove`. (Cargos: ver docs/discord/02 §tabelas `roles`, `member_roles`.)

**P1**
5. Perfil: `C2S::GetProfile { user_id, server_id: Option<i64>, mutuals: bool }` → `S2C::Profile { user, member: Option<Member>, bio, pronouns, banner_color, created_at, mutual_servers: Vec<(id,nome)>, mutual_friends: Vec<User>, note, relation: None|Friend|Incoming|Outgoing|Blocked }` (espelha `GET /users/{id}/profile` **[F]**).
6. `C2S::SetCustomStatus { text, emoji, expires_at }`, `C2S::SetNote { user_id, note }` → `S2C::NoteUpdate`, `C2S::SetNick { server_id, user_id, nick }`, `C2S::SetFriendNick { user_id, nick }`.
7. Moderação (depende de permissões): `KickMember`, `BanMember`, `TimeoutMember { until }`, `SetMemberRoles`. `C2S::BlockUser/UnblockUser` (tabela `blocks` já existe) e "ignorar" (só cliente, lista local).
8. Lista paginada: `C2S::SubscribeMembers { server_id, channel_id, ranges: Vec<(u32,u32)> }`, `S2C::MemberList { server_id, list_id, groups: Vec<Group{id,count}>, ops: Vec<Op>, member_count, online_count }` com `Op { Sync{range,items}, Update{index,item}, Insert, Delete, Invalidate }`, item = `Group{id,count}` ou `Member`. Só necessário passando de ~1000 membros; até lá, manter o `Snapshot` completo e só virtualizar a renderização.

**Colunas/tabelas novas (esboço)**
```sql
ALTER TABLE users
  ADD COLUMN is_bot       boolean  NOT NULL DEFAULT false,
  ADD COLUMN bio          text     CHECK (char_length(bio) <= 190),        -- limite [I]
  ADD COLUMN pronouns     text     CHECK (char_length(pronouns) <= 40),   -- [I]
  ADD COLUMN banner_color integer,
  ADD COLUMN status_pref  smallint NOT NULL DEFAULT 0,  -- 0 online, 1 ausente, 2 não perturbar, 3 invisível
  ADD COLUMN status_until timestamptz,
  ADD COLUMN custom_text  text     CHECK (char_length(custom_text) <= 128), -- [I]
  ADD COLUMN custom_emoji text,
  ADD COLUMN custom_until timestamptz;
CREATE TABLE roles (id bigint PRIMARY KEY, server_id bigint NOT NULL REFERENCES servers ON DELETE CASCADE,
  name text NOT NULL, color integer NOT NULL DEFAULT 0, hoist boolean NOT NULL DEFAULT false,
  position integer NOT NULL, icon text, permissions bigint NOT NULL DEFAULT 0, managed boolean NOT NULL DEFAULT false);
CREATE TABLE member_roles (server_id bigint, user_id bigint, role_id bigint REFERENCES roles ON DELETE CASCADE,
  PRIMARY KEY (server_id, user_id, role_id));
CREATE TABLE user_notes (owner_id bigint, target_id bigint, note text NOT NULL CHECK (char_length(note) <= 256), PRIMARY KEY (owner_id, target_id));
ALTER TABLE friendships ADD COLUMN requester_nick text, ADD COLUMN target_nick text;
```
`members.nickname` já existe. Presença efêmera (online/ausente/sessões) fica **em memória** no servidor (mapa `user_id → Presence`, contagem de sessões); só `status_pref/custom_*` persistem. Índice de ordenação da lista: manter na memória, por servidor, um `BTreeMap<(grupo, nome_minúsculo, id), user_id>` reconstruído na partida; evita `ORDER BY lower(coalesce(...))` a cada consulta.

## 6. Plano de implementação em egui

**Em `theme.rs` (novo ou ajustado):** `Status` + `status_color` + `badge_spec(size)` + `paint_status(...)` + `avatar_with_status(...)` (substitui o `avatar(... online: Option<bool> ...)`, mantendo o antigo como atalho); `one_line(ui, texto, fonte, cor, largura)` com reticências; `crown(painter, centro, cor)`; `chip(ui, texto, cor)`; `menu_item(ui, rótulo, perigo) -> Response`; `menu_sep(ui)`.
**Em `main.rs`:** reescrever `members_panel` (hoje filtra duas vezes por frame, linhas 682–715), novo `MemberRows` em cache, `user_popout`, `user_context_menu`, `status_menu` (no rodapé do `channel_panel`, onde hoje há o texto fixo "Online" na linha ~668), `profile_modal`, `IdleTracker`.

### 6.1 Indicador de presença (trecho 1)
```rust
// theme.rs
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status { Online, Idle, Dnd, Offline }

pub const IDLE: Color32 = Color32::from_rgb(0xc9, 0xa4, 0x5c); // ocre; ver pergunta 1

pub fn status_color(s: Status) -> Color32 {
    match s { Status::Online => OK, Status::Idle => IDLE, Status::Dnd => RED, Status::Offline => MUTED }
}

/// (diâmetro do indicador, espessura do anel de recorte, recuo para dentro). Tabela do bundle (#778712).
pub fn badge_spec(avatar: f32) -> (f32, f32, f32) {
    match avatar.round() as u32 {
        0..=20 => (6.0, 2.0, 0.0),   // 16, 20
        21..=24 => (8.0, 3.0, 0.0),
        25..=32 => (10.0, 3.0, 0.0),
        33..=48 => (12.0, 4.0, 0.0), // 40, 44, 48
        49..=56 => (14.0, 4.0, 2.0),
        57..=80 => (16.0, 6.0, 4.0), // 72, 80
        81..=96 => (20.0, 8.0, 6.0),
        97..=120 => (24.0, 8.0, 8.0),
        _ => (30.0, 10.0, 10.0),
    }
}

/// `c` = centro do indicador, `s` = diâmetro, `hole` = cor REAL do fundo sob o avatar (inclui o hover da linha).
pub fn paint_status(p: &egui::Painter, c: Pos2, s: f32, st: Status, hole: Color32) {
    p.circle_filled(c, s * 0.5, status_color(st));
    match st {
        Status::Online => {}
        Status::Idle => p.circle_filled(c + vec2(-0.25 * s, -0.25 * s), 0.375 * s, hole), // lua
        Status::Dnd => {
            let r = ((0.125 * s).round() as u8).max(1);
            p.rect_filled(Rect::from_center_size(c, vec2(0.75 * s, 0.25 * s)), egui::CornerRadius::same(r), hole);
        }
        Status::Offline => p.circle_filled(c, 0.25 * s, hole), // anel
    }
}

pub fn avatar_with_status(ui: &mut Ui, name: &str, id: i64, size: f32, st: Option<Status>, hole: Color32, dim: f32) -> Rect {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let p = ui.painter();
    let (bg, fg) = user_tint(id);
    p.circle_filled(rect.center(), size / 2.0, bg.gamma_multiply(dim));
    let ch = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    p.text(rect.center(), Align2::CENTER_CENTER, ch, FontId::proportional(size * 0.4), fg.gamma_multiply(dim));
    if let Some(st) = st {
        let (s, stroke, off) = badge_spec(size);
        let c = rect.right_bottom() - vec2(s / 2.0 + off, s / 2.0 + off);
        p.circle_filled(c, s / 2.0 + stroke, hole);      // recorte no avatar
        paint_status(p, c, s, st, hole);
    }
    rect
}
```
Armadilhas: (a) o recorte é pintado com a cor do fundo, então precisa receber a cor **atual** da linha (hover/selecionado), não `SIDE`; (b) o círculo do recorte sai 3 px do quadrado do avatar: pintar depois do texto/linha vizinha não importa, mas **antes** do próximo widget para não ser coberto; (c) `rect_filled` com `CornerRadius::same(u8)` não aceita fração: arredondar e travar em 1; (d) com `pixels_per_point` fracionário o indicador de 10 px fica borrado: alinhar o centro a meio pixel, `c = (c * ppp).round() / ppp`.

### 6.2 Painel virtualizado (trecho 2)
```rust
const ROW_H: f32 = 42.0;

enum RowKey { Header { gi: usize }, Member { gi: usize, mi: usize }, Skeleton(f32) }

struct MemberRows { version: u64, server: i64, rows: Vec<RowKey>, groups: Vec<Group> } // cache; só reconstrói quando `version` muda

fn members_panel(&mut self, ui: &mut egui::Ui, s: &Session, server: i64, acts: &mut Vec<Act>) {
    let Some(sv) = s.snap.servers.iter().find(|x| x.id == server) else { return };
    self.rows.ensure(sv, &s.presence, &s.roles);             // O(n log n) só quando algo mudou
    let rows_len = self.rows.rows.len();
    let pr = egui::Panel::right("members")
        .frame(egui::Frame::new().fill(SIDE))
        .resizable(false)
        .exact_size(240.0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;           // ANTES do show_rows: ele soma o spacing à altura
            egui::ScrollArea::vertical()
                .id_salt(("members", server))                 // um scroll por servidor
                .auto_shrink([false, false])
                .show_rows(ui, ROW_H, rows_len, |ui, range| {
                    for i in range {
                        match self.rows.rows[i] {
                            RowKey::Header { gi } => group_header(ui, &self.rows.groups[gi]),
                            RowKey::Member { gi, mi } => {
                                let m = &self.rows.member(gi, mi);
                                let sel = self.popout.as_ref().is_some_and(|p| p.user == m.user_id);
                                let r = member_row(ui, m, sel);
                                if r.clicked() { /* toggle popout; shift => insere menção */ }
                                r.context_menu(|ui| user_menu(ui, m, acts));
                            }
                            RowKey::Skeleton(f) => skeleton_row(ui, f),
                        }
                    }
                });
        });
    edge(ui, pr.response.rect, false);
}
```
Armadilhas: `show_rows` assume altura uniforme; por isso o cabeçalho **também** tem 42 px (no Discord são 42 e 42 **[F]**). Para teste de carga, o mock deve gerar 5 000 membros (não rodar build com o jogo aberto). Para ordenação e agrupamento use a regra de §4; o `Vec<RowKey>` guarda só índices, não referências, para não brigar com o borrow checker.

Linha de membro (núcleo, pintura direta em vez de `list_row`, para controlar o recorte):
```rust
fn member_row(ui: &mut Ui, m: &MemberView, selected: bool) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
    let hov = anim(ui, resp.id.with("h"), resp.hovered() && !selected, 0.12);
    let inner = rect.shrink2(vec2(8.0, 1.0));
    let fill = if selected { L3 } else { lerp(L1, L2, hov) };
    if selected || hov > 0.0 { ui.painter().rect_filled(inner, egui::CornerRadius::same(8), fill); }
    let off = m.status == Status::Offline;
    let dim = if off && !selected { 0.5 + 0.5 * hov } else { 1.0 };

    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner.shrink2(vec2(8.0, 0.0)))
        .layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.spacing_mut().item_spacing.x = 12.0;
    avatar_with_status(&mut child, &m.name, m.user_id, 32.0, (!off).then_some(m.status), fill, dim);

    // coluna de texto: nome (+selos) e subtexto
    let text_left = child.cursor().left();
    let max_w = inner.right() - 8.0 - text_left;
    let name_col = if m.color.is_some() && !off { m.color.unwrap() } else if off { MUTED } else { TEXT };
    let two = m.subtext.is_some() && !off;
    let top = inner.center().y - if two { 18.0 } else { 9.0 };
    let g = one_line(ui, &m.name, FontId::proportional(14.5), name_col.gamma_multiply(dim), max_w - decorators_w(m));
    let gw = g.size().x;
    ui.painter().galley(pos2(text_left, top), g, name_col);
    let mut x = text_left + gw + 6.0;
    if m.is_bot { x += app_tag(ui.painter(), pos2(x, top + 2.0)) + 4.0; }
    if m.is_owner { crown(ui.painter(), pos2(x + 8.0, top + 9.0), crown_color()); }
    if two { /* subtexto: 12 px, TEXT2/MUTED, ícone + nome da atividade • status */ }
    resp
}
```

### 6.3 Popout ancorado e fechamento (trecho 3)
```rust
struct Popout { user: i64, anchor: Rect, opened_pass: u64 }

fn user_popout(&mut self, ctx: &egui::Context, panel_left: f32) {
    let Some(po) = &self.popout else { return };
    const W: f32 = 300.0;
    let pos = pos2(panel_left - 16.0 - W, po.anchor.top());        // esquerda do painel, 16 px de folga [F spacing:16]
    let area = egui::Area::new(egui::Id::new("user_popout"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .constrain(true)                                           // mantém dentro da janela (desce/sobe se faltar espaço)
        .show(ctx, |ui| { theme::glass(12).show(ui, |ui| { ui.set_width(W); self.profile_card(ui, po.user); }); });
    let inside = area.response.rect.contains(ctx.input(|i| i.pointer.latest_pos()).unwrap_or(Pos2::ZERO));
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let outside_click = ctx.input(|i| i.pointer.any_click()) && !inside && ctx.cumulative_pass_nr() != po.opened_pass;
    if esc || outside_click { self.popout = None; }
}
```
Armadilhas: o clique que abre o popout é o mesmo que seria visto como "fora" na passada seguinte; por isso `opened_pass`. Se o clique é em **outra linha**, ela chama `self.popout = Some(...)` na mesma passada: ordem de execução = painel primeiro, popout depois, e a guarda por `opened_pass` evita fechar o que acabou de abrir (conferir o nome `cumulative_pass_nr` no egui 0.37; pode ser `frame_nr` em versões antigas). `Area::constrain(true)` limita ao `screen_rect`, mas não "vira" para a direita se não couber à esquerda: calcular antes, caso o painel esteja escondido, e usar `pos2(anchor.right()+8, …)`. Se o `egui::Popup` do 0.37 oferecer `close_behavior` suficiente, preferir ele: não verifiquei a API.

Cartão (banner + avatar sobreposto):
```rust
struct CardSpec { w: f32, banner_h: f32, avatar: f32, ax: f32, ay: f32 }
const POPOUT: CardSpec = CardSpec { w: 300.0, banner_h: 105.0, avatar: 80.0, ax: 10.0, ay: 4.0 };   // [F]
const MODAL:  CardSpec = CardSpec { w: 600.0, banner_h: 210.0, avatar: 120.0, ax: 16.0, ay: 5.0 };  // [F]

fn card_header(ui: &mut Ui, sp: &CardSpec, tint: Color32, name: &str, id: i64, st: Status) -> f32 {
    let (rect, _) = ui.allocate_exact_size(vec2(sp.w, sp.banner_h), Sense::hover());
    ui.painter().rect_filled(rect, egui::CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }, tint);
    let c = pos2(rect.left() + sp.ax + sp.avatar / 2.0, rect.bottom() - sp.ay);   // centro na borda inferior [I]
    let (_, stroke, _) = badge_spec(sp.avatar);
    ui.painter().circle_filled(c, sp.avatar / 2.0 + stroke, L3);                  // anel de recorte sobre o banner
    paint_avatar_at(ui.painter(), c, sp.avatar, name, id, Some(st), L3);
    sp.avatar / 2.0 - sp.ay + 8.0   // altura a reservar abaixo do banner antes do nome
}
```

### 6.4 Ausente automático robusto (trecho 4)
O `update` do egui não roda com a janela minimizada/escondida; então o relógio de ociosidade deve viver na **thread do gateway** (`gateway.rs`, que já faz laço de 50 ms):
```rust
// compartilhado entre UI e gateway
pub struct Activity(pub std::sync::Arc<std::sync::atomic::AtomicI64>); // ms desde a época do último input

// UI (a cada frame, barato):
let active = ctx.input(|i| {
    i.pointer.any_released() || i.raw_scroll_delta != egui::Vec2::ZERO
        || i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. } | egui::Event::Text(_) | egui::Event::WindowFocused(true)))
});
if active { self.activity.0.store(now_ms(), std::sync::atomic::Ordering::Relaxed); }

// gateway.rs, no laço:
let idle = now_ms() - activity.0.load(Relaxed) > 10 * 60 * 1000;
if idle != self.idle_sent { out.send(C2S::SetIdle { idle })?; self.idle_sent = idle; }
```
Regras iguais ao Discord **[F]**: 10 min; mover o mouse não conta (por isso `any_released`, não `any_moved`). O servidor ignora `SetIdle` se `status_pref != Online`. Armadilha: ao voltar do suspend, o primeiro input zera o contador; o servidor trata `idle=false` como disponível imediatamente.

### 6.5 Outras armadilhas do egui
- Sem negrito em fonte variável (Inter): hierarquia por tamanho e cor (nome TEXT, subtexto MUTED); o nome de exibição do cartão em Newsreader (serifa) 20 px, como os demais títulos.
- Sem emoji colorido: o emoji do status personalizado só renderiza se a fonte de fallback tiver o glifo (monocromático). Opções: limitar a um conjunto pequeno de glifos que a fonte cobre, ou carregar uma fonte de emoji monocromática; nunca prometer paridade.
- Reticências: usar `LayoutJob` com `TextWrapping { max_width, max_rows: 1, break_anywhere: true, overflow_character: Some('…') }` e `painter.layout_job(job)`; `Label::truncate` não deixa medir a largura para posicionar a coroa.
- `context_menu` por linha dentro de `show_rows` funciona, mas só as linhas visíveis existem; não guardar `Response` entre frames.
- Tooltips de 150 ms: `resp.on_hover_text_at_pointer` não tem atraso configurável; usar `Tooltip::for_widget`/`show_tooltip_for` com atraso próprio se o 0.37 permitir **[?]**, senão `on_hover_ui` com `style.interaction.tooltip_delay = 0.15`.
- Cor do cargo: `Role.color: u32` (RGB) → `Color32::from_rgb`; cuidado com contraste sobre L1 (cargos muito escuros somem): clarear com `lerp(c, TEXT, 0.35)` quando a luminância relativa < 0,25. O Discord tem o ajuste "saturação/contraste" e um modo de alto contraste **[F `ROLE_STYLE_ADJUSTED`, `CONTRAST_LEVEL_INCREASED`]**.

## 7. Ordem de entrega (cada passo verificável)

1. **Linha e cabeçalho no padrão** (sem mudar protocolo): linhas de 42 px, avatar 32 com indicador por forma (só Disponível e anel), "ONLINE — 3" com travessão, offline atenuado e sem indicador, coroa desenhada, largura 240, sombra de seleção. *Usuário vê:* painel idêntico em estrutura ao do Discord.
2. **Virtualização**: lista plana + `show_rows` + cache por versão; mock com 5 000 membros. *Vê:* rolagem fluida, sem custo por frame.
3. **Presença de 4 estados**: `Status` no proto/servidor, mascarar Invisível, menu no painel do usuário (Disponível/Ausente/Não perturbar/Invisível + durações), ausente automático por 10 min. *Vê:* trocar estado e outro cliente ver a forma mudar (lua, barra, anel).
4. **Subtexto e status personalizado**: campo de texto + emoji + expiração, linha 2 da lista, "Compartilhando tela" para quem está em `snap.live`. *Vê:* status aparecendo na lista e sumindo na hora marcada.
5. **Cargos**: `roles/member_roles`, grupos hoist por posição, cor do nome (e opção "bolinha"). *Vê:* a lista agrupada por cargo.
6. **Popout de perfil**: `GetProfile`, banner, avatar 80, nome em serifa, bio, "Membro desde", cargos como chips, nota, botão Mensagem, "⋯"; abre à esquerda a 16 px, fecha fora/Esc. *Vê:* clicar em alguém abre o cartão.
7. **Menu de contexto** (grupos 1–5 do §2.5) com Mencionar, Bloquear, Expulsar/Banir, Copiar ID. *Vê:* botão direito funcional; itens destrutivos em vermelho.
8. **Apelidos e notas**: `SetNick`, `SetFriendNick`, `SetNote`; ordem da lista passa a usar o apelido. *Vê:* renomear alguém e a lista reordenar.
9. **Modal de perfil completo** (600×210, avatar 120): abas Informações, Atividade, Amigos em comum, Servidores em comum.
10. **Lista paginada por faixas** (§4): só se um servidor passar de ~1 000 membros.

## 8. Perguntas em aberto (só as que mudam a construção)

1. **Cor de "Ausente" e da coroa.** O âmbar do Grafite significa "ao vivo". Proponho ocre `#c9a45c` para Ausente e um âmbar apagado para a coroa; aceita, ou prefere Ausente em cinza claro (a lua já diferencia pela forma)?
2. **Nome do estado.** O pt-BR do Discord diz "Não perturbar" (você escreveu "não perturbe"). Sigo o bundle?
3. **Avatar de imagem.** Hoje o avatar é uma letra sobre cor. Upload de avatar e banner entra antes do perfil (muda armazenamento e proto), ou o cartão nasce com letra e cor?
4. **Quem aparece na lista de grandes servidores.** O Discord mostra Offline para todos? Não consegui confirmar se há corte; no Telacast mostro todos, com virtualização. Alguma preferência de esconder "Offline" acima de N membros?
5. **Moderação (Expulsar/Banir/Castigo)** entra junto do menu agora, ou só depois do editor de cargos e permissões?

### Maiores incertezas
1. Medidas de CSS (padding, tipografia em px, raio, largura real da lista) não estão no cache; só alturas de JS são firmes.
2. Popout, modal e menu de contexto são chunks não capturados: composição e ordem de itens são inferência a partir de enumerações e strings.
3. Comportamento da lista em servidores muito grandes (ocultar offline, limite de faixas) e posição vertical exata do avatar sobre o banner.
