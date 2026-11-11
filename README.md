# my-pdf-tools

A local desktop app for working with PDF files. Everything runs on your own machine: no network, no telemetry, your documents never leave your computer.

> **Status: early development (pre-alpha).** Markdown → PDF works end to end, and PDF → Markdown works for slide decks; the mode for flowing notes is next. There is nothing to download yet.

## What it does

The first version (V1) focuses on converting between PDF and Markdown:

- **Markdown → PDF** *(available)*: drag `.md` files onto the window, choose paper size, margins and font size, and get a PDF with the same typography on every operating system. Supports headings, emphasis, lists (nested and task lists), code blocks, tables, quotes, links and local images.
- **PDF → Markdown**, with two modes:
  - *Slides* *(available)*: every page becomes a `##` section with the slide's title and its bullets as nested lists. Footers and slide numbers repeated on many slides are dropped, and you can convert a range of pages. Images of diagrams are not extracted yet.
  - *Notes* *(planned)*: flowing text from digital PDFs (headings, lists, code, tables).

Each conversion produces a short report with non-fatal warnings (for example, a page with no usable text, or a PDF whose pages are mostly portrait), because PDF → Markdown is heuristic and lossy by nature.

The long-term vision is a local "all-in-one" PDF toolbox where new tools can be added without breaking the existing ones.

## Not in V1

OCR (scanned or handwritten PDFs), machine-learning models, other formats such as DOCX or HTML, auto-update, and a web or hosted version.

## Tech stack

- [Tauri 2](https://tauri.app/) for the desktop shell
- React + TypeScript (Vite) for the UI
- Rust for the conversion engine
- [PDFium](https://pdfium.googlesource.com/pdfium/) (the engine of Chromium's PDF viewer), loaded as a shared library, to read PDFs
- [Typst](https://typst.app/), embedded with bundled fonts, to write PDFs

The engine is built around a neutral intermediate document model, so each format needs one reader and one writer instead of a converter for every pair. Markdown → PDF needs no external programs and gives reproducible output.

## Project layout

```
crates/core      domain: document model, Tool contract, errors, jobs (depends on nothing)
crates/engines   adapters: Markdown reader and writer, PDF reader (PDFium), PDF writer (Typst)
crates/tools     use cases (md_to_pdf, pdf_to_md) and the tool registry
src-tauri        desktop shell: Tauri commands, job manager, wiring
src              React UI (drawn from the tool descriptors the engine returns)
fixtures         sample documents and golden files (fixtures/pdf: PDFs written in Typst by us)
licenses         third-party license texts shipped with the program
scripts          get-pdfium.ps1: downloads PDFium with a pinned checksum
```

Dependencies only point one way: `core ← engines ← tools ← src-tauri`. A new tool is a module in `crates/tools` registered in `Registry::new()`; the UI picks it up from its descriptor.

## Development

Requirements: [Rust](https://rustup.rs/) (stable), [Node.js](https://nodejs.org/), PowerShell (only to run the download script below: Windows already has `powershell`; on macOS install `pwsh` with `brew install --cask powershell`) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform.

**PDFium.** Reading PDFs needs its shared library, which is not part of the crates. Download it once (the script checks a pinned SHA-256):

```powershell
# Windows (PowerShell; -Platform win-x64 is the default)
powershell -File scripts/get-pdfium.ps1

# macOS (needs pwsh, see above)
pwsh scripts/get-pdfium.ps1 -Platform mac-univ
```

It is unpacked into `pdfium/` (ignored by git). The tests and `npm run tauri dev` find it there. Without it, the tests that need PDFium are skipped locally (and fail on CI); set `PDFIUM_DIR` to use a library from somewhere else.

Run the tests (this also regenerates the TypeScript types under `src/shared/ipc/bindings/`, which are committed):

```bash
cargo test --workspace
```

Golden files (expected output) are compared exactly. After an intended change, regenerate them and review the diff:

```bash
UPDATE_GOLDEN=1 cargo test --workspace            # bash / zsh
```

```powershell
$env:UPDATE_GOLDEN = "1"; cargo test --workspace  # PowerShell
Remove-Item Env:UPDATE_GOLDEN                      # then clear it
```

 The PDFs in `fixtures/pdf/` come from the `.typ` files next to them: edit those and run `cargo run -p mpt-engines --example gen_fixtures`.

Run the app:

```bash
npm install
npm run tauri dev
```

Try it with `fixtures/md-sample/sample.md` (Markdown → PDF) and `fixtures/pdf/slides.pdf` (PDF → Markdown). The result is written next to the input file and never overwrites an existing one (for the PDF, a `slides.md` appears in `fixtures/pdf/`; git ignores it).

**Release build.** The program is a folder, not a single file: PDFium goes next to the executable. [`.github/workflows/ci.yml`](.github/workflows/ci.yml) has the exact steps, which come down to:

```bash
# Windows: builds target/release/my-pdf-tools.exe. Ship it in a folder together with
#   pdfium/win-x64/bin/pdfium.dll, LICENSE, NOTICE, licenses/ and pdfium/win-x64/LICENSE
npx tauri build --no-bundle

# macOS: the library goes inside the .app, so Tauri signs it with the rest
npx tauri build --bundles app --config src-tauri/ci/macos-pdfium.conf.json
```

## Platforms

Windows and macOS. Builds are currently unsigned, so the operating system will show a security warning on first launch. The macOS build is for Apple Silicon only for now; an Intel Mac cannot open it.

## License

Copyright (C) 2026 Luis Hernández Cuadrado. Licensed under the [AGPL-3.0](LICENSE). Contributions are welcome and are accepted under the same license.

Third-party components keep their own licenses; see [NOTICE](NOTICE) and the [licenses](licenses) folder.
