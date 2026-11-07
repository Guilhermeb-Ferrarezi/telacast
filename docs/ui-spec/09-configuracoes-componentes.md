# 09 - Configurações em tela cheia e biblioteca de componentes

Legenda: **[F]** confirmado no cliente web do Discord (arquivo e trecho); **[I]** inferência ou valor conhecido do produto, não visto no bundle; **[?]** não consegui confirmar; **[D]** decisão de projeto do Telacast (Grafite), sem equivalente no Discord.

Arquivos citados: `web.0bcf9a9e83b2c3c7.js` (principal, 12,9 MB); as constantes numéricas marcadas **[F]** foram conferidas também em `web.f86c748842ee176f.js` e `web.72656029554c0370.js` (idênticas). Textos em pt-BR vêm de `12064a7d28fc7eb1.js` (catálogo pt-BR) cruzado por chave com `2552f3a00fc8192a.js` (en-US).

## 0. Aviso sobre a evidência (leia antes de usar os números)

O cache do cliente não trouxe o CSS dos componentes: os 4 `952007.*.css` só têm **variáveis de tema** (cores, opacidades), e os módulos CSS-modules (`module 233680` etc.) não estão em nenhum dos 93 arquivos. Também faltam os chunks de código das páginas de configurações (só sobrou o catálogo de textos e os enums de seção). Consequências:

- **Comportamento, tempos, molas, limites, listas, atalhos e textos: [F]**, com valores reais do bundle.
- **Larguras, alturas, paddings e raios em px: [I]** (valores do Discord que conheço) ou **[D]** (escolhidos para o Grafite). Onde o bundle dá um número de px, está marcado [F].
- **Cores**: o bundle dá as *opacidades* de cada estado (hover 12%, ativo 16% etc.) e os papéis dos tokens; as cores finais seguem o Grafite (L0–L4, âmbar).

## 1. Resumo e fora do escopo

Configurações é uma **camada de tela cheia** (não um modal) que cobre o app inteiro: coluna esquerda de navegação (218 px de lista, com cabeçalhos de grupo em caixa alta), coluna de conteúdo com largura máxima 740 px e, à direita, o botão fechar redondo com o rótulo "ESC". As mesmas peças servem para configurações do **usuário** e do **servidor** (só muda a lista de seções). Esc fecha na ordem: menu de contexto, popout, modal, camada. Páginas com formulário usam uma barra "Alterações não salvas" (salvar/redefinir); páginas de preferências aplicam na hora.

A biblioteca de componentes (botão, campo, interruptor, caixa, rádio, slider, seletor, cor, modal, popout, tooltip, menu de contexto, toast, rolagem) é descrita abaixo com medidas Telacast e os comportamentos reais do Discord (molas, atrasos, limites). O plano é criar `app/src/widgets.rs` (+ `motion.rs`, `overlay.rs`) e só depois montar as páginas.

Fora do escopo: cobrança/Nitro/impulso/assinaturas/inventário, conexões e jogos registrados, sobreposição de jogo, modo streamer, opções de desenvolvedor, Centro da Família, integrações/webhooks/emojis/stickers/soundboard/comunidade/onboarding do servidor, registro de auditoria detalhado (só citado), temas de perfil/efeitos de nome, e o conteúdo dos menus de contexto de canal/mensagem/usuário (outras áreas; aqui só o componente).

## 2. Árvore de componentes

### 2.1 Camada de configurações (de fora para dentro)

```
Camada Configurações (tela cheia, acima de tudo)                       [F: layer, role=dialog, aria-modal]
├─ Fundo L2 (conteúdo) + região esquerda L1 + filete vertical LINE
├─ Região esquerda  (largura = 218 + metade do espaço livre)           [I: flex 1 0 218 / 1 1 800 → sobra dividida]
│  └─ Barra lateral 218 px, encostada na borda direita da região
│     ├─ (usuário) cabeçalho do grupo "CONFIG. DO USUÁRIO" ... (servidor) nome do servidor em caixa alta
│     ├─ item (32 px) × N, separador 1 px, cabeçalho de grupo, ...
│     └─ rodapé: "Novidades", "Sair" (vermelho), versão do app (11 px MUTED)
├─ Região de conteúdo
│  ├─ Coluna de conteúdo: largura máx 740 + 40 de cada lado (rola sozinha)   [I]
│  │  └─ título da página (serifa 26), subtítulo, abas opcionais, seções, barra de salvar
│  └─ Ferramentas (sticky, topo 60 px): botão fechar Ø36 + "ESC" 11 px abaixo
```

Medidas Telacast (px a 100% de zoom):

| Peça | Medida | Evidência |
|---|---|---|
| Duração entrada/saída da camada | 150 ms (CSS `FULL_SCREEN_LAYER_ANIMATION_DURATION`) | **[F]** `319060`: `"150ms"` |
| Variante antiga da camada | mola tension 100 / friction 10, escala 1,1→1 e opacidade 0→1; saída escala →1,1; a camada de baixo vai a escala 0,93 e opacidade 0 | **[F]** `Mh={friction:10,tension:100}`, `componentWillEnter` |
| Movimento reduzido | só opacidade, sem escala | **[F]** `transform: i.enabled||e ? void 0 : [{scale}]` |
| Telacast: entrada | 150 ms ease-out, escala 1,04→1 + opacidade; saída 150 ms inversa | **[D]** |
| Largura da barra lateral | 218 (lista útil ≈ 192: padding 20 à esquerda, 6 à direita) | **[I]** |
| Item da barra | altura 32, gap 2, padding-x 10, raio 8, fonte 14,5 | **[D]** (Discord ≈ 6+20+6 px, raio 4) |
| Cabeçalho de grupo | caixa alta 11 px, MUTED, padding 6×10, 16 px acima | **[I]** |
| Separador | 1 px LINE, margem 8×10 | **[I]** |
| Coluna de conteúdo | largura máx 740; padding topo 60, laterais 40, fundo 80 | **[I]** |
| Fechar | círculo Ø36, traço 2 px MUTED (hover TEXT, fundo L3); "ESC" 11 px MUTED a 8 px abaixo | **[I]** (texto "Esc" **[F]** i18n) |
| Seção | 40 px entre seções; rótulo caps 11 → 8 px → controle | **[I]/[D]** |
| Cartão de formulário | L3, filete LINE2 1 px, raio 12, padding 16–20 | **[D]** (= `theme::glass(12)`) |

Posicionamento horizontal (largura da janela W): `side=218`, `extra=max(0, W-218-800)/2`, região esquerda = `218+extra`; a barra fica no fim dessa região e a coluna de conteúdo começa logo depois dela. Em janelas estreitas (< 1018) a região esquerda é só 218 e a coluna encolhe.

### 2.2 Seções (itens da barra lateral)

Os identificadores de seção do usuário são **[F]** (`enum` em `web.0bcf…` offset ≈ 8,76 M): `account, account-standing, profile-customization, content-and-social, data-and-privacy, family-center, sessions, authorized-apps, connections, clips, premium, guild-boosting, subscriptions, inventory, billing, appearance, accessibility, voice, text, notifications, keybinds, language, windows, linux, streamer-mode, advanced, activity-privacy, registered-games, overlay, developer-options`. O Discord agrupa em **Config. do usuário** / cobrança / **Config. do aplicativo** / **Config. de atividade** **[F]** (strings "Config. do aplicativo", "Config. de atividade", "Configurações de cobrança").

**Usuário no Telacast** (ordem proposta; rótulos pt-BR do catálogo):

| Grupo | Item | Id | Observação |
|---|---|---|---|
| Config. do usuário | Minha conta | `account` | P0 |
| | Perfis | `profile-customization` | P1 |
| | Privacidade (Dados e privacidade + Conteúdo e social) | `data-and-privacy` | P1 |
| | Dispositivos (Sessões) | `sessions` | P2: a tabela `sessions` já existe |
| separador | | | |
| Config. do aplicativo | Aparência | `appearance` | P0 (inclui Acessibilidade, ver 2.3) |
| | Voz e vídeo | `voice` | P0 |
| | Notificações | `notifications` | P0 |
| | Atalhos do teclado | `keybinds` | P1 |
| | Idioma e horário | `language` | P3 |
| | Avançado | `advanced` | P3 |
| separador | Novidades, **Sair** (vermelho), versão | | |

**Servidor** (cabeçalho com o nome do servidor; só aparece quem tem permissão ou é dono). O enum de seções de servidor **[F]** contém `OVERVIEW, ROLES, CHANNELS, MEMBERS, INVITES, MODERATION, BANS, AUDIT_LOG, SECURITY, DELETE, ...`. Telacast: **Visão geral, Cargos, Canais, Membros, Convites, Moderação (inclui Banimentos)**, separador, **Excluir servidor** (vermelho, só dono). Rótulos **[F]** pt-BR: "Visão geral do servidor", "Cargos", "Canais", "Membros", "Convites", "Moderação", "Banimentos", "Registro de auditoria", "Excluir servidor".

