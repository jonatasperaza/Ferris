# Ferris — Sub-projeto 2.6: Medição e Quebra de Linha de Texto

Status: aprovado para implementação
Data: 2026-09-25

## Contexto

Sub-projetos 1 (compositor), 1.5 (hardening), 2.1 (`ferris-dom`), 2.2
(`ferris-css`), 2.3 (`ferris-style`), 2.4 (`ferris-layout`, box
model/block layout) e 2.5 (`ferris-paint`+`ferris-scene`, ponte
layout→compositor, primeiro marco visual) estão completos e publicados
em `github.com/jonatasperaza/Ferris`. A maior limitação documentada em
2.4 e 2.5: todo texto direto de um elemento vira um único
`TextCommand` sem quebra de linha nem medição real — texto longo vaza
visualmente da caixa, e a altura da caixa nunca cresce pra acomodar
múltiplas linhas.

Este documento cobre a peça 6 da decomposição do "sub-projeto 2" (motor
HTML/CSS/DOM): medir texto com métricas reais de fonte e quebrar o
texto direto de cada elemento em múltiplas linhas, com a altura da
caixa crescendo corretamente. Elementos inline de verdade (`span`/`a`/
`em` compartilhando linha com texto vizinho) ficam para uma peça
futura (2.7) — decisão explícita, tomada durante o brainstorming, de
quebrar este escopo grande em duas peças menores.

## Objetivo

Dado o texto direto de um elemento e a largura de conteúdo disponível
(já calculada pelo box model existente), quebrar esse texto em linhas
que cabem na largura, usando métricas reais de fonte (não uma
heurística), e fazer a altura da caixa do elemento crescer
corretamente pra acomodar todas as linhas — com o resultado visual na
janela GPU batendo com o que foi calculado no layout.

**Critério de sucesso:** dado um parágrafo de texto longo dentro de uma
caixa estreita, `layout()` produz um `LayoutBox` com `text_lines`
contendo o número certo de linhas quebradas e `height` correspondente,
validado por testes automatizados (lógica pura, sem GPU) — e, rodando
o binário real do compositor com HTML/CSS de teste, o texto aparece
visualmente quebrado dentro da caixa (verificado manualmente via
screenshot + amostragem de pixel, mesmo padrão das peças 2.4/2.5).

## Arquitetura

Novo crate `ferris-text`, sétimo membro do workspace, dependendo só de
`cosmic-text` (a biblioteca de shaping/medição que o `glyphon` já usa
por baixo no renderer — sem GPU, sem `wgpu`, sem `glyphon`). Medição no
layout e desenho na tela usam o mesmo motor de fontes, então a quebra
calculada bate com o que aparece na janela.

`ferris-layout` passa a depender de `ferris-text`. A extração e o
colapso de whitespace do texto direto de um elemento (hoje em
`ferris-paint`, implementados na correção I3 da peça 2.5) migram pra
`ferris-layout` — quem mede/quebra o texto também precisa do texto já
limpo, e layout precisa saber a altura resultante antes de qualquer
vizinho empilhar. `ferris-paint` para de ter qualquer lógica de texto
própria: só lê as linhas já quebradas que `LayoutBox` carrega.

**Simplificação deliberada** (documentada como limitação, não
corrigida nesta peça): o texto direto de um elemento é sempre
desenhado ANTES dos filhos-bloco dele na pilha vertical, independente
da ordem real no DOM. Misturar texto e blocos intercalados na ordem
correta exigiria "anonymous block boxes" de verdade — fora de escopo
aqui, planejado pra quando elementos inline de verdade entrarem (peça
2.7).

## Componentes

```rust
// ferris-text/src/lib.rs
pub fn wrap_lines(text: &str, font_size: f32, max_width: f32) -> Vec<String>
pub fn line_height(font_size: f32) -> f32 {
    font_size * 1.2 // mesma convenção do renderer (Metrics::new(size, size*1.2))
}
```

`wrap_lines` usa um `FontSystem` do `cosmic-text` inicializado uma
única vez (estático/lazy, compartilhado entre chamadas — recriar a
cada chamada recarregaria fontes do sistema toda vez, inviável em
performance). Texto vazio retorna `vec![]`. `max_width <= 0` é
clampado pra um mínimo positivo antes de chamar o `cosmic-text`
(mesmo padrão de clamping já usado na peça 2.4, evita comportamento
degenerado). Uma palavra única mais larga que `max_width` ainda
produz pelo menos uma linha (não trava, não panica, só estoura
visualmente).

`LayoutBox` (peça 2.4, `ferris-layout/src/layout.rs`) ganha um campo
novo, extensão aditiva:

