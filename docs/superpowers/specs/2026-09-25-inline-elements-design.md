# Ferris — Sub-projeto 2.7: Elementos Inline Compartilhando Linha

Status: aprovado para implementação
Data: 2026-09-25

## Contexto

Sub-projetos 1 (compositor), 1.5 (hardening), 2.1-2.5 (motor HTML/CSS/
DOM/layout/paint completo) e 2.6 (`ferris-text`, medição/quebra de
linha real via `cosmic-text`) estão completos e publicados em
`github.com/jonatasperaza/Ferris`. Hoje todo elemento sem
`display:none` vira um bloco empilhado verticalmente — não existe
conceito de "inline" de verdade: `span`/`a`/`em`/`strong`/etc. viram
bloco próprio, nunca compartilham linha com o texto ao redor. A
decomposição decidida durante o brainstorming da peça 2.6 previu esta
peça como o passo seguinte: elementos inline participando da mesma
linha de texto que os elementos vizinhos, com quebra de linha
misturando texto puro e elementos inline.

Esta sessão está rodando com autonomia total concedida pelo usuário
(ausente) — o design abaixo incorpora diretamente as decisões que
normalmente seriam perguntas ao usuário, cada uma justificada.

## Objetivo

Dado um bloco cujos filhos diretos são uma mistura de texto e
elementos inline (`span`, `a`, `em`, `strong`, `b`, `i`, `small`,
`code`, `sub`, `sup`, `u`, `mark`, ou qualquer elemento com
`display: inline` explícito via CSS), produzir linhas quebradas onde
cada trecho de texto — seja do bloco ou de um elemento inline aninhado
— aparece na posição horizontal correta dentro da linha, com a cor
resolvida a partir do elemento de origem daquele trecho especificamente
(não mais uma cor única pro bloco inteiro).

**Critério de sucesso:** dado `<p>Hello <b>bold</b> world</p>` com
`b { color: red }`, `layout()` produz uma única linha com três
trechos posicionados lado a lado ("Hello ", "bold", " world"), o
trecho do meio marcado com o estilo do `<b>`; `paint()` produz 3
`TextCommand`s com cores diferentes na posição x correta; verificado
por testes automatizados (lógica pura) e, pro caso de quebra de linha
real na janela GPU, por verificação visual manual (mesmo padrão das
peças 2.4-2.6).

## Arquitetura

Nenhum crate novo — esta peça estende `ferris-text` e `ferris-layout`
(já publicados), e ajusta `ferris-paint` pra consumir a nova forma de
`LayoutBox`.

`ferris-text` ganha duas funções novas, reusando a mesma lógica de
medição via `cosmic-text` já corrigida (bug de bidi) na peça 2.6:
- `wrap_line_spans(text: &str, font_size: f32, max_width: f32) -> Vec<(usize, usize)>`
  — mesmo mecanismo de `wrap_lines`, mas retorna os intervalos de byte
  (dentro do `text` de entrada) de cada linha quebrada, em vez das
  strings já fatiadas. `wrap_lines` passa a ser reimplementada em
  termos dela (fatiando `text[start..end]` pra cada intervalo
  retornado) — evita duplicar a lógica de driving do `cosmic-text`,
  sem mudar o comportamento já testado de `wrap_lines`.
- `measure_width(text: &str, font_size: f32) -> f32` — largura em
  pixels de `text` numa única linha, sem quebra (usa
  `buffer.set_size(None, None)`, mesmo ajuste que corrigiu o bug de
  duplo-wrap da peça 2.6, e lê `LayoutRun.line_w`, campo já confirmado
  existir nesse formato pela sonda da revisão final da peça 2.6).

`ferris-layout` ganha `is_inline_display(tag_name: &str, style:
&HashMap<String,String>) -> bool` (checa `style.get("display")`
primeiro — `"inline"`/`"block"` explícitos vencem — senão cai numa
lista fixa de tags HTML inline conhecidas, default bloco pra qualquer
outra coisa) e um novo tipo `InlineRun<'a>` carregando o texto de um
trecho, o estilo de origem (referência, não cópia — quem resolve cor é
`ferris-paint`, como já acontece hoje pro bloco inteiro) e o
deslocamento horizontal dentro da linha. `LayoutBox.text_lines:
Vec<String>` (peça 2.6) é substituído por `LayoutBox.lines:
Vec<Vec<InlineRun<'a>>>` — mudança de tipo no mesmo campo
conceitual, mesmo padrão de extensão que a peça 2.6 já fez no `LayoutBox`
original da peça 2.4.