### 2.3 Conteúdo de cada página do usuário

Textos em pt-BR são os do catálogo **[F]**; a disposição é **[I]/[D]**. Autoaplica = muda ao tocar; Salvar = usa a barra de alterações.

1. **Minha conta** (Salvar só para o nome). Cartão do perfil (faixa na cor do perfil, avatar 80 px, nome exibido, @usuário, botão "Editar perfil" que leva a Perfis). Cartão "Informações de conta": **Nome de usuário** (3–32, tabela `users`) com Editar; e-mail e telefone **não existem** no esquema (ocultar até haver). Seção "Senha e autenticação": botão **Mudar senha** (modal: senha atual, nova, confirmar); 2FA fica fora. Seção "Remoção da conta": **Desativar conta** (secundário vermelho), **Excluir conta** (crítico) com modal de confirmação digitando o nome de usuário.
2. **Perfis** (Salvar). Abas "Perfil do usuário" e "Perfis por servidor" (enum `USER_PROFILE/GUILD` **[F]**). Formulário à esquerda e **Prévia** do cartão à direita (sticky). Campos: **Nome exibido** (1–32 conforme `users.display_name`), **Pronomes** (≤ 40 **[I]**), **Avatar** (Mudar avatar / Remover, recorte quadrado), **Cores do perfil** (20 cores predefinidas + personalizada, ver 2.6 Seletor de cor), **Sobre mim** (≤ 190 **[I]**, contador `n/190`). Em "Perfis por servidor": **Apelido do servidor** (`members.nickname`).
3. **Privacidade** (autoaplica). "Mensagens diretas": interruptor **Permitir mensagens diretas de membros do servidor**, interruptor **Solicitações de mensagens**. "Quem pode enviar pedido de amizade": caixas **Todos / Amigos de amigos / Membros do servidor** **[F]**. Lista **Usuários bloqueados** (tabela `blocks`) com Desbloquear. "Atividade": mostrar quando estou transmitindo, mostrar status online. Fora: "Mensagem direta segura" (filtro de mídia), exportar dados.
4. **Aparência** (autoaplica), com a Acessibilidade embutida:
   - **Tema**: hoje só "Grafite (escuro)"; manter a fileira de 3 blocos (Escuro / Claro / Sincronizar com computador **[F]**) escondida até haver tema claro (pergunta 1).
   - **Densidade da interface** (compacta / padrão / espaçosa; flags `UI_DENSITY_COMPACT/SPACIOUS` **[F]**).
   - **Exibição das mensagens**: dois blocos de rádio com mini-prévia, "Acolhedor: moderno, bonito e agradável para os olhos." e "Compacto: encaixe mais mensagens na tela de uma vez." **[F]**.
   - **Tamanho da fonte do chat**: slider com marcas `[12,14,15,16,18,20,24]` px, padrão 16 (mín 12, máx 24) **[F]** `FONT_SIZES`. Telacast: padrão 15 (o corpo atual é 14,5) **[D]**.
   - **Espaço entre grupos de mensagens** (slider, marcas 0/4/8/16/24 **[I]**) **[F]** rótulo; **Nível de zoom**: slider com marcas `[50,67,75,80,90,100,110,125,150,175,200]`% padrão 100 **[F]** `ZOOM_SCALES`; atalhos `mod+plus`, `mod+minus`, `mod+0` **[F]**.
   - **Acessibilidade**: **Movimento reduzido** (automático / ligado / desligado; `prefersReducedMotion:"auto"` **[F]**); **Saturação** (slider 0–100%, passo 5, `SATURATION_INCREMENTS` **[F]**); **Sempre sublinhar links**; **Mostrar cores dos cargos ao lado dos nomes** (modo do cargo: no nome ou ponto, `roleStyle:"username"` **[F]**); **Reproduzir GIFs automaticamente quando possível**; **Mostrar botão de enviar mensagem**; **Mostrar ícones nos interruptores** (`switchIconsEnabled` **[F]**); **Duração dos avisos** (slider 3–20 s, padrão 3 s **[F]**); modo de contraste (padrão / alto) **[F]**.
5. **Notificações** (autoaplica). Interruptor **Ativar notificações na área de trabalho**; seção **Sons** (lista de interruptores: Mensagem, entrar/sair da chamada, Silenciar/Desativar áudio, chamada recebida; cada um com botão ▶ de teste) e volume dos sons; **Piscar na barra de tarefas**; **Contador de não lidos no ícone**; **Taxa de texto-para-voz** (slider). Padrões por servidor ficam em Visão geral do servidor ("Configurações de notificação padrão" **[F]**).
6. **Atalhos do teclado** (autoaplica). Tabela agrupada, uma linha por ação: nome à esquerda, teclas como "keycaps" à direita, clique grava, lixeira remove, botão **Adicionar um atalho**. Grupos **[F]** (ids): `NAVIGATION`, `DND` (arrastar e soltar), `CHAT`, `VOICE_AND_VIDEO`, `MISCELLANEOUS`, `MESSAGE`. Ver 4.5 para as combinações padrão reais.
7. **Voz e vídeo** (autoaplica). Todos os rótulos **[F]**: **Dispositivo de entrada** + **Volume de entrada**; **Dispositivo de saída** + **Volume de saída**; **Teste do microfone** (botão + medidor de nível ao vivo); **Modo de entrada** (rádio: **Detecção de voz** / **Aperte para Falar**); se detecção de voz: interruptor **Determinar sensibilidade de entrada automaticamente** e, se desligado, slider **Sensibilidade de entrada** com o medidor logo abaixo; se aperte-para-falar: gravador de **Atalho** e slider **Atraso na voz após soltar o Aperte para Falar**; **Supressão de ruído**, **Cancelamento de eco**, **Controle automático do microfone** (interruptores); **Câmera** (seletor) + botão **Testar vídeo** (prévia 16:9); seção "Transmissão" (Telacast): monitor, quadros por segundo (30/60/90/120/144) e qualidade (4/8/15/30 Mbps), que hoje vivem no modal "Transmitir tela".

### 2.4 Conteúdo de cada página do servidor

| Página | Conteúdo | Salvamento |
|---|---|---|
| **Visão geral** | **Nome do servidor** (1–50, `servers.name`); ícone (clique abre seletor de arquivo, remover); **Canal de mensagens do sistema** (seletor de canal) com interruptores de aviso de entrada; **Configurações de notificação padrão** (rádio: Todas as mensagens / Apenas @menções); no fim, cartão vermelho "Excluir servidor" (só dono) | Barra de salvar |
| **Cargos** | Painel esquerdo: busca, botão **Criar cargo**, lista arrastável (cor, nome, contagem de membros); **@everyone** fixo no fim. Editor à direita com abas **Exibição** (nome, **Cor do cargo** com seletor, **Exibir membros do cargo separadamente dos membros conectados**, **Mencionável**), **Permissões** (busca + grupos "Permissões gerais do servidor", "…de canal de texto", "…de canal de voz", "Permissões avançadas"; cada permissão é uma linha com nome, descrição e interruptor; **Administrador** em vermelho com aviso), **Gerenciar membros** (lista + Adicionar). Menu do cargo: **Duplicar cargo**, **Excluir cargo** | Barra de salvar |
| **Canais** | Árvore de categorias e canais (linha 40 px, alça de arrastar, ícone do tipo, nome, lápis e lixeira no hover). Botões **Criar canal** / **Criar categoria**. Clicar abre o modal "Editar canal": **Nome** (1–100), **Assunto** (≤ 1024 **[I]**), **Modo Lento** (seletor), **Canal privado** (interruptor), **Excluir canal** | Aplica no modal (Salvar) |
| **Membros** | Busca **"Pesquisar pelo nome de usuário ou ID"**; tabela com avatar+nome, **Entrou** (data), cargos (chips coloridos), menu ⋯: Editar cargos, Apelido, **Castigo** (timeout), **Expulsar**, **Banir**; rolagem infinita; contador no topo | Imediato (com confirmação em ações destrutivas) |
| **Convites** | Tabela: **Link de convite** (código + copiar), **Criado por**, **Usos** (`n / máx`), **Expira**; botão **Revogar** (vermelho). Botão **Criar convite**: modal com **Expira após** (30 min, 1 h, 6 h, 12 h, 1 dia, 7 dias, nunca **[I]**) e **Número máximo de usos** (Sem limite, 1, 5, 10, 25, 50, 100 **[I]**; "Sem limite" **[F]**) | Imediato |
| **Moderação** | **Nível de verificação** (seletor), **Proteção contra invasões** (interruptor), sub-aba **Banimentos** (busca + lista com **Desbanir**), sub-aba **Castigos** (membros em timeout com tempo restante). Registro de auditoria: P3 | Imediato / barra de salvar nos seletores |

Regra de acesso: a barra de servidor só lista páginas para as quais o ator tem permissão (Gerenciar servidor, Gerenciar cargos, Gerenciar canais, Expulsar/Banir etc.; o dono vê tudo). A checagem real é sempre no servidor.