```rust
pub struct LayoutBox<'a> {
    pub styled_node: &'a StyledNode<'a>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub margin: Edges,
    pub border: Edges,
    pub padding: Edges,
    pub text_lines: Vec<String>, // NOVO — linhas já quebradas, prontas pra pintar
    pub children: Vec<LayoutBox<'a>>,
}
```

Um novo helper público `ferris_layout::length::resolve_font_size(style:
&HashMap<String, String>) -> f32` centraliza a validação de
`font-size` (só `px` explícito e finito/positivo é aceito, qualquer
outra coisa cai no default `16.0` — mesma validação da correção I2 da
peça 2.5), usado tanto internamente por `layout_block` (pra medir o
texto) quanto por `ferris-paint` (pra montar o `TextCommand.size`) —
nunca duplicado em dois lugares.

## Fluxo de dados

Em `layout_block`, logo depois da largura de conteúdo ser resolvida
(algoritmo já existente da peça 2.4) e antes do loop de filhos:
extrai e colapsa o texto direto do elemento (concatena `Node::Text`
filhos diretos ignorando `Node::Comment`, `split_whitespace().join("
")`), resolve `font-size` via `resolve_font_size`, chama
`ferris_text::wrap_lines(texto, font_size, width)` — se o texto
colapsado for vazio, `text_lines` fica `vec![]` sem chamar
`wrap_lines`. A altura do bloco de texto
(`text_lines.len() as f32 * ferris_text::line_height(font_size)`) vira
o valor inicial do cursor vertical antes do loop de filhos-bloco
(que continua empilhando exatamente como já funciona desde 2.4) — a
altura automática do elemento já inclui o bloco de texto
naturalmente, sem soma separada.

Em `ferris-paint::paint_node`: em vez de extrair/colapsar texto do
`Element` (lógica removida), itera `node.text_lines`, empurrando um
`TextCommand` por linha em `(node.x, node.y + i as f32 *
ferris_text::line_height(font_size))`, usando `resolve_font_size` pra
obter o mesmo `size` que o layout usou pra medir.

## Limitações conhecidas (documentadas, não corrigidas nesta peça)

- **Sem elementos inline de verdade** — `span`/`a`/`em` continuam
  virando bloco próprio, não compartilham linha com texto vizinho.
  Fica pra a peça 2.7.
- **Texto sempre antes dos filhos-bloco** — independente da ordem
  real no DOM, simplificação documentada (sem "anonymous block
  boxes").
- **Sem `text-align`** — só alinhamento à esquerda.
- **Sem `white-space: pre`/`pre-wrap`** — sempre colapsa espaço
  (herdado da correção I3 da peça 2.5).
- **Só LTR** — sem suporte a texto bidirecional, consistente com o
  resto do projeto.
- **Quebra pode variar minimamente entre máquinas** com fontes
  diferentes instaladas (mesma resolução de família `SansSerif` do
  renderer) — não é bug, é a natureza de usar fontes reais do
  sistema em vez de uma heurística fixa.

## Testes

TDD por componente, mesmo padrão das peças anteriores:
- **`wrap_lines`**: texto curto cabe numa linha só; texto longo
  quebra em várias linhas; string vazia → `vec![]`; palavra única
  mais larga que `max_width` não trava, produz ao menos uma linha;
  `max_width <= 0` não panica.
- **`resolve_font_size`**: `px` explícito e finito/positivo usa
  direto; ausente/`%`/`auto`/zero/negativo/não-finito cai no default
  `16.0`.
- **`layout_block`**: altura da caixa cresce corretamente com texto
  de várias linhas (comparado contra uma caixa com texto de uma linha
  só); `text_lines` populado com a contagem esperada pra uma largura
  de containing block conhecida; elemento sem texto direto continua
  com `text_lines: vec![]` sem afetar a altura.
- **`paint_node`**: múltiplas `text_lines` geram múltiplos
  `TextCommand` com `y` espaçado corretamente por `line_height`;
  `text_lines` vazio não gera nenhum `TextCommand`.
- **Integração**: HTML+CSS reais via pipeline completo (`ferris-dom` +
  `ferris-css` + `ferris-style` + `ferris-layout` + `ferris-paint`),
  parágrafo de texto longo numa caixa estreita, conferindo o número
  de linhas quebradas esperado e os `TextCommand`s resultantes.
- **Verificação manual visual** (obrigatória, não substituível por
  teste automatizado): rodar o binário real do compositor com um
  fixture de texto longo numa caixa estreita, capturar screenshot,
  confirmar visualmente (e por amostragem de pixel, se aplicável) que
  o texto quebra dentro da caixa em vez de vazar — peças 2.4 e 2.5 só
  acharam bugs reais rodando de verdade, não só lendo/testando código
  sem GPU.
