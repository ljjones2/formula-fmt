# formula-fmt

A validating parser and pretty printer for spreadsheet formulas.

Formula text is deceptively hard to work with programmatically. The
operator precedence has a few real surprises (`=-2^2` evaluates to `4` in
Excel and Google Sheets, not `-4`, because unary minus binds tighter than
exponentiation), cell references come in several shapes (`A1`, `$A$1`,
`Sheet1!A1`), and there's no single canonical way to write the same formula.
If you're generating formulas from another format, migrating a workbook, or
writing a lint pass over a large model, you end up wanting a real parser
instead of a pile of regular expressions.

`formula-fmt` tokenizes and parses formula text using the same operator
precedence spreadsheets document, reports the exact character position where
a formula breaks, and re-serializes the parsed tree back into a canonical
form: consistent function-name casing, consistent spacing, and only the
parentheses actually required to preserve meaning. Re-parsing the canonical
output always produces the same tree.

## Usage

```
$ formula-fmt "=SUM(A1:A10, 3)*2"
valid
=SUM(A1:A10, 3)*2

$ formula-fmt "=sum(a1:a10)+total"
valid
=SUM(A1:A10)+total

$ formula-fmt "=1+2*"
error at position 5: expected a value, found 'end of formula'
  1+2*
      ^

$ echo '=a1+$B$2^-2' | formula-fmt --json
{"valid":true,"input":"=a1+$B$2^-2","canonical":"=A1+$B$2^-2","ast":{"type":"binary","op":"add","left":{"type":"reference","sheet":null,"column":"A","row":1,"colAbsolute":false,"rowAbsolute":false},"right":{"type":"binary","op":"pow","left":{"type":"reference","sheet":null,"column":"B","row":2,"colAbsolute":true,"rowAbsolute":true},"right":{"type":"unary","op":"neg","operand":{"type":"number","value":2}}}}}

$ formula-fmt --eval "=1+2*3"
valid
=1+2*3
value: 7

$ formula-fmt --eval --set A1=5 --set B1=7 "=A1+B1"
valid
=A1+B1
value: 12

$ formula-fmt --eval "=SUM(A1:A10)"
valid
=SUM(A1:A10)
value: evaluation of function calls is not supported yet
```

If no formula argument is given, `formula-fmt` reads one line from stdin.
The leading `=` is optional either way.

## Building

```
cargo build --release
./target/release/formula-fmt "=A1+B1"
```

No third-party dependencies; the standard library is enough for a
hand-written lexer, parser, and JSON serializer.

## What's parsed today

- Numbers (including scientific notation), text literals with `""`
  escaping, and `TRUE`/`FALSE`
- Cell references, with or without `$`, including a sheet prefix
  (`Sheet1!A1`, or `'My Sheet'!A1` when the name needs quoting)
- Ranges (`A1:B10`), arithmetic (`+ - * / ^`), concatenation (`&`),
  comparisons (`= <> < <= > >=`), unary `+`/`-`, and postfix `%`
- The intersect reference operator, written as a bare space between two
  references (`A1:A10 A5:A15`), binding tighter than unary minus but looser
  than `:`
- The union reference operator, written as a parenthesized comma list
  (`(A1:A2,B1:B2)`), including as a function argument (`SUM((A1:A2,B1:B2))`)
- Function calls with comma-separated arguments
- Defined names (any identifier that isn't a valid cell reference)
- Error literals (`#NULL!`, `#DIV/0!`, `#VALUE!`, `#REF!`, `#NAME?`, `#N/A`,
  `#NUM!`, `#GETTING_DATA!`), matched case-insensitively and canonicalized
  to uppercase
- Array literals (`{1,2;3,4}`), rows separated by `;` and columns by `,`,
  holding only constants - numbers (optionally negated), text, booleans,
  and error values, never cell references or nested formulas

## Evaluating

`--eval` computes a value for arithmetic, comparisons, concatenation, and
the unary operators, with the same type coercion spreadsheets use (text
that looks numeric coerces in arithmetic, booleans count as 1/0, an error
operand short-circuits the whole expression). Cell references and defined
names resolve against an in-memory workbook built from `--set` flags:

```
$ formula-fmt --eval --set A1=5 --set Total=A1*2 "=Total+1"
valid
=Total+1
value: 11
```

`--set TARGET=VALUE` is repeatable; `TARGET` is a cell reference or a
defined name, `VALUE` is any formula expression, evaluated against the
workbook built from the `--set` flags seen so far, so later assignments
can reference earlier ones. A cell that's never set reads as 0, matching
how spreadsheets treat a blank cell in arithmetic. Function calls, ranges,
and the reference operators still report as unsupported instead of
guessing, since there's no function library or multi-cell result type yet.

## What's not there yet

- A function library (`SUM`, `IF`, and the rest), so `--eval` can resolve
  function calls instead of reporting them unsupported
- Range and array evaluation, so a formula can produce more than one
  scalar value
