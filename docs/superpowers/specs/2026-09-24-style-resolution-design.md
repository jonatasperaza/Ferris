# Ferris — Sub-projeto 2.3: Resolução de Estilo

Status: aprovado para implementação
Data: 2026-09-24

## Contexto

Sub-projetos 1 (compositor), 1.5 (hardening), 2.1 (`ferris-dom`, HTML
parser → DOM tree) e 2.2 (`ferris-css`, CSS parser → stylesheet) estão
completos e publicados em `github.com/jonatasperaza/Ferris`. `ferris-dom`
expõe `dom::{Node, Element}` com `tag_name`/nomes de atributo já
normalizados para minúsculo. `ferris-css` expõe
`stylesheet::{Stylesheet, Rule, StyleRule, MediaRule, Selector,
SelectorComponent, SimpleSelector, AttributeSelector, Combinator,
Declaration}`, com o mesmo contrato de case (nomes de tipo/atributo em
`Selector` já minúsculos).

Este documento cobre a peça 3 da decomposição do "sub-projeto 2" (motor
HTML/CSS/DOM): casar os `Selector`s do `ferris-css` contra a árvore
`Node` do `ferris-dom`, calcular especificidade e cascata, e produzir um
estilo computado por elemento. Peças da decomposição:

1. HTML parser → DOM tree (`ferris-dom`) — completo
2. CSS parser → stylesheet (`ferris-css`) — completo
3. **Resolução de estilo** ← este documento (`ferris-style`)
4. Layout engine (box model, block/inline layout)
5. Ponte layout → `scene::Frame` (integração com o compositor)

## Objetivo

Dado um `&Element` raiz (a árvore DOM) e um `&Stylesheet`, produzir uma
árvore paralela onde cada elemento carrega seu estilo computado — o
conjunto final de pares propriedade→valor depois de casar seletores,
calcular especificidade, aplicar `!important`, e resolver a cascata.
Valores continuam texto opaco (mesma filosofia do `ferris-css`) — sem
interpretar unidades, cores, nem fazer herança de propriedades entre
elementos (herança é decisão do layout engine, peça 4).

**Critério de sucesso:** dado um HTML pequeno (via `ferris-dom`) e um CSS
pequeno com seletores compostos, combinadores, especificidade
conflitante e `!important` (via `ferris-css`), `resolve_styles` produz o
`HashMap<String, String>` esperado para elementos específicos da árvore,
validado por testes automatizados — sem GPU, sem verificação manual
(lógica pura, mesmo padrão das peças 1 e 2).

## Arquitetura

Novo crate `ferris-style`, quarto membro do workspace, dependendo de
`ferris-dom` e `ferris-css` via path dependencies (zero dependências
externas, mesma disciplina das peças anteriores).

- `lib.rs` — `pub fn resolve_styles<'a>(root: &'a Element, stylesheet:
  &Stylesheet) -> StyledNode<'a>`, e `pub struct StyledNode<'a> {
  element: &'a Element, style: HashMap<String, String>, children:
  Vec<StyledNode<'a>> }`.
- `matcher.rs` — `element_matches_simple(element: &Element, simple:
  &SimpleSelector) -> bool`, `selector_matches(selector: &Selector,
  element: &Element, ancestors: &[&Element], preceding_siblings:
  &[&Element]) -> bool`.
- `specificity.rs` — `specificity(selector: &Selector) -> (u32, u32,
  u32)` (contagem de id / classe+atributo / tipo, modelo CSS padrão
  a-b-c).
- `cascade.rs` — reúne declarações de regras que casaram, detecta
  `!important`, ordena e funde num `HashMap` final.

A árvore é percorrida de cima para baixo (sem ponteiro-pai em
`ferris-dom` — a travessia carrega a cadeia de ancestrais e a lista de
irmãos-anteriores como parâmetros da recursão, sem exigir nenhuma
mudança no crate já fechado `ferris-dom`).

## Componentes

```rust
pub struct StyledNode<'a> {
    pub element: &'a Element,
    pub style: std::collections::HashMap<String, String>,
    pub children: Vec<StyledNode<'a>>,
}
```

`matcher.rs`:
- `element_matches_simple` testa `type_name`/`id`/`classes`/`attributes`
  de um `SimpleSelector` isoladamente contra um `Element` — tipo compara
  `element.tag_name`; id compara o atributo HTML `id`; classe verifica
  se o token aparece entre os tokens separados por espaço do atributo
  HTML `class`; atributo verifica presença (`value: None`) ou igualdade
  exata de valor (`value: Some(v)`).
