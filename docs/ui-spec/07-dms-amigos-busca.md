# 07 — Início, DMs, amigos e busca

Área: tela Início (amigos), lista de mensagens diretas, cabeçalho da DM, DM em grupo, painel de perfil na DM, seletor rápido (Ctrl+K), busca de mensagens e estados vazios.

**Convenção de evidência.** **[F]** confirmado no cliente web; **[I]** inferência; **[?]** não confirmado.
Abreviações de fonte: `web@N` = `web.0bcf9a9e83b2c3c7.js`, deslocamento de caractere N, aproximado (±3 kB) (os números abaixo foram conferidos também em `web.3f31…`, `web.4894…`, `web.7265…` e `web.f86c…`: mesmos valores nas 5 versões). `pt:CHAVE` = chave de i18n no dicionário pt-BR (`12064a7d28fc7eb1.js`).

**Limite da evidência (importante).** Os `.css` do cache só trazem as variáveis de tema; **os CSS-modules dos componentes não estão no cache**, e os chunks lazy do cabeçalho da DM, do painel de perfil e da interface de busca de mensagens também não. Portanto: alturas de linha, tamanhos de avatar, tempos de animação feitos em JS, atalhos, ordenação, pontuação, limites e **todas as strings** são **[F]**; larguras de coluna, paddings, raios, tamanhos de fonte e a aparência do cabeçalho/perfil/busca são **[I]** (memória do produto + coerência com o Grafite). Onde o número vem do bundle, a fonte está ao lado.

---

## 1. Resumo e escopo

O Início é uma página de "Amigos" com barra de abas em pílula (Disponível, Todos, Pendente, Sugestões, Adicionar amigo), busca local, lista em seções com linhas de 61 px e botões de ação redondos que aparecem no hover, e uma coluna "Ativo agora" à direita. A coluna esquerda lista as DMs por última atividade (linha de 44 px, avatar 32, botão de fechar no hover). O seletor rápido (Ctrl+K) é um modal de uma linha de entrada e linhas de 34 px, com prefixos `@ # ! * $`, pontuação fuzzy com reforço por frequência/amizade e histórico de canais recentes. A busca de mensagens é um painel lateral direito com filtros localizados (`de:`, `em:`, `tem:`, `menciona:`, `antes:`, `durante:`, `depois:`, `fixadas:`), ordenação (mais recentes/antigos/relevantes) e paginação por cursor.

**Fora do escopo:** chamadas de voz/vídeo e o card de chamada (área de voz), conteúdo/composer/lista de mensagens do chat (outro agente), perfil completo em modal e configurações de privacidade, sugestões de amizade por contatos/importação, presentes e aniversários de amizade, "pedidos de mensagem"/filtro de spam, atividades (jogos), abas múltiplas (Ctrl+T), Caixa de entrada (Ctrl+I), anúncios e Nitro.

---

## 2. Árvore de componentes e medidas

Unidades em px lógicos do egui. Layout geral (já existente em `main.rs`): trilho de servidores 68 → coluna de canais/DMs 240 → centro → (direita opcional). Em Início/DM o painel direito muda de papel (Ativo agora / perfil / busca).

### 2.1 Início (`View::Home`)

```
Centro
├─ Cabeçalho (altura 48 [I]; o app hoje usa 63, ver §8-5)
│   ├─ ícone "amigos" 24 [F: uI.A.Icon aH.$]  + título "Amigos" (serifa, 20)  [F: pt:TdEu5X]
│   ├─ divisor vertical 1×24, LINE2, margem 8 [F existe: uI.A.Divider; medida I]
│   ├─ barra de abas "top-pill" [F: type:"top-pill", web@1269400]
│   │    pílula: altura 28, padding-x 10, raio 6, gap 4 [I]
│   │    ordem [F]: Disponível · Todos · Pendente (+badge) · Sugestões (+badge) · Adicionar amigo
│   │    "Adicionar amigo" tem classe própria (botão preenchido; selecionado vira texto colorido sem fundo) [F classe GU; I visual]
│   └─ à direita: botão "Novo grupo privado" (ícone) [F: toolbar AQ, tooltip pt:HfOgA/; atalho Ctrl+Shift+T]
├─ Corpo (duas colunas)
│   ├─ Coluna principal (flex)
│   │    ├─ Busca (campo 32 alto, ícone lupa à esquerda, botão limpar quando há texto) [F: role=searchbox, size "md", leading lupa, clearable]
│   │    │    margem 8 30 8 30 [I], raio 8, fundo L3
│   │    ├─ [live region] "N resultados encontrados"/"Nenhum resultado encontrado" [F pt:xIWGxu] (acessibilidade; no egui vira o texto de contagem na seção)
│   │    └─ Lista em seções (rolável)
│   │         ├─ Cabeçalho de seção: altura 50 [F: sectionHeight:50], texto caps 12/16 MUTED [I]
│   │         │    "Online — N" · "Todos os amigos — N" · "Recebidos — N" · "Enviados — N" · "Sugestões de amizade - N" [F pt:BagU2U rHRrhC g+3FIa npJsRl DYMZ/p]
│   │         │    em Pendente/Recebidos, à direita: link "Limpar tudo" se houver ≥5 pedidos recebidos [F: T>=5, pt:O8k7O4]
│   │         ├─ Linha de pessoa: altura 61 [F: 61 no estado base, 72 só em aniversário de amizade]
│   │         ├─ Rodapé de seção: altura 48 quando existe [F: 48*(footer!=null)]
│   │         └─ virtualização acima de 64 relações [F: isVirtualizedList: l>=64]
│   └─ Coluna "Ativo agora" (largura 360 [I], borda esquerda LINE, padding 16 [I], rolável)
│        ├─ título "Ativo agora" (heading-lg/semibold ≈ 20/24) [F pt:F4Zggw]
│        └─ cartões de atividade, ou estado vazio (§2.8)
```

**Linha de pessoa (amigo)** [F estrutura; I medidas]:

```
[ linha divisória superior 1px LINE ]   ← some quando a linha está em hover/ativa [F: h.yF + h._]
[ avatar 32 + ponto de status | nome (semibold 16/20) | subtexto (14/16, MUTED) ........ (36)(36) ]
```
- Avatar 32 com ponto de status: diâmetro 10, anel de recorte 3 [F: tabela `SIZE_32 {size:32,status:10,stroke:3}` em `web@12354173`; demais: 24→8/3, 40→12/4, 48→12/4, 56→14/4+2, 72→16/6+4, 80→16/6+4, 96→20/8+6, 120→24/8+8].
- Nome = apelido do amigo > nome global > usuário [F `comparator`]. Ao passar o mouse mostra também o identificador completo (aqui: só o `username`, que já é único) [F: `showAccountIdentifier`].
- Subtexto: atividade se houver; senão o texto de status [F E7]: `Disponível` · `Online via dispositivo móvel` · `Ausente` · `Não perturbar` · `Invisível` (nunca mostrado a terceiros) · `Offline` · `Transmitindo` [F pt:WbGtnH 9hghLD qWbtVU jaNpQH bg24HO Vv0abJ XKYej5]. Usuário ignorado mostra `Ignorado` [F pt:tFY5Zb].
- Em Pendente o subtexto é: nota do pedido (≤120 car.) ou texto de origem; para a nota recebida há um botão "ver" que revela a nota (olho) [F componente Ay].
- Padding horizontal: o hover/foco sangra 8 px para cada lado da coluna [F: `offset:{left:-8,right:-8}`], raio 8 [I]. Padding vertical interno ≈ 14 [I] (61 − 1 − 32 = 28 → 14 em cima e embaixo).
- **Botões de ação redondos** [F: componente `Ar.A`, `ActionTypes {ACCEPT=0, DENY=1, DEFAULT=2}`; tamanho I]: círculo 36, gap 10, ícone 20.
  - Amigo (Todos/Disponível): `Mensagem` (balão) e `Mais` (3 pontos) [F pt:g33r/P, UKOtz+].
  - Pedido recebido: `Aceitar` (✓, tipo ACCEPT) e `Ignorar` (✕, tipo DENY) [F pt:Zcibdf, xuio0C]; aceitar tem estado `loading` (clique ignorado enquanto carrega) [F].
  - Pedido enviado: `Cancelar` (✕, DENY) [F pt:eaq81S].
  - Sugestão: `Aceitar` (adicionar) e `Ignorar` [F AK].
  - Visibilidade: os botões ficam sempre no layout; a linha em hover/foco os "destaca" (`shouldHighlight`) [F]. Aqui: botão ocioso L3, linha em hover L3 → botão passa a L2 (mais escuro, contraste) e no hover do botão L4; ACCEPT colore o ícone com `OK`, DENY com `RED` [I].
- **Menu "Mais"** [F Aa]: `Iniciar chamada de vídeo` (só se o cliente suporta vídeo), `Iniciar chamada de voz`, `Desfazer amizade` (vermelho) [F pt:oCqlGG focH1t cvSt1J]. Confirmação ao remover: título `Remover "NOME"`, corpo `Você tem certeza de que deseja remover NOME dos seus amigos?`, botões `Desfazer amizade` (crítico) / `Cancelar` [F pt:fPLvZd l5FFq6 ETE/oC]. Item de chamadas fica oculto no Telacast até existir DM-call (§8).
- Clique na linha (fora dos botões) abre a DM com a pessoa (abre a existente, ou cria) [F `openPrivateChannel`; aria "Abrir conversa com X" pt:Qb5Km1].
- Clique direito: mesmo menu de contexto da pessoa (perfil, mensagem, chamar, remover) [I].

