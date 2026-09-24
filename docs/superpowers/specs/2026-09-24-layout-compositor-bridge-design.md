# Ferris — Sub-projeto 2.5: Ponte Layout → Compositor

Status: aprovado para implementação
Data: 2026-09-24

## Contexto

Sub-projetos 1 (compositor, wgpu/glyphon/winit já rodando a 120fps), 1.5
(hardening do compositor), 2.1 (`ferris-dom`, HTML parser), 2.2
(`ferris-css`, CSS parser), 2.3 (`ferris-style`, resolução de estilo) e
2.4 (`ferris-layout`, box model/block layout) estão completos e
publicados em `github.com/jonatasperaza/Ferris`. `ferris-layout` expõe
`LayoutBox<'a> { styled_node: &'a StyledNode<'a>, x, y, width, height:
f32, margin, border, padding: Edges, children: Vec<LayoutBox<'a>> }`
via `layout(root: &StyledNode, viewport_width: f32, viewport_height:
f32) -> Option<LayoutBox<'a>>` (`x`/`y`/`width`/`height` são a content
box; `styled_node` dá acesso de volta ao `Element` original e ao
`HashMap<String, String>` de estilo computado).
`ferris-compositor/src/scene.rs` expõe `Frame { commands:
Vec<DrawCommand> }`, `DrawCommand::{Rect(RectCommand{x, y, width,
height, color: [f32;4], corner_radius}), Text(TextCommand{x, y,
content: String, size, color})}` — tudo em pixels lógicos, o renderer
já aplica o fator de escala do display.

Este documento cobre a peça 5 (última) da decomposição do "sub-projeto
2" (motor HTML/CSS/DOM): transformar a árvore `LayoutBox` num
`scene::Frame` e integrar com a janela GPU real, produzindo o primeiro
marco visual do projeto — uma página HTML+CSS de verdade desenhada na
tela. Peças da decomposição:

1. HTML parser → DOM tree (`ferris-dom`) — completo
2. CSS parser → stylesheet (`ferris-css`) — completo
3. Resolução de estilo (`ferris-style`) — completo
4. Layout engine (box model, block layout) (`ferris-layout`) — completo
5. **Ponte layout → compositor** ← este documento (`ferris-paint` +
   integração em `ferris-compositor`)

## Objetivo

Dado uma `&LayoutBox`, produzir um `scene::Frame` desenhável — um
retângulo de fundo por elemento com `background-color` definido, e um
comando de texto por elemento com conteúdo de texto direto — e integrar
esse pipeline completo (parse HTML+CSS → resolução de estilo → layout
→ pintura) em `ferris-compositor/src/main.rs`, substituindo a demo
animada atual por uma página HTML+CSS real renderizada na janela.

**Critério de sucesso:** `cargo run -p ferris-compositor` abre a janela
e mostra uma página fixture (HTML+CSS embutidos no repositório)
renderizada de verdade — retângulos com as cores de fundo certas nas
posições certas, texto visível (sem quebra de linha) — verificado
manualmente rodando o binário e olhando a janela, mais testes
automatizados de lógica pura pra `ferris-paint` (sem GPU, mesmo padrão
das peças 1-4).

## Arquitetura

Novo crate `ferris-paint`, sexto membro do workspace, dependendo de
`ferris-layout` e `ferris-compositor` (path dependencies, pra usar
`LayoutBox`/`Edges` e `Frame`/`DrawCommand`/`RectCommand`/`TextCommand`
respectivamente) — zero dependências externas, mesma disciplina das
peças anteriores.

- `color.rs` — `pub fn parse_color(value: Option<&str>) -> Option<[f32;
  4]>`, interpretando hex (`#rgb`/`#rrggbb`) e uma lista fixa de
  palavras-chave CSS1/HTML4 (case-insensitive).
- `paint.rs` — `pub fn paint(root: &LayoutBox) -> scene::Frame`, mais a
  função interna recursiva `paint_node`, que gera um `RectCommand` de
  fundo (quando `background-color` está definido) e um `TextCommand`
  (quando o elemento tem texto direto) por `LayoutBox`, recursando nos
  filhos.

`ferris-compositor/src/main.rs` é modificado (não `ferris-compositor`
vira uma nova responsabilidade — ele continua só desenhando `Frame`s
genéricos; quem sabe converter HTML/CSS num `Frame` é `ferris-paint`)
pra rodar o pipeline completo uma vez, guardar o `Frame` resultante no
estado da `App`, e usá-lo em cada redraw no lugar de
`scene::build_test_scene` (que continua existindo em `scene.rs`,
intocado, com seus próprios testes passando — só o que `main.rs`
desenha muda). O overlay de FPS (texto com estatísticas de
performance) continua sendo anexado a cada redraw por cima do `Frame`
da página, como já acontece hoje.

## Componentes

```rust
// ferris-paint/src/color.rs
pub fn parse_color(value: Option<&str>) -> Option<[f32; 4]>
```

Reconhece `#rgb` (expande cada dígito duplicando, `#f00` → `#ff0000`),
`#rrggbb` direto, e as 16 palavras-chave CSS1/HTML4 (`black`, `silver`,
`gray`, `white`, `maroon`, `red`, `purple`, `fuchsia`, `green`, `lime`,
`olive`, `yellow`, `navy`, `blue`, `teal`, `aqua`) mais `orange` e
`transparent` (→ `[0.0, 0.0, 0.0, 0.0]`), todas case-insensitive.
Qualquer outra coisa (ausente, malformado, formato não suportado como
`rgb(...)`/`hsl(...)`) → `None`, nunca panic.

```rust
// ferris-paint/src/paint.rs
pub fn paint(root: &LayoutBox) -> scene::Frame
```