`ferris-paint` para de gerar um `TextCommand` por linha (peça 2.6) e
passa a gerar um `TextCommand` por TRECHO dentro de cada linha,
usando o `x_offset` já calculado pelo layout e resolvendo a cor a
partir do `style` daquele trecho especificamente.

## Componentes

```rust
// ferris-layout/src/layout.rs
pub struct InlineRun<'a> {
    pub text: String,
    pub style: &'a std::collections::HashMap<String, String>,
    pub x_offset: f32, // pixels a partir do início da linha
}
```

`is_inline_display`:
```rust
fn is_inline_display(tag_name: &str, style: &HashMap<String, String>) -> bool {
    match style.get("display").map(String::as_str) {
        Some("inline") => return true,
        Some("block") => return false,
        _ => {}
    }
    matches!(
        tag_name,
        "a" | "span" | "em" | "strong" | "b" | "i" | "small"
            | "code" | "sub" | "sup" | "u" | "mark"
    )
}
```

## Fluxo de dados

`layout_block` (algoritmo já existente das peças 2.4/2.6) ganha um
passo novo logo no início do processamento de filhos, substituindo o
"colete só `Node::Text` diretos" da peça 2.6 por
`flatten_inline_content`:

1. Percorre os filhos diretos do elemento, EM ORDEM, parando no
   primeiro filho que seja um elemento BLOCO (não-inline,
   `display` diferente de `none`) — todo filho até esse ponto (texto
   direto, elementos inline, e recursivamente o conteúdo deles) entra
   na "sequência inline achatada"; o filho onde parou e todos depois
   dele viram filhos-bloco (`children`), exatamente como já acontece
   hoje.
2. Construção da sequência achatada: concatena o texto BRUTO (sem
   colapsar ainda) de cada `Node::Text` e de cada elemento inline
   recursivamente, marcando cada trecho de origem com o `style` do
   elemento de onde veio (o texto direto do próprio bloco usa o
   `style` do bloco; texto dentro de um `<b>` aninhado usa o `style`
   do `<b>`). Elementos `display:none` e comentários são ignorados
   nessa fase, em qualquer profundidade.
3. Colapsa espaço em branco sobre a sequência JUNTA (não por trecho
   isolado — colapsar cada trecho separadamente perderia o espaço que
   separa trechos vizinhos, ex: `"Hello "` colapsado sozinho vira
   `"Hello"`, perdendo o espaço antes do próximo trecho). O colapso
   escaneia caractere a caractere, comprimindo sequências de espaço em
   um único espaço e aparando as pontas do resultado inteiro, mantendo
   o controle de qual trecho de origem cada byte retido pertence
   (um espaço colapsado que fica na fronteira entre dois trechos de
   estilos diferentes é atribuído ao trecho ANTERIOR — decisão
   arbitrária mas inofensiva, já que espaço não tem aparência visual
   própria).

   **Exemplo passo-a-passo** (documentado aqui pra pinar o algoritmo
   antes da implementação, mesmo padrão usado pra `resolve_cascade` na
   peça 2.3): `<p>Hello <b>bold</b> world</p>`, texto direto do `<p>`
   dividido pelo HTML em dois `Node::Text`: `"Hello "` (antes do
   `<b>`) e `" world"` (depois). Sequência bruta com origem:
   `[("Hello ", p), ("bold", b), (" world", p)]`. Concatenado:
   `"Hello bold world"` (16 caracteres). Colapso: já não tem espaço
   duplo, resultado idêntico, `"Hello bold world"`. Trechos após
   colapso: `[(0..6, p, "Hello ")`, `(6..10, b, "bold")`,
   `(10..16, p, " world")]` — note que o segundo espaço (entre "bold"
   e "world") pertence ao trecho `p` seguinte, não ao `b`, porque
   veio do `Node::Text(" world")` original.
4. Se a sequência achatada colapsada for vazia, `lines` fica `vec![]`
   (mesmo comportamento de "sem texto" da peça 2.6). Senão, chama
   `ferris_text::wrap_line_spans(&colapsado, font_size, width)`
   (`font_size` resolvido do PRÓPRIO bloco via `resolve_font_size`,
   uniforme pra linha inteira — elementos inline não têm `font-size`
   próprio nesta peça, limitação documentada abaixo).
5. Pra cada intervalo de linha retornado por `wrap_line_spans`,
   interseca esse intervalo com os trechos de origem (passo 3) —
   cada sobreposição não-vazia vira um `InlineRun` daquela linha, com
   `x_offset` acumulado chamando `ferris_text::measure_width` em cada
   trecho anterior da mesma linha e somando.
