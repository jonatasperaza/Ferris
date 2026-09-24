# Ferris — Sub-projeto 2.4: Layout Engine (Box Model)

Status: aprovado para implementação
Data: 2026-09-24

## Contexto

Sub-projetos 1 (compositor), 1.5 (hardening), 2.1 (`ferris-dom`, HTML
parser → DOM tree), 2.2 (`ferris-css`, CSS parser → stylesheet) e 2.3
(`ferris-style`, resolução de estilo) estão completos e publicados em
`github.com/jonatasperaza/Ferris`. `ferris-style` expõe
`StyledNode<'a> { element: &'a Element, style: HashMap<String, String>,
children: Vec<StyledNode<'a>> }` via `resolve_styles(root: &Element,
stylesheet: &Stylesheet) -> StyledNode`. Valores de estilo continuam
texto opaco (`"10px"`, `"50%"`, `"auto"`, etc.) — nenhuma interpretação
de unidade acontece antes desta peça. Não há herança de propriedades
entre elementos (decisão explicitamente adiada por 2.3 para esta peça).

Este documento cobre a peça 4 da decomposição do "sub-projeto 2" (motor
HTML/CSS/DOM): transformar a árvore `StyledNode` num box model —
calcular posição e dimensões reais (em pixels lógicos) de cada
elemento, seguindo block layout (elementos empilhados verticalmente).
Peças da decomposição:

1. HTML parser → DOM tree (`ferris-dom`) — completo
2. CSS parser → stylesheet (`ferris-css`) — completo
3. Resolução de estilo (`ferris-style`) — completo
4. **Layout engine (box model, block layout)** ← este documento
   (`ferris-layout`)
5. Ponte layout → `scene::Frame` (integração com o compositor) — é
   nesta peça seguinte que HTML+CSS real aparece desenhado numa janela
   pela primeira vez.

## Objetivo

Dado um `&StyledNode` raiz e as dimensões do viewport (largura/altura
em pixels lógicos), produzir uma árvore paralela `LayoutBox` onde cada
elemento carrega sua posição e dimensões finais calculadas: content
box (x, y, width, height) mais os quatro lados de margin/border/padding
resolvidos em pixels.

**Escopo desta peça:** só block layout — todo elemento é tratado como
bloco empilhado verticalmente (largura padrão preenche o pai, altura
padrão é a soma dos filhos). Sem inline layout (texto fluindo lado a
lado, quebra de linha) — isso fica para uma peça futura. Sem herança de
propriedades CSS entre elementos (não tem efeito visual sem medição de
texto, que ainda não existe). Sem colapso de margins verticais
adjacentes (limitação conhecida, documentada abaixo).

**Critério de sucesso:** dado um `StyledNode` pequeno com box model
composto (margin/border/padding/width/height explícitos e implícitos
misturados, `display: none` em algum elemento, larguras em `%` e
`px`), `layout` produz as coordenadas/dimensões esperadas para
elementos específicos da árvore, validado por testes automatizados —
sem GPU, sem verificação manual (lógica pura, mesmo padrão das peças
1-3).

## Arquitetura

Novo crate `ferris-layout`, quinto membro do workspace, dependendo de
`ferris-dom` (indiretamente, via o tipo `Element` referenciado por
`StyledNode`) e `ferris-style` (`StyledNode`) via path dependencies —
zero dependências externas, mesma disciplina das peças anteriores.
Não depende de `ferris-compositor` (a ponte compositor↔layout é
responsabilidade da peça 5, não desta).

Algoritmo: uma única passada recursiva top-down, a técnica padrão de
block layout usada por motores de browser reais — a largura de cada
elemento é resolvida "de cima pra baixo" (o pai decide o espaço
disponível do filho antes de recursar nele) e a altura é resolvida "de
baixo pra cima" (o pai soma a altura dos filhos depois que eles
retornam), tudo na mesma travessia recursiva, sem precisar de duas
passadas separadas sobre a árvore.

- `length.rs` — `pub enum Length { Px(f32), Percent(f32), Auto }` e
  `pub fn parse_length(value: Option<&str>) -> Length`, que interpreta
  uma string opaca do `StyledNode.style` (ou `None` quando a
  propriedade está ausente) num `Length` tipado.
- `layout.rs` — `pub struct Edges { pub top: f32, pub right: f32,
  pub bottom: f32, pub left: f32 }`, `pub struct LayoutBox<'a> {
  pub styled_node: &'a StyledNode<'a>, pub x: f32, pub y: f32,
  pub width: f32, pub height: f32, pub margin: Edges, pub border: Edges,
  pub padding: Edges, pub children: Vec<LayoutBox<'a>> }`, e
  `pub fn layout<'a>(root: &'a StyledNode<'a>, viewport_width: f32,
  viewport_height: f32) -> LayoutBox<'a>` (mais a função interna
  recursiva `layout_block`).

## Componentes

```rust
pub enum Length {
    Px(f32),
    Percent(f32),
    Auto,
}

pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

pub struct LayoutBox<'a> {
    pub styled_node: &'a StyledNode<'a>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub margin: Edges,
    pub border: Edges,
    pub padding: Edges,
    pub children: Vec<LayoutBox<'a>>,
}
```

