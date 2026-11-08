# my-pdf-tools

A local desktop app for working with PDF files. Everything runs on your own machine: no network, no telemetry, your documents never leave your computer.

> **Status: early development (pre-alpha).** Markdown → PDF works end to end; PDF → Markdown is next. There is nothing to download yet.

## What it does

The first version (V1) focuses on converting between PDF and Markdown:

- **Markdown → PDF** *(available)*: drag `.md` files onto the window, choose paper size, margins and font size, and get a PDF with the same typography on every operating system. Supports headings, emphasis, lists (nested and task lists), code blocks, tables, quotes, links and local images.
- **PDF → Markdown** *(planned)*, with two modes:
  - *Notes*: flowing text from digital PDFs (headings, lists, code, tables).
  - *Slides*: one section per slide, nested bullet lists, images for diagrams.

Each conversion produces a short report with non-fatal warnings (for example, low-confidence tables), because PDF → Markdown is heuristic and lossy by nature.

The long-term vision is a local "all-in-one" PDF toolbox where new tools can be added without breaking the existing ones.

## Not in V1

OCR (scanned or handwritten PDFs), machine-learning models, other formats such as DOCX or HTML, auto-update, and a web or hosted version.

## Tech stack

- [Tauri 2](https://tauri.app/) for the desktop shell
- React + TypeScript (Vite) for the UI
- Rust for the conversion engine

The engine is built around a neutral intermediate document model, so each format needs one reader and one writer instead of a converter for every pair. Markdown → PDF uses an embedded [Typst](https://typst.app/) compiler with bundled fonts: no external programs, and reproducible output.

## Project layout

```
crates/core      domain: document model, Tool contract, errors, jobs (depends on nothing)
crates/engines   adapters: Markdown reader, Typst PDF writer
crates/tools     use cases (md_to_pdf) and the tool registry
src-tauri        desktop shell: Tauri commands, job manager, wiring
src              React UI (drawn from the tool descriptors the engine returns)
fixtures         sample documents and golden files
```

Dependencies only point one way: `core ← engines ← tools ← src-tauri`. A new tool is a module in `crates/tools` registered in `Registry::new()`; the UI picks it up from its descriptor.

## Development

Requirements: [Rust](https://rustup.rs/) (stable), [Node.js](https://nodejs.org/) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform.

Run the tests (this also regenerates the TypeScript types under `src/shared/ipc/bindings/`, which are committed):

```bash
cargo test --workspace
```

Run the app:

Requirements: [Rust](https://rustup.rs/) (stable), [Node.js](https://nodejs.org/) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform.

```bash
npm install
npm run tauri dev
```

Try it with `fixtures/md-sample/sample.md`. The PDF is written next to the input file and never overwrites an existing one.

To produce a release build:

```bash
npm run tauri build
```

## Platforms

Windows and macOS. Builds are currently unsigned, so the operating system will show a security warning on first launch.

## License

Copyright (C) 2026 Luis Hernández Cuadrado. Licensed under the [AGPL-3.0](LICENSE) (see also [NOTICE](NOTICE)). Contributions are welcome and are accepted under the same license.
