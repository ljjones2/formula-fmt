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
- The union reference operator, written as a parenthesized comma list
  (`(A1:A2,B1:B2)`), including as a function argument (`SUM((A1:A2,B1:B2))`)
- Function calls with comma-separated arguments
- Defined names (any identifier that isn't a valid cell reference)

## What's not there yet

- The intersect (space) reference operator - this needs the lexer to stop
  treating all whitespace as insignificant, which the union operator didn't
- Error literals (`#REF!`, `#VALUE!`, ...)
- Array literals (`{1,2;3,4}`)
- An actual evaluator - this only validates and reformats, it doesn't
  compute a value
