# Ferris — Sub-projeto 2.12: Lexer e Parser de JavaScript (`ferris-js`)

Status: aprovado para implementação
Data: 2026-09-26

## Contexto

Sub-projetos 1 (compositor), 1.5 (hardening), 2.1-2.7 (motor
HTML/CSS/DOM/layout/paint/texto/inline completo), 2.8 (carregamento de
página real via `ferris-loader`), 2.9 (navegação de uma aba), 2.10
(múltiplas abas), 2.11 (imagens `<img>`) estão completos e publicados em
`github.com/jonatasperaza/Ferris`. Hoje o Ferris não executa nenhum
JavaScript — `<script>` é completamente ignorado pelo pipeline de
carregamento.

O objetivo declarado do usuário é o Ferris virar um navegador de verdade,
usável no dia a dia. JavaScript é indispensável pra isso, mas "suporte a
JavaScript" completo (lexer, parser, interpretador, bindings de DOM,
modelo de execução de página) é maior em escopo do que tudo já construído
até agora combinado. Esta peça (2.12) é deliberadamente a primeira fatia,
isolada: **só lexer e parser**, produzindo uma AST a partir de texto JS,
sem nenhuma execução. Fatias futuras, já decompostas e aprovadas pelo
usuário, mas fora do escopo desta peça:

- 2.13 — interpretador de expressões (avalia AST de expressões, sem
  statements/funções ainda)
- 2.14 — statements, funções e escopo (motor JS isolado completo, ainda
  sem tocar a página)
- 2.15 — bindings de DOM (`document`/elementos acessíveis a partir do JS)
- 2.16 — integração no carregamento de página (`<script>` executa de
  verdade, `DOMContentLoaded`, relayout/repaint)

**Calibração de expectativa:** mesmo depois de 2.12-2.16 completos, o
Ferris não vai abrir "a maioria dos sites da internet" — falta ainda CSS
moderno (flexbox/grid), Web APIs (fetch, etc.) e suporte a JS além do
subconjunto ES5-ish desta fatia (classes, arrow functions, template
literals, destructuring, async/await, módulos ES2015+). Esta peça é só o
primeiro degrau.

## Objetivo

Um crate novo `ferris-js` que tokeniza uma string de código JavaScript
(subconjunto ES5-ish) em `Vec<Token>`, e faz parsing desses tokens numa
AST (`Program`), devolvendo `Result<Program, ParseError>` — o primeiro
parser deste projeto que pode genuinamente falhar (diferente de
HTML/CSS, que a especificação real exige que sempre produzam alguma
árvore). Nunca deve *panicar* em entrada malformada, mas syntax error de
verdade retorna `Err`, sem tentativa de recuperação — é assim que
JavaScript de verdade se comporta.

**Escopo da sintaxe (subconjunto ES5-ish aprovado):**
- Literais: número, string, booleano, `null`, `undefined`
- Declarações de variável: `var`, `let`, `const`
- Operadores: aritméticos, comparação, lógicos, atribuição (incluindo
  `+=` etc.), unários (`!`, `-`, `typeof`)
- Controle de fluxo: `if`/`else`, `for`, `while`, `return`, `break`,
  `continue`
- Funções: declaração (`function foo() {}`) e expressão
  (`const f = function() {}`)
- Literais de objeto e array
- Acesso a membro (`obj.prop`, `obj[expr]`) e chamada de função
- Operador ternário (`cond ? a : b`)
- `new` (parsing apenas — sem semântica, isso é 2.13+)

**Explicitamente fora de escopo (adiado para depois de 2.16, ou nunca):**
classes, arrow functions, template literals, destructuring, spread/rest,
async/await, generators, módulos (`import`/`export`), regex literals,
`try`/`catch`.

**Critério de sucesso:** dado um snippet de JS real dentro do
subconjunto ES5-ish, `Tokenizer::tokenize` produz a sequência de tokens
correta e `Parser::parse` produz a AST correta — validado por testes
automatizados de lógica pura (tokenização de cada categoria de token,
parsing de cada tipo de expressão e statement, um snippet real
combinando vários recursos). Dado um snippet com erro de sintaxe real
(chave sem fechar, token inesperado), `Parser::parse` devolve `Err` sem
panicar — validado por testes dedicados de erro.

## Arquitetura

Novo crate `ferris-js`, folha do workspace (sem dependência de nenhum
outro crate do Ferris — mesma posição que `ferris-dom`/`ferris-css`: só
lida com texto → estrutura, não conhece layout, paint, ou o compositor).
Mesmo padrão de tokenizer+parser já usado em `ferris-dom` (HTML) e
`ferris-css` (CSS): `Tokenizer::tokenize(&str) -> Vec<Token>` (função
pura, sem estado externo) seguido por `Parser::parse(&[Token]) -> ...`.

**Diferença deliberada em relação a HTML/CSS:** os parsers de HTML/CSS
nunca falham — a especificação real exige que produzam sempre alguma
árvore, mesmo de entrada quebrada (tag-soup, propriedade desconhecida
etc.), então lá `parse` devolve a árvore diretamente, sem `Result`. JS de
verdade não tem essa convenção de recuperação de erro: um script com
erro de sintaxe simplesmente não roda. Por isso `Parser::parse` aqui
devolve `Result<Program, ParseError>` — o primeiro parser deste projeto
com essa forma. A regra de nunca panicar em entrada malformada continua
valendo integralmente; a diferença é que aqui existe um caminho de erro
*tratável e esperado*, em vez de sempre produzir uma árvore best-effort.