**Aba Adicionar amigo** [F EH/EG/EF]:
```
Cabeçalho: "Adicionar amigo" (heading-lg/semibold)  [pt:w5uwoI]
           "Você pode adicionar amigos com o nome de usuário Discord deles."  → no Telacast: "…com o nome de usuário deles."  [pt:Rn/sLl adaptado]
Formulário (card L3, raio 8, altura ≥ 52):
  [ campo "Insira um nome de usuário" (maxLength 37)  ................ [Enviar pedido de amizade] ]
  [ campo opcional "Personalize sua solicitação (opcional)" (maxLength 120) .......... contador 120−n ]
  texto de ajuda (body-sm, MUTED): "O que você escrever aqui também aparecerá nas suas mensagens diretas se vocês se tornarem amigos."
  sucesso (verde): "Sucesso! Seu pedido de amizade foi enviado para NOME."   erro (vermelho): texto do servidor
Abaixo: bloco "Outros lugares para fazer amigos" + "Ninguém vem à cabeça? Confira nossa lista de servidores públicos…" + botão "Explorar Servidores Públicos" [F pt:IuMx1Y DwevPs AhKnz4]
```
- Botão de envio **desabilitado** até haver texto; Enter envia; após sucesso limpa o campo e devolve o foco [F]. Mensagem de nota: só texto, < 120, quebras de linha viram espaço [F, `ckHwck`].
- Nomes com `@` inicial têm o `@` removido antes do envio [F].
- O bloco "Outros lugares…" só faz sentido se o Telacast tiver descoberta de servidores; senão omitir (§8-7).

### 2.2 Coluna de DMs (painel esquerdo em Início e DM)

```
Painel 240 (L1), padding-top 8 / bottom 8 [F: paddingTop=8, paddingBottom:8]
├─ Itens fixos (linhas de 40 [F: getRowHeight=40 p/ filhos]): "Amigos" (+badge vermelho de pedidos recebidos) [F titlebar usa pt:TdEu5X]
├─ divisor 25 de altura quando existir [F "section-divider*"→25]
├─ "Encontre ou comece uma conversa" (quick launcher, altura 67 [F key "dm-quick-launcher"→67]): botão-campo 28 alto que abre o seletor rápido (Ctrl+K) [F pt:LzcpeZ; medida do botão I]
├─ Cabeçalho de seção "Mensagens diretas" altura 24 [F getSectionHeight: 24 quando seção>0]
│    texto caps 12/16, MUTED; à direita: [+] "Criar mensagem" [F pt:bA875g, atalho Ctrl+Shift+T] e [···] menu "Limpar grupos inativos" [F H()/pt:RQbtb8, só quando há GDMs]
└─ Linhas de DM  (altura por densidade [F]: compacto 40 · padrão 44 · confortável 50)
```
**Linha de DM** [F estrutura em `web@787424`]:
```
[pílula de não lido 4×16 à esquerda, só se há menção (contador de menções da DM; que toda mensagem de DM o incrementa é I)] [F: ew.dM + getMentionCount>0]
[avatar 32 + status | nome (14/20, semibold se não lida) / subtexto ]  ........  [ícones de estado] [✕ fechar]
```
- 1:1: avatar 32 com ponto de status; subtexto **só aparece se houver atividade** (jogo, voz, transmissão) — online comum não mostra subtexto [F: `(0,G.A)({activities,status,...})`]. Estado "transmitindo" substitui o ponto por indicador roxo/âmbar [F `STREAMING`].
- Grupo: avatar = "facepile" diagonal de 2 avatares se ≥2 destinatários e sem ícone; com ícone, o ícone redondo [F]. Subtexto `N membros` com N = destinatários + 1 [F pt:CxSA5N]. Ponto de status vira "online" se algum membro está online [F]; "digitando" anima o avatar [F isTyping].
- DM do sistema: subtexto "Mensagem Oficial" e selo [F; fora do escopo].
- À direita: ícone de **estado** — bloqueado / ignorado / favorito (apenas um dos três) [F eX/ez/eY] — e botão **fechar** (✕). Tooltip/aria: `Fechar mensagem direta` (1:1) ou `Sair do grupo` (grupo) [F pt:jsvgc3, 26C4oi]; descrição `Remove DMs da sua lista sem excluir o histórico de mensagens` [F pt:OElXpV]. O ✕ só aparece em hover/foco/linha selecionada [I].
- **Silenciada/bloqueada/ignorada** (e não selecionada nem em hover): linha esmaecida (texto MUTED, avatar com opacidade ~0.6) [F `muted=(silenc||ign||bloq)&&!(sel||hover)`].
- **Selecionada**: fundo L3; **hover**: L2 [I, igual `list_row` atual].
- Menu de contexto (clique direito ou botão ⋯) [F handler; itens por i18n/I]: 1:1 → `Perfil`, `Marcar como lida` (pt:e6RscS), `Adicionar nota`, `Chamar`, `Silenciar conversa` ▸ (15 min, 1 h, 3 h, 8 h, 24 h, até religar [F pt: "Por 30 minutos/1 hora/3 horas/8 horas/24 horas/Até eu ligá-las de novo"]), `Fechar mensagem direta`, `Remover amigo`, `Bloquear`, `Copiar ID`. Grupo → `Marcar como lida`, `Editar nome do grupo`, `Silenciar conversa`, `Sair do grupo`, `Copiar ID`.
- Esqueleto de carregamento [F `Z`]: 10 linhas fantasmas, círculo d=32 + barra 144×20 raio 10, passo vertical 44, opacidade de 1,0 a 0,1 em degraus de 0,1 (SVG 184×428). No egui: pintar com `L3`, sem animação (ou pulsar a opacidade ±0,15 em 1,2 s, [I]).
- Lista vazia (sem nenhuma DM): o esqueleto ocupa 428 px de altura [F: `428` quando `privateChannelIds.length===0`]; no Telacast mostrar texto (§2.8) em vez do esqueleto quando o snapshot já chegou.
- Rolagem: ao selecionar uma DM fora da área visível, rola para ela (`44*(idx+fixos)+8` com folga de 8) [F `scrollToChannel`]. Posição de rolagem persistida por sessão [F `updateChannelListScroll`, debounce 100 ms].

### 2.3 Cabeçalho da DM (1:1) — **[I]**, chunk não disponível; strings [F]

```
Barra 48 (L2, borda inferior LINE)
├─ esquerda: avatar 24 + ponto de status  |  nome (semibold 16)  |  [badge "Bot"/verificado quando aplicável]  |  (status em texto MUTED 12, opcional)
└─ direita (ícones 24, gap 16, cor TEXT2→TEXT no hover; tooltip no hover):
     Iniciar chamada de voz (focH1t) · Iniciar chamada de vídeo (oCqlGG) · Mensagens fixadas (mp1N/2) [Ctrl+P]
     · Adicionar amigos à DM / "Novo grupo privado" (Xjlbvs/HfOgA/) [Ctrl+Shift+T]
     · Mostrar perfil do usuário / Ocultar perfil do usuário (+FAsHq / niD64e) [Ctrl+U]
     · campo de busca (expande ao focar; placeholder "Buscar DM com NOME" InYY4f ou "Buscar nesta mensagem direta" jRkYAh) [Ctrl+F]
     · Caixa de entrada [Ctrl+I]  · Ajuda
```
- Início da conversa (acima da 1ª mensagem): avatar 80, nome grande, `Este é o começo do seu histórico de mensagens diretas com NOME.` [F pt:Qvg+6+]; botões `Remover amigo`/`Bloquear`/`Denunciar` e "N servidores em comum" [I].
- Aviso de não-amigo: faixa com `Enviar pedido de amizade` / `Pedido de amizade enviado` [F pt:PMsq/b, xMH6vD, usados no picker].
- "Digitando": `NOME está digitando...` / `Várias pessoas estão digitando...` [F pt:lJ9sZX uVDhqZ], fica no composer (outro agente); aqui só o **ponto no avatar da lista** anima.

### 2.4 DM em grupo — **[F** nas regras/strings do picker; **I** no visual do cabeçalho]

- **Criar** (Ctrl+Shift+T, botão `+` da seção, ou "Novo grupo privado" no Início): popover/modal "Nova mensagem" [F pt:jD1qzM] — título muda: sem canal = `Nova mensagem`; com DM 1:1 = `Adicionar às DMs` (pt:PWkO7+); com grupo = `Convidar para grupo privado` (NB5DFD); lotado = `Esse grupo está lotado!` (OtTQDz).
  - Entrada com *tags* (chips) dos selecionados + campo de busca; Backspace com campo vazio remove a última tag [F]. Setas ↑/↓ movem a seleção com volta circular, Enter marca/desmarca [F handleKeyDown].
  - Seções: `Amigos` (cabeçalho 32 de altura, recolhível) e `Membros do servidor` (pt:y29JXs) quando há busca; linha de usuário **48** de altura com checkbox, avatar e nome; "carregando" 40 [F: 48 / 32 / 40]. Sem amigos: `Adiciona amigos para começar a conversar.` (1x/UBX) + botão `Adicionar amigo`. Sem resultado: `Nenhum resultado encontrado.` + `Buscar amigos ou pessoas dos seus servidores.` / `Não consegue encontrá-los? Tente enviar um link de convite.` [F pt:z3yXsY 7w2iq7 l6Rj+t].
  - Rodapé: botão primário `Criar mensagem de grupo` (≥2 selecionados) ou `Criar mensagem` (1) [F pt:h1/FMu bA875g]; desabilitado se 0 selecionados ou acima do limite; com ≥2, aparece campo `Nome do grupo (opcional)` + ícone (pt:YynaLK). Contador: `Você pode adicionar mais N pessoas.`; limite: `Grupos privados podem ter até N membros.` / `Esse grupo tem um limite de N membros.` [F pt:t9mhPt barkEr xYr004]. **Valor de N não está no bundle** (depende de experimento de Nitro) [?]; Discord clássico = 10 [I].
  - Se for amigo só do destinatário: `Você precisa ser amigo de X para criar um grupo privado.` (eg+R9x).
  - Em grupo existente: link de convite com `Ou, envie um link de convite`, botão `Criar`/`Copiar`/`Copiado`, validade (`Seu link de convite expira em N horas/dias.`) [F pt:dTqtcN qzxqUf OpuAlK q30c5i ZVdJMy T96qss]. Convite "expira em 24 h" [F pt:+zWvOQ].