`paint_node` calcula o retângulo do **border-box** (padding+border+
content, não a margin) a partir de `node.x/y/width/height` e
`node.padding`/`node.border`; se `background-color` no estilo do
elemento parseia com `parse_color`, empurra um `RectCommand` nessa
área com `corner_radius: 0.0` (border-radius não é interpretado nesta
peça — limitação documentada). Concatena todos os `Node::Text` filhos
diretos do `Element` (via `node.styled_node.element.children`,
ignorando `Node::Comment`); se o resultado (trimado) não for vazio,
empurra um `TextCommand` no canto do content box
(`node.x`/`node.y`), com `size` = `font-size` em `px` do estilo (via
`ferris_layout::length::parse_length`; qualquer valor que não seja
`Px` — ausente, `%`, `auto` — cai no default `16.0`, já que não há
herança de `font-size` ainda) e `color` = `color` do estilo via
`parse_color`, com fallback pra preto (`[0.0, 0.0, 0.0, 1.0]`, o valor
inicial padrão do CSS pra essa propriedade). Depois recursa nos
filhos, na ordem — fundo do próprio nó, texto do próprio nó, filhos —
que garante ordem de pintura de trás pra frente (fundo do pai sempre
atrás do conteúdo dos filhos).

## Fluxo de dados

Em `ferris-compositor/src/main.rs`: um arquivo fixture
(`ferris-compositor/assets/fixture.html` + `fixture.css`, embutidos via
`include_str!`, sem I/O em runtime) é parseado uma vez em
`App::resumed`, logo após `Renderer::new` ter sucesso (é o primeiro
ponto em que `gpu.logical_width()`/`gpu.logical_height()` — o viewport
lógico inicial — ficam disponíveis): HTML via
`ferris_dom::tokenizer::Tokenizer` + `ferris_dom::parser::Parser`, CSS
via `ferris_css::tokenizer::Tokenizer` + `ferris_css::parser::Parser`,
depois `ferris_style::resolve_styles`, depois
`ferris_layout::layout::layout(&styled, viewport_width,
viewport_height)`, depois `ferris_paint::paint::paint(&layout_box)`. O
`Frame` resultante é guardado em `self.page_frame: Option<scene::Frame>`
no `App`. Cada `RedrawRequested` usa `self.page_frame` (clonado) no
lugar de `scene::build_test_scene(...)`, e continua anexando o
`TextCommand` do overlay de FPS por cima, exatamente como hoje.

**Decisão de escopo:** sem re-layout em resize de janela — o `Frame` é
calculado uma vez no tamanho inicial e fica fixo; redimensionar a
janela não recalcula o layout (documentado como limitação, não
crash — o conteúdo simplesmente não se realinha ao novo tamanho).

O fixture (nosso conteúdo, não CSS de usuário arbitrário) usa
`.expect(...)` no `Option<LayoutBox>` retornado por `layout` — se a
raiz do fixture fosse `display:none` isso seria um erro de autoria
nosso, não um caso de entrada externa a tratar graciosamente.

## Limitações conhecidas (documentadas, não corrigidas nesta peça)

- **Sem quebra de linha/medição real de texto** — cada elemento com
  texto direto produz um único `TextCommand` sem wrap, que pode vazar
  visualmente da caixa se o texto for longo. Consistente com a
  decisão de "block-only layout" da peça 2.4 (sem inline layout).
- **Sem `border-radius`** — todo `RectCommand` de fundo tem
  `corner_radius: 0.0`.
- **Sem desenho de borda** — `border-width` já afeta o cálculo de
  layout (empurra conteúdo, desde a peça 2.4), mas não é pintado
  visualmente; `border-color`/`border-style` nem existem em lugar
  nenhum do pipeline ainda.
- **Cor limitada a hex + 18 palavras-chave** — `rgb()`/`rgba()`/`hsl()`
  e as ~130 cores CSS3 estendidas não são reconhecidas (caem em
  `None`/fallback).
- **`font-size` sem herança nem unidades relativas** — só `px`
  explícito funciona; qualquer outra coisa vira o default `16.0`.
- **Sem re-layout em resize** — o `Frame` da página é calculado uma
  vez no tamanho inicial da janela.

## Testes

TDD por componente, mesmo padrão das peças anteriores:
- **`parse_color`**: cada palavra-chave, case-insensitive, `#rgb` e
  `#rrggbb` válidos, hex malformado (tamanho errado, caractere
  inválido), `transparent`, valor ausente, formato não suportado
  (`rgb(...)`) — todos `None` exceto os válidos.
- **`paint_node`**: retângulo de fundo usa dimensões do border-box
  (margin/border/padding não-zero testados explicitamente), elemento
  sem `background-color` não produz `RectCommand`, texto concatena
  `Node::Text` diretos ignorando `Node::Comment` interposto, elemento
  sem texto não produz `TextCommand`, `font-size` em `px` usado
  direto, `font-size` ausente/`%`/`auto` cai no default `16.0`, `color`
  ausente/inválido cai em preto, recursão visita todos os descendentes
  produzindo comandos na ordem certa (fundo antes de filhos).
- **Integração**: HTML+CSS reais via pipeline completo (`ferris-dom` +
  `ferris-css` + `ferris-style` + `ferris-layout` + `ferris-paint`),
  conferindo comandos específicos (conteúdo/cor/posição) do `Frame`
  final de uma página pequena mas realista.
- **`main.rs`**: sem teste automatizado (código de GUI, mesmo padrão
  do sub-projeto 1) — verificação manual obrigatória antes de
  considerar a peça completa: rodar `cargo run -p ferris-compositor` e
  confirmar visualmente que o fixture aparece renderizado (fundos
  coloridos nas posições certas, texto visível), substituindo os
  retângulos animados de antes.