- `selector_matches` casa os `components` de um `Selector` da direita
  para a esquerda (mesma técnica usada por motores de browser reais):
  o último componente (sempre um `SimpleSelector`) precisa casar o
  elemento alvo; cada combinador à esquerda dele é resolvido consultando
  `ancestors` (para `Descendant`/`Child`) ou `preceding_siblings` (para
  `NextSibling`/`SubsequentSibling`) recursivamente.

`specificity.rs`: conta, por `Selector`, quantos ids / quantas
classes+atributos / quantos tipos aparecem em seus `SimpleSelector`s
componentes, retornando a tripla `(a, b, c)` comparável lexicograficamente
(a especificidade CSS padrão).

`cascade.rs`: para cada elemento, recebe a lista de `(especificidade,
ordem_de_origem, &Declaration)` de toda regra cujo `Selector` casou.
Detecta `!important` no final do valor (case-insensitive, ignorando
espaços), removendo-o do valor armazenado e marcando a declaração como
importante. Ordena a lista por `(important, especificidade,
ordem_de_origem)` crescente e insere num `HashMap` na ordem resultante
— cada inserção posterior sobrescreve a anterior para a mesma
propriedade, implementando a cascata sem lógica de "vencedor" explícita.

## Fluxo de dados

`resolve_styles(root, stylesheet)` inicia a recursão em `root` com
`ancestors=[]` e `preceding_siblings=[]`. Em cada `Element` visitado:
para toda `StyleRule` de nível superior do `Stylesheet` (regras dentro
de `Rule::Media` são ignoradas — ver limitações abaixo), para todo
`Selector` da regra, chama `selector_matches`; se casar, registra as
`declarations` da regra com a especificidade do `Selector` e a ordem de
origem (índice da regra no `Stylesheet.rules`). Depois de examinar todas
as regras, `cascade.rs` funde o resultado num `HashMap` e monta o
`StyledNode` do elemento. Recursa nos filhos que são `Node::Element`,
passando `ancestors + [element]` e a lista de irmãos-`Element`-anteriores
dentro do mesmo pai — nós `Node::Text`/`Node::Comment` não entram na
árvore de saída.

## Limitações conhecidas (documentadas, não corrigidas nesta peça)

- **Regras dentro de `@media` são ignoradas** — a condição é texto opaco
  no `ferris-css`, avaliá-la de verdade (viewport, etc.) é trabalho de
  uma peça futura.
- **Cadeias de 3+ combinadores misturando tipo ancestral e tipo irmão**
  (ex: `.x + .a > .b + .c`) podem falhar em casar corretamente, porque a
  travessia só carrega a lista de irmãos-anteriores do elemento
  imediatamente casado, não de cada nível de ancestral. Cadeias de até 2
  combinadores — a esmagadora maioria do CSS real, incluindo qualquer
  combinação de um único `Descendant`/`Child`/`NextSibling`/
  `SubsequentSibling` com no máximo um combinador adicional — funcionam
  corretamente.
- **Sem herança de propriedades entre elementos** — cada elemento só
  recebe as declarações que casaram diretamente nele; propriedades
  herdáveis (como `color`, `font-family` em CSS real) não propagam de
  pai para filho nesta peça — decisão do layout engine (peça 4).
- **Sem pseudo-classes/pseudo-elementos/seletores funcionais** — já fora
  de escopo desde o `ferris-css`, permanece fora de escopo aqui.

## Testes

TDD por componente, mesmo padrão das peças 1 e 2:
- **`element_matches_simple`**: testes isolados por tipo, id, classe
  única, classe entre várias no atributo `class`, atributo com e sem
  valor, seletor composto (todos precisam bater), caso de não-bater em
  cada categoria.
- **`selector_matches`**: testes isolados por combinador, construindo
  `Element`s e as listas `ancestors`/`preceding_siblings` manualmente
  (sem precisar do parser de verdade) — descendente direto e indireto,
  filho direto, próximo-irmão, irmão-geral, e o caso de não-casar quando
  a relação não existe.
- **`specificity`**: casos conhecidos — `#id` vence `.classe` vence
  `tipo`; seletor composto soma especificidade de todos os componentes;
  seletor com combinador soma especificidade de ambos os lados.
- **`cascade`**: especificidade maior vence; empate de especificidade
  desempata por ordem de origem (a regra que vem depois no stylesheet
  vence); `!important` vence qualquer coisa mesmo com especificidade
  menor; `!important` é removido do valor final armazenado.
- **Integração**: HTML pequeno via `ferris-dom::parser::Parser::parse` +
  CSS pequeno via `ferris-css::parser::Parser::parse`, alimentando
  `resolve_styles`, checando o `HashMap` final de elementos específicos
  da árvore resultante — pelo menos um caso exercitando seletor
  composto + combinador + conflito de especificidade + `!important`
  juntos.
