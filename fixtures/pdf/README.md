# Synthetic PDF fixtures

Test inputs for the PDF reader. They are written by us in Typst (`*.typ`) and compiled
with the project's own embedded Typst, so they carry no third-party copyright.

Regenerate after editing a `.typ` file:

```bash
cargo run -p mpt-engines --example gen_fixtures
```

- `slides.typ`: 16:9 slides, title and bullets close in size, a footer repeated on every
  slide, and the same title ("Ejemplo") on three slides.
- `notes.typ`: A4 notes with three heading levels, a list, a code block, a running header
  and page numbers.
- `twocol.typ`: a title over a two-column body (reading order = left column, then right).
- `clauses.typ`: gazette-style legal text in two columns: numbered clauses with an indented first
  line (no hanging indent) and an ordinal with a raised letter.
- `table.typ`: a paragraph and a table with a header row and numeric cells.

Real PDFs for manual checks go in `fixtures/private/` (ignored by git).