`x`/`y`/`width`/`height` em `LayoutBox` representam a **content box**
(não incluem margin/border/padding — esses ficam em `Edges`
separadamente; quem consumir a árvore depois soma pra desenhar
background/borda nas caixas corretas).

`length.rs`: `parse_length` reconhece o sufixo `px` (número seguido de
`px`, espaços tolerados) e `%` (número seguido de `%`), a palavra-chave
`auto` (case-insensitive), e qualquer outra coisa (valor ausente,
string vazia, unidade desconhecida como `vh`/`rem`/`em` fora de escopo,
texto malformado) vira `Length::Auto` — mesma filosofia pragmática de
"nunca panic em CSS de entrada" do `ferris-css`/`ferris-style`.
Elementos com `display:none` são excluídos da árvore de `LayoutBox`
(nem eles nem os filhos geram caixa).

## Fluxo de dados

`layout(root, viewport_width, viewport_height)` monta o containing
block inicial (`content_x = 0`, `content_y = 0`,
`content_width = viewport_width`) e chama `layout_block(root,
containing_block, is_root: true)`:

1. Lê `margin`/`border`/`padding`/`width`/`height`/`display` do
   `style` do nó via `parse_length`. Ausência de propriedade tem
   default por categoria: margin/border/padding ausentes → `0px`;
   width/height ausentes → `Auto`.
2. Se `display: none`, o nó (e filhos) não gera `LayoutBox` — filtrado
   antes de recursar, não depois.
3. Resolve a própria largura de conteúdo: `Px`/`Percent` (do
   `content_width` do containing block recebido) usa o valor direto;
   `Auto` preenche o espaço disponível
   (`content_width do containing block - margins - borders - paddings
   deste nó`), clampado em `0.0` se o resultado for negativo.
4. Calcula a própria posição de conteúdo (`content_x`, `content_y`)
   somando margin+border+padding ao canto do containing block
   recebido.
5. Recursa nos filhos (excluindo `display:none`) em ordem, empilhando
   verticalmente sem colapso de margin: cada filho recebe o
   `content_width` deste nó como seu containing block, e um cursor `y`
   que avança, a cada filho processado, pela altura total da caixa
   dele (margin + border + padding + content height).
6. Própria altura: `height` explícito (`Px` sempre; `Percent` só
   quando `is_root`, ou seja, só resolve contra o viewport — mais
   fundo na árvore, `%` de altura vira `Auto`, mesmo comportamento que
   a maioria dos browsers reais tem contra um ancestral de altura
   `auto`) usa o valor, clampado em `0.0`; `Auto` = soma das alturas
   totais dos filhos (shrink-to-fit).
7. Retorna o `LayoutBox` com `x`/`y`/`width`/`height` = content box
   deste nó, mais `margin`/`border`/`padding` resolvidos e a lista de
   `LayoutBox` dos filhos.

## Limitações conhecidas (documentadas, não corrigidas nesta peça)

- **Sem inline layout** — todo elemento é bloco, empilhado
  verticalmente. Texto vira uma caixa de altura fixa/somada, sem
  quebra de linha nem fluxo lado a lado. Fica para uma peça futura.
- **Sem herança de propriedades CSS** — cada `LayoutBox` só usa as
  propriedades de box model lidas diretamente do próprio `StyledNode`,
  sem propagação de propriedades herdáveis do pai (não tem efeito
  visual nesta peça, que não mede texto).
- **Sem colapso de margins verticais adjacentes** — `margin-bottom` de
  um bloco mais `margin-top` do próximo são somados (não o `max()` das
  regras reais de CSS). Resultado visualmente próximo mas não idêntico
  a um browser real em alguns casos.
- **`height: %` só resolve contra o viewport** — mais fundo na árvore
  (contra um ancestral de altura `auto`), vira `Auto`. `width: %`
  sempre resolve normalmente contra o pai imediato (sem ambiguidade).
- **Sem `box-sizing: border-box`** — `width`/`height` explícitos
  sempre definem a content box (comportamento `content-box`, o default
  do CSS); a propriedade `box-sizing` não é lida.

## Testes

TDD por componente, mesmo padrão das peças anteriores:
- **`parse_length`**: `px`, `%`, `auto` (case-insensitive), ausente
  (`None`), malformado, espaços extras ao redor do número/unidade.
- **`layout_block` largura**: explícito `px`, `%` do pai, `auto`
  preenchendo espaço, subtração correta de margin/border/padding do
  espaço disponível, clamping em `0.0` quando o resultado seria
  negativo.
- **`layout_block` altura**: explícito `px`, `auto` = soma dos filhos,
  `%` resolvendo contra o viewport na raiz e virando `Auto` mais
  fundo na árvore.
- **`display:none`**: exclui o nó e seus filhos da árvore de
  `LayoutBox` resultante.
- **Empilhamento vertical**: múltiplos filhos, `y` de cada um
  acumulando corretamente a altura total (incluindo margin) do
  anterior.
- **Integração**: HTML+CSS reais via
  `ferris-dom::parser::Parser::parse` +
  `ferris-css::parser::Parser::parse` +
  `ferris-style::resolve_styles`, alimentando `layout`, conferindo
  coordenadas/dimensões esperadas de 2-3 elementos específicos de uma
  página pequena mas realista (nav + header + main, mesmo padrão da
  peça 2.3).