- **Cabeçalho do grupo** [I]: facepile 24 + nome do grupo (editável: `Editar nome do grupo`, `Nome do grupo (opcional)`; sem nome → nomes dos membros separados por vírgula [I]) + `N membros`; ícones: chamar, vídeo, fixadas, **Adicionar amigos ao grupo privado** (AQAPts), **Mostrar lista de membros** (gxChDx, **Ctrl+U** alterna Membros em grupo e Perfil em 1:1 [F keybind TOGGLE_USERS]), busca.
- **Lista de membros do grupo** (painel direito 240 [I]): você + destinatários, ordenados por nome; o **dono** tem coroa; menu de contexto do dono: `Remover do grupo`, `Tornar dono(a) do grupo` [F pt:n5zMI+ 6t3CyN]; qualquer membro: `Perfil`, `Mensagem`, `Adicionar amigo`.
- **Sair**: modal `Sair de "NOME"` / `Deseja mesmo sair de NOME? Você não poderá voltar a este grupo até receber um convite novamente.` [F pt:hJ5Ap4 SSIVOu]; toast `Você saiu do grupo` / erro `Falha ao sair do grupo` [F pt:Gdj/qt AKajgt]. Há opção "sair em silêncio" (`silent`) que não avisa o grupo [F: `closePrivateChannel(id, selected, silent)`].
- Mensagens de sistema do grupo: `X mudou o nome do grupo: Y. Editar grupo`, `X removeu o nome do grupo personalizado.` [F pt:rk0be9 hToFyf]; boas-vindas `Bem-vindo(a) às suas novas mensagens de grupo! Convide seus amigos assim que estiver tudo pronto` (0Q7uk0) e `Bem-vindo (ou vinda) ao começo do grupo NOME.` (MFwcqO).

### 2.5 Painel de perfil na DM (direita) — **[F]** comportamento do container; **[I]** conteúdo

- **Container** [F `ChannelSectionStore`, `web@9438487`]: a coluna direita tem uma única seção ativa, com prioridade `BUSCA > chat lateral > AMIGOS(Ativo agora, só se habilitado) > PERFIL (se a conversa é DM 1:1 e `isProfileOpen`) > RESUMOS > MEMBROS (só se não é DM)`. **`isProfileOpen` começa ligado, `isMembersOpen` começa desligado** [F valores iniciais]. Alternar uma seção fecha as outras; abrir a busca fecha perfil/membros e fechar a busca devolve a seção anterior [F handlers `TOGGLE_*`, `SEARCH_RESULTS_CLOSE`]. Estado persistido.
- **Largura 340** [I; Discord ≈ 340], fundo L1, borda esquerda LINE, rolável.
- Conteúdo [I]: banner 60 de cor do usuário (cor sólida do `user_tint`, **sem gradiente**) → avatar 80 sobreposto (−40) com status → nome (serifa 20) + `username` (MUTED) → cartão L3 raio 8 com seções: **Sobre mim** (pt:61W33d), **Membro desde** (data de entrada; pt:AOcwWB `Membro desde`), **Amigos em comum** (`N amigos em comum`; vazio `Nenhum amigo em comum`; pt:afBKs5 z7y34b /5p4gx), **Servidores em comum** (`N servidores em comum`; `Vocês não têm servidores em comum`; pt:sySsXR LeYU4d bFgqYJ), **Nota** (`Adicionar nota (visível apenas para você)`; pt:1ZZtts, campo local de até 256 [?]) e botão `Ver perfil completo` [?]. Lista de mútuos clicável (abre a DM/servidor).
- Em grupo, o mesmo slot mostra a **lista de membros** (§2.4), não o perfil.

### 2.6 Seletor rápido (Ctrl+K) — **[F]** quase tudo (`web@7718000–7742000`, `web@7732780`, `web@10068758`)

```
Scrim: preto 50% cobrindo a janela; clique fora fecha.   [I]
Modal "Quick Switcher" (aria-label), tamanho "lg": largura 440–500 [I], topo ≈ 12% da altura [I], raio 12, L3, borda LINE2
├─ Campo (role=combobox, aria-autocomplete=list, sem corretor ortográfico, foco automático)
│    placeholder: "Aonde você gostaria de ir?" [F pt:VtvewW]; altura 48, fonte 18 [I]; sem borda, só uma linha inferior LINE
├─ Lista (role=listbox, aria-label "Resultados"; linhas de 34 [F rowHeight:34]; paddingBottom 10 [F]; altura máx ≈ 7 linhas + cabeçalhos [I])
│    Cabeçalho (HEADER): texto caps 12, MUTED, altura 34 (mesma regra de linha) [I]
│    Linha: [ícone/avatar 20] [nome] [rótulo inline MUTED (categoria/servidor)] .......... [badge de menções] [pontuação (só depuração)]
└─ Rodapé "PRO TIP" [F renderProtip]: "Escreva @ # ! * $ no começo da caixa de pesquisa para limitar os resultados. Saiba mais"
     cada símbolo é uma "tecla" de 20×20 L4 com o rótulo ao lado: @ Nomes de usuário · # Canais de texto · ! Canais de voz · * Servidores · $ Perfis de Jogo [F pt:GQRCGn wrwhuZ jz+hJp WuwCWi MfbDzS]
     o rodapé fica mais discreto (opacidade menor) quando há consulta [F classe ov]
```
- **Estado vazio inicial** (sem consulta e `seenTutorial=false`): ilustração com 2 grupos de setas animadas que sobem (mola friction 15, tension 100, deslocamento máx 250) + `Buscar servidores, canais ou mensagens diretas` + `Selecione um resultado e pressione ENTER para pular para ele` [F pt:Mp0IGK 3CbpwH, `Y`, `springTo(min(e,250))`]. Desligado se "reduzir movimento". Marca `seenTutorial` ao primeiro `QUICKSWITCHER_SWITCH_TO`.
- **Sem resultados** com consulta: `Não consegue encontrar o que você está procurando?` + link `Saiba mais sobre o Quick Switcher` [F pt:bpbATE 4iPfEO; link omitido no Telacast].
- **Abrir/fechar** [F]: `Ctrl/⌘+K` (se nenhuma camada/modal estiver aberta) abre; `Ctrl+K` ou `Ctrl+T` dentro dele fecham; `Esc` limpa a consulta se houver, senão fecha. Mouse só move a seleção depois de o ponteiro mover (evita seleção por mouse parado sobre a lista) [F `mouseFocusDisabled`].
- **Teclado** [F `handleKeyDown`]: `↑/↓` (e `Ctrl+P/Ctrl+N`) movem a seleção **circularmente pulando cabeçalhos**; `Enter` abre o selecionado; `Alt+Enter` abre o menu de contexto do item; `Ctrl+Enter` abre em nova aba (só com abas ativas; aqui: omitir). Colar (Ctrl+V) em qualquer lugar do app com o modal aberto anexa ao campo.
- **Prefixos** [F `AT`, `web@3185000`]: `@` usuários, `#` canais de texto, `!` canais de voz, `*` servidores, `$` perfis de jogo (omitir), `@@` usuários globais (fora de servidores em comum). O prefixo vira modo (`queryMode`); com modo, o limite sobe de 5 para 100 resultados e o cabeçalho passa a dizer: `Busque amigos e usuários em: SERVIDOR` / `Procurando todos os usuários` / `Procurando canais de texto` / `Procurando canais de voz` / `Procurando servidores` [F pt:FREzQs XFYW1o W26k4V zUoI5C olADPs]. Digitar só o prefixo (sem texto) mostra os mais recentes/frequentes do tipo (até 100) [F `queryX("", 100)`].
- **Consulta vazia** (sem prefixo) [F `web@7726159`]: ordem das seções: `Rascunhos` (canais com rascunho recente, pt:4B63jZ) → `Menções` (canais com menção, pt:61Df13) → `Canais não lidos` (pt:ieCAhD) → `Canais anteriores` (pt:80lOZ1; histórico de visita sem o canal atual). O histórico é limitado a **3 itens se houver as seções acima, 7 se não**; o canal atual é excluído e canais sem permissão de ver são filtrados.
- **Com consulta (modo geral)** [F `QueryEngine`, `web@3163800`]: junta usuários, DMs de grupo, canais de texto, canais de voz, servidores, links/rotas do app; deduplica por tipo+id; ordena por **pontuação decrescente, desempate alfabético** [F comparador `989133`]. Jogos (`$`) limitados a 3 no modo geral. Cada tipo é limitado a `_limit` itens antes de unir (5 no geral, 100 com prefixo) [F `setLimit`]; se só um tipo teve resultado, refaz a consulta com o limite alto [F `_willRefetchIfSingleCategoryResults`, limite de refetch >5].
- **Pontuação** [F `web@10068758`]: por campo, `10` = igual (case-insens.), `7` = começa com / casa no início, `5` = contém, `3` = todos os termos (separados por vírgula/espaço) aparecem, `1` = fuzzy (subsequência); em usuários o prefixo no username/apelido/nome global vale `10` e a comparação sem acentos vale `1`; fuzzy limitado a 50 candidatos. **Nota final = 1000 × nota × reforço**, com reforço `1 + (frecência_do_item / maior_frecência)` do tipo, `+0,2` se amigo, `+0,1` se tem DM aberta. Para usuário via grupo, a frecência do grupo é dividida entre os membros.
- **Enter** [F `selectResult` `web@7721015`]: servidor → vai ao servidor; canal de texto → abre; canal de voz → entra na voz **e** abre o chat; usuário → abre/cria a DM e rola a lista de DMs até ela; grupo → abre e rola; link de convite/rota do app → navega (nota 11 para links reconhecidos). Fecha o modal e fecha qualquer outro modal aberto.
- **Contexto** (clique direito / Alt+Enter): menus iguais aos da lista (servidor, canal, usuário) [F].
- **Linha de usuário/DM**: avatar 20 com status, badge de menções (vermelho) se a DM tem não lidas; **linha de canal**: ícone de canal, nome, rótulo cinza com a categoria, badge de menções, até 4 avatares de quem está na voz (canais de voz); **servidor**: ícone 20 + nome + rótulo `Servidor` (pt:5qyruI). Texto da linha em destaque se não lido/menção, mais apagado caso contrário [F classes XB/xr].