6. `text_block_height` (renomeado internamente, mesmo cálculo da peça
   2.6: `lines.len() as f32 * line_height`) continua sendo o
   deslocamento inicial do cursor vertical antes dos filhos-bloco
   empilharem — mesma regra "conteúdo inline sempre antes dos
   filhos-bloco" já estabelecida.

Em `ferris-paint::paint_node`: pra cada linha em `node.lines`, pra
cada `InlineRun` na linha, empurra um `TextCommand` em
`(node.x + run.x_offset, node.y + i as f32 * line_height)`, com `size`
= `resolve_font_size(&node.styled_node.style)` (do bloco, uniforme) e
`color` = `parse_color(run.style.get("color")...)` (do trecho
específico, com fallback pra preto como já acontece hoje).

## Limitações conhecidas (documentadas, não corrigidas nesta peça)

- **Elementos inline não têm `font-size` próprio** — herdam sempre o
  tamanho do bloco pai; `font-size` declarado num elemento inline é
  ignorado pro layout/pintura. Evita ter que resolver baseline entre
  tamanhos de fonte diferentes numa mesma linha.
- **Elementos inline não têm box model próprio** — sem
  `margin`/`border`/`padding`/`background-color` em elementos inline;
  só a cor do texto (`color`) é respeitada. Consistente com manter
  esta peça tratável.
- **Conteúdo inline sempre antes dos filhos-bloco** — generalização
  direta da regra já documentada na peça 2.6. Conteúdo inline
  aparecendo DEPOIS de um irmão-bloco, no mesmo pai, não ganha seu
  próprio grupo de linhas separado — raro em HTML real, mas existe.
- **Sem `text-align`, sem `vertical-align`** — mesmo estado de antes.
- **Sem `white-space: pre`/`pre-wrap`** — herdado da peça 2.5/2.6.
- **Sem `<br>` como quebra de linha explícita** — `<br>` (se
  presente) não é tratado como elemento inline especial nesta peça;
  cai no default "bloco desconhecido", interrompendo a sequência
  inline no meio de uma frase de forma um tanto estranha
  visualmente — aceitável, documentado, candidato a peça futura.

## Testes

TDD por componente, mesmo padrão das peças anteriores:
- **`ferris-text::wrap_line_spans`**: retorna os mesmos intervalos que,
  fatiados manualmente do texto de entrada, batem com o que
  `wrap_lines` já retornava como strings (teste de equivalência
  contra o comportamento já aprovado da peça 2.6); texto vazio →
  `vec![]`; `max_width <= 0` não panica.
- **`ferris-text::measure_width`**: string vazia → `0.0`; string mais
  longa mede mais largo que uma mais curta com a mesma fonte (teste
  de propriedade, não valor exato — largura de fonte real não é
  determinística o bastante pra fixar um número); mesmo texto em
  fonte maior mede mais largo.
- **`is_inline_display`**: cada tag da lista fixa retorna `true`;
  tag desconhecida (ex: `div`) retorna `false`; `display: inline`
  explícito vence mesmo numa tag normalmente bloco; `display: block`
  explícito vence mesmo numa tag normalmente inline.
- **`flatten_inline_content`** (ou o comportamento equivalente via
  `layout_block`): o exemplo passo-a-passo documentado acima
  (`<p>Hello <b>bold</b> world</p>`) reproduzido como teste,
  conferindo os 3 trechos exatos com seus estilos de origem e
  intervalos de byte; elemento com `display:none` aninhado no meio
  da sequência inline é ignorado sem quebrar a concatenação ao redor;
  filho bloco no meio de filhos inline interrompe a sequência
  corretamente (conteúdo inline antes dele vira uma linha, ele vira
  filho-bloco empilhado).
- **`x_offset`**: dois trechos na mesma linha têm o segundo trecho
  começando exatamente onde o primeiro termina (largura do primeiro
  trecho via `measure_width`).
- **`paint_node`**: múltiplos `InlineRun`s na mesma linha geram
  múltiplos `TextCommand`s com `x` crescente e cores diferentes
  quando os estilos de origem diferem.
- **Integração**: HTML+CSS reais via pipeline completo, incluindo o
  exemplo `<p>Hello <b>bold</b> world</p>` com `b { color: red }`
  através do parser real, e um caso de quebra de linha genuína
  misturando texto+elemento inline numa caixa estreita.
- **Verificação manual visual** (obrigatória): rodar o binário real
  do compositor com um fixture contendo texto+elemento inline
  colorido, capturar screenshot, confirmar visualmente as cores e
  posições corretas — peças 2.4-2.6 só acharam bugs reais rodando de
  verdade.
