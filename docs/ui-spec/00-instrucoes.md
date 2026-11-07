# Instruções comuns dos agentes de especificação de UI

Objetivo: reproduzir no **Telacast** (egui 0.37, Rust) a interface e o comportamento do cliente do Discord, área por área.
Cada agente cuida de UMA área e entrega UM arquivo: `~/telacast/docs/ui-spec/NN-<area>.md`.

## Fonte a analisar (somente leitura)
- Cliente web do Discord extraído do cache do app, em
  `/tmp/claude-1000/-home-guilherme/8c2069bb-bd91-464f-8973-77a7f03ba318/scratchpad/discord-bundle/`
  (93 arquivos `.js`/`.css`, ~81 MB, minificados, uma linha enorme por arquivo; há várias versões de `web.*.js`: use o maior/mais recente e confirme em outro).
- **Linhas gigantes**: nunca imprima uma linha inteira. Use `grep -o -E '.{0,300}PADRAO.{0,600}'`, ou Python fatiando janelas de ±800 caracteres ao redor de cada ocorrência. Procure por nomes de classe CSS-modules, strings de i18n, nomes de componentes, constantes numéricas, `className:`, `style:{`, chaves de animação.
- Já existem notas de pesquisa por documentação em `~/telacast/docs/discord/` (leia o arquivo da sua área primeiro, é rápido) e a maquete em `~/telacast/docs/ui-preview/grafite.html`.
- O estado atual do nosso app: `~/telacast/app/src/main.rs`, `~/telacast/app/src/theme.rs`; protocolo em `~/telacast/proto/src/lib.rs`; esquema em `~/telacast/server/migrations/0001_init.sql`. Leia o que for da sua área.

## Regras
1. **Só leitura no código.** Não edite `app/`, `server/`, `proto/`. Só crie o seu arquivo em `docs/ui-spec/`.
2. **Não rode cargo/build** (a máquina tem pouca RAM). Os trechos de código são esboços; escreva-os com cuidado para egui 0.37 (`ui.painter()`, `Frame`, `Panel`, `ScrollArea`, `Sense`, `animate_bool_with_time`...), mas não os compile.
3. **Replicar estrutura e comportamento, não os ativos.** Não copie código-fonte do Discord literalmente, nem ícones/SVG/imagens/logotipo/nomes de marca. Descreva medidas, hierarquia, estados, regras e fluxos com as suas palavras. Ícones: indique qual glifo/forma simples desenhar com o painter ou qual caractere usar.
4. **Visual do Telacast = Grafite** (camadas L0–L4, acento âmbar, títulos em serifa; ver `theme.rs`). A estrutura, as medidas, os estados e o comportamento seguem o Discord; as cores e a tipografia seguem o Grafite. Não proponha gradientes, brilhos nem fundos animados (o usuário rejeitou isso como "cara de IA").
5. **Honestidade sobre a evidência.** Marque cada afirmação: **[F]** = confirmada no código do cliente (cite o arquivo e o trecho curto ou a constante), **[I]** = inferência, **[?]** = não consegui confirmar. Prefira números reais do bundle (px, ms, easing, limites de caracteres) a palpites.
6. Texto da interface em **português do Brasil**; pegue as strings do bundle (há `pt-BR`) quando existirem.

## Estrutura obrigatória do arquivo
1. **Resumo** (5–8 linhas) e o que está fora do escopo.
2. **Árvore de componentes** (de fora para dentro) com medidas exatas (largura, altura, padding, gap, raio, tamanho de fonte/linha/peso).
3. **Estados e interações** (hover, foco, seleção, não lido, silenciado, desativado, carregando, vazio, erro), com tempos e easing das transições.
4. **Regras de comportamento** (agrupamento, ordenação, limites, atalhos de teclado, rolagem, arrastar, menus de contexto).
5. **Dados necessários**: o que o servidor/protocolo precisa fornecer que hoje falta (mensagens `C2S`/`S2C`, colunas, índices). Seja específico, em lista priorizada.
6. **Plano de implementação em egui**: funções/widgets a criar, onde entram em `theme.rs`/`main.rs`, esboços de código dos 2–4 trechos mais difíceis, armadilhas do egui (ex.: sem letter-spacing, sem fonte em negrito de fonte variável, sem emoji colorido).
7. **Ordem de entrega** em passos pequenos, cada um verificável (o que o usuário vê ao final do passo).
8. **Perguntas em aberto** para o usuário (só as que mudam o que será construído).

Seja denso e concreto. Nada de enrolação; o leitor é quem vai implementar logo depois. Ao terminar, responda só com: o caminho do arquivo, 5 linhas de resumo e as 3 maiores incertezas.