### 2.7 Busca de mensagens — painel direito

**Entrada** (campo no cabeçalho do canal/DM; `Ctrl+F` foca, e preenche com o filtro do canal atual; `Ctrl+Shift+F` foca sem preencher) [F keybind `FOCUS_SEARCH: mod+f, mod+shift+f`, `prefillCurrentChannel=!shift`]:
- Placeholder [F pt]: em servidor `Buscar em SERVIDOR`; em DM `Buscar DM com NOME` (ou `Buscar nesta mensagem direta`); em "todas as DMs" `Buscar em todas as mensagens diretas`; genérico `Buscar por mensagens, pessoas etc.`
- O campo vira um editor de **tokens**: texto livre + chips de filtro. Limite de tamanho: `Você atingiu o comprimento máximo para uma consulta de busca.` [F pt:zzAcsv; valor ?].
- **Popover de sugestões** abaixo do campo (L3, raio 8, sombra do tema) com dois modos [F `ia`, `w`]:
  - Campo vazio → **Histórico** (`Histórico`, itens clicáveis, `Remover do histórico de pesquisa`, `Limpar histórico de pesquisa`) e **Filtros** ("Filtros" + lista com descrição; `Mais filtros: datas, tipo de autor e muito mais`).
  - Digitando um filtro (`de:`) → sugestões de valores (até **10**) [F `maxResults=10`]: usuários (os que falaram recentemente primeiro, vindos de `getRecentlyTalked`, mais autores recentes da busca, até 15 [F `l.size<15`]); `eu` como atalho para si [F pt:Qf3ptv]; canais (canal atual primeiro quando vazio).
- **Ordem e conjunto de filtros** [F `d()` em `web@7994800`]: `de:`, `em:`, `tem:`, `menciona:`, `durante:`, `antes:`, `depois:`, `autorType:`, `fixadas:`. Aliases localizados [F pt:1TUdFo WNpFHa CqCvir i96lO+ h2NzSd qZ+7BA KSDx7M us8IQi 0B74eY]; também existem `linkDe:`, `nomeArquivo:`, `tipoArquivo:` (pt:RpRAZD 5xtLRC TMNjFm), omitidos aqui.
  - `de:` usuário (nome, `eu`, id) · `menciona:` usuário · `em:` canal (nome entre aspas se tiver espaço/aspas, `#canal`, ou nome da DM/grupo) · `tem:` valor de {`link`, `anexo`(embed), `enquete`, `encaminhar`, `arquivo`, `vídeo`, `imagem`, `som`, `figurinha`} — prefixo `-` nega (`-tem:link`) [F `$` com `startsWith("-")`].
  - Datas: `antes:`/`depois:`/`durante:` aceitam `hoje`, `ontem`, `semana`, `mês`, `ano`, nome de mês, dia da semana, ano (2015…ano atual), `AAAA-MM-DD`, `AAAA-MM` [F `x`,`k`, `v`,`P`,`M`]. "antes" usa o início do intervalo como limite superior (`max_id`), "depois" usa o fim do intervalo como inferior (`min_id`), "durante" usa os dois [F `min_id/max_id` via `fromTimestamp`]. UI alternativa: seletor `Escolha uma data` com `Depois`/`Antes`/`Entre`/`Data`, `Adicionar data`, `Remover filtro de data N` (pt).
  - `fixadas:` `verdadeiro`/`falso` [F `F=/(true|false)/`; pt:UJxL3V `verdadeiro ou falso`].
  - O token digitado gera `author_id[]`, `mentions[]`, `channel_id[]`, `has[]`, `min_id`, `max_id`, `pinned`; o texto restante vira `content` (junção por espaço) [F `v()`].
- Atalhos prontos (abas do painel do "Pesquisa" em servidores) [F `web@7980926`]: `Recentes`/`Mídia` → `tem: imagem, vídeo`; `Arquivos` → `tem: arquivo`; `Links` → `tem: link`; `Fixadas` → `fixadas: true`; `Mensagens` → sem filtro. Os rótulos pt: `Fotos e mídia`, `Fixadas`, `Link do site`.

**Painel de resultados** (direita, largura **~420** [I], L1, borda esquerda LINE, rolável) [F: seção SEARCH da coluna direita, `YvQ.SEARCH`]:
```
Cabeçalho 48: "Resultados de busca" (pt:zkoeq/) · contagem "N resultados" · [ordenar ▾] · [✕ fechar]
   ordenar: Mais recentes (padrão) · Mais antigos · Mais relevantes   [F pt:SCsC5m/K6oEu2 · 0gitSE · q8gB52; enum NEWEST/OLDEST/MOST_RELEVANT → (timestamp desc | timestamp asc | relevance desc)]
Faixa de filtros ativos (chips removíveis), "Filtros (N)" abre modal  [F pt:uaR4sI kP6oFy YfwFu1 "Limpar filtros"]
Aviso: "Ocultamos N mensagens de usuários que você bloqueou ou ignorou." / "Ocultar N mensagens…" (alternar)  [F pt:EJHRcV OvJs9w]
Lista de resultados: cada hit é um cartão:
   ├─ cabeçalho do hit: "# canal" ou nome da DM (e servidor, se houver), MUTED 12; clicável
   ├─ mensagem: avatar 40 · autor (semibold) · hora · texto com os termos em destaque (fundo AMBER 25%) · anexos em miniatura
   └─ hover: botão "Ir para" no canto; clique em qualquer lugar pula para a mensagem no canal (carrega o contexto em volta) [I]
Rodapé: paginação (§4) + "Buscamos por toda parte. Infelizmente, não encontramos nada." quando vazio
```
- **Contexto do resultado:** o cliente atual consome **apenas a primeira mensagem de cada grupo** do payload (`messages.map(e=>{let[t]=e})` `web@7982900`), ou seja, o hit sozinho + cabeçalho de local; o contexto é obtido ao saltar para a mensagem. O Telacast pode devolver 1 mensagem antes/depois (esmaecidas) como opção [I]; recomendação: **não** na v1.
- **Estados** [F pt]: buscando (spinner), `Busca iniciada`; indexando: `Antes de buscar, precisamos indexar esta conversa. Aguenta aí!` / `…suas mensagens…` / `…este servidor…` (pt:Q0JJjv Br0xJA AXPbZr) e `N resultados indexados até o momento. Ainda estamos indexando as mensagens mais antigas.` (4Y3O+O); erro `Deixamos a lupa cair. Você poderia tentar a busca de novo?` (uvDZBZ); indisponível `Busca indisponível`; atualizado `Resultados atualizados.` (anúncio).
- **Busca em todas as DMs** (opção "xDM"): `Busca em todas as minhas mensagens diretas` / `Busca apenas nas mensagens diretas selecionadas` / `Por padrão, buscando nas mensagens diretas...` (pt:Kr+lPi E9JM4J VkoLsy); no Telacast iniciar sempre restrito à conversa atual e oferecer o interruptor.

### 2.8 Estados vazios (texto exato, pt-BR) [F salvo indicação]

| Onde | Título/corpo | Ação |
|---|---|---|
| Todos (0 amigos) | `Não há amigos no momento. Clique em “Adicionar Amigo” para enviar pedidos de amizade.` (pt:Y4vMY8) | botão primário `Adicionar amigo` |
| Disponível (0 online) | `Não há Amigos online no momento. Volte mais tarde!` (pt:v7HbiQ) → no Telacast "Disponível": `Não há amigos disponíveis agora. Volte mais tarde!` [adaptação] | — |
| Pendente (0) | `Não há pedidos de amizade pendentes. Clique em “Adicionar Amigo” para enviar pedidos de amizade.` (pt:aCYQ+P) | botão secundário `Ver outras solicitações`/`Ver solicitações de spam` (só com spam; omitir) |
| Enviados (0) | `Não há pedidos de amizade enviados. Clique em “Adicionar amigo”…` (pt:PBv0Dr) | — |
| Sugestões (0) | `Não há sugestões de amizade pendentes. Clique em “Adicionar amigo” para enviar pedidos de amizade.` (pt:vgI/EF) | omitir aba sem sugestões |
| Bloqueados (0) | `Não há ninguém bloqueado no momento.` (pt:9xdyWB) | — |
| Busca em amigos, 0 hits | ilustração "sem resultados" + `Nenhum resultado encontrado` (pt:fAW3z4) | — |
| Ativo agora, vazio | título `Por enquanto, está quieto...` + `Quando um(a) amigo(a) começa uma atividade, como jogar um jogo ou bater papo por voz, mostraremos aqui!` (pt:ngJ/5u 99ZWxQ) → Telacast: `Quando um amigo começar uma transmissão ou entrar numa chamada de voz, mostraremos aqui.` | — |
| Lista de DMs, 0 | `Você ainda não tem conversas. Use “Encontre ou comece uma conversa”.` [I; a lista real mostra esqueleto 428 px, F] | — |
| Seletor rápido, 0 hits | `Não consegue encontrar o que você está procurando?` (pt:bpbATE) | — |
| Seletor, 0 histórico | sem seção; mostra só o estado inicial (setas) | — |
| Busca de msgs, 0 hits | `Buscamos por toda parte. Infelizmente, não encontramos nada.` (pt:V6nAfF) | `Limpar filtros` |
| Criar grupo, 0 amigos | `Adiciona amigos para começar a conversar.` (pt:1x/UBX) | `Adicionar amigo` |
| Picker, 0 hits | `Nenhum resultado encontrado.` (pt:z3yXsY) | — |
| Perfil, sem mútuos | `Vocês não têm amigos em comum` / `Vocês não têm servidores em comum` (pt:RnD2yZ bFgqYJ) | — |