### 2.5 Tokens de estado (do Discord) e equivalentes Grafite

As opacidades abaixo são **[F]** (variáveis `.theme-dark` em `952007.c7b995731f0792cf.css`); a coluna Grafite é decisão **[D]**.

| Papel | Discord (escuro) | Grafite |
|---|---|---|
| Item interativo: repouso / hover / ativo / selecionado | branco 8 / 12 / 16 / 20 % (`--interactive-background-*`) | transparente / L2 (sobre L1) ou L4 (sobre L3) / L4 / L3 |
| Botão secundário: repouso / hover / ativo | 12 / 16 / 20 %, borda 4 % | L3 / L4 / L4 com borda AMBER |
| Botão primário | `blurple-50` / 60 / 65, borda branca 8 % | AMBER / AMBER_HOT / AMBER, tinta AMBER_INK |
| Botão crítico primário | `red-new-50` / 60 / 65, texto branco | RED, tinta L0 |
| Botão crítico secundário | fundo 12 / 16 / 20 %, texto e ícone `red-new-18` | contorno RED (já em `Kind::Danger`) |
| Bordas | sutil 12 %, normal 20 %, forte 44 %, mudo 4 %, foco `blue-new-30` | LINE (12 %), LINE2 (20 %), TEXT2 (44 %), foco = AMBER_HOT |
| Campo: fundo / borda / ativo / erro / somente leitura | preto 8 % / 20 % / marca / `red-new-18` / branco 8 % | uma camada abaixo do contêiner / LINE2 / AMBER / RED / LINE |
| Caixa e rádio: borda | 64 % (hover 80 %); marcado: fundo marca, borda 12 % | MUTED (hover TEXT2); marcado AMBER |
| Interruptor: desligado / ligado | fundo preto 12 %, borda 20 % (hover 40 %) / marca, borda 12 % | L1 + LINE2 / AMBER; polegar branco (**Grafite: TEXT2 desligado, AMBER_INK ligado**) |
| Slider: trilho | `neutral-47` | L4 trilho, AMBER preenchimento |
| Escurecimento de modal | preto **72 %**; lightbox 92 %; fundo do menu de contexto 72 % | igual (alpha 184) |
| Toast sucesso / crítico | fundo verde / vermelho 20 % + borda `*-42` | fundo L3 + faixa de 2 px OK / RED (sem tingir o cartão inteiro) |
| Rolagem fina / auto | polegar `neutral-38` / `neutral-36`, trilho transparente | polegar L4→LINE2 (hover MUTED), trilho transparente |

### 2.6 Biblioteca de controles (medidas e regras)

Escala de texto usada nas medidas (Mana usa `body-xs/sm/md`): xs 12/16, sm 13,5/18, md 14,5/20 **[D]** (Discord ≈ 12, 14, 16) **[I]**.

**Botão** — `Button` do Mana **[F]** (`web.0bcf…` ≈ 10,49 M): props `variant`, `size ∈ {xs, sm, md}`, `text`, `icon`, `iconPosition (start|end)`, `fullWidth`, `loading`, `rounded`, `minWidth`, `disabled`.

| Variante | Uso | Evidência |
|---|---|---|
| `primary` | ação principal (1 por área) | **[F]** |
| `secondary` | ação neutra, Cancelar | **[F]** |
| `critical-primary` | confirmar ação destrutiva | **[F]** |
| `critical-secondary` | destrutivo discreto (Sair, Revogar) | **[F]** |
| `icon-only` / `tertiary` (fundo transparente, texto `neutral-4`) | ícone puro, ação terciária | **[F]** `--redesign-button-tertiary-*` |
| `overlay-primary` / `overlay-secondary` | sobre imagem/vídeo (preto 56–64 %) | **[F]** |
| `expressive`, `connected`, `premium` | fora do escopo | **[F]** |

| Tamanho | Altura | Padding-x | Fonte | Ícone | Raio |
|---|---|---|---|---|---|
| xs | 24 | 8 | 12 | 12 | 7 |
| sm | 32 | 12 | 13 | 16 | 9 |
| md | 40 | 16 | 14 | 18 | 9 |

Alturas e paddings **[D]** (xs/sm/md e as fontes `body-xs/sm/md` medium são **[F]**; hoje `big_button`=40 e `small_button`=30 → subir o pequeno para 32). Ícone só: quadrado h×h. Estados: hover 120 ms (o `anim` atual), pressionado (borda AMBER), desativado opacidade 0,5 **[I]**, foco por teclado: anel 2 px AMBER_HOT a 2 px de distância, só quando o último input foi teclado **[F]** (`keyboardModeEnabled`). `loading`: o rótulo permanece (largura não muda), três pontos pulsando no centro, o rótulo fica invisível; ao terminar o spinner some após **500 ms** e cliques/hover/teclas são ignorados enquanto `loading||disabled` **[F]**. Ordem num rodapé: secundário, depois primário (primário à direita). `minWidth` opcional (ex.: 96).

**Campo de texto** — `TextInput` do Mana **[F]** (≈ 11,11 M): `size` (md padrão), `leading`/`trailing` (acessórios), `clearable` (✕ aparece só com texto), `revealable` (olho, só `type=password`), `showCharacterCount` (`n/máx`), `maxLength/minLength`, `error`, `validateOn:"change"`, `readOnly`, `defaultDirty`.

| Item | Medida Telacast |
|---|---|
| Altura sm / md / lg | 32 / 40 / 48 **[I]** |
| Raio, borda | 9 (igual ao tema), 1 px; foco: borda AMBER (sem anel extra) |
| Padding horizontal | 12; com acessório 8 do lado dele; acessório Ø16 |
| Rótulo | acima, caps 11 MUTED + `*` AMBER se obrigatório; descrição abaixo do rótulo 12 px MUTED; ajuda/erro abaixo do campo, 12 px |
| Erro | borda RED + mensagem RED desliza de cima: margin-top 0→8, altura 0→auto, opacidade 0→1, `translateY(-100%)→0`, mola tension 250 **[F]** (`A` em 11,12 M) ≈ 150 ms |
| Validação | só depois da primeira edição (`dirty`), mensagens "mínimo N", "máximo N" **[F]** |
| Layout do campo | `vertical` (padrão) ou `horizontal` (rótulo à esquerda, `fullWidth`) **[F]** |
| Tipos de limites no Telacast | usuário 3–32, nome exibido 1–32, servidor 1–50, canal 1–100 (CHECKs do esquema) |

**Interruptor (switch)** — **[F]** (`data-size="md"`, `data-mana-component="switch"`, ≈ 11,09 M). Trilho com borda 1 px; polegar desenhado num SVG `viewBox 24` como retângulo 16×16 raio 8 a 4 px das bordas; durante a transição o polegar **estica e achata** (largura 16→22, altura 16→10 no meio do trajeto) e volta a 16×16; duração **300 ms** (**200 ms** e sem esticar, só deslizando, com movimento reduzido); desativado = opacidade **0,5**; ícone ✓/✕ dentro do polegar só se "Mostrar ícones nos interruptores" estiver ligado; cores transitam por mola. A linha inteira (rótulo+descrição) é clicável no menu **[F]**.
Telacast: trilho **44×24** **[D]**, polegar Ø16 com inset 4, curso 20 px. Linha de configuração: rótulo 14,5 TEXT, descrição 12,5 MUTED abaixo, interruptor à direita, 16 px de padding vertical e filete LINE entre linhas.

**Caixa de seleção (checkbox)** — **[F]** (`data-mana-component="checkbox"`, ≈ 10,50 M): quadro **20×20**; props `label`, `description` (corpo sm, cor sutil), ícone à esquerda, `readOnly`, `disabled`, indeterminado, variante "indicador" só de exibição (usada dentro de menus); animações de entrada/saída do ✓ (classes `animateIn/animateOut`). Telacast: quadro 20×20, raio 6, borda 1,5 px MUTED, marcado = AMBER cheio com ✓ AMBER_INK (traço 2 px desenhado com 2 segmentos, 120 ms); rótulo 14,5 a 10 px do quadro; área clicável = linha inteira.

**Rádio** — **[F]** (`ea.T` no menu, indicador 20×20 aproximado). Telacast: círculo Ø20, borda 1,5 px MUTED; marcado = anel AMBER + miolo Ø10 AMBER. Existe também o **bloco-rádio** (Aparência: Acolhedor/Compacto, Tema): cartão L3 altura mín 56, título + descrição, selecionado = borda AMBER 1 px **[I]**.