Nesta peça o crate não tem nenhuma dependência dos demais crates do
Ferris nem é referenciado por eles ainda — é adicionado ao workspace
(`Cargo.toml` raiz) mas fica "solto", sem integração no pipeline de
carregamento de página (isso é 2.16). Compila e roda seus próprios testes
de forma independente.

## Componentes

```rust
// ferris-js/src/tokenizer.rs
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    StringLiteral(String),
    Identifier(String),
    Keyword(Keyword),
    Punct(Punct),   // ( ) { } [ ] ; , . :  ?
    Op(Op),         // + - * / % = == === != !== < > <= >=
                     // && || ! ++ -- += -= *= /=
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Keyword {
    Var, Let, Const, Function, Return, If, Else, For, While,
    Break, Continue, True, False, Null, Undefined, New, This, Typeof,
}

pub struct Tokenizer;
impl Tokenizer {
    pub fn tokenize(src: &str) -> Vec<Token>;
}

// ferris-js/src/ast.rs
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    StringLiteral(String),
    Boolean(bool),
    Null,
    Undefined,
    Identifier(String),
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    Logical { op: LogicalOp, left: Box<Expr>, right: Box<Expr> },
    Unary { op: UnaryOp, argument: Box<Expr> },
    Assignment { op: AssignOp, target: Box<Expr>, value: Box<Expr> },
    Call { callee: Box<Expr>, arguments: Vec<Expr> },
    Member { object: Box<Expr>, property: Box<Expr>, computed: bool },
    Function { name: Option<String>, params: Vec<String>, body: Vec<Stmt> },
    Conditional { test: Box<Expr>, consequent: Box<Expr>, alternate: Box<Expr> },
    New { callee: Box<Expr>, arguments: Vec<Expr> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expression(Expr),
    VarDecl { kind: DeclKind, declarations: Vec<(String, Option<Expr>)> },
    FunctionDecl { name: String, params: Vec<String>, body: Vec<Stmt> },
    If { test: Expr, consequent: Box<Stmt>, alternate: Option<Box<Stmt>> },
    For { init: Option<Box<Stmt>>, test: Option<Expr>, update: Option<Expr>, body: Box<Stmt> },
    While { test: Expr, body: Box<Stmt> },
    Return(Option<Expr>),
    Block(Vec<Stmt>),
    Break,
    Continue,
}

// ferris-js/src/parser.rs
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,   // índice do token onde o erro ocorreu
}

pub struct Parser;
impl Parser {
    pub fn parse(tokens: &[Token]) -> Result<Program, ParseError>;
}
```

Parsing de expressão usa precedência de operadores por camadas (padrão
Pratt/descida recursiva: `parse_assignment` → `parse_conditional` →
`parse_logical_or` → ... → `parse_unary` → `parse_call_member` →
`parse_primary`), igual à técnica já usada em `ferris-css` pra
seletores/valores compostos.

## Fluxo de dados

```
código-fonte JS (String)
    │
    ▼  Tokenizer::tokenize
Vec<Token>
    │
    ▼  Parser::parse
Result<Program, ParseError>
```

Sem execução nesta peça — o `Program` é só dado estrutural. Nenhuma
integração com `ferris-loader` ou o pipeline de renderização ainda
(2.16 fará isso).

## Tratamento de erro

- **Tokenizer nunca falha:** entrada malformada (caractere inesperado,
  string sem fechar) produz o melhor token possível ou é ignorada
  caractere a caractere — nunca panica, nunca devolve `Err` (mesmo
  espírito do tokenizer de HTML/CSS: sempre produz uma sequência de
  tokens, mesmo que não faça sentido sintático depois).
- **Parser pode devolver `Err`:** token inesperado, chave/parêntese sem
  fechar, EOF prematuro → `ParseError { message, position }` com uma
  mensagem legível (ex.: `"esperado ')' mas encontrou '{'"`) e a posição
  do token problemático. Nunca panica (`unwrap`/`expect`/indexação fora
  de limites são proibidos no caminho de erro — usar `.get()` e
  `ok_or_else` para checar limites).
- Nenhum erro de runtime (essa camada não executa nada) — só erro de
  sintaxe.

## Testes

- **Tokenizer:** um teste por categoria de token (número, string,
  identificador, cada keyword, cada operador/pontuação), incluindo casos
  de borda (número decimal, string com aspas simples/duplas, operadores
  de dois caracteres como `===`/`!==`/`&&`/`||`/`++`/`--`).
- **Parser — expressões:** um teste por variante de `Expr` (literal,
  binário, lógico, unário, atribuição, chamada, member access simples e
  computado, array/object literal, ternário, `new`), cobrindo
  precedência (ex.: `1 + 2 * 3` parseia como `2 * 3` primeiro).
- **Parser — statements:** um teste por variante de `Stmt` (var/let/const
  com e sem inicializador, function declaration, if/else, if sem else,
  for completo e com partes omitidas, while, return com e sem valor,
  block, break, continue).
- **Snippet real combinado:** um teste com uma função JS pequena mas
  realista (ex. um loop `for` com `if` dentro somando valores num array)
  parseada e comparada contra a `Program` esperada por igualdade
  estrutural (`AST` deriva `PartialEq`).
- **Casos de erro:** chave/parêntese sem fechar, token inesperado no
  lugar de uma expressão, EOF no meio de uma declaração — cada um
  verificado como `Err` (não panic), com a mensagem/posição plausível.
- Toda entrada de teste malformada roda por baixo de um harness que
  falha o teste em caso de panic (padrão já usado nos outros parsers
  deste projeto).