Ilustrações: o Discord usa imagens de 376×162, 421×218, 433×232, 415×200 (claro/escuro) [F `Au`]; o Telacast **não** usa ilustração (regra Grafite, sem ativos de terceiros): usar um glifo de linha de 56 px (círculo com traço) em LINE2 acima do texto, texto 14/20 TEXT2 centrado, largura máx. 380.

---

## 3. Estados e interações

| Elemento | Estado | Regra | Tempo / easing |
|---|---|---|---|
| Aba (pílula) | ocioso / hover / selecionada / foco de teclado | ocioso: texto TEXT2 sem fundo; hover: fundo L3 + TEXT; selecionada: fundo L4 + TEXT; foco: contorno AMBER 2 px | 120 ms ease-out (igual `list_row`) |
| Aba "Adicionar amigo" | ocioso / selecionada | ocioso: pílula preenchida AMBER, texto AMBER_INK; selecionada: sem fundo, texto AMBER [I] | 120 ms |
| Badge de aba | `Pendente` e `Sugestões` mostram contagem; vermelho (RED) com texto escuro; some em 0 [F `hV count`] | — | — |
| Abas escondidas | aba só existe se tiver dados (`Disponível`, `Todos`, `Pendente`, `Sugestões`); `Adicionar amigo` sempre [F `show`] | anúncio "Aba X adicionada" | — |
| Seção inicial ao conectar | `Disponível` se há amigos; `Pendente` se não há amigos mas há pedido recebido; senão `Adicionar amigo` [F `w()`] | — | — |
| Linha de pessoa | ocioso → hover | fundo transparente → L3, raio 8, sangra 8 px; linha divisória some; botões "destacam" | fundo 120 ms |
| Linha de pessoa | entrar/sair da lista | **altura e opacidade vão de 0↔1 em 200 ms** [F `componentWillEnter/Leave`: `Animated.timing 200`] (aceitar/recusar/remover/ficar offline) | 200 ms linear→use `ease_out` |
| Linha de pessoa | menu aberto | permanece "ativa" (fundo L3) até fechar [F `isActiveRow`] | — |
| Botão redondo | ocioso/hover/pressionado/`loading` | `loading` ignora cliques e troca o ícone por spinner de 14 px [F `h?void 0:onClick`] | 120 ms |
| Foco de teclado em lista | modo teclado | setas ↑/↓ percorrem linhas; Tab alterna entre linha e seus botões; rola para manter visível com folga 8 [F `scrollIntoViewNode padding:8`] | — |
| Busca de amigos | texto por seção | **cada aba lembra a sua própria consulta**; trocar de aba não apaga [F estado `d[section]`]; casa por *substring* (case-insens.) em username, apelido, nome global [F `filter`] | — |
| Contagem de busca | `N resultados encontrados` / `Nenhum…` | região "polite" | — |
| Linha de DM | ocioso / hover / selecionada / não lida / silenciada | ver §2.2; não lida = texto em TEXT + pílula âmbar 4×16 na borda esquerda [I cor]; silenciada = texto MUTED, avatar a 60% | 120 ms |
| Linha de DM | digitando | ponto do avatar anima 3 pontos (ciclo 1,2 s) [I; F `isTyping`] | — |
| Botão ✕ da DM | aparece em hover/foco/selecionada; clique não propaga; fechar a DM aberta leva à página de Amigos [F `closePrivateChannel(..., selected)` → `FRIENDS`] | fade 100 ms |
| Lista de DMs | nova mensagem | a DM sobe ao topo imediatamente (ordem por última mensagem) [F `MESSAGE_CREATE` reordena] | 150 ms de deslizamento [I] |
| Seletor rápido | abrir | `instant:true` — **sem animação de entrada** no modal [F `openModal(..., {instant:!0})`] | 0 ms; scrim fade 100 ms [I] |
| Seletor rápido | item selecionado | fundo L4 + texto TEXT; hover do mouse só vale depois de mover [F] | 0 ms (troca imediata) |
| Seletor rápido | rolagem | `scrollToIndex` mantendo 10 px de folga | — |
| Busca de msgs | resultado em hover | fundo L2; aparece "Ir para" | 120 ms |
| Busca de msgs | carregando | skeleton de 3 cartões | — |
| Painel direito | alternar seção | troca imediata (sem slide) [I]; largura constante | — |
| Erro de rede em envio de pedido | texto vermelho sob o campo, `aria-invalid` | — | — |

---

## 4. Regras de comportamento

**Amigos [F `web@9916190`]**
- Modelo de linha: `(userId, tipo, status, atividades, apelido, spam, ignorado, mutualGuilds…)`. Tipos: `AMIGO`, `BLOQUEADO`, `PENDENTE_RECEBIDO`, `PENDENTE_ENVIADO`, `SUGESTÃO(99)`.
- **Disponível** = amigos com status ≠ offline (inclui ausente e não-perturbar; invisível aparece como offline para os outros). **Todos** = todos os amigos. **Pendente** = recebidos (não spam, não ignorados) + enviados; o **badge conta só os recebidos**. A seção "Recebidos" vem antes de "Enviados" (ordenação por `[tipo, nome]`).
- **Ordenação dentro de cada seção:** por `apelido ?? nome global ?? username`, minúsculas, ascendente [F `comparator`]. Em "Todos" os amigos online **não** sobem ao topo (a aba Disponível é que os filtra).
- **Contadores:** `getRelationshipCounts` por tipo; se todos forem 0 → vai para "Adicionar amigo".
- **Limite de pendentes:** `Você atingiu o limite de pedidos de amizade pendentes.` (pt:k1K15p).
- **Limpar tudo** (≥5 recebidos): confirma com `Você tem certeza?` + `Isso vai limpar N pedidos de amizade recebidos. Os usuários que enviaram os pedidos não serão informados.` + `Confirmar`/`Cancelar` [F pt:z2pFjo 0nTvEw cY+Oob ETE/oC].
- **Aceitar** move a linha para Todos (anim. de saída 200 ms) e cria a DM sob demanda; **Ignorar** remove o pedido sem avisar quem enviou; **Cancelar** remove o enviado.
- **Pedido mútuo** (A pede a B enquanto B já pediu a A) vira amizade [I, regra do servidor].
- **Adicionar:** chave de envio = nome de usuário; o texto digitado é cortado/aparado; erros do servidor viram mensagem no campo.
- **Telefone/e-mail:** ignorar.

**Lista de DMs [F `web@9906591`]**
- **Ordenação:** favoritas primeiro, depois todas as outras por **id da última mensagem decrescente** (id = snowflake com tempo ⇒ mais recente no topo); sem mensagem usa o id do canal (data de criação). Pedidos de mensagem (spam) ficam fora da lista. Nova mensagem em qualquer DM recalcula e a sobe.
- **Fechar** é otimista: remove da lista imediatamente, depois `DELETE /channels/{id}`; **não apaga o histórico**; uma nova mensagem recebida **reabre** a DM no topo [F + I]. Anuncia `mensagens diretas fechadas`/`falha ao fechar mensagens diretas`. Se estava selecionada → vai para Amigos.
- **Abrir DM com alguém:** se já existe DM 1:1 cacheada → seleciona; senão cria (idempotente por par) e seleciona [F `openPrivateChannel`/`_openCachedDMChannel`].
- Não lido: `mention_count > 0` ⇒ pílula + texto forte [F `getMentionCount>0` decide o destaque da linha]. Que *toda* mensagem de DM incrementa esse contador é **[I]** (regra do produto; a linha só lê o contador). Ler = abrir a conversa (rolada até o fim). `Marcar como lida` no menu.
- **Atalhos** [F keybinds em `web@2329178`/`2336818`; alvo exato de cada um **I**]: `Alt+↑/↓` canal anterior/seguinte (em DM, percorre a lista de DMs [I]); `Ctrl+Alt+↑/↓` servidor anterior/seguinte [I]; `Ctrl+Shift+T` novo grupo; `Ctrl+K` seletor; `Ctrl+U` perfil/membros; `Ctrl+F` busca; `Ctrl+P` fixadas; `Ctrl+Alt+←/→` histórico de navegação.
- Densidade: compacto 40 / padrão 44 / confortável 50 [F]; no Telacast só **44**.

**Seletor rápido:** ver §2.6 (prefixos, pontuação, dedupe, limites, teclas). Histórico de canais: lista persistida (`channelHistory`), atualizada a cada `CHANNEL_SELECT` (move para o início, sem duplicar) [F `X(n)`]; exclui o canal aberto.

**Busca de mensagens**
- **Escopo:** servidor → todo o servidor; canal (`em:`) → só o canal; DM → só a conversa; "todas as DMs" opcional [F contextos GUILD / GUILD_CHANNEL / CHANNEL / THREAD / DMS].
- **Paginação por cursor:** o servidor responde `{ total_results, cursor, messages, doing_historical_index, documents_indexed, analytics_id }`; o cliente acumula mensagens sem duplicar (`messageIds` Set) e guarda o `cursor` para "carregar mais" ao rolar até o fim [F `SearchMessageStore`, `web@7982000`]. Também existe a mensagem `Só podemos exibir até N páginas de resultados. Tente fazer uma busca mais específica!` (pt:E+2azY) ⇒ há **teto de páginas** [F string; N ?]. Tamanho de página: **25** [I, API clássica]. No Telacast: paginação por `before_id` (keyset) de 25 em 25; rolagem infinita; teto de 40 páginas.
- Ordenação padrão `timestamp desc` [F `u={sort_by:"timestamp",sort_order:"desc"}`].
- **Mensagens de bloqueados/ignorados** são omitidas e contadas no aviso [F].
- **Mensagem editada/excluída** durante a exibição atualiza/remove o cartão (`MESSAGE_UPDATE`) [F handler no store]; reações atualizam o cartão.
- **Histórico de buscas:** últimas buscas por contexto, `Limpar histórico de pesquisa` [F strings; tamanho ?, usar 8].
- Sem acesso a privacidade: se `hidePersonalInformation` (modo streamer), `de:`/`em:` de DM/menções são ocultados [F `d()` usa `hidePersonalInformation`].