**Slider** — **[F]** (≈ 11,08 M e 205,513): grabber **16 px**, que vai a **20 px** em hover/arraste/foco; barra **4 px**, que vai a **6 px** nos mesmos estados; transição **100 ms** (mola rígida); preenchimento até o grabber; **balão** com o valor (tooltip aberto à força) enquanto o ponteiro está sobre o grabber ou arrastando; `minValue 0`, `maxValue 100`, `keyboardStep 1` (padrão), `markers` com rótulo + risco (`markAbove/markBelow`), `stickToMarkers` (gruda na marca mais próxima), `equidistant` (marcas espaçadas igualmente mesmo com valores irregulares, ex.: zoom e fonte), marca de valor padrão, variante `mini`, `hideBubble`, `onValueRender`. Valor do rótulo padrão = `NN%`. Telacast: altura total 24 (44 com marcas), trilho L4, preenchimento AMBER, grabber TEXT (sem sombra), marcas 1×6 px LINE2 com rótulo 11 px MUTED acima; teclado ←/→ ± passo, Home/End; clique no trilho salta e inicia arraste.

**Menu suspenso (select)** — **[F]** (≈ 960,5 K): botão-campo com o valor (texto md medium) e seta (⌃ aberto, ⌄ fechado, tamanho sm); `placeholder`; `clearable` (✕ xs); `isProcessing` mostra 3 pontos (raio 3,5); popout **sem animação**, `spacing 0` (colado ao botão), **largura = largura do botão** (ou `auto`/fixa), `maxVisibleItems = 7` (mais que 7 → rolagem fina; ≤ 7 → sem barra), altura medida por uma cópia oculta das 7 primeiras opções; opção = rótulo + ✓ 20×20 à direita se selecionada, esmaecida se desativada; `closeOnSelect`, `preventCloseOnSelect` por opção; ↓ abre, Esc fecha **sem propagar** (não fecha a camada), setas navegam com volta ao início/fim (`wrap:true`), foco volta ao botão; posição `bottom` (padrão) ou `top`. Seleção única (`select` troca) ou múltipla (`toggle` num Set) **[F]**. Telacast: campo md 40 px, raio 9; lista L3 + LINE2, raio 10, opção 36 px, padding-x 12, hover L4; sem sombra extra.

**Seletor de cor** — Cores predefinidas de cargo **[F]** (`rr` em 0,39 M): `#1abc9c #2ecc71 #3498db #9b59b6 #e91e63 #f1c40f #e67e22 #e74c3c #95a5a6 #607d8b #11806a #1f8b4c #206694 #71368a #ad1457 #c27c0e #a84300 #992d22 #979c9f #546e7a` (20, duas fileiras de 10) e cor padrão cinza `#99aab5` = "sem cor" **[F]**. Para nomes com cor sólida o contraste mínimo contra o fundo é **3:1** (`minContrastRatio: 3`) **[F]**. Telacast: grade de 20 amostras de 28×28 raio 8, 8 px de gap, selecionada = anel TEXT 2 px + ✓; amostra "personalizada" abre `egui::color_picker` embutido + campo hex de 6 dígitos; amostra "padrão" riscada. Avisar (texto RED 12 px) se o contraste contra L2 < 3:1. Sem gradientes e sem "cores aprimoradas".

**Modal** — dois sistemas coexistem **[F]**: o legado `ModalRoot` (`935462`, tamanhos `small|medium|large|dynamic`) e o Mana `Modal` (≈ 10,97 M; `size sm|md|lg`, `paddingSize sm|lg`, `maxHeight default|viewport`, `dismissable`, `role dialog|alertdialog`, `fullScreenOnMobile`).

| Item | Valor | Evidência |
|---|---|---|
| Escurecimento (scrim) | preto 72 %, entra com mola tension 1000 / friction 48; "lightbox" 92 %; variante com desfoque (não replicar) | **[F]** `--background-scrim`, `150591` |
| Entrada do cartão (Mana) | opacidade 0→1 e escala **0,9→1**, mola tension 1000 / friction 48 (≈ 170 ms), com atraso de **64 ms** | **[F]** `kS` em `150591` |
| Saída | mola tension 1200 / friction 80 (superamortecida, ≈ 200 ms), escala →0,9, opacidade →0; movimento reduzido: só opacidade | **[F]** |
| Entrada do legado | 200 ms, ease-out quadrático, escala 0,7→1 (`subtle`: sem escala) | **[F]** `ModalRoot` |
| Animação de empilhar | `slide-up` padrão; `fade` com movimento reduzido | **[F]** `WMy.SLIDE_UP/FADE` |
| Larguras | sm 440, md 600, lg 800 | **[I]** |
| Raio, borda, sombra | 14 (Grafite `glass(14)`), LINE2, sombra do tema | **[D]** |
| Cabeçalho | título (serifa 22) + subtítulo 13,5 MUTED + botão fechar ✕ Ø32 no canto; altura ≈ 64 | **[D]** |
| Corpo | rola em barra fina; sombra/risco no topo quando rolou > **12 px** do topo, no fundo quando faltam > 12 px (reavaliado a cada 200 ms) | **[F]** `getDistanceFromTop()>12` |
| Rodapé (barra de ações) | botões à direita (secundário, primário); slot esquerdo opcional (caixa "Não mostrar isso novamente" **[F]**); padding 16, altura ≈ 72, filete LINE acima | **[F]/[D]** |
| Foco | foco inicial no **último** botão `primary/critical-primary/expressive`; foco preso dentro; devolve ao elemento que abriu (`returnRef`) | **[F]** `findLastIndex` |
| Confirmação | `alertdialog`, ações [Cancelar (secundário), Confirmar (`critical-primary` ou `primary`)], carregando enquanto `onConfirm` resolve; se falhar mostra aviso inline de erro e reabilita | **[F]** `732159` |
| Lazy | se o conteúdo demora > **300 ms**, mostra modal pequeno com spinner | **[F]** `h()` em `901248` |
| Empilhamento | `replace` (esconde os de baixo, padrão) ou `stack`; só o último recebe Esc | **[F]** |
| Fechar | Esc ou clique no escurecimento, exceto `dismissable:false`/`closable:false` | **[F]** |

**Popout** — **[F]** (`265872`, `44808`): posição padrão `right`, alinhamento derivado (`top|bottom` → `left`; `left|right` → `top`), `spacing 8`, inverte automaticamente se não couber (`autoInvert`), empurra para dentro da janela (`nudgeAlignIntoViewport`), animações `TRANSLATE` (padrão) / `SCALE` / `FADE` / `NONE`; **movimento reduzido força FADE**; o estado "animando" é limpo em `transitionend` ou no máx. 200 ms; a animação começa 10 ms após montar; popout carregado sob demanda mostra spinner após 300 ms; fecha ao clicar fora, Esc ou pedido do alvo; `clickTrap` opcional. Telacast: L3 + LINE2, raio 12, sombra do tema; entrada 120 ms ease-out (opacidade + 6 px de deslocamento no sentido oposto à âncora) **[D]**.

**Tooltip** — **[F]** (`900002`, `188645`): atraso padrão **300 ms** (`delay:300, position:"top"`; também há usos de 750 ms), `spacing 8`, `hideOnClick`, esconde ao sair/clique/menu de contexto/blur, mostra no foco **só** se o último input foi teclado; `overflowOnly` (só mostra se o texto está truncado); entrada **escala 0,95→1 + opacidade**, mola tension 2400 / friction 52 (≈ 100 ms); movimento reduzido: só opacidade; cores `primary|grey|brand|green|red`; setinha (caret) SVG **16×10** com ponta arredondada, deslocamento limitado a `largura/2 − 8`; largura máx 200 (simples) ou 272 / 372 (rico, padding md/lg) **[F]** derivado da função `A`. Telacast: fundo L4 + LINE2, raio 8, texto 13 TEXT, padding 6×10, caret 14×7 triangular preenchido da mesma cor; posições top/bottom/left/right; animação só opacidade 90 ms **[D]**.

**Menu de contexto** — **[F]** (menu em `web.0bcf…` offsets ≈ 860 K–896 K; abertura em `318009` e `442433`):

| Item | Valor |
|---|---|
| Abertura | no cursor (ou no centro do alvo com Shift+F10), posição `right`/alinhamento `top` com inversão automática; **sem animação**; clique fora só fecha (bloqueia o clique: `blockPointerEvents`); fecha ao redimensionar a janela; conteúdo carregado sob demanda mostra spinner após 300 ms |
| Largura mínima | 188 px, ou 144 quando uma flag interna do componente está ligada (provavelmente compacto/mobile **[?]**), via `--custom-menu-flexible-min-width`; margem da janela 16 px (48 com a mesma flag) |
| Item | leading (ícone/avatar/status) • rótulo (rola em marquee se não couber: duração `clamp(excesso/40, 2, 5)` s) + subtexto `body-xs` opcional • atalho • selo • "…" de carregando • ícone à direita • seta de submenu. Altura 32 (44 com subtexto), padding-x 8, raio 6, menu com padding 6–8 e raio 10 **[D]** |
| Tipos de item | normal, **checkbox**, **rádio**, **interruptor**, **campo de texto**, **controle** (ex.: slider de volume), controle composto, item personalizado **[F]** |
| Cores | `default`, `brand`, `danger`, `premium`, `premium-gradient`, `success` **[F]**; Telacast usa default, danger (texto RED, hover alpha(RED, 0,14)), brand (AMBER), success (OK) |
| Separadores e grupos | `separator` explícito, e **grupos** (`groupstart/groupend`) recebem separador automático entre eles quando o anterior tinha itens **[F]**. Separador = 1 px LINE com 4 px de margem vertical **[I]** |
| Submenu | abre ao **passar o mouse** (ou foco do teclado), `placement right-start`, `spacing 4`, `autoFlip`, `viewportPadding 48`, `shiftPadding 8`, `maxHeight 360` com rolagem; submenus longos viram lista virtualizada com `listPadding [6,0,6,8]` **[F]** |
| Teclado | ↑/↓ circulam (volta ao início/fim), → entra no submenu, ← sai, Enter/Espaço aciona, Esc fecha; digitar não busca |
| Fechar ao acionar | itens normais fecham e executam a ação no frame seguinte; **checkbox/rádio/interruptor não fecham**; Shift segura aberto quando `dontCloseOnActionIfHoldingShiftKey` **[F]** |
| Vazio | um item desativado "vazio" |

