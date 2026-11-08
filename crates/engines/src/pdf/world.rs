//! In-memory Typst `World`: one source, virtual files (images) and the bundled fonts.
//! No file system, no network, no system fonts, fixed date: reproducible output.

use std::collections::HashMap;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

const MAIN_PATH: &str = "/main.typ";

pub struct MemoryWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: FileId,
    source: Source,
    files: HashMap<String, Bytes>,
}

impl MemoryWorld {
    /// `files` maps a path without leading slash (e.g. `assets/img-1.png`) to its bytes.
    pub fn new(markup: String, files: HashMap<String, Bytes>) -> Self {
        let fonts: Vec<Font> = typst_assets::fonts()
            .flat_map(|data| Font::iter(Bytes::new(data)))
            .collect();
        let mut book = FontBook::new();
        for font in &fonts {
            book.push(font.info().clone());
        }
        let main = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new(MAIN_PATH).expect("constant path is valid"),
        )
        .intern();
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            main,
            source: Source::new(main, markup),
            files,
        }
    }
}

impl World for MemoryWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let key = id.vpath().get_without_slash();
        self.files
            .get(key)
            .cloned()
            .ok_or_else(|| FileError::NotFound(key.into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        Datetime::from_ymd(2026, 1, 1)
    }
}