---

## 5. Dados necessários (protocolo / banco), em ordem de prioridade

Hoje: `Snapshot { me, servers, friends:[{user,status}], dms:[{channel_id,user}], live }`; `User { id, name, online }`; C2S `FriendRequest/Accept/Remove`, `OpenDm`; sem presença rica, sem leitura, sem busca, sem grupos.

**P0 — Início e lista de DMs corretos**
1. `Dm` ganhar: `last_message_id: i64` (ordenar), `last_message_ts`, `unread: bool`/`mention_count: u32` (já existe `read_state`), `muted_until: Option<i64>`, `closed: bool` (não vai no snapshot; ver 2), `kind: Direct|Group`. `Snapshot.dms` já filtra por `channel_members`.
2. Coluna `channel_members.closed_at TIMESTAMPTZ NULL` (+ índice parcial `WHERE closed_at IS NULL`) e `C2S::CloseDm { channel_id }`; no `Send`, limpar `closed_at` de todos os membros e empurrar `S2C::DmOpened`/`ChannelUpdate`.
3. `C2S::Ack { channel_id, message_id }` e `S2C::ReadState { channel_id, last_read_id, mention_count }` (alimentar pílula de não lido; hoje o app mantém `unread: HashSet`).
4. `User` ganhar `status: Online|Idle|Dnd|Offline` (substitui `online: bool`) e `S2C::Presence { user_id, status }` (hoje só o snapshot).
5. `Friend` ganhar `since: i64`, `note: Option<String>` (nota do pedido, ≤120) e `nickname: Option<String>`; coluna `friendships.note TEXT CHECK (char_length(note) <= 120)`.
6. `C2S::FriendRequest { username, note }`; erros tipados: `NotFound`, `AlreadyFriends`, `RequestLimit`, `Blocked`, `SelfRequest`.
7. Evento `S2C::FriendUpdate { user, status|removed }` (hoje a lista inteira volta no snapshot ⇒ sem animação de entrada/saída por linha).

**P1 — Seletor rápido e perfil**
8. Nada novo no servidor para o seletor (usa snapshot + histórico local); persistir `quick_history: Vec<Target>` no `eframe::Storage`.
9. `C2S::GetProfile { user_id }` → `S2C::Profile { user, bio, created_at, mutual_friends:[User], mutual_servers:[ServerInfo-lite], note }`; colunas `users.bio TEXT`, `users.created_at`; tabela `user_notes (owner_id, target_id, note, PK)`; consultas de interseção em `friendships` e `members`.
10. `Blocks`: `C2S::Block/Unblock { user_id }` + `Snapshot.blocked: Vec<User>` (tabela `blocks` já existe) + efeitos (DM inerte, mensagens ocultas).

**P2 — DM em grupo**
11. `channels.kind` já tem 10 (DM); adicionar `11 = grupo`, `channels.owner_id`, `channels.name` já existe (opcional p/ grupo: relaxar o `CHECK (char_length(name) BETWEEN 1 AND 100)` ou gravar nome derivado), `channels.icon`. `Dm` vira `{channel_id, kind, recipients:Vec<User>, name:Option<String>, icon:Option<String>, owner:Option<i64>}` (o campo `user` atual vira `recipients[0]`).
12. `C2S::CreateGroupDm { user_ids, name }`, `AddToGroup`, `RemoveFromGroup`, `LeaveGroup { channel_id, silent }`, `RenameGroup`, `TransferOwner`; eventos `S2C::GroupUpdated`. Limite de membros configurável (padrão 10).
13. Convite de grupo: reaproveitar `server_invites` com `channel_id` (24 h).

**P3 — Busca de mensagens**
14. `messages` + índice de texto: `ALTER TABLE messages ADD COLUMN tsv tsvector GENERATED ALWAYS AS (to_tsvector('portuguese', content)) STORED; CREATE INDEX messages_tsv ON messages USING GIN (tsv);` + `pg_trgm` (`CREATE INDEX … USING GIN (content gin_trgm_ops)`) para termos parciais; índice composto `messages (channel_id, id DESC)` já existe.
15. Dados para `tem:`/`fixadas:`/`menciona:`: tabelas `attachments (message_id, kind, name, size, …)` e `message_mentions (message_id, user_id)` e coluna `messages.pinned BOOLEAN`. Para `tem:link`, detectar URL na escrita (`messages.has_link BOOLEAN`).
16. `C2S::Search { scope: Server(i64)|Channel(i64)|Dms, text, from:Vec<i64>, mentions:Vec<i64>, channels:Vec<i64>, has:Vec<Has>, pinned:Option<bool>, min_id, max_id, sort: Newest|Oldest|Relevance, cursor: Option<Cursor> }` → `S2C::SearchResult { request_id, total, hits:Vec<Message+channel_name>, next:Option<Cursor>, hidden_count }` e `S2C::SearchError`. Relevância: `ts_rank_cd(tsv, plainto_tsquery('portuguese', $1))` com desempate por `id desc`. Permissão: filtrar por membros (`channel_members`/`members`).
17. `search_history (user_id, scope, query, ts)` ou só local.

**P4 — Ativo agora**
18. `Snapshot.live` já tem transmissões; acrescentar `voice: Vec<{user_id, channel_id}>` (presença em voz) e `S2C::LiveUpdate`. A coluna usa só isso (sem jogos).

---

## 6. Plano de implementação em egui

### 6.1 Onde entra

- `theme.rs`: novos widgets genéricos — `round_icon_button`, `pill_tabs`, `search_field`, `status_dot`, `avatar_with_status(size)` (substitui o `avatar(..., online)` atual para as 3 variantes de status), `empty_state(glifo, texto)`, `kbd_chip`, `unread_pill`, `facepile`, `section_header`. Constantes: `ROW_FRIEND=61.0`, `ROW_DM=44.0`, `SECTION_FRIEND=50.0`, `SECTION_DM=24.0`, `ROW_QS=34.0`.
- `main.rs`: quebrar `friends_view` em `home_header`, `people_list`, `people_row`, `active_now`, `add_friend_form`; `channel_panel` ganha `dm_list` (substitui o laço atual) e `dm_row`; novos módulos `quick_switcher.rs`, `msg_search.rs`, `profile_panel.rs`, `group_dm.rs` (arquivos novos em `app/src/`) e `Session.qs: QuickSwitcher`, `Session.search: SearchState`, `Session.right: RightPanel`.
- `Act` ganha: `CloseDm(i64)`, `OpenQuickSwitcher`, `Search(SearchQuery)`, `ToggleRight(RightPanel)`.
- Estado da coluna direita: `enum RightPanel { None, Profile, Members, Search, ActiveNow }` com a prioridade do §2.5 calculada em `fn right_section(view, flags, search_open)`.

### 6.2 Armadilhas do egui para esta área

- **Hover com botões sobrepostos.** Uma `Response` de linha com `Sense::click()` perde o `hovered()` quando um botão filho a cobre. Calcule o hover da linha com `ui.rect_contains_pointer(row_rect)` (respeita camada e clipe) e use esse booleano para revelar botões/✕; aloque a linha **antes** dos botões (os criados depois ficam por cima e recebem o clique).
- **Negrito.** `RichText::strong()` no egui **não troca o peso** (só a cor "forte"). A Inter do app é variável: não há semibold. Opções: (a) distinguir nome vs subtexto por tamanho/cor (TEXT 15 vs MUTED 13); (b) embutir `Inter-SemiBold.ttf` estático como `FontFamily::Name("semibold")`. Recomendo (b), 1 arquivo de ~300 KB. Até lá, nada de `.strong()` esperando negrito.
- **Sem letter-spacing.** Cabeçalhos em caixa alta ficam levemente apertados: usar `size(11.5)` + cor MUTED e `to_uppercase()`; não tentar espaçar manualmente por caractere.
- **Sem emoji colorido / sem SVG.** Ícones: desenhar com `Painter` (linhas/arcos/círculos) em funções `icon_*`; `✓ ✕ ⋯ ＋` existem na Inter, mas o peso varia. Para consistência, desenhar o ✓ e o ✕ com `line_segment` em stroke 2 (cap redondo via `Stroke` + `circle_filled` nas pontas).
- **Foco do atalho global.** `Ctrl+K` deve ser consumido **antes** de desenhar widgets, senão o `TextEdit` focado o engole: `ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::K))` no topo de `update`. O mesmo vale para `Ctrl+F`, `Ctrl+U`, `Ctrl+Shift+T`.
- **Modal com scrim.** `egui::Area::new(Id::new("qs")).order(Order::Foreground).fixed_pos(pos)` + um `ctx.layer_painter(LayerId::new(Order::Middle, ..))` para o scrim e `ctx.input(|i| i.pointer.any_click())` fora do retângulo para fechar. Use `ctx.content_rect()` (em 0.37 `screen_rect()` é obsoleto).
- **Listas longas.** Linhas de altura fixa ⇒ `ScrollArea::vertical().show_rows(ui, ROW, n, |ui, range| …)` (lista de DMs 44, amigos 61, resultados do seletor 34). Cabeçalhos de seção de altura diferente quebram `show_rows`: achatar para `Vec<Item>` com enum `Header|Row` e **usar altura própria por item** com um mapa de offsets (`Vec<f32>` acumulado + busca binária) em vez de `show_rows`.
- **Animação de entrada/saída de linhas.** Manter `Session.row_anim: HashMap<(Section,i64), f32>`; ao remover do estado real, deixar o "fantasma" até `animate_value_with_time(id, 0.0, 0.20) == 0.0`, desenhando com `ui.set_opacity(a)` e altura `ROW*a`. Chamar `ctx.request_repaint()` enquanto `0<a<1`.
- **Texto com destaque** (resultado de busca): montar `egui::text::LayoutJob` com seções de cor/fundo (`TextFormat { background: alpha(AMBER,0.25), .. }`).
- **Skeleton:** sem gradiente; retângulos L3 com `Rounding` 10 e pulso de opacidade (`0.7 + 0.15*sin(t)`).
- Sem build/teste agora (RAM): os trechos abaixo são esboços.