**Toast** — **[F]** (`345874`, `435462`, `252392`): um toast por vez por superfície (app, overlay, popout…); os seguintes entram numa **fila FIFO**; duração = `max(toast.duration ?? 3000, preferência do usuário)`; a preferência vai de **3 a 20 s**, padrão 3 s; variantes `default`, `success` (ícone ✓ verde), `critical` (ícone vermelho); anuncia ao leitor de tela (`assertive` se crítico); posição `top` (padrão) ou `bottom`; no app o topo fica abaixo da barra superior e o fundo a **132 px** da borda; entra e sai com animação CSS cujas durações vêm de tokens (`toast.ANIMATION_DURATION_MS`, `QUEUE_ENTER_DELAY_MS`, valores **[?]**); texto `body-md` forte com limite de linhas por token. Telacast **[D]**: cartão L3 + LINE2 raio 10, faixa esquerda 3 px OK/RED, ícone 16, texto 14, padding 12×14, largura máx 420, centralizado no topo a 16 px, entra 180 ms (desce 8 px + opacidade), sai 150 ms, intervalo de 120 ms entre toasts da fila; clique fecha.

**Barra de rolagem fina** — **[F]**: três tipos `thin`, `auto` e `none` (`689175`), opção `fade` (esmaece as bordas), `scrollbarGutter` `stable` (reserva o espaço, padrão) / `auto` / `false`; cores `thin` polegar `neutral-38`, `auto` polegar `neutral-36`, trilhos transparentes; rolagem programática anima com mola; PageUp/PageDown = **0,9 × altura visível**. Larguras: thin ≈ 8 px (polegar 4 px visível) e auto ≈ 16 px (polegar 8) **[I]**. Telacast: tudo `thin`: faixa 8 px, polegar 4 px, raio 2, cor `LINE2` (hover MUTED, arrastando TEXT2), altura mínima 40, **sem trilho**, visível ao rolar/hover do contêiner.

## 3. Estados e interações

| Elemento | Repouso | Hover | Pressionado | Selecionado/ligado | Foco (teclado) | Desativado | Carregando | Erro |
|---|---|---|---|---|---|---|---|---|
| Item da barra lateral | MUTED, sem fundo | TEXT, fundo L2 (120 ms) | fundo L3 | fundo L3, TEXT | anel AMBER_HOT 2 px | n/a | n/a | n/a |
| Botão | ver 2.5 | +1 passo de camada (120 ms) | borda AMBER | n/a | anel 2 px | opacidade 0,5, sem hover | rótulo oculto + pontos | n/a |
| Campo | L3/LINE2 | borda TEXT2 | n/a | borda AMBER | igual selecionado | opacidade 0,5 | n/a | borda RED + mensagem desliza |
| Interruptor | L1 + LINE2 | borda MUTED | polegar estica | AMBER | anel | 0,5 | n/a | n/a |
| Slider | trilho 4, grabber 16 | trilho 6, grabber 20, balão | idem | n/a | grabber 20 | 0,5 | n/a | n/a |
| Menu: item | sem fundo | fundo L4 (foco por mouse **ou** teclado) | n/a | ✓ à direita | igual hover | texto MUTED, sem hover | "…" pulsando | n/a |

Estados vazios: lista de bloqueados ("Você não bloqueou ninguém."), convites ("Nenhum convite ativo."), banimentos ("Ninguém foi banido."), membros sem resultado ("Nenhuma opção corresponde à sua busca" **[F]** pt-BR), cargos sem membros. Todos: texto central MUTED 13,5 e, abaixo, a ação possível.

**Barra "Alterações não salvas"** (páginas com Salvar): barra flutuante no fundo da coluna de conteúdo (largura da coluna, altura 52, L3 + LINE2, raio 12, 16 px da borda), texto "Há alterações não salvas" à esquerda e botões **Redefinir** (secundário, texto "Redefinir" **[F]**) e **Salvar alterações** (primário **[F]**); entra subindo 150 ms; ao tentar sair com alterações pendentes ela **treme** 3 ciclos de ±4 px em 300 ms (e o botão fechar não fecha) **[I]**; fechar mesmo assim pede "Descartar as alterações? Há alterações não salvas, deseja mesmo descartá-las?" **[F]** textos. Páginas de preferências não têm barra.

**Movimento reduzido** (`Reduzir movimento`: automático/ligado/desligado) zera: escala de modal e tooltip, esticar do interruptor, deslocamento do popout e do toast, e a entrada da camada vira só opacidade **[F]** (todas as quatro implementações do bundle consultam `reducedMotion.enabled`). No egui: `motion::tween` devolve o alvo na hora quando ligado.

## 4. Regras de comportamento

1. **Abrir/fechar.** Engrenagem no painel de usuário abre "Minha conta"; "Configurações do servidor" no menu do servidor abre "Visão geral". Sempre uma camada por vez; abrir outra troca o conteúdo. O fechar é o ✕ redondo ou Esc. Lembra a última página só durante a sessão.
2. **Cadeia do Esc** **[F]** (`POP_LAYER` em `390435`): (a) chamada recebida é recusada, (b) menu de contexto fecha, (c) popout fecha, (d) modal dismissable fecha, (e) a camada é fechada. Cada Esc consome um nível. O seletor (select) fecha sem propagar. Telacast mantém `overlays: Vec<Overlay>` e processa Esc uma vez por frame.
3. **Navegação.** Clicar item troca a página sem animar o conteúdo; a rolagem da coluna volta ao topo; o item selecionado rola para ficar visível na barra lateral. Itens de grupos não permitidos não aparecem (nada de cinza).
4. **Salvar.** Páginas com formulário: edição local em `Draft` (cópia do estado do servidor); Salvar envia e, com sucesso, mostra toast "Alterações salvas"; falha de campo mostra o erro no campo (`S2C::Error` com `field`); Redefinir volta ao último estado do servidor. Preferências locais aplicam e persistem na hora (eframe `Storage`).
5. **Atalhos** — combinações padrão reais **[F]** (`web.0bcf…`, objetos `binds:[...]`; `mod` = Ctrl no Linux/Windows, Cmd no macOS): servidor anterior `mod+alt+up` / `mod+shift+[` / `ctrl+shift+tab`, próximo `mod+alt+down` / `mod+shift+]` / `ctrl+tab`; canal anterior/próximo `alt+up` / `alt+down`; histórico voltar/avançar `alt+left` / `alt+right` (no macOS `mod+[` / `mod+]`); alternar último servidor e DMs `mod+alt+right`; voltar ao canal de voz `mod+alt+left`; ir ao servidor N `mod+1…9`; seletor rápido `mod+k` (e `mod+t` na web); marcar canal como lido `esc`; marcar servidor como lido `shift+esc`; rolar `pageup` / `pagedown`; ir à primeira não lida `shift+pageup`; emojis `mod+e`; GIFs `mod+g`; stickers `mod+s`; fixadas `mod+p`; caixa de entrada `mod+i` (marcar a do topo como lida `mod+shift+e`); lista de membros `mod+u`; buscar `mod+f` (`mod+shift+f` busca no canal atual); novo servidor `mod+shift+n`; nova DM em grupo `mod+shift+t`; nova thread `mod+shift+alt+n`; enviar arquivo `mod+shift+u`; copiar link do canal `mod+shift+l`; recolher/expandir categorias `mod+shift+a`; microfone `mod+shift+m`; áudio `mod+shift+d`; atender chamada `mod+return`; iniciar chamada `ctrl+'`; soundboard `mod+shift+b`; zoom `mod+plus`, `mod+minus`, `mod+0`; lista de atalhos `mod+/`; menu de contexto do elemento focado `shift+f10`; ajuda `f1`. Gravador: clique entra em "Gravando… pressione as teclas", Esc cancela, Backspace limpa, conflito mostra aviso. No Linux/Wayland o Discord pede ao portal do sistema para atalhos globais (`reofferWaylandPortalShortcut`, `openWaylandPortalShortcutSettings`) **[F]**.
6. **Servidor.** Excluir servidor: modal crítico exigindo digitar o nome; sair/transferir posse fora do escopo. Revogar convite e expulsar usam modal de confirmação `critical-primary`; banir usa o mesmo modal com o texto "Você tem certeza de que deseja banir este usuário?" **[F]**.
7. **Tooltips** em toda ação só com ícone (fechar, copiar, lixeira, alça). Menu de contexto: clique direito em linhas de cargo, canal e membro (itens: ver 2.4).
8. **Teclado.** Tab percorre os controles da página; o anel só aparece depois de uma tecla de navegação e some ao mover o mouse **[F]**; setas navegam em listas/menus/rádios; Enter aciona; Espaço alterna caixas e interruptores.

