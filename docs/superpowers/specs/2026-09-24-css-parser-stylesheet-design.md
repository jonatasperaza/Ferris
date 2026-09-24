# Ferris — Sub-projeto 2.2: CSS Parser → Stylesheet

Status: aprovado para implementação
Data: 2026-09-24

## Contexto

Sub-projetos 1 (compositor), 1.5 (hardening) e 2.1 (HTML parser → DOM,
crate `ferris-dom`) estão completos e publicados em
`github.com/jonatasperaza/Ferris`. O repositório já é um Cargo workspace.
`ferris-dom` expõe `dom::{Node, Element}`, `tokenizer::{Token, Tokenizer}`
e `parser::Parser`, com uma arquitetura estabelecida: tokenizer separado
do parser, cada camada testável isoladamente, e regras de recuperação de
erro pragmáticas em vez do algoritmo completo do spec.

Este documento cobre a peça 2 da decomposição do "sub-projeto 2" (motor
HTML/CSS/DOM): um parser CSS que produz uma representação de stylesheet
em Rust, sem aplicar nada ainda (sem casar seletor com DOM, sem cascata,
sem cálculo de especificidade — isso é a peça 3, "resolução de estilo").

Peças da decomposição:
1. HTML parser → DOM tree (`ferris-dom`) — completo
2. **CSS parser → stylesheet** ← este documento (`ferris-css`)
3. Resolução de estilo (cascata, computed style)
4. Layout engine (box model, block/inline layout)
5. Ponte layout → `scene::Frame` (integração com o compositor)

## Objetivo

Parsear um subset pragmático de CSS — seletores de tipo, classe, id,
atributo (`[attr=value]` e `[attr]`), combinadores descendente/filho/
irmão-seguinte/irmão-geral (` `, `>`, `+`, `~`), declarações
`propriedade: valor;` com propriedade e valor tratados como texto opaco
(sem validar nomes de propriedade conhecidos nem estruturar valores), e
`@media (condição) { regras aninhadas }` com a condição também tratada
como texto opaco — em uma estrutura `Stylesheet` navegável em Rust, sem
nenhuma dependência de GPU/DOM/renderização.

Fora de escopo: pseudo-classes (`:hover`), pseudo-elementos (`::before`),
seletores `:not()`/funcionais, `@import`/`@font-face`/outras at-rules
além de `@media`, `@media` aninhado dentro de `@media`, cálculo de
especificidade, casamento de seletor contra o DOM, cascata, valores CSS
estruturados/tipados, comentários CSS com conteúdo relevante (são
descartados no tokenizer), escapes CSS em strings/identificadores.

**Critério de sucesso:** dada uma folha de estilo CSS pequena com
seletores compostos (tipo+classe+id+atributo), combinadores, múltiplas
declarações e um bloco `@media`, `Parser::parse` retorna um `Stylesheet`
cuja estrutura bate exatamente com o CSS de entrada, validado por testes
automatizados — sem GPU, sem verificação manual (este sub-projeto é
lógica pura, mesmo padrão do 2.1).

## Arquitetura

Novo crate `ferris-css`, terceiro membro do workspace Cargo (ao lado de
`ferris-compositor` e `ferris-dom`), zero dependências externas.

- `tokenizer.rs` — `Token` (enum) e `Tokenizer` (`&str` → `Vec<Token>`).
  Tokenizer é genérico/sem estado: não distingue "dentro de um seletor"
  de "dentro de um valor de declaração" — produz o mesmo conjunto de
  tokens estruturais em qualquer posição, e é o `Parser` quem interpreta
  o significado pelo contexto (exatamente como o parser CSS real
  funciona — a tokenização é livre de contexto no spec CSS Syntax).
- `parser.rs` — `Parser` (`&[Token]` → `stylesheet::Stylesheet`).
- `stylesheet.rs` — os tipos de dados da árvore de stylesheet.

## Componentes

```rust
pub enum Token {
    Ident(String),      // identificador/palavra: div, color, red, 10px, 1.5
    Hash(String),        // #algo — id de seletor OU cor hex em valor, sem o '#'
    Str(String),          // "..." ou '...', sem as aspas
    Delim(char),           // . * > + ~ = [ ] ( ) — caracteres estruturais avulsos
    Comma,
    Colon,
    Semicolon,
    OpenBrace,
    CloseBrace,
    AtKeyword(String),      // @media, @import — sem o '@'
    Whitespace,              // preservado, coalescido em um único token por run
}

pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

pub enum Rule {
    Style(StyleRule),
    Media(MediaRule),
}

pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

pub struct MediaRule {
    pub condition: String,
    pub rules: Vec<StyleRule>,
}

pub struct Declaration {
    pub property: String,
    pub value: String,
}

pub struct Selector {
    pub components: Vec<SelectorComponent>,
}

pub enum SelectorComponent {
    Simple(SimpleSelector),
    Combinator(Combinator),
}

pub struct SimpleSelector {
    pub type_name: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: Vec<AttributeSelector>,
}

pub struct AttributeSelector {
    pub name: String,
    pub value: Option<String>,
}

pub enum Combinator {
    Descendant,
    Child,
    NextSibling,
    SubsequentSibling,
}
```