### 6.3 Esboço A — linha de pessoa (amigos) com botões redondos

```rust
#[derive(Clone, Copy, PartialEq)]
pub enum Round { Chat, More, Accept, Deny }

pub struct RowOut { pub clicked: bool, pub btn: Option<Round> }

/// Linha de 61 px. `show` = 0..1 (animação de entrada/saída). `hot_extra` = menu aberto.
pub fn people_row(
    ui: &mut egui::Ui, id: egui::Id, user: &p::User, sub: &str, status: Presence,
    btns: &[Round], loading: Option<Round>, show: f32, hot_extra: bool,
) -> RowOut {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_FRIEND * show), Sense::click());
    if show < 0.999 { ui.set_opacity(show); }
    let hot = ui.rect_contains_pointer(rect) || hot_extra;       // não usar resp.hovered()
    let t = ui.ctx().animate_bool_with_time(id.with("hot"), hot, 0.12);
    let p = ui.painter();
    let r = egui::CornerRadius::same(8);
    // fundo sangra 8 px para os lados
    if t > 0.0 { p.rect_filled(rect.expand2(vec2(8.0, -1.0)), r, lerp(L2, L3, t)); }
    // divisória superior some no hover
    if t < 0.99 { p.hline(rect.x_range(), rect.top(), Stroke::new(1.0, alpha(LINE, 1.0 - t))); }

    let cy = rect.center().y;
    // avatar 32 + status (10, anel 3)
    let av = Rect::from_center_size(pos2(rect.left() + 16.0, cy), vec2(32.0, 32.0));
    avatar_with_status(ui, av, user, status, lerp(L2, L3, t));
    // texto
    let tx = rect.left() + 32.0 + 12.0;
    p.text(pos2(tx, cy - 9.0), Align2::LEFT_CENTER, &user.name,
           FontId::new(15.5, semibold()), TEXT);
    p.text(pos2(tx, cy + 10.0), Align2::LEFT_CENTER, sub, FontId::proportional(13.0), MUTED);

    // botões redondos da direita para a esquerda
    let mut out = RowOut { clicked: false, btn: None };
    let mut x = rect.right() - 18.0;
    for (i, b) in btns.iter().rev().enumerate() {
        let c = pos2(x, cy);
        let br = Rect::from_center_size(c, vec2(36.0, 36.0));
        let r = ui.interact(br, id.with(("b", i)), Sense::click()); // criado depois ⇒ por cima
        let bh = animate(ui, r.id.with("h"), r.hovered(), 0.10);
        let idle = lerp(L3, L2, t);                                   // linha em hover ⇒ botão mais escuro
        p.circle_filled(c, 18.0, lerp(idle, L4, bh));
        let ink = match (b, bh > 0.0) {
            (Round::Accept, true) => OK, (Round::Deny, true) => RED, _ => lerp(TEXT2, TEXT, t.max(bh)),
        };
        if loading == Some(*b) { spinner(ui, c, 7.0, ink); } else { icon(p, *b, c, ink); }
        if r.clicked() && loading.is_none() { out.btn = Some(*b); }
        ui.ctx().set_cursor_icon_if(r.hovered(), CursorIcon::PointingHand);
        x -= 36.0 + 10.0;
    }
    if resp.clicked() && out.btn.is_none() { out.clicked = true; }   // clique na linha abre DM
    out
}

fn icon(p: &egui::Painter, b: Round, c: Pos2, col: Color32) {
    let s = Stroke::new(2.0, col);
    match b {
        Round::Accept => { p.line_segment([c + vec2(-6.0, 0.5), c + vec2(-2.0, 4.5)], s);
                            p.line_segment([c + vec2(-2.0, 4.5), c + vec2(6.0, -4.0)], s); }
        Round::Deny   => { p.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], s);
                            p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], s); }
        Round::More   => for dy in [-6.0, 0.0, 6.0] { p.circle_filled(c + vec2(0.0, dy), 1.8, col); },
        Round::Chat   => { let r = Rect::from_center_size(c + vec2(0.0, -1.0), vec2(15.0, 11.0));
                            p.rect_stroke(r, 4.0, s, StrokeKind::Inside);
                            p.line_segment([c + vec2(-3.0, 4.5), c + vec2(-5.5, 8.0)], s); }
    }
}
```

### 6.4 Esboço B — linha de DM (44 px) com ✕ no hover, pílula de não lido e muted

```rust
pub fn dm_row(ui: &mut Ui, d: &DmView, selected: bool) -> (bool /*abrir*/, bool /*fechar*/) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_DM), Sense::click());
    let hover = ui.rect_contains_pointer(rect);
    let t = animate(ui, resp.id.with("h"), hover && !selected, 0.12);
    let dim = d.muted && !selected && !hover;
    let p = ui.painter();
    let rr = egui::CornerRadius::same(8);
    if selected { p.rect_filled(rect, rr, L3); } else if t > 0.0 { p.rect_filled(rect, rr, lerp(L1, L2, t)); }
    if d.unread && !dim {                                      // pílula 4×16 na borda esquerda
        p.rect_filled(Rect::from_center_size(pos2(rect.left() + 2.0, rect.center().y), vec2(4.0, 16.0)), 2.0, AMBER);
    }
    let av = Rect::from_center_size(pos2(rect.left() + 8.0 + 16.0, rect.center().y), vec2(32.0, 32.0));
    match d.kind {
        Kind::Direct => avatar_with_status(ui, av, &d.peer, d.status, bg_for(selected, t)),
        Kind::Group  => facepile(ui, av, &d.members, bg_for(selected, t)),
    }
    let x = av.right() + 12.0;
    let col = if dim { MUTED } else if d.unread || selected || hover { TEXT } else { TEXT2 };
    let two = d.sub.is_some();                                   // subtexto só com atividade / "N membros"
    p.text(pos2(x, rect.center().y - if two { 8.0 } else { 0.0 }), Align2::LEFT_CENTER,
           &d.name, FontId::new(14.5, if d.unread { semibold() } else { FontId::proportional(14.5).family }), col);
    if let Some(s) = &d.sub { p.text(pos2(x, rect.center().y + 9.0), Align2::LEFT_CENTER, s, FontId::proportional(12.0), MUTED); }

    // ✕ só no hover/seleção; criado depois da linha ⇒ clique não propaga
    let mut close = false;
    if hover || selected {
        let c = pos2(rect.right() - 16.0, rect.center().y);
        let br = Rect::from_center_size(c, vec2(20.0, 20.0));
        let r = ui.interact(br, resp.id.with("x"), Sense::click()).on_hover_text(
            if matches!(d.kind, Kind::Group) { "Sair do grupo" } else { "Fechar mensagem direta" });
        let h = r.hovered();
        let s = Stroke::new(1.6, if h { TEXT } else { MUTED });
        p.line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], s);
        p.line_segment([c + vec2(-4.0, 4.0), c + vec2(4.0, -4.0)], s);
        close = r.clicked();
    }
    (resp.clicked() && !close, close)
}
```
`DmView` é montado em `Session` a cada frame a partir de `snap.dms` ordenado por `last_message_id` desc (favoritas primeiro quando existirem). A lista usa `show_rows` com `ROW_DM` (ou offsets se houver cabeçalhos).

### 6.5 Esboço C — seletor rápido (parser de prefixo, pontuação, teclado)

```rust
#[derive(Clone, Copy, PartialEq)]
pub enum Mode { All, User, UserGlobal, Text, Voice, Server }

pub fn parse_prefix(q: &str) -> (Mode, &str) {
    if let Some(r) = q.strip_prefix("@@") { return (Mode::UserGlobal, r); }
    match q.chars().next() {
        Some('@') => (Mode::User, &q[1..]),   Some('#') => (Mode::Text, &q[1..]),
        Some('!') => (Mode::Voice, &q[1..]),  Some('*') => (Mode::Server, &q[1..]),
        _ => (Mode::All, q),
    }
}

/// 10 igual · 7 prefixo · 5 contém · 3 todos os termos · 1 subsequência · 0 nada  (mesma escala do cliente)
pub fn field_score(name: &str, q: &str) -> u32 {
    let (n, q) = (fold(name), fold(q));            // minúsculas + sem acento
    if n == q { return 10 }
    if n.starts_with(&q) || n.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(&q)) { return 7 }
    if n.contains(&q) { return 5 }
    if q.split([',', ' ']).filter(|t| !t.is_empty()).all(|t| n.contains(t)) { return 3 }
    if is_subsequence(&q, &n) { return 1 }
    0
}

pub struct Item { pub kind: Kind, pub id: i64, pub label: String, pub hint: String, pub score: f32, pub header: bool }

pub fn rank(items: &mut Vec<Item>) {                // maior nota primeiro; empate alfabético
    items.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap().then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase())));
}
// nota final = 1000 * field_score * boost ; boost = 1 + freq/freq_max + (amigo ? 0.2 : 0) + (tem DM ? 0.1 : 0)

pub fn qs_input(ctx: &egui::Context, qs: &mut QuickSwitcher) -> Option<Item> {
    // teclas consumidas ANTES de o TextEdit ver o evento
    let (down, up, enter, esc) = ctx.input_mut(|i| (
        i.consume_key(Modifiers::NONE, Key::ArrowDown) || i.consume_key(Modifiers::CTRL, Key::N),
        i.consume_key(Modifiers::NONE, Key::ArrowUp)   || i.consume_key(Modifiers::CTRL, Key::P),
        i.consume_key(Modifiers::NONE, Key::Enter),
        i.consume_key(Modifiers::NONE, Key::Escape)));
    if down { qs.sel = next_non_header(&qs.items, qs.sel, 1); }      // circular, pula cabeçalhos
    if up   { qs.sel = next_non_header(&qs.items, qs.sel, -1); }
    if esc  { if qs.query.is_empty() { qs.open = false } else { qs.query.clear(); qs.refresh() } }
    if enter { return qs.items.get(qs.sel).cloned(); }
    None
}
```
Desenho: `Area` central; campo `TextEdit::singleline` 48 alto sem moldura com `request_focus()` ao abrir; lista com `ScrollArea` e linhas de 34; para a seleção por mouse, só aplicar `hover → sel` se `ctx.input(|i| i.pointer.delta() != Vec2::ZERO)` (equivale ao `mouseFocusDisabled`). A consulta vazia usa `qs.history` (3 ou 7 itens, §2.6).