## 5. Dados necessários (o que falta no servidor/protocolo)

Hoje: `proto` tem `User{id,name,online}`, `ServerInfo{id,name,owner,invite,channels,members}`; `C2S` só cria/entra em servidor, cria canal, envia mensagem, amigos, DMs; o esquema tem `users, sessions, servers, server_invites, members(nickname, joined_at), channels(parent_id, position), friendships, blocks, read_state`. Lista priorizada:

**P0 (sem servidor)** — nada de rede: `Prefs` local (tema, densidade, zoom, fonte, movimento reduzido, sons, notificações, dispositivos de áudio, atalhos) em `eframe::Storage`.

**P1 — conta e servidor básico**
1. `C2S::UpdateProfile{display_name, pronouns, about, accent_color}` + colunas `users.pronouns TEXT`, `users.about TEXT`, `users.accent_color INT`, `users.avatar_hash TEXT`; `User` do `Snapshot` ganha `display_name`.
2. `C2S::ChangePassword{old,new}`, `C2S::DeleteAccount{password}`; `C2S::ListSessions` / `S2C::Sessions(Vec<{id,device,last_used_at,current}>)` / `C2S::RevokeSession{id}` (tabela `sessions` já tem `device` e `last_used_at`).
3. `C2S::UpdateServer{id, name, icon?, system_channel_id?, default_notifications?}`, `C2S::DeleteServer{id}`; colunas `servers.icon_hash`, `system_channel_id`, `default_notifications SMALLINT`, `verification_level SMALLINT`.
4. Convites: `C2S::ListInvites{server}` → `S2C::Invites(...)`, `C2S::CreateInvite{server, max_age_s?, max_uses?}`, `C2S::RevokeInvite{code}` (a tabela `server_invites` já tem `created_by, max_uses, uses, expires_at, revoked_at`).
5. Membros: `ServerInfo.members` precisa de `joined_at`, `nickname`, `role_ids`; `C2S::KickMember{server,user}`.
6. Erros por campo: `S2C::Error{msg, field: Option<String>}`.

**P2 — cargos, canais, moderação**
7. Tabelas `roles(id, server_id, name, color INT, hoist BOOL, mentionable BOOL, permissions BIGINT, position INT)` (o cargo `@everyone` tem `id = server_id`) e `member_roles(server_id, user_id, role_id)`; bits de permissão conforme `docs/discord/02-servidores-cargos.md` §2.5; mensagens `CreateRole/UpdateRole/DeleteRole/ReorderRoles/SetMemberRoles` e `S2C::Roles`. Cálculo de permissões **no servidor** em função única (`compute_permissions`).
8. Canais: `C2S::UpdateChannel{id,name,topic,slowmode,private}`, `DeleteChannel`, `MoveChannel{id,parent_id,position}`; colunas `channels.topic`, `slowmode_s`.
9. Moderação: tabelas `bans(server_id,user_id,reason,created_by,created_at)` e `member_timeouts(server_id,user_id,until)`; `C2S::Ban/Unban/Timeout/ListBans`.
10. Apelido por servidor: `C2S::SetNickname{server,user?,nickname}` (coluna já existe).

**P3** — `user_settings(user_id, data JSONB, updated_at)` para sincronizar preferências entre dispositivos; `audit_log`; sons personalizados; avatar/ícone com upload (endpoint de arquivos, limites).

Índices: `bans(server_id, created_at DESC)`, `server_invites(server_id, revoked_at)` (já há por `server_id`), `member_roles(user_id)`.

## 6. Plano de implementação em egui 0.37

### 6.1 Arquivos e funções

- `app/src/motion.rs`: `tween`, `reduced_motion(ctx)`, `shake(ctx, id)`.
- `app/src/widgets.rs`: `btn(ui, text, Variant, Size) -> Response`, `icon_btn`, `field(ui, &mut String, FieldOpts) -> Response`, `switch`, `checkbox`, `radio`, `radio_card`, `slider`, `select`, `color_swatches`, `keycap`, `toggle_row(ui, label, desc, &mut bool)`, `section(ui, title, body)`, `settings_card`.
- `app/src/overlay.rs`: `modal`, `confirm`, `popout`, `tip`, `toast` (fila), `OverlayStack` (Esc).
- `app/src/menu.rs`: `menu_item`, `menu_sep`, `context_menu(resp, build)`.
- `app/src/settings.rs`: `Settings` (estado), `Page`, `settings_layer`, uma função por página (`page_account`, `page_profile`, …). `Prefs` e `Draft<T>` (valor + original + `dirty()`).
- `theme.rs` ganha: `SCRIM`, `RING`, `ease_in`, tamanhos (`H_XS/H_SM/H_MD`), `kb_mode(ctx) -> bool`. Substituir `big_button`/`small_button` por `btn` aos poucos (mantendo os nomes antigos como atalhos).

### 6.2 Armadilhas do egui

- **Sem letter-spacing**: rótulos em caixa alta ficam mais apertados que no Discord; compensar com tamanho 11 e cor MUTED (como já faz `caps`).
- **Sem negrito em fonte variável**: `Inter.ttf` não dá peso 600; para `semibold` carregar uma fonte estática como `FontFamily::Name("semibold")` ou usar cor/tamanho para hierarquia (preferível por ora).
- **Sem escala de `Ui`**: a escala 0,9→1 do modal usa `ctx.set_transform_layer(layer_id, TSTransform)` na camada do `Area` (a hit-test acompanha); o texto fica levemente borrado durante os ~170 ms. **Verificar** a assinatura em 0.37 (`egui::emath::TSTransform::new(translation, scaling)`).
- **Retângulo da janela**: usar `ctx.content_rect()` (nos exemplos); em versões antigas era `screen_rect()`. Conferir ao compilar.
- **Sem backdrop-blur**: usar só o escurecimento.
- **Área inicial invisível**: `Area` renderiza o primeiro frame com tamanho desconhecido (alpha 0), então o primeiro frame do modal não aparece; aceitável.
- **Ordem de camadas**: o escudo e o cartão do modal são dois `Area` em `Order::Foreground`; clicar no escudo o traz ao topo. Chamar `ctx.move_to_top(card_layer)` a cada frame depois de criar o escudo. Popups de `select` dentro de modais também usam `Foreground`: criar depois do cartão.
- **Escudo precisa de `Sense::click()`** (não `hover`) para bloquear cliques nos painéis atrás.
- **Menus novos do egui (0.32+)**: `Response::context_menu` e `containers::menu::{MenuButton, SubMenuButton, MenuConfig}` existem; o fechamento automático ao clicar vem de `PopupCloseBehavior`. Usar `CloseOnClickOutside` e chamar `ui.close()` só nos itens normais para reproduzir "checkbox não fecha". **Verificar nomes** ao compilar.
- **Teclado nos menus**: o egui não navega com ↑/↓ nem tem foco por item; implementar com um `focus: usize` guardado em `ctx.data` e lista de retângulos do frame anterior (P2).
- **Anel de foco só no teclado**: guardar `kb_mode` (verdadeiro ao ver `Tab`/setas/Enter, falso ao ver movimento de ponteiro) em `ctx.data`.
- **Esc em `TextEdit`**: o egui já tira o foco do campo com Esc; consumir a tecla (`input_mut(|i| i.consume_key(...))`) só quando um overlay for fechado, senão o campo perde o foco antes do overlay.
- **Cursor mão**: reaproveitar o `hand(ui,&resp)` de `theme.rs`.
- **Reduzir movimento**: `Prefs.reduced_motion` (auto/on/off); no modo auto não há como ler a preferência do sistema no Linux pelo egui (ler `gsettings` ou apenas tratar auto como desligado) **[?]**.

### 6.3 Esboço 1 — interpolação e modal (escala em volta do centro)