## Fluxo de dados

`&str` de entrada → `Tokenizer` reconhece char a char e emite `Token`s;
comentários `/* ... */` são reconhecidos e descartados sem virar token.
`Parser` consome os tokens sequencialmente:

- **Fora de um bloco `{}`**: espera um `AtKeyword` (início de at-rule) ou
  uma lista de seletores separada por `Comma`, terminando em `OpenBrace`.
- **Um seletor** é uma sequência de `SimpleSelector`s separados por
  `Combinator`s: `Delim('>')` → `Child`, `Delim('+')` → `NextSibling`,
  `Delim('~')` → `SubsequentSibling`; um `Whitespace` sem nenhum desses
  delimitadores ao redor → `Descendant`. Um `SimpleSelector` é montado a
  partir de um `Ident` opcional (tipo) seguido de zero ou mais de: `Hash`
  (id), `Delim('.')` + `Ident` (classe), `Delim('[')` + `Ident` [+ `Delim('=')`
  + (`Ident` | `Str`)] + `Delim(']')` (atributo) — em qualquer ordem.
- **Dentro de um bloco `{}`** (regra de estilo): cada declaração é
  `Ident` (propriedade) + `Colon` + tokens do valor + `Semicolon` (ou
  `CloseBrace` fechando a última declaração sem `;`). O valor é
  reconstruído concatenando o texto original dos tokens entre `:` e o
  terminador, preservando espaços onde havia `Whitespace` entre tokens e
  reintroduzindo o prefixo (`#` para `Hash`, aspas para `Str`) removido
  pela tokenização.
- **`@media`**: `AtKeyword("media")` + tokens até `OpenBrace` (essa
  sequência intermediária, reconstruída do mesmo jeito que um valor de
  declaração, vira `MediaRule.condition`) + zero ou mais `StyleRule`s até
  o `CloseBrace` correspondente.

## Tratamento de erros

Sem o algoritmo de recuperação de erro completo do CSS (spec imenso,
mesma decisão que o sub-projeto 2.1 tomou para HTML). Regra única: uma
regra de estilo malformada — um bloco `{` sem `}` correspondente até o
fim do input, ou uma declaração sem `:` — é descartada inteira; o parser
avança até encontrar o próximo `}` de profundidade compatível (ou o fim
do input) e continua parseando as regras seguintes normalmente, sem
travar o parse do restante da folha.

## Testes

Mesmo padrão do sub-projeto 2.1: TDD por componente.

- **Tokenizer:** testes unitários isolados — reconhece um seletor de tipo
  simples, uma classe, um id, um seletor de atributo com e sem valor,
  cada um dos 4 combinadores, uma declaração com valor de múltiplas
  palavras, uma string entre aspas, um comentário (descartado), um
  `@media` com condição.
- **Parser:** testes unitários isolados — dado um `Vec<Token>` construído
  manualmente, verifica a estrutura do `Stylesheet` resultante: seletor
  composto (tipo+classe+id+atributo) montado corretamente, lista de
  seletores separada por vírgula, cada um dos 4 combinadores produzindo o
  `Combinator` certo (incluindo o descendente implícito por espaço),
  declarações com valor reconstruído corretamente (incluindo valores
  multi-palavra e com vírgula, tipo `font-family: Arial, sans-serif`),
  `@media` com regras aninhadas, a regra de recuperação de erro (bloco
  malformado descartado sem afetar as regras seguintes).
- **Integração:** 2-3 testes fim-a-fim (`Tokenizer` → `Parser` juntos)
  com CSS pequeno realista, incluindo pelo menos um caso com seletor
  composto + combinador + `@media`.

## Fora de escopo (adiado para peças futuras desta decomposição)

Pseudo-classes/pseudo-elementos, seletores funcionais (`:not()` etc),
at-rules além de `@media` (`@import`, `@font-face`, `@keyframes`, etc),
`@media` aninhado, especificidade, casamento de seletor contra DOM
(peça 3), cascata (peça 3), valores CSS estruturados/tipados (peça 4 ou
além, quando o layout engine precisar interpretar unidades/cores de
verdade), escapes CSS, layout, renderização.