### 6.6 Esboço D — consulta de busca com filtros localizados

```rust
#[derive(Default, Debug)]
pub struct Parsed {
    pub text: String, pub from: Vec<String>, pub mentions: Vec<String>, pub in_ch: Vec<String>,
    pub has: Vec<(Has, bool /*negado*/)>, pub on: Option<Range>, pub before: Option<i64>, pub after: Option<i64>, pub pinned: Option<bool>,
}
const KEYS: &[(&str, Key)] = &[("de", F::From), ("menciona", F::Mentions), ("em", F::In), ("tem", F::Has),
    ("durante", F::During), ("antes", F::Before), ("depois", F::After), ("fixadas", F::Pinned)];

/// "de:ana tem:-link em:\"geral 2\" antes:2026-01-01 olá mundo" → Parsed
pub fn parse(q: &str) -> Parsed {
    let mut out = Parsed::default();
    let mut it = tokens(q);                          // respeita aspas e \" ; devolve (texto, é_filtro)
    while let Some(tok) = it.next() {
        if let Some((k, v)) = split_filter(&tok) {   // k em KEYS (sem acento, minúsculas), v pode vir como próximo token
            apply(&mut out, k, v);
        } else { if !out.text.is_empty() { out.text.push(' ') } out.text.push_str(&tok); }
    }
    out
}
```
Autocomplete: achar o token sob o cursor com `TextEditOutput.cursor_range`; se for `chave:` + prefixo, consultar fontes (usuários recentes na conversa, canais, valores de `tem:`). Resolver nomes → ids no cliente antes de enviar `C2S::Search`. Chips: renderizar o campo como `Frame` com `horizontal_wrapped` (chips + `TextEdit` final); Backspace no início remove o último chip.

---

## 7. Ordem de entrega (cada passo verificável)

1. **Início "limpo"**: cabeçalho 48 com abas em pílula (Disponível, Todos, Pendente+badge, Adicionar amigo), seções com títulos `Todos os amigos — N` etc., linhas de 61 com avatar+status e subtexto de status, hover com fundo e divisória sumindo. *Usuário vê:* a tela Amigos com cara do Discord em Grafite.
2. **Botões redondos + menu Mais + confirmação de remover**; ícones desenhados. *Vê:* ✉ e ⋯ em círculos, ✓/✕ nos pendentes, diálogo "Remover X".
3. **Busca local de amigos** (por aba, com contagem) + estados vazios com texto; animação 200 ms de entrada/saída ao aceitar/remover. *Vê:* filtrar e a linha sumir suavemente.
4. **Aba Adicionar amigo completa** (nota ≤120, desativado sem texto, sucesso/erro inline) — exige `FriendRequest{note}` (P0-6). *Vê:* o formulário e as mensagens.
5. **Lista de DMs**: ordenação por última mensagem, linha de 44, avatar 32, não lido (pílula), ✕ fechar no hover, esqueleto, cabeçalho "Mensagens diretas" + botão `+` — exige `last_message_id`, `closed_at`, `CloseDm`, `ReadState` (P0-1..3). *Vê:* DM nova sobe ao topo; fechar some e reabre com nova mensagem.
6. **Presença 4 estados** (Disponível/Ausente/Não perturbar/Offline) nos pontos de status e subtextos (P0-4).
7. **Seletor rápido Ctrl+K**: modal, prefixos, pontuação, histórico (3/7), setas/Enter/Esc, cabeçalhos, rodapé de dica. *Vê:* Ctrl+K abre; `@ana`, `#geral` filtram; Enter navega.
8. **Painel de perfil na DM** (340 px) com alternância Ctrl+U (P1-9). *Vê:* coluna direita com perfil, mútuos e nota.
9. **Cabeçalho da DM** (ícones, tooltips, alternar perfil, campo de busca no lugar) — chamadas ficam desabilitadas até a voz.
10. **Busca de mensagens**: painel direito, filtros `de: em: tem:` primeiro, depois datas/`fixadas:`; ordenação, cursor, estados (P3). *Vê:* Ctrl+F, resultados, "Ir para".
11. **Grupos**: criação (Ctrl+Shift+T), facepile, cabeçalho, membros, sair, dono (P2).
12. **Ativo agora** com transmissões/voz (P4) e **Bloqueados** (se confirmado).

---

## 8. Perguntas em aberto

1. **Aba "Bloqueados":** o cliente atual **não** tem essa aba na barra (só Disponível/Todos/Pendente/Sugestões/Adicionar amigo; bloqueados ficam em Configurações) [F]. Você listou "Bloqueados" no pedido. Quer a aba (como nas versões antigas) ou só a lista dentro de Configurações?
2. **DM em grupo agora?** Exige mudar `Dm`/`channels` e várias mensagens do protocolo (P2). Entra antes ou depois da busca de mensagens?
3. **Busca de mensagens no Postgres:** ok usar FTS `portuguese` + `pg_trgm` (índices GIN extras, mais disco/RAM no servidor Oracle A1)? Alternativa: `ILIKE` sem índice (lento acima de ~100 mil mensagens).
4. **Presença com Ausente/Não perturbar/Invisível** agora, ou só online/offline por enquanto? Muda `User.online` e a UI de status do cartão do usuário.
5. **Altura do cabeçalho:** o app usa 63 px; o Discord usa 48 [I]. Padronizar em 48 em todas as telas (combina com as outras áreas) ou manter 63?
6. **"Ativo agora":** sem jogos, só transmissões ao vivo e voz. Aceitável? O bloco "Outros lugares para fazer amigos"/"Explorar servidores" depende de existir descoberta pública; omito?
7. **Fonte semibold estática:** posso pedir para embutir um `Inter-SemiBold.ttf` (§6.2)? Sem isso, nomes e títulos não terão peso 600.
8. **Chamadas na DM:** esconder os botões de voz/vídeo até a área de voz definir o fluxo, ou deixar desabilitados com tooltip?

---

### Apêndice — tabela de evidências rápidas

| Fato | Valor | Fonte |
|---|---|---|
| Altura da linha de amigo / com aniversário | 61 / 72 | `web@1257100` (`rowHeight`), `web@1262000` (`toValue:height??61`) |
| Altura do cabeçalho de seção (amigos) / rodapé | 50 / 48 | idem (`sectionHeight:50`, `footerHeight:48*`) |
| Lista de amigos virtualizada a partir de | 64 relações | `web@1269800` |
| Animação entrar/sair linha | 200 ms altura+opacidade | `web@7103450`+ (`componentWillEnter/Leave`) |
| Linha DM compacto/padrão/confortável | 40 / 44 / 50 | `web@788500` (`getRowHeight`) |
| Itens fixos / divisor / quick launcher / cabeçalho de seção DM | 40 / 25 / 67 / 24 | idem |
| Padding da lista de DMs | 8 topo, 8 fundo | idem |
| Avatar na linha / status | 32 / ponto 10, anel 3 | `web@12354173` |
| Ordenação DM | favoritas, depois `-timestamp(lastMessageId)` | `web@9906591` |
| Destaque de DM não lida | `getMentionCount>0` ⇒ pílula + texto forte (contagem por mensagem = I) | `web@779300` |
| Fechar DM | otimista + `DELETE`, reabre com mensagem nova, vai a Amigos se selecionada | `web@578040` |
| Seletor: atalho | `mod+k` | `web@2334403` |
| Seletor: prefixos | `@ # ! * $`, `@@` global | `web@3185000` |
| Seletor: linha / folga inferior | 34 / 10 | `web@7742000` |
| Seletor: limite de resultados | 5 geral, 100 com prefixo | `web@7730300` |
| Seletor: histórico sem consulta | 3 (com seções) ou 7 | `web@7729000` |
| Seletor: pontuação | 10/7/5/3/1 × 1000 × reforço (+0,2 amigo, +0,1 DM) | `web@10070900` |
| Container direito | prioridade BUSCA>chat>AMIGOS>PERFIL>RESUMOS>MEMBROS; perfil aberto por padrão | `web@9438487` |
| Atalhos | Ctrl+U perfil/membros, Ctrl+F busca, Ctrl+P fixadas, Ctrl+I caixa, Ctrl+Shift+T grupo | `web@2348372`–`2352577` |
| Filtros de busca (ordem) | de, em, tem, menciona, durante, antes, depois, autorType, fixadas | `web@7994800` |
| Ordenação da busca | newest (padrão) / oldest / relevância | `web@7987500` |
| Sugestões de autor | até 10; autores recentes até 15 | `web@7998000` |
| Resposta da busca | `total_results, cursor, doing_historical_index, documents_indexed` | `web@7982000` |
| Adicionar amigo | `maxLength 37`; nota ≤120 | `web@1240000`, `web@7103300` |