```rust
// motion.rs
use eframe::egui::{self, Id};

#[derive(Clone, Copy)]
struct Tw { from: f32, to: f32, t0: f64 }

/// Leva um valor até `target` em `dur` s, começando `delay` s depois da mudança. Easing: theme::ease_out.
pub fn tween(ctx: &egui::Context, id: Id, target: f32, dur: f32, delay: f32) -> f32 {
    if crate::prefs::reduced_motion(ctx) { return target; }
    let now = ctx.input(|i| i.time);
    let at = |s: &Tw| -> f32 {
        let t = ((now - s.t0) as f32 - delay) / dur;
        s.from + (s.to - s.from) * crate::theme::ease_out(t)
    };
    let mut s: Tw = ctx.data(|d| d.get_temp(id)).unwrap_or(Tw { from: target, to: target, t0: now });
    if (s.to - target).abs() > 1e-4 {
        s = Tw { from: at(&s), to: target, t0: now };      // recomeça de onde está
        ctx.data_mut(|d| d.insert_temp(id, s));
    }
    let v = at(&s);
    if (v - s.to).abs() > 1e-3 { ctx.request_repaint(); }
    v
}
```

```rust
// overlay.rs
use eframe::egui::{self, emath::TSTransform, Align2, Color32, Id, Margin, Order, Sense};
use crate::{motion::tween, theme::*};

#[derive(Clone, Copy)]
pub enum Size { Sm, Md, Lg }
impl Size { fn w(self) -> f32 { match self { Size::Sm => 440.0, Size::Md => 600.0, Size::Lg => 800.0 } } }

/// Devolve true quando o usuário pediu para fechar (Esc é tratado no OverlayStack).
pub fn modal(
    ctx: &egui::Context, key: &str, open: bool, size: Size, dismissable: bool,
    title: &str, body: impl FnOnce(&mut egui::Ui), footer: impl FnOnce(&mut egui::Ui),
) -> bool {
    let id = Id::new(("modal", key));
    let on = open as u8 as f32;
    let scrim = tween(ctx, id.with("s"), on, 0.17, 0.0);
    let card = tween(ctx, id.with("c"), on, if open { 0.18 } else { 0.15 }, if open { 0.064 } else { 0.0 });
    if scrim < 0.002 && !open { return false; }
    let screen = ctx.content_rect();
    let mut want_close = false;

    egui::Area::new(id.with("shield")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let r = ui.allocate_rect(screen, Sense::click());                // bloqueia o que está atrás
        ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha((0.72 * 255.0 * scrim) as u8));
        if r.clicked() && dismissable && open { want_close = true; }
    });

    let area = egui::Area::new(id.with("card")).order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_opacity(card);
            glass(14).inner_margin(Margin::same(0)).show(ui, |ui| {
                ui.set_width(size.w().min(screen.width() - 32.0));
                egui::Frame::new().inner_margin(Margin { left: 24, right: 24, top: 20, bottom: 8 }).show(ui, |ui| {
                    ui.horizontal(|ui| { ui.label(serif(title, 22.0)); });
                });
                egui::ScrollArea::vertical().max_height(screen.height() * 0.7).auto_shrink([false, true])
                    .show(ui, |ui| { egui::Frame::new().inner_margin(Margin::symmetric(24, 8)).show(ui, body); });
                ui.painter().hline(ui.min_rect().x_range(), ui.cursor().top(), egui::Stroke::new(1.0, LINE));
                egui::Frame::new().inner_margin(Margin::symmetric(16, 16)).show(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), footer);   // 1º = mais à direita
                });
            });
        });
    ctx.move_to_top(area.response.layer_id);

    // escala 0,9 → 1 em volta do centro do cartão
    let s = 0.9 + 0.1 * card;
    let c = area.response.rect.center().to_vec2();
    ctx.set_transform_layer(area.response.layer_id, TSTransform::new(c * (1.0 - s), s));
    want_close
}
```

### 6.4 Esboço 2 — interruptor com polegar que estica, e slider

```rust
// widgets.rs
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(44.0, 24.0), Sense::click());
    if resp.clicked() { *on = !*on; resp.mark_changed(); }
    let t = motion::tween(ui.ctx(), resp.id, *on as u8 as f32, 0.30, 0.0);        // 300 ms [F]
    let hov = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.12);
    let cr = egui::CornerRadius::same(12);
    let p = ui.painter();
    p.rect_filled(rect, cr, lerp(L1, AMBER, t));
    let edge = lerp(lerp(LINE2, MUTED, 0.6 * hov), alpha(AMBER_INK, 0.25), t);
    p.rect_stroke(rect, cr, Stroke::new(1.0, edge), StrokeKind::Inside);
    let m = (std::f32::consts::PI * t).sin();                  // 0 nos extremos, 1 no meio
    let (w, h) = (16.0 + 6.0 * m, 16.0 - 6.0 * m);             // estica e achata [F: 16→22 / 16→10]
    let cx = rect.left() + 12.0 + 20.0 * t;                    // centro do polegar
    let th = Rect::from_center_size(pos2(cx, rect.center().y), vec2(w, h));
    p.rect_filled(th, egui::CornerRadius::same((h / 2.0) as u8), lerp(TEXT2, AMBER_INK, t));
    hand(ui, &resp);
    resp
}

/// marks: valores onde há marca; se `stick`, o valor gruda nelas.
pub fn slider(ui: &mut Ui, v: &mut f32, min: f32, max: f32, step: f32,
              marks: &[f32], stick: bool, fmt: impl Fn(f32) -> String) -> Response {
    let h = if marks.is_empty() { 24.0 } else { 44.0 };
    let (rect, mut resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click_and_drag());
    let (x0, x1, y) = (rect.left() + 10.0, rect.right() - 10.0, rect.bottom() - 12.0);
    if resp.is_pointer_button_down_on() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let f = ((pos.x - x0) / (x1 - x0)).clamp(0.0, 1.0);
            let mut nv = min + f * (max - min);
            nv = if stick && !marks.is_empty() {
                *marks.iter().min_by(|a, b| (**a - nv).abs().total_cmp(&(**b - nv).abs())).unwrap()
            } else { (nv / step).round() * step };
            if (nv - *v).abs() > f32::EPSILON { *v = nv; resp.mark_changed(); }
        }
    }
    let act = resp.hovered() || resp.dragged();
    let a = ui.ctx().animate_bool_with_time(resp.id.with("a"), act, 0.10);       // 100 ms [F]
    let (bar, grab) = (4.0 + 2.0 * a, 16.0 + 4.0 * a);                            // [F] 4→6 e 16→20
    let fx = x0 + (x1 - x0) * ((*v - min) / (max - min));
    let p = ui.painter();
    p.rect_filled(Rect::from_min_max(pos2(x0, y - bar / 2.0), pos2(x1, y + bar / 2.0)), egui::CornerRadius::same(3), L4);
    p.rect_filled(Rect::from_min_max(pos2(x0, y - bar / 2.0), pos2(fx, y + bar / 2.0)), egui::CornerRadius::same(3), AMBER);
    for m in marks {
        let mx = x0 + (x1 - x0) * ((m - min) / (max - min));
        p.line_segment([pos2(mx, y - 9.0), pos2(mx, y - 3.0)], Stroke::new(1.0, LINE2));
        p.text(pos2(mx, y - 20.0), Align2::CENTER_CENTER, fmt(*m), FontId::proportional(11.0), MUTED);
    }
    p.circle_filled(pos2(fx, y), grab / 2.0, TEXT);
    if act { /* balão: reutilizar tip_at(ctx, pos2(fx, y - 18.0), &fmt(*v)) */ }
    hand(ui, &resp);
    resp
}
```

### 6.5 Esboço 3 — camada de configurações e cadeia de Esc

```rust
// settings.rs
pub fn settings_layer(ctx: &egui::Context, st: &mut Settings, stack: &mut OverlayStack, acts: &mut Vec<Act>) {
    let id = Id::new("settings");
    let t = tween(ctx, id, st.open as u8 as f32, 0.15, 0.0);               // 150 ms [F]
    if t <= 0.001 && !st.open { return; }
    let screen = ctx.content_rect();
    let area = egui::Area::new(id).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        ui.set_opacity(t);
        let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());   // absorve cliques
        let side = 218.0;
        let left_w = side + ((screen.width() - side - 800.0).max(0.0)) * 0.5;
        let p = ui.painter();
        p.rect_filled(rect, 0.0, L2);
        p.rect_filled(Rect::from_min_size(rect.min, vec2(left_w, rect.height())), 0.0, L1);
        p.vline(rect.min.x + left_w - 0.5, rect.y_range(), Stroke::new(1.0, LINE));

        // barra lateral (encostada à direita da região esquerda)
        let side_rect = Rect::from_min_max(
            pos2(rect.min.x + left_w - side, rect.top() + 60.0), pos2(rect.min.x + left_w - 6.0, rect.bottom() - 24.0));
        let mut sb = ui.new_child(egui::UiBuilder::new().max_rect(side_rect).layout(Layout::top_down(Align::Min)));
        egui::ScrollArea::vertical().id_salt("set_side").auto_shrink([false, false]).show(&mut sb, |ui| {
            for g in st.nav() {                       // grupos: (cabeçalho, itens)
                if !g.title.is_empty() { ui.add_space(16.0); ui.label(caps(g.title)); ui.add_space(6.0); }
                for it in g.items { if side_item(ui, it.label, st.page == it.page, it.danger).clicked() { st.go(it.page); } }
            }
        });

        // coluna de conteúdo
        let col_x = rect.min.x + left_w;
        let col_w = (740.0 + 80.0).min(rect.right() - col_x - 36.0 - 56.0);
        let col = Rect::from_min_size(pos2(col_x, rect.top()), vec2(col_w, rect.height())).shrink2(vec2(40.0, 0.0));
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(col).layout(Layout::top_down(Align::Min)));
        egui::ScrollArea::vertical().id_salt(("set_page", st.page as u8)).auto_shrink([false, false]).show(&mut c, |ui| {
            ui.add_space(60.0);
            st.page_ui(ui, acts);
            ui.add_space(80.0);
        });
        if st.dirty() { save_bar(ui, Rect::from_min_size(pos2(col_x + 40.0, rect.bottom() - 68.0), vec2(col_w - 80.0, 52.0)), st); }

        // fechar + ESC
        let cc = pos2(col_x + col_w + 12.0 + 18.0, rect.top() + 60.0 + 18.0);
        if close_button(ui, cc) { stack.request_close(Overlay::Settings); }
    });
    // entrada: escala 1,04 → 1 em volta do centro
    let s = 1.0 + 0.04 * (1.0 - t);
    let c = screen.center().to_vec2();
    ctx.set_transform_layer(area.response.layer_id, TSTransform::new(c * (1.0 - s), s));
}

pub enum Overlay { Menu, Popout, Modal, Settings }
impl OverlayStack {
    /// Chamar uma vez por frame, antes de desenhar. Fecha só o nível do topo.
    pub fn handle_esc(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if let Some(top) = self.top() {                       // prioridade: Menu > Popout > Modal > Settings
                self.close(top);
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            }
        }
    }
}
```

### 6.6 Esboço 4 — item de menu de contexto e tooltip com atraso

```rust
// menu.rs
#[derive(Clone, Copy, PartialEq)] pub enum Tone { Normal, Danger, Brand }
pub struct Item<'a> { pub label: &'a str, pub tone: Tone, pub shortcut: Option<&'a str>, pub sub: Option<&'a str>,
                      pub check: Option<bool>, pub submenu: bool, pub enabled: bool }

pub fn menu_item(ui: &mut Ui, it: Item) -> Response {
    ui.set_min_width(188.0);                                                  // [F] min 188
    let h = if it.sub.is_some() { 44.0 } else { 32.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h),
        if it.enabled { Sense::click() } else { Sense::hover() });
    let hot = it.enabled && resp.hovered();
    let r = rect.shrink2(vec2(4.0, 0.0));
    let p = ui.painter();
    if hot {
        let bg = if it.tone == Tone::Danger { alpha(RED, 0.14) } else { L4 };
        p.rect_filled(r, egui::CornerRadius::same(6), bg);
    }
    let ink = match (it.enabled, it.tone, hot) {
        (false, _, _) => MUTED, (_, Tone::Danger, _) => RED, (_, Tone::Brand, _) => AMBER, (_, _, true) => TEXT, _ => TEXT2 };
    p.text(pos2(r.left() + 10.0, rect.center().y - if it.sub.is_some() { 7.0 } else { 0.0 }),
           Align2::LEFT_CENTER, it.label, FontId::proportional(14.0), ink);
    if let Some(s) = it.sub { p.text(pos2(r.left() + 10.0, rect.center().y + 9.0), Align2::LEFT_CENTER, s, FontId::proportional(12.0), MUTED); }
    if let Some(k) = it.shortcut { p.text(pos2(r.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, k, FontId::proportional(12.0), MUTED); }
    if it.submenu { p.text(pos2(r.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, "›", FontId::proportional(16.0), MUTED); }
    if let Some(c) = it.check { if c { /* ✓ desenhado com 2 segmentos em r.right()-22 */ } }
    if it.enabled { hand(ui, &resp); }
    resp
}
pub fn menu_sep(ui: &mut Ui) { ui.add_space(4.0); let y = ui.cursor().top(); ui.painter().hline(ui.min_rect().x_range(), y, Stroke::new(1.0, LINE)); ui.add_space(5.0); }

// overlay.rs: tooltip com atraso de 300 ms [F], só depois de ficar parado
pub fn tip(resp: &Response, text: &str) {
    let ctx = &resp.ctx;
    let key = resp.id.with("tip");
    let now = ctx.input(|i| i.time);
    let over = resp.hovered() && !resp.is_pointer_button_down_on();            // clique esconde [F: hideOnClick]
    let start: Option<f64> = ctx.data(|d| d.get_temp(key));
    match (over, start) {
        (true, None) => { ctx.data_mut(|d| d.insert_temp(key, now)); ctx.request_repaint_after(std::time::Duration::from_millis(300)); }
        (false, Some(_)) => ctx.data_mut(|d| d.remove::<f64>(key)),
        _ => {}
    }
    let shown = start.map_or(false, |s| now - s >= 0.30) && over;
    let a = motion::tween(ctx, key.with("a"), shown as u8 as f32, 0.09, 0.0);
    if a < 0.01 { return; }
    let pos = resp.rect.center_top() + vec2(0.0, -8.0);                        // spacing 8 [F]
    egui::Area::new(key.with("area")).order(Order::Tooltip).fixed_pos(pos).pivot(Align2::CENTER_BOTTOM)
        .interactable(false).show(ctx, |ui| {
            ui.set_opacity(a);
            egui::Frame::new().fill(L4).stroke(Stroke::new(1.0, LINE2)).corner_radius(egui::CornerRadius::same(8))
                .inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| ui.label(RichText::new(text).size(13.0).color(TEXT)));
            // caret: triângulo 14×7 apontando para baixo, colado ao fundo do quadro
        });
}
```

Notas: `menu_item` dentro de `resp.context_menu(|ui| …)` ou `MenuButton`; itens normais chamam `ui.close()` após `clicked()`; checkbox/interruptor não. O seletor (`select`) segue o mesmo esquema do tooltip (Area em `Order::Foreground`, `fixed_pos` abaixo do campo, largura = `field.rect.width()`, `ScrollArea::max_height(7 * 36.0)` só se > 7 opções, fechar em clique fora/Esc/seleção).

## 7. Ordem de entrega (passos pequenos e verificáveis)

1. **Núcleo de movimento e camadas** (`motion.rs`, `OverlayStack`): o usuário vê o modal "Criar canal" com escurecimento 72 %, entrada de escala 0,9→1 em ~170 ms e fecha com Esc ou clicando fora.
2. **Botões e campos** (`btn` xs/sm/md, variantes, `field` com contador/erro/limpar): os modais atuais (Adicionar servidor, Criar canal, Convidar) passam a usar a biblioteca; botão em `loading` mostra os três pontos.
3. **Interruptor, caixa, rádio, linha de configuração**: uma tela de teste (atalho de depuração) mostra todos os estados, incluindo o polegar esticando e desativado a 0,5.
4. **Slider e seletor** (com marcas de zoom/fonte, balão, lista de 7 itens com rolagem fina).
5. **Tooltip com atraso e menu de contexto** (item, separador, perigo em vermelho, submenu por hover): substituir `on_hover_text` da barra de servidores e o menu do servidor.
6. **Toast em fila** (topo, 3 s, variantes): troca o `toast()` atual.
7. **Camada de configurações vazia**: engrenagem abre, barra lateral com grupos e itens, ✕/ESC fecham, Esc respeita menu/popout/modal. O usuário vê a casca completa.
8. **Aparência + Notificações + Voz e vídeo** (só preferências locais, sem servidor): zoom, tamanho da fonte, densidade, movimento reduzido já funcionam de verdade; seletor de dispositivos de áudio.
9. **Atalhos**: tabela e gravador (global só depois).
10. **Minha conta e Perfis** (P1 do protocolo: `UpdateProfile`, `ChangePassword`) com barra de alterações não salvas.
11. **Servidor: Visão geral, Convites, Membros** (P1) e depois **Cargos, Canais, Moderação** (P2).
12. Acessibilidade fina: anel de foco só por teclado, navegação por setas nos menus, contraste de cores de cargo.

## 8. Perguntas em aberto

1. **Tema claro?** O Grafite é só escuro; mantenho a linha "Tema" escondida (recomendado) ou quer um segundo tema?
2. **Posição do toast**: o Discord usa o topo; hoje o Telacast usa o fundo. Mudo para o topo (recomendado) ou mantenho?
3. **Salvar**: confirma o modelo "preferências aplicam na hora, formulários usam barra de alterações" como no Discord?
4. **Perfis por servidor** (apelido por servidor) entram já na página Perfis ou ficam para a etapa de cargos?
5. **Atalhos globais (aperte-para-falar)** no Linux/Wayland dependem do portal do sistema; entra no escopo agora ou depois do resto?
6. **Fonte semibold**: posso adicionar uma Inter estática em peso 600 para títulos de seção e botões, ou continuo só com tamanho/cor?
